//! LLM provider manager with automatic tool-call loop and cache warming.
//!
//! [`LlmManager`] holds registered providers, drives provider switching,
//! implements the tool-call loop that repeats LLM calls until the model
//! produces a final text answer (or a safety limit is hit), and manages
//! prompt-cache lifecycle (warmup on boot, background keep-warm pings,
//! fingerprint persistence).
//!
//! ## Cascading model routing
//!
//! When enabled (default), the chat loop starts with the auto-detected
//! effort level and escalates to a more capable model if quality
//! heuristics indicate the response is inadequate.  See [`CascadeConfig`]
//! and [`CascadeRouter`] for details.
//!
//! ## Semantic response caching
//!
//! A keyword-based semantic cache avoids redundant LLM calls for
//! identical or near-identical questions.  See [`SemanticCache`] for
//! details.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::task::JoinHandle;
use tracing::{info, warn, error, debug};

use aios_core::channel::ChannelContext;
use aios_core::types::{EffortLevel, LlmResponse, Message, QualityMode, ToolSchema, Usage};

use crate::cascade::{CascadeConfig, CascadeRouter};
use crate::context::ContextManager;
use crate::error::{LlmError, Result};
use crate::prefetch::Prefetcher;
use crate::provider::LlmProvider;
use crate::semantic_cache::SemanticCache;

/// Safety limit — the model cannot loop more than this many tool-call rounds.
const MAX_TOOL_ROUNDS: usize = 25;

/// Default path for the cache fingerprint file.
const DEFAULT_FINGERPRINT_FILENAME: &str = "cache_fingerprint.json";

/// Signature for the tool executor callback.
///
/// Receives `(tool_name, arguments, channel_context)` and returns a string
/// result that is fed back to the model as a tool-result message.
///
/// The [`ChannelContext`] tells the executor which channel is active so
/// channel-aware tools (e.g. `ui_panel`, `display`) can adapt their output.
///
/// The callback is wrapped in `Arc` so it can be shared across `.await` points.
pub type ToolExecutor =
    Arc<dyn Fn(String, serde_json::Value, ChannelContext) -> String + Send + Sync>;

/// Manages registered LLM providers and drives the tool-call loop.
///
/// # Cache warming
///
/// When the active provider supports prompt caching (i.e.
/// [`cache_config()`](LlmProvider::cache_config) returns `Some`), `LlmManager`
/// can:
///
/// 1. **Warm up on boot** — call [`warmup_cache`](Self::warmup_cache) to
///    re-send the last system prompt + tools and re-link the cloud cache.
/// 2. **Keep warm** — call [`start_keep_warm`](Self::start_keep_warm) to
///    spawn a background tokio task that pings the provider at the interval
///    specified by the provider's [`CacheConfig`](crate::provider::CacheConfig).
/// 3. **Auto-save** — after every successful `chat()` call the fingerprint
///    is persisted to disk, so the next boot can warm up.
///
/// # Example
///
/// ```ignore
/// let mut manager = LlmManager::new();
/// manager.register_provider(Box::new(ClaudeProvider::new("sk-ant-...", None, None)));
/// manager.register_provider(Box::new(OpenAIProvider::new("sk-...", None, None)));
/// manager.set_active("claude")?;
/// manager.set_tool_executor(Arc::new(|name, args, _channel| format!("executed {name}")));
/// manager.warmup_cache().await?;
/// let _handle = manager.start_keep_warm();
/// let response = manager.chat("What time is it?", None, &[], None).await?;
/// ```
pub struct LlmManager {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    active_name: Option<String>,
    tool_executor: Option<ToolExecutor>,
    max_tool_rounds: usize,
    /// Path to the fingerprint file.  Defaults to `~/.aios/cache_fingerprint.json`.
    fingerprint_path: PathBuf,
    /// Context pruning manager.
    context_manager: ContextManager,
    /// Speculative data pre-fetcher.
    prefetcher: Prefetcher,
    /// Manual effort level override.  `None` means auto-detect.
    effort_override: Option<EffortLevel>,
    /// Configuration for cascading model routing (escalation).
    cascade_config: CascadeConfig,
    /// Keyword-based semantic response cache.
    semantic_cache: SemanticCache,
    /// Current quality mode controlling the escalation strategy.
    quality_mode: QualityMode,
    /// Whether the user has requested a retry (reset after each chat call).
    user_retry_pending: bool,
    /// The currently active output channel.  Passed to tool executors so
    /// channel-aware tools can adapt their behaviour.
    active_channel: ChannelContext,
}

impl LlmManager {
    /// Default TTL for the semantic response cache (5 minutes).
    const DEFAULT_CACHE_TTL_SECS: u64 = 300;
    /// Default maximum entries in the semantic response cache.
    const DEFAULT_CACHE_MAX_ENTRIES: usize = 200;

    /// Create a new, empty manager with no registered providers.
    pub fn new() -> Self {
        let fingerprint_path = dirs_fingerprint_path();
        Self {
            providers: HashMap::new(),
            active_name: None,
            tool_executor: None,
            max_tool_rounds: MAX_TOOL_ROUNDS,
            fingerprint_path,
            context_manager: ContextManager::default(),
            prefetcher: Prefetcher::new(),
            effort_override: None,
            cascade_config: CascadeConfig::default(),
            semantic_cache: SemanticCache::new(
                Duration::from_secs(Self::DEFAULT_CACHE_TTL_SECS),
                Self::DEFAULT_CACHE_MAX_ENTRIES,
            ),
            quality_mode: QualityMode::default(),
            user_retry_pending: false,
            active_channel: ChannelContext::default(),
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

    // -- Active channel -----------------------------------------------------

    /// Set the currently active output channel.
    ///
    /// This is passed to tool executors so channel-aware tools
    /// (e.g. `ui_panel`, `display`) can adapt their rendering.
    pub fn set_active_channel(&mut self, channel: ChannelContext) {
        self.active_channel = channel;
    }

    /// Get the currently active output channel.
    pub fn active_channel(&self) -> &ChannelContext {
        &self.active_channel
    }

    // -- Fingerprint path ---------------------------------------------------

    /// Override the default fingerprint path.
    pub fn set_fingerprint_path(&mut self, path: PathBuf) {
        self.fingerprint_path = path;
    }

    /// Return the current fingerprint path.
    pub fn fingerprint_path(&self) -> &std::path::Path {
        &self.fingerprint_path
    }

    // -- Tool routing (Optimization 1) ---------------------------------------

    /// Given a user message, determine which tool categories are likely needed.
    ///
    /// Uses fast keyword matching — no LLM call required.  Returns a list of
    /// category names suitable for passing to
    /// [`ToolRegistry::get_schemas_by_categories`](aios_tools::ToolRegistry::get_schemas_by_categories).
    ///
    /// If no keywords match, returns an empty vector which signals the caller
    /// to fall back to sending all tools.
    pub fn route_tools(&self, user_message: &str) -> Vec<String> {
        let msg_lower = user_message.to_lowercase();
        let mut categories = Vec::new();

        // Filesystem keywords.
        let filesystem_keywords = ["file", "read", "write", "directory", "folder", "path", "save"];
        if filesystem_keywords.iter().any(|kw| msg_lower.contains(kw)) {
            categories.push("filesystem".to_string());
        }

        // Memory keywords.
        let memory_keywords = ["remember", "recall", "forget", "memory", "memorize"];
        if memory_keywords.iter().any(|kw| msg_lower.contains(kw)) {
            categories.push("memory".to_string());
        }

        // System keywords.
        let system_keywords = ["system", "process", "info", "command", "run", "execute", "shell"];
        if system_keywords.iter().any(|kw| msg_lower.contains(kw)) {
            categories.push("system".to_string());
        }

        // Network keywords.
        let network_keywords = ["url", "web", "search", "download", "fetch", "http", "website"];
        if network_keywords.iter().any(|kw| msg_lower.contains(kw)) {
            categories.push("network".to_string());
        }

        // UI keywords.
        let ui_keywords = ["show", "display", "image", "notification", "panel", "render"];
        if ui_keywords.iter().any(|kw| msg_lower.contains(kw)) {
            categories.push("ui".to_string());
        }

        categories
    }

    // -- Effort level (Optimization 4) ----------------------------------------

    /// Set or clear a manual effort level override.
    ///
    /// Pass `Some(level)` to force a specific effort, or `None` to use
    /// automatic detection.
    pub fn set_effort_override(&mut self, level: Option<EffortLevel>) {
        self.effort_override = level;
    }

    /// Return the current effort override, if any.
    pub fn effort_override(&self) -> Option<EffortLevel> {
        self.effort_override
    }

    /// Automatically determine the effort level based on message content.
    ///
    /// Uses keyword heuristics — no LLM call required.
    ///
    /// - **Low**: short messages (<30 chars), simple questions, greetings
    /// - **High**: messages containing "refactor", "security", "analyze",
    ///   "audit", "review all", "delete", destructive operations, long
    ///   complex instructions (>500 chars)
    /// - **Medium**: everything else
    pub fn auto_effort(&self, message: &str) -> EffortLevel {
        // If there's a manual override, use it.
        if let Some(override_level) = self.effort_override {
            return override_level;
        }

        let msg_lower = message.to_lowercase();
        let msg_len = message.len();

        // High effort indicators.
        let high_keywords = [
            "refactor", "security", "analyze", "analyse", "audit",
            "review all", "delete all", "remove all", "rewrite",
            "restructure", "migrate", "vulnerability", "critical",
            "dangerous", "destructive", "rm -rf", "format disk",
        ];
        if high_keywords.iter().any(|kw| msg_lower.contains(kw)) {
            return EffortLevel::High;
        }

        // Long, complex instructions suggest high effort.
        if msg_len > 500 {
            return EffortLevel::High;
        }

        // Low effort indicators: short messages, greetings, simple questions.
        let low_keywords = [
            "hello", "hi", "hey", "thanks", "thank you", "ok", "okay",
            "bye", "good morning", "good night", "what time",
            "what date", "who are you", "help",
        ];
        if msg_len < 30 && low_keywords.iter().any(|kw| msg_lower.contains(kw)) {
            return EffortLevel::Low;
        }

        // Very short messages (single word, quick commands) are low effort.
        if msg_len < 15 && !msg_lower.contains(' ') {
            return EffortLevel::Low;
        }

        EffortLevel::Medium
    }

    // -- Cascade configuration ------------------------------------------------

    /// Return the current cascade configuration.
    pub fn cascade_config(&self) -> &CascadeConfig {
        &self.cascade_config
    }

    /// Replace the cascade configuration.
    pub fn set_cascade_config(&mut self, config: CascadeConfig) {
        self.cascade_config = config;
    }

    // -- Quality mode ---------------------------------------------------------

    /// Return the current quality mode.
    pub fn quality_mode(&self) -> QualityMode {
        self.quality_mode
    }

    /// Set the quality mode.
    pub fn set_quality_mode(&mut self, mode: QualityMode) {
        self.quality_mode = mode;
        info!("Quality mode set to: {mode}");
    }

    /// Mark that the user has requested a retry.
    ///
    /// This flag is consumed (reset) by the next `chat()` call.
    pub fn mark_user_retry(&mut self) {
        self.user_retry_pending = true;
    }

    // -- Semantic cache -------------------------------------------------------

    /// Return a reference to the semantic response cache.
    pub fn semantic_cache(&self) -> &SemanticCache {
        &self.semantic_cache
    }

    /// Return a mutable reference to the semantic response cache.
    pub fn semantic_cache_mut(&mut self) -> &mut SemanticCache {
        &mut self.semantic_cache
    }

    // -- Context pruning (Optimization 2) -----------------------------------

    /// Return a reference to the context manager.
    pub fn context_manager(&self) -> &ContextManager {
        &self.context_manager
    }

    /// Replace the context manager with a custom configuration.
    pub fn set_context_manager(&mut self, cm: ContextManager) {
        self.context_manager = cm;
    }

    // -- Prefetcher (Optimization 3) ----------------------------------------

    /// Return a reference to the speculative pre-fetcher.
    pub fn prefetcher(&self) -> &Prefetcher {
        &self.prefetcher
    }

    // -- System prompt builder ----------------------------------------------

    /// Build the system prompt injected at the start of every request.
    ///
    /// Includes the current UTC date/time, a summary of available tools,
    /// any extra context key-value pairs, and optionally recent episodic
    /// memories.
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
        Self::get_system_prompt_with_memory(context, available_tools, None)
    }

    /// Build the system prompt with optional episodic memory context.
    ///
    /// Like [`get_system_prompt`](Self::get_system_prompt) but appends a
    /// block of recent episodic memories when `episodic_context` is provided.
    pub fn get_system_prompt_with_memory(
        context: Option<&HashMap<String, String>>,
        available_tools: &[ToolSchema],
        episodic_context: Option<&str>,
    ) -> String {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();

        let mut parts: Vec<String> = vec![
            "You are AiOS, an AI-native operating system.  You are the \
             primary actor: humans express intent through the prompt and \
             you decide how to fulfill it."
                .to_string(),
            format!("Current date/time: {now}"),
            "IMPORTANT tool usage rules:\n\
             - When the user asks you to remember, memorize, or store something, you MUST use the \
             'memory' tool with action 'memorize'. Do NOT just say you will remember — actually \
             call the tool.\n\
             - When the user asks what you remember, or asks about something they previously told \
             you to remember, use the 'memory' tool with action 'recall' or 'list_keys'.\n\
             - When the user asks you to forget something, use the 'memory' tool with action 'forget'.\n\
             - Always use tools to perform actions. Never claim to have done something without \
             actually calling the appropriate tool."
                .to_string(),
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

        if let Some(ep_ctx) = episodic_context {
            if !ep_ctx.is_empty() {
                parts.push(ep_ctx.to_string());
            }
        }

        parts.join("\n\n")
    }

    // -- Cache management ---------------------------------------------------

    /// Warm up the cloud cache on startup.
    ///
    /// Loads the fingerprint from disk and, if the active provider supports
    /// caching, sends a minimal request to re-link the cloud cache.
    ///
    /// This is safe to call even if no fingerprint exists, no provider is
    /// active, or the provider does not support caching — it simply no-ops.
    pub async fn warmup_cache(&self) -> Result<()> {
        let provider = match self.active_provider() {
            Ok(p) => p,
            Err(_) => {
                debug!("warmup_cache: no active provider");
                return Ok(());
            }
        };

        if provider.cache_config().is_none() {
            debug!("warmup_cache: provider '{}' does not support caching", provider.name());
            return Ok(());
        }

        if provider.api_key().is_empty() {
            debug!("warmup_cache: no API key set, skipping");
            return Ok(());
        }

        let fp = match provider.load_fingerprint(&self.fingerprint_path)? {
            Some(fp) => fp,
            None => {
                info!("warmup_cache: no fingerprint found at {:?}, skipping", self.fingerprint_path);
                return Ok(());
            }
        };

        info!(
            "warmup_cache: re-linking cache for provider '{}' (hash={}, model={})",
            provider.name(),
            fp.hash,
            fp.model,
        );

        let tools = fp.tools();
        let system_prompt = if fp.system_prompt.is_empty() {
            None
        } else {
            Some(fp.system_prompt.as_str())
        };
        let tools_ref = if tools.is_empty() {
            None
        } else {
            Some(tools.as_slice())
        };

        match provider.warmup(system_prompt, tools_ref).await {
            Ok(()) => {
                info!("warmup_cache: success");
                Ok(())
            }
            Err(e) => {
                warn!("warmup_cache: warmup request failed: {e}");
                // Non-fatal — the app can still function without a warm cache.
                Ok(())
            }
        }
    }

    /// Spawn a background keep-warm task that periodically pings the cloud
    /// cache to prevent expiry.
    ///
    /// The task reads the provider's [`CacheConfig::keep_warm_interval`] and
    /// sleeps for that duration between pings.  If a recent real request was
    /// sent (within the interval), the ping is skipped because the cache is
    /// already warm.
    ///
    /// Returns `None` if the active provider does not support caching or has
    /// no API key.  Otherwise returns the [`JoinHandle`] which can be used
    /// to abort the task on shutdown.
    ///
    /// # Panics
    ///
    /// Must be called from within a tokio runtime context.
    pub fn start_keep_warm(&self) -> Option<JoinHandle<()>> {
        let provider = match self.active_provider() {
            Ok(p) => p,
            Err(_) => return None,
        };

        let cache_config = provider.cache_config()?;

        if provider.api_key().is_empty() {
            debug!("start_keep_warm: no API key, not starting");
            return None;
        }

        let interval = cache_config.keep_warm_interval;
        let fp_path = self.fingerprint_path.clone();

        // We need to read the fingerprint to know what prompt+tools to send.
        // Load it now; if it's missing we can't keep-warm anything.
        let fp = match provider.load_fingerprint(&fp_path) {
            Ok(Some(fp)) => fp,
            _ => {
                debug!("start_keep_warm: no fingerprint, not starting");
                return None;
            }
        };

        let system_prompt = if fp.system_prompt.is_empty() {
            None
        } else {
            Some(fp.system_prompt.clone())
        };
        let tools = fp.tools();

        // We need to clone the provider name so we can look it up later.
        // However, the provider is behind a trait object and is not Send-able
        // by moving.  Instead we capture the data we need for warmup and
        // rely on the fact that warmup is just an HTTP call.
        //
        // To call provider.warmup() we need &self, but we can't move the
        // provider into the task.  Instead, we reconstruct a minimal
        // ClaudeProvider from the API key + model.  This is acceptable
        // because warmup() only needs the HTTP client + API key + model.
        let api_key = provider.api_key().to_string();
        let model = provider.model().to_string();
        let provider_name = provider.name().to_string();

        info!(
            "Starting keep-warm task for '{}': interval={:?}",
            provider_name, interval,
        );

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;

                // Build a fresh provider for the warmup ping.
                // We only support Claude caching today — if more providers
                // add caching we'd need a factory here.
                let warmup_provider = crate::claude::ClaudeProvider::new(
                    api_key.clone(),
                    Some(model.clone()),
                    Some(1), // max_tokens=1 for warmup
                );

                let sp = system_prompt.as_deref();
                let tools_ref = if tools.is_empty() {
                    None
                } else {
                    Some(tools.as_slice())
                };

                // Note: last_request_at() on the warmup provider is always None
                // (freshly constructed), so we skip the stale check and always
                // send the ping. The real provider's usage is not accessible here.
                debug!("keep-warm: sending ping for '{}'", provider_name);
                match warmup_provider.warmup(sp, tools_ref).await {
                    Ok(()) => {
                        debug!("keep-warm: ping succeeded");
                    }
                    Err(e) => {
                        warn!("keep-warm: ping failed: {e}");
                        // Non-fatal — we'll retry on the next interval.
                    }
                }
            }
        });

        Some(handle)
    }

    /// Save the cache fingerprint for the active provider.
    ///
    /// Called automatically after each successful `chat()` call when the
    /// provider supports caching.
    fn auto_save_fingerprint(&self) {
        let provider = match self.active_provider() {
            Ok(p) => p,
            Err(_) => return,
        };

        if provider.cache_config().is_none() {
            return;
        }

        if let Err(e) = provider.save_fingerprint(&self.fingerprint_path) {
            warn!("Failed to save cache fingerprint: {e}");
        }
    }

    // -- Core chat loop -----------------------------------------------------

    /// Send a user message and run the tool-call loop to completion.
    ///
    /// The integrated flow:
    ///
    /// 1. **Auto-detect effort level** (Optimization 4) — or use a manual
    ///    override.  Sets the effort on the active provider, controlling
    ///    model selection and token budget.
    /// 2. **Start speculative pre-fetch** (Optimization 3) — analyse the
    ///    message and spawn background tasks to fetch data that tools may need.
    /// 3. **Check context pruning** (Optimization 2) — if the conversation
    ///    history is too long, produce a summarization prompt (to be sent to
    ///    the LLM externally) and prune old messages.
    /// 4. Append the user message to `conversation_history`.
    /// 5. Call the active provider's [`send_message`](LlmProvider::send_message).
    /// 6. If the response contains tool calls **and** a tool executor is set,
    ///    execute each tool, append results, and loop back to step 5.
    /// 7. Return the final [`LlmResponse`].
    ///
    /// A safety limit of [`MAX_TOOL_ROUNDS`] (25) prevents infinite loops.
    ///
    /// After a successful response the cache fingerprint is automatically
    /// saved to disk if the provider supports caching.
    ///
    /// # Arguments
    ///
    /// * `user_message` — The human's input.
    /// * `conversation_history` — Mutable reference to the conversation.
    ///   Pass `None` to use a fresh, internally-created list.
    /// * `tools` — Tool schemas available to the model.  If you have
    ///   already routed tools via [`route_tools`](Self::route_tools), pass
    ///   the filtered set.  Otherwise pass the full set and routing will
    ///   be skipped (the caller is responsible for routing when desired).
    /// * `system_prompt` — Explicit system prompt.  If `None`, a default is
    ///   generated via [`get_system_prompt`](Self::get_system_prompt).
    pub async fn chat(
        &mut self,
        user_message: &str,
        conversation_history: Option<&mut Vec<Message>>,
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
    ) -> Result<LlmResponse> {
        // -- 0. Detect user retry and consume the flag ------------------------
        let user_requested_retry = self.user_retry_pending
            || CascadeRouter::is_retry_request(user_message);
        self.user_retry_pending = false;

        // -- 0b. Semantic cache check -----------------------------------------
        // Skip cache for slash commands, when tools are available, and on retry.
        if !user_message.starts_with('/') && tools.is_empty() && !user_requested_retry {
            if let Some(cached) = self.semantic_cache.get(user_message) {
                info!(
                    "Semantic cache hit for: {}",
                    &user_message[..user_message.len().min(40)]
                );
                return Ok(LlmResponse {
                    content: Some(cached),
                    tool_calls: Vec::new(),
                    usage: Usage::default(),
                });
            }
        }

        // -- 1. Auto-detect effort level + quality mode routing ---------------
        let auto_effort = self.auto_effort(user_message);
        let mut current_effort = CascadeRouter::starting_effort(
            self.quality_mode,
            auto_effort,
        );
        if let Ok(provider) = self.active_provider_mut() {
            provider.set_effort(current_effort);
        }
        debug!(
            "chat: quality_mode={}, effort={current_effort}, message_len={}",
            self.quality_mode,
            user_message.len(),
        );

        // -- 2. Start speculative pre-fetch (Optimization 3) ----------------
        self.prefetcher.prefetch(user_message).await;

        // Use the caller's history or create a temporary one.
        let mut owned_history: Vec<Message>;
        let history: &mut Vec<Message> = match conversation_history {
            Some(h) => h,
            None => {
                owned_history = Vec::new();
                &mut owned_history
            }
        };

        // -- 3. Check context pruning (Optimization 2) ----------------------
        if self.context_manager.needs_pruning(history) {
            let estimated = ContextManager::estimate_tokens(history);
            info!(
                "Context pruning triggered: estimated {} tokens (threshold: {})",
                estimated,
                self.context_manager.prune_threshold(),
            );

            let _summarization_prompt = ContextManager::summarization_prompt(history);

            let pruned = self.context_manager.prune(
                history,
                "(Earlier conversation context was summarized to manage context length.)",
            );
            *history = pruned;
        }

        history.push(Message::user(user_message));

        let sys_prompt: String;
        let system_prompt = match system_prompt {
            Some(sp) => sp,
            None => {
                let mut base = Self::get_system_prompt(None, tools);
                // Inject active channel info so the AI adapts its responses.
                let channel = &self.active_channel;
                base.push_str(&format!(
                    "\n\nActive channel: {}. {}",
                    channel.kind,
                    match channel.kind {
                        aios_core::channel::ChannelKind::Signal =>
                            "Keep responses concise. No markdown formatting. Max ~4000 chars.",
                        aios_core::channel::ChannelKind::Voice =>
                            "Keep responses short and conversational. No formatting, no code blocks.",
                        aios_core::channel::ChannelKind::Web =>
                            "Full web interface. Markdown, code blocks, and images are supported.",
                        aios_core::channel::ChannelKind::Desktop =>
                            "Full desktop interface. Markdown, code blocks, and images are supported.",
                    }
                ));
                sys_prompt = base;
                &sys_prompt
            }
        };

        let mut total_usage = Usage::default();
        let mut rounds: usize = 0;
        let mut escalation_count: usize = 0;

        // -- Cascade + tool-call outer loop ------------------------------------
        'cascade: loop {
            let response = {
                let provider = self.active_provider()?;
                provider
                    .send_message(history, tools, Some(system_prompt))
                    .await?
            };

            // Accumulate token usage across rounds.
            total_usage.input_tokens += response.usage.input_tokens;
            total_usage.output_tokens += response.usage.output_tokens;

            if !response.has_tool_calls() {
                // -- Cascade quality check (text-only responses) ---------------
                if self.cascade_config.enabled
                    && escalation_count < self.cascade_config.max_escalations
                {
                    if let Some(reason) = CascadeRouter::should_escalate(
                        &response,
                        self.quality_mode,
                        current_effort,
                        false,
                        user_requested_retry && escalation_count == 0,
                    ) {
                        if let Some(next) = CascadeRouter::escalate(current_effort) {
                            info!(
                                "Escalating from {current_effort} to {next}: {reason:?}"
                            );
                            current_effort = next;
                            escalation_count += 1;
                            if let Ok(p) = self.active_provider_mut() {
                                p.set_effort(current_effort);
                            }
                            continue 'cascade;
                        }
                    }
                }

                // No tool calls — we have the final answer.
                history.push(Message::assistant(
                    response.content.as_deref().unwrap_or(""),
                ));

                // Auto-save fingerprint after successful chat.
                self.auto_save_fingerprint();

                // Store in semantic cache if applicable.
                // (We only cache text-only responses, not tool-call responses.)
                if let Some(content) = &response.content {
                    if !user_message.starts_with('/') && tools.is_empty() {
                        self.semantic_cache.put(user_message, content);
                    }
                }

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

                self.auto_save_fingerprint();

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

                    self.auto_save_fingerprint();

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
            let mut tool_error_occurred = false;
            for tc in &response.tool_calls {
                let channel = self.active_channel.clone();
                let result = std::panic::catch_unwind(
                    std::panic::AssertUnwindSafe(|| {
                        executor(tc.name.clone(), tc.arguments.clone(), channel.clone())
                    }),
                );
                let result_str = match result {
                    Ok(s) => {
                        // Check if the tool result looks like an error.
                        let lower = s.to_lowercase();
                        if lower.starts_with("error") || lower.contains("error executing") {
                            tool_error_occurred = true;
                        }
                        s
                    }
                    Err(_) => {
                        error!("Tool '{}' panicked during execution", tc.name);
                        tool_error_occurred = true;
                        format!("Error executing tool '{}': tool panicked", tc.name)
                    }
                };

                history.push(Message::tool_result(&tc.id, result_str));
            }

            // -- Cascade check after tool errors --
            if tool_error_occurred
                && self.cascade_config.enabled
                && escalation_count < self.cascade_config.max_escalations
            {
                if let Some(reason) = CascadeRouter::should_escalate(
                    &response,
                    self.quality_mode,
                    current_effort,
                    true,
                    false,
                ) {
                    if let Some(next) = CascadeRouter::escalate(current_effort) {
                        info!(
                            "Escalating after tool error from {current_effort} to {next}: {reason:?}"
                        );
                        current_effort = next;
                        escalation_count += 1;
                        if let Ok(p) = self.active_provider_mut() {
                            p.set_effort(current_effort);
                        }
                        // Continue with the updated conversation — the tool
                        // results are already in history for the next round.
                    }
                }
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

/// Compute the default fingerprint path: `~/.aios/cache_fingerprint.json`.
fn dirs_fingerprint_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".aios")
        .join(DEFAULT_FINGERPRINT_FILENAME)
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

    #[test]
    fn default_fingerprint_path_is_under_aios() {
        let mgr = LlmManager::new();
        let path = mgr.fingerprint_path();
        assert!(
            path.to_string_lossy().contains(".aios"),
            "fingerprint path should be under .aios: {:?}",
            path,
        );
        assert!(
            path.to_string_lossy().ends_with("cache_fingerprint.json"),
            "fingerprint path should end with cache_fingerprint.json: {:?}",
            path,
        );
    }

    #[test]
    fn set_fingerprint_path_overrides_default() {
        let mut mgr = LlmManager::new();
        let custom = PathBuf::from("/tmp/custom_fp.json");
        mgr.set_fingerprint_path(custom.clone());
        assert_eq!(mgr.fingerprint_path(), custom.as_path());
    }

    #[test]
    fn auto_save_fingerprint_noop_without_provider() {
        let mgr = LlmManager::new();
        // Should not panic.
        mgr.auto_save_fingerprint();
    }

    #[test]
    fn auto_save_fingerprint_noop_for_non_caching_provider() {
        use crate::openai::OpenAIProvider;

        let mut mgr = LlmManager::new();
        mgr.register_provider(Box::new(OpenAIProvider::new("", None, None)));
        // Should not panic — OpenAI doesn't support caching.
        mgr.auto_save_fingerprint();
    }

    #[tokio::test]
    async fn warmup_cache_noop_without_provider() {
        let mgr = LlmManager::new();
        let result = mgr.warmup_cache().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn warmup_cache_noop_for_non_caching_provider() {
        use crate::openai::OpenAIProvider;

        let mut mgr = LlmManager::new();
        mgr.register_provider(Box::new(OpenAIProvider::new("", None, None)));
        let result = mgr.warmup_cache().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn warmup_cache_noop_without_api_key() {
        use crate::claude::ClaudeProvider;

        let mut mgr = LlmManager::new();
        mgr.register_provider(Box::new(ClaudeProvider::new("", None, None)));
        let result = mgr.warmup_cache().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn warmup_cache_noop_without_fingerprint() {
        use crate::claude::ClaudeProvider;

        let mut mgr = LlmManager::new();
        mgr.register_provider(Box::new(ClaudeProvider::new("test-key", None, None)));
        // Point to a non-existent fingerprint.
        mgr.set_fingerprint_path(PathBuf::from("/tmp/nonexistent_warmup_test_1234.json"));
        let result = mgr.warmup_cache().await;
        assert!(result.is_ok());
    }

    #[test]
    fn start_keep_warm_returns_none_without_provider() {
        let mgr = LlmManager::new();
        assert!(mgr.start_keep_warm().is_none());
    }

    #[test]
    fn start_keep_warm_returns_none_for_non_caching_provider() {
        use crate::openai::OpenAIProvider;

        let mut mgr = LlmManager::new();
        mgr.register_provider(Box::new(OpenAIProvider::new("", None, None)));
        assert!(mgr.start_keep_warm().is_none());
    }

    #[test]
    fn start_keep_warm_returns_none_without_api_key() {
        use crate::claude::ClaudeProvider;

        let mut mgr = LlmManager::new();
        mgr.register_provider(Box::new(ClaudeProvider::new("", None, None)));
        assert!(mgr.start_keep_warm().is_none());
    }

    // -----------------------------------------------------------------------
    // Optimization 1: Tool routing tests
    // -----------------------------------------------------------------------

    #[test]
    fn route_tools_filesystem() {
        let mgr = LlmManager::new();
        let cats = mgr.route_tools("read the file /etc/hostname");
        assert!(cats.contains(&"filesystem".to_string()));
    }

    #[test]
    fn route_tools_memory() {
        let mgr = LlmManager::new();
        let cats = mgr.route_tools("Remember that my name is Alice");
        assert!(cats.contains(&"memory".to_string()));
    }

    #[test]
    fn route_tools_system() {
        let mgr = LlmManager::new();
        let cats = mgr.route_tools("Run the command ls -la");
        assert!(cats.contains(&"system".to_string()));
    }

    #[test]
    fn route_tools_network() {
        let mgr = LlmManager::new();
        let cats = mgr.route_tools("Fetch the URL https://example.com");
        assert!(cats.contains(&"network".to_string()));
    }

    #[test]
    fn route_tools_ui() {
        let mgr = LlmManager::new();
        let cats = mgr.route_tools("Show me the image");
        assert!(cats.contains(&"ui".to_string()));
    }

    #[test]
    fn route_tools_multiple_categories() {
        let mgr = LlmManager::new();
        let cats = mgr.route_tools("Read the file and show it as an image");
        assert!(cats.contains(&"filesystem".to_string()));
        assert!(cats.contains(&"ui".to_string()));
    }

    #[test]
    fn route_tools_no_match() {
        let mgr = LlmManager::new();
        let cats = mgr.route_tools("Hello, how are you?");
        assert!(cats.is_empty());
    }

    #[test]
    fn route_tools_case_insensitive() {
        let mgr = LlmManager::new();
        let cats = mgr.route_tools("DOWNLOAD the FILE from the WEB");
        assert!(cats.contains(&"filesystem".to_string()));
        assert!(cats.contains(&"network".to_string()));
    }

    // -----------------------------------------------------------------------
    // Optimization 4: Effort level tests
    // -----------------------------------------------------------------------

    #[test]
    fn auto_effort_low_for_greeting() {
        let mgr = LlmManager::new();
        assert_eq!(mgr.auto_effort("hello"), EffortLevel::Low);
        assert_eq!(mgr.auto_effort("hi"), EffortLevel::Low);
        assert_eq!(mgr.auto_effort("thanks"), EffortLevel::Low);
    }

    #[test]
    fn auto_effort_medium_for_normal() {
        let mgr = LlmManager::new();
        assert_eq!(
            mgr.auto_effort("What is the capital of France?"),
            EffortLevel::Medium,
        );
    }

    #[test]
    fn auto_effort_high_for_security() {
        let mgr = LlmManager::new();
        assert_eq!(
            mgr.auto_effort("Analyze the security of this configuration"),
            EffortLevel::High,
        );
    }

    #[test]
    fn auto_effort_high_for_refactor() {
        let mgr = LlmManager::new();
        assert_eq!(
            mgr.auto_effort("Refactor the authentication module"),
            EffortLevel::High,
        );
    }

    #[test]
    fn auto_effort_high_for_long_message() {
        let mgr = LlmManager::new();
        let long = "x".repeat(600);
        assert_eq!(mgr.auto_effort(&long), EffortLevel::High);
    }

    #[test]
    fn auto_effort_override() {
        let mut mgr = LlmManager::new();
        mgr.set_effort_override(Some(EffortLevel::High));
        // Even a greeting should use High when overridden.
        assert_eq!(mgr.auto_effort("hello"), EffortLevel::High);
    }

    #[test]
    fn auto_effort_override_none_restores_auto() {
        let mut mgr = LlmManager::new();
        mgr.set_effort_override(Some(EffortLevel::Low));
        assert_eq!(mgr.auto_effort("Analyze all vulnerabilities"), EffortLevel::Low);
        mgr.set_effort_override(None);
        assert_eq!(mgr.auto_effort("Analyze all vulnerabilities"), EffortLevel::High);
    }

    // -----------------------------------------------------------------------
    // Optimization 2: Context pruning integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn context_manager_accessible() {
        let mgr = LlmManager::new();
        // Default thresholds should be reasonable.
        assert!(mgr.context_manager().prune_threshold() > 0);
        assert!(mgr.context_manager().max_tokens() > 0);
    }

    #[test]
    fn set_custom_context_manager() {
        let mut mgr = LlmManager::new();
        let custom_cm = ContextManager::with_config(50_000, 5_000, 3_000);
        mgr.set_context_manager(custom_cm);
        assert_eq!(mgr.context_manager().prune_threshold(), 5_000);
        assert_eq!(mgr.context_manager().prune_window(), 3_000);
    }

    // -----------------------------------------------------------------------
    // Optimization 3: Prefetcher integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn prefetcher_accessible() {
        let mgr = LlmManager::new();
        assert_eq!(mgr.prefetcher().cache_size(), 0);
    }

    // -----------------------------------------------------------------------
    // Episodic memory integration in system prompt
    // -----------------------------------------------------------------------

    #[test]
    fn system_prompt_with_memory_includes_episodes() {
        let ep_ctx = "Recent experiences:\n- [2h ago] Fixed SSH keys (task_completion)";
        let prompt = LlmManager::get_system_prompt_with_memory(None, &[], Some(ep_ctx));
        assert!(prompt.contains("Recent experiences:"));
        assert!(prompt.contains("Fixed SSH keys"));
    }

    #[test]
    fn system_prompt_with_memory_none_is_same_as_without() {
        let prompt_without = LlmManager::get_system_prompt(None, &[]);
        let prompt_with_none = LlmManager::get_system_prompt_with_memory(None, &[], None);
        assert_eq!(prompt_without, prompt_with_none);
    }

    #[test]
    fn system_prompt_with_memory_empty_string_omitted() {
        let prompt = LlmManager::get_system_prompt_with_memory(None, &[], Some(""));
        assert!(!prompt.contains("Recent experiences"));
    }

    // -----------------------------------------------------------------------
    // Quality mode tests
    // -----------------------------------------------------------------------

    #[test]
    fn quality_mode_default_is_balanced() {
        let mgr = LlmManager::new();
        assert_eq!(mgr.quality_mode(), QualityMode::Balanced);
    }

    #[test]
    fn set_quality_mode() {
        let mut mgr = LlmManager::new();
        mgr.set_quality_mode(QualityMode::Saver);
        assert_eq!(mgr.quality_mode(), QualityMode::Saver);
        mgr.set_quality_mode(QualityMode::Thorough);
        assert_eq!(mgr.quality_mode(), QualityMode::Thorough);
    }

    #[test]
    fn mark_user_retry_sets_flag() {
        let mut mgr = LlmManager::new();
        // Before marking, the flag is false (tested indirectly).
        assert!(!mgr.user_retry_pending);
        mgr.mark_user_retry();
        assert!(mgr.user_retry_pending);
    }

    // -----------------------------------------------------------------------
    // Cascade config tests
    // -----------------------------------------------------------------------

    #[test]
    fn cascade_config_default() {
        let mgr = LlmManager::new();
        let cfg = mgr.cascade_config();
        assert!(cfg.enabled);
        assert_eq!(cfg.max_escalations, 2);
        assert_eq!(cfg.min_valid_length, 10);
    }

    #[test]
    fn set_cascade_config() {
        let mut mgr = LlmManager::new();
        let custom = CascadeConfig {
            enabled: false,
            max_escalations: 5,
            min_valid_length: 20,
        };
        mgr.set_cascade_config(custom);
        assert!(!mgr.cascade_config().enabled);
        assert_eq!(mgr.cascade_config().max_escalations, 5);
    }

    // -----------------------------------------------------------------------
    // Semantic cache tests (integration via manager)
    // -----------------------------------------------------------------------

    #[test]
    fn semantic_cache_accessible() {
        let mgr = LlmManager::new();
        assert_eq!(mgr.semantic_cache().len(), 0);
    }

    #[test]
    fn semantic_cache_put_and_get() {
        let mut mgr = LlmManager::new();
        mgr.semantic_cache_mut().put("What time is it?", "It is 3pm.");
        let result = mgr.semantic_cache_mut().get("What time is it?");
        assert_eq!(result, Some("It is 3pm.".to_string()));
    }

    #[test]
    fn semantic_cache_slash_commands_not_cached() {
        let mut mgr = LlmManager::new();
        mgr.semantic_cache_mut().put("/help", "Available commands...");
        assert_eq!(mgr.semantic_cache().len(), 0);
    }
}
