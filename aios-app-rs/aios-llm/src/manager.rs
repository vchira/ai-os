//! LLM provider manager with automatic tool-call loop.
//!
//! [`LlmManager`] holds registered providers, drives provider switching,
//! and implements the tool-call loop that repeats LLM calls until the model
//! produces a final text answer (or a safety limit is hit).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tracing::{info, warn, error};

use aios_core::types::{LlmResponse, Message, ToolSchema, Usage};

use crate::error::{LlmError, Result};
use crate::provider::LlmProvider;

/// Safety limit — the model cannot loop more than this many tool-call rounds.
const MAX_TOOL_ROUNDS: usize = 25;

/// Signature for the tool executor callback.
///
/// Receives `(tool_name, arguments)` and returns a string result that is fed
/// back to the model as a tool-result message.
///
/// The callback is wrapped in `Arc` so it can be shared across `.await` points.
pub type ToolExecutor =
    Arc<dyn Fn(String, serde_json::Value) -> String + Send + Sync>;

/// Manages registered LLM providers and drives the tool-call loop.
///
/// # Example
///
/// ```ignore
/// let mut manager = LlmManager::new();
/// manager.register_provider(Box::new(ClaudeProvider::new("sk-ant-...", None, None)));
/// manager.register_provider(Box::new(OpenAIProvider::new("sk-...", None, None)));
/// manager.set_active("claude")?;
/// manager.set_tool_executor(Arc::new(|name, args| format!("executed {name}")));
/// let response = manager.chat("What time is it?", None, &[], None).await?;
/// ```
pub struct LlmManager {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    active_name: Option<String>,
    tool_executor: Option<ToolExecutor>,
    max_tool_rounds: usize,
}

impl LlmManager {
    /// Create a new, empty manager with no registered providers.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            active_name: None,
            tool_executor: None,
            max_tool_rounds: MAX_TOOL_ROUNDS,
        }
    }

    // -- Provider registry --------------------------------------------------

    /// Register (or replace) a provider.
    ///
    /// The first provider registered is automatically set as active.
    pub fn register_provider(&mut self, provider: Box<dyn LlmProvider>) {
        let name = provider.name().to_string();
        info!("Registered LLM provider: {}", name);
        self.providers.insert(name.clone(), provider);
        if self.active_name.is_none() {
            self.active_name = Some(name);
        }
    }

    /// Switch the active provider by name.
    ///
    /// Returns [`LlmError::NoProvider`] if the name has not been registered.
    pub fn set_active(&mut self, name: &str) -> Result<()> {
        if !self.providers.contains_key(name) {
            return Err(LlmError::NoProvider);
        }
        self.active_name = Some(name.to_string());
        info!("Active LLM provider set to: {}", name);
        Ok(())
    }

    /// Return a reference to the currently active provider.
    ///
    /// Returns [`LlmError::NoProvider`] if no provider has been registered.
    pub fn active_provider(&self) -> Result<&dyn LlmProvider> {
        let name = self.active_name.as_deref().ok_or(LlmError::NoProvider)?;
        self.providers
            .get(name)
            .map(|p| p.as_ref())
            .ok_or(LlmError::NoProvider)
    }

    /// Return a mutable reference to the currently active provider.
    pub fn active_provider_mut(&mut self) -> Result<&mut dyn LlmProvider> {
        let name = self
            .active_name
            .as_deref()
            .ok_or(LlmError::NoProvider)?
            .to_string();
        match self.providers.get_mut(&name) {
            Some(p) => Ok(p.as_mut()),
            None => Err(LlmError::NoProvider),
        }
    }

    /// Return a read-only view of registered provider names.
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Return the name of the currently active provider, if any.
    pub fn active_name(&self) -> Option<&str> {
        self.active_name.as_deref()
    }

    // -- API key management -------------------------------------------------

    /// Set the API key for the given provider.
    ///
    /// Returns [`LlmError::NoProvider`] if the provider name is unknown.
    pub fn set_api_key(&mut self, provider_name: &str, key: String) -> Result<()> {
        let provider = self
            .providers
            .get_mut(provider_name)
            .ok_or(LlmError::NoProvider)?;
        provider.set_api_key(key);
        info!("API key updated for provider: {}", provider_name);
        Ok(())
    }

    // -- Tool executor callback ---------------------------------------------

    /// Set the callback used to execute tool calls during the chat loop.
    pub fn set_tool_executor(&mut self, executor: ToolExecutor) {
        self.tool_executor = Some(executor);
    }

    /// Clear the tool executor.
    pub fn clear_tool_executor(&mut self) {
        self.tool_executor = None;
    }

    // -- System prompt builder ----------------------------------------------

    /// Build the system prompt injected at the start of every request.
    ///
    /// Includes the current UTC date/time, a summary of available tools, and
    /// any extra context key-value pairs the caller wants to surface.
    ///
    /// # Arguments
    ///
    /// * `context` — Arbitrary key-value pairs merged into the prompt
    ///   (e.g. `("hardware", "x86 + RTL8139 NIC")`).
    /// * `available_tools` — Tool definitions; only names and descriptions
    ///   are included in the prompt body.
    pub fn get_system_prompt(
        context: Option<&HashMap<String, String>>,
        available_tools: &[ToolSchema],
    ) -> String {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();

        let mut parts: Vec<String> = vec![
            "You are AiOS, an AI-native operating system.  You are the \
             primary actor: humans express intent through the prompt and \
             you decide how to fulfill it."
                .to_string(),
            format!("Current date/time: {now}"),
        ];

        if !available_tools.is_empty() {
            let tool_lines: Vec<String> = available_tools
                .iter()
                .map(|t| format!("  - {}: {}", t.name, t.description))
                .collect();
            parts.push(format!("Available tools:\n{}", tool_lines.join("\n")));
        }

        if let Some(ctx) = context {
            let ctx_lines: Vec<String> = ctx
                .iter()
                .map(|(k, v)| format!("  {k}: {v}"))
                .collect();
            parts.push(format!("System context:\n{}", ctx_lines.join("\n")));
        }

        parts.join("\n\n")
    }

    // -- Core chat loop -----------------------------------------------------

    /// Send a user message and run the tool-call loop to completion.
    ///
    /// 1. Append the user message to `conversation_history`.
    /// 2. Call the active provider's [`send_message`](LlmProvider::send_message).
    /// 3. If the response contains tool calls **and** a tool executor is set,
    ///    execute each tool, append results, and loop back to step 2.
    /// 4. Return the final [`LlmResponse`] (which has no outstanding tool calls).
    ///
    /// A safety limit of [`MAX_TOOL_ROUNDS`] (25) prevents infinite loops.
    ///
    /// # Arguments
    ///
    /// * `user_message` — The human's input.
    /// * `conversation_history` — Mutable reference to the conversation.
    ///   Pass `None` to use a fresh, internally-created list.
    /// * `tools` — Tool schemas available to the model.
    /// * `system_prompt` — Explicit system prompt.  If `None`, a default is
    ///   generated via [`get_system_prompt`](Self::get_system_prompt).
    pub async fn chat(
        &self,
        user_message: &str,
        conversation_history: Option<&mut Vec<Message>>,
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
    ) -> Result<LlmResponse> {
        let provider = self.active_provider()?;

        // Use the caller's history or create a temporary one.
        let mut owned_history: Vec<Message>;
        let history: &mut Vec<Message> = match conversation_history {
            Some(h) => h,
            None => {
                owned_history = Vec::new();
                &mut owned_history
            }
        };

        history.push(Message::user(user_message));

        let sys_prompt: String;
        let system_prompt = match system_prompt {
            Some(sp) => sp,
            None => {
                sys_prompt = Self::get_system_prompt(None, tools);
                &sys_prompt
            }
        };

        let mut total_usage = Usage::default();
        let mut rounds: usize = 0;

        loop {
            let response = provider
                .send_message(history, tools, Some(system_prompt))
                .await?;

            // Accumulate token usage across rounds.
            total_usage.input_tokens += response.usage.input_tokens;
            total_usage.output_tokens += response.usage.output_tokens;

            if !response.has_tool_calls() {
                // No tool calls — we have the final answer.
                history.push(Message::assistant(
                    response.content.as_deref().unwrap_or(""),
                ));
                return Ok(LlmResponse {
                    content: response.content,
                    tool_calls: Vec::new(),
                    usage: total_usage,
                });
            }

            // --- Tool-call round -----------------------------------------------
            rounds += 1;
            if rounds > self.max_tool_rounds {
                warn!(
                    "Tool-call loop exceeded {} rounds; returning last response as-is.",
                    self.max_tool_rounds,
                );
                history.push(Message::assistant_with_tools(
                    response.content.clone(),
                    response.tool_calls.clone(),
                ));
                return Ok(LlmResponse {
                    content: response.content,
                    tool_calls: response.tool_calls,
                    usage: total_usage,
                });
            }

            let executor = match &self.tool_executor {
                Some(e) => e.clone(),
                None => {
                    warn!(
                        "Model requested tool calls but no tool_executor is configured. \
                         Returning response with pending tool calls."
                    );
                    history.push(Message::assistant_with_tools(
                        response.content.clone(),
                        response.tool_calls.clone(),
                    ));
                    return Ok(LlmResponse {
                        content: response.content,
                        tool_calls: response.tool_calls,
                        usage: total_usage,
                    });
                }
            };

            // Record assistant message with its tool calls.
            history.push(Message::assistant_with_tools(
                response.content.clone(),
                response.tool_calls.clone(),
            ));

            // Execute each tool and feed results back.
            for tc in &response.tool_calls {
                let result = std::panic::catch_unwind(
                    std::panic::AssertUnwindSafe(|| {
                        executor(tc.name.clone(), tc.arguments.clone())
                    }),
                );
                let result_str = match result {
                    Ok(s) => s,
                    Err(_) => {
                        error!("Tool '{}' panicked during execution", tc.name);
                        format!("Error executing tool '{}': tool panicked", tc.name)
                    }
                };

                history.push(Message::tool_result(&tc.id, result_str));
            }

            // Loop back to send the updated conversation.
        }
    }
}

impl Default for LlmManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aios_core::types::ToolSchema;

    #[test]
    fn system_prompt_includes_datetime() {
        let prompt = LlmManager::get_system_prompt(None, &[]);
        assert!(prompt.contains("Current date/time:"));
        assert!(prompt.contains("AiOS"));
    }

    #[test]
    fn system_prompt_includes_tools() {
        let tools = vec![ToolSchema {
            name: "memory_store".into(),
            description: "Persist a key-value pair".into(),
            parameters: serde_json::json!({}),
        }];
        let prompt = LlmManager::get_system_prompt(None, &tools);
        assert!(prompt.contains("memory_store"));
        assert!(prompt.contains("Persist a key-value pair"));
    }

    #[test]
    fn system_prompt_includes_context() {
        let mut ctx = HashMap::new();
        ctx.insert("hardware".into(), "x86_64 with RTL8139 NIC".into());
        let prompt = LlmManager::get_system_prompt(Some(&ctx), &[]);
        assert!(prompt.contains("hardware"));
        assert!(prompt.contains("RTL8139"));
    }

    #[test]
    fn register_auto_selects_first() {
        use crate::claude::ClaudeProvider;

        let mut mgr = LlmManager::new();
        mgr.register_provider(Box::new(ClaudeProvider::new("", None, None)));
        assert_eq!(mgr.active_name(), Some("claude"));
    }

    #[test]
    fn set_active_unknown_returns_error() {
        let mut mgr = LlmManager::new();
        assert!(mgr.set_active("nonexistent").is_err());
    }

    #[test]
    fn provider_names_lists_all() {
        use crate::claude::ClaudeProvider;
        use crate::openai::OpenAIProvider;

        let mut mgr = LlmManager::new();
        mgr.register_provider(Box::new(ClaudeProvider::new("", None, None)));
        mgr.register_provider(Box::new(OpenAIProvider::new("", None, None)));

        let mut names = mgr.provider_names();
        names.sort();
        assert_eq!(names, vec!["claude", "openai"]);
    }
}
