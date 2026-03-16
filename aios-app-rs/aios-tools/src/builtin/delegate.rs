//! Delegate tool — hand off tasks to specialized sub-agents.
//!
//! The `delegate_to` tool allows the kernel AI to spawn a specialized sub-agent
//! for complex tasks.  Each sub-agent gets its own conversation context and
//! tool subset, allowing focused expertise on the delegated task.
//!
//! Like [`UiPanelTool`](super::UiPanelTool), this tool uses a callback pattern:
//! the LLM manager registers the callback that actually runs the sub-agent
//! conversation.  This keeps the tool layer decoupled from the LLM layer.

use std::sync::{Arc, Mutex};

use aios_core::types::ToolResult;
use tracing::warn;

use crate::tool::Tool;

// ---------------------------------------------------------------------------
// Callback type
// ---------------------------------------------------------------------------

/// Callback signature for delegate_to: receives `(agent_type, task, context)`
/// and returns a result string from the sub-agent, or `None` if delegation
/// could not be performed.
pub type DelegateCallback =
    Arc<dyn Fn(String, String, String) -> Option<String> + Send + Sync>;

// ---------------------------------------------------------------------------
// DelegateTool
// ---------------------------------------------------------------------------

/// Delegate a task to a specialized sub-agent.
///
/// The callback is set by the LLM manager at startup via
/// [`set_delegate_callback`](DelegateTool::set_delegate_callback).  When the
/// tool is executed, it parses the arguments and invokes the callback, which
/// runs a sub-agent conversation and returns the result.
pub struct DelegateTool {
    delegate_callback: Arc<Mutex<Option<DelegateCallback>>>,
}

impl DelegateTool {
    /// Create a new `DelegateTool` with no callback registered.
    pub fn new() -> Self {
        Self {
            delegate_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Register the callback that runs a sub-agent conversation.
    ///
    /// The callback receives `(agent_type, task, context)` and must return:
    /// - `Some(result)` with the sub-agent's response
    /// - `None` if delegation could not be performed
    pub fn set_delegate_callback(
        &self,
        cb: impl Fn(String, String, String) -> Option<String> + Send + Sync + 'static,
    ) {
        let mut guard = self.delegate_callback.lock().unwrap();
        *guard = Some(Arc::new(cb));
    }
}

impl Default for DelegateTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for DelegateTool {
    fn name(&self) -> &str {
        "delegate_to"
    }

    fn description(&self) -> &str {
        "Delegate a task to a specialized sub-agent. Use this for complex \
         multi-step tasks that benefit from focused expertise. Available agents: \
         coder (code analysis/debugging), researcher (web search/info gathering), \
         sysadmin (system commands/processes), file_manager (file operations), \
         analyst (data analysis)."
    }

    fn category(&self) -> &str {
        "system"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent_type": {
                    "type": "string",
                    "enum": ["coder", "researcher", "sysadmin", "file_manager", "analyst"],
                    "description": "The type of specialized sub-agent to delegate to."
                },
                "task": {
                    "type": "string",
                    "description": "Detailed description of what the sub-agent should do."
                },
                "context": {
                    "type": "string",
                    "description": "Relevant context from the current conversation."
                }
            },
            "required": ["agent_type", "task"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let agent_type = match args.get("agent_type").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => return ToolResult::fail("'agent_type' is required."),
        };

        // Validate agent type.
        let valid_types = ["coder", "researcher", "sysadmin", "file_manager", "analyst"];
        if !valid_types.contains(&agent_type.as_str()) {
            return ToolResult::fail(format!(
                "Invalid agent type: {agent_type:?}. Valid types: {}",
                valid_types.join(", ")
            ));
        }

        let task = match args.get("task").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => return ToolResult::fail("'task' is required and must not be empty."),
        };

        let context = args
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Invoke the callback.
        let callback = {
            let guard = self.delegate_callback.lock().unwrap();
            guard.clone()
        };

        match callback {
            Some(cb) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cb(agent_type.clone(), task.clone(), context)
                }));
                match result {
                    Ok(Some(response)) => ToolResult::ok_with_data(
                        response.clone(),
                        serde_json::json!({
                            "agent_type": agent_type,
                            "task": task,
                            "result": response,
                        }),
                    ),
                    Ok(None) => ToolResult::fail(
                        "Delegation could not be performed (no agent orchestrator available).",
                    ),
                    Err(_) => ToolResult::fail("Delegate callback panicked."),
                }
            }
            None => {
                warn!("delegate_to callback not registered; cannot delegate");
                ToolResult::fail(
                    "No delegate callback registered. Cannot delegate without an agent orchestrator.",
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_callback_returns_error() {
        let tool = DelegateTool::new();
        let r = tool.execute(serde_json::json!({
            "agent_type": "coder",
            "task": "Fix the login bug"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("callback"));
    }

    #[test]
    fn missing_agent_type_returns_error() {
        let tool = DelegateTool::new();
        let r = tool.execute(serde_json::json!({
            "task": "Do something"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("agent_type"));
    }

    #[test]
    fn missing_task_returns_error() {
        let tool = DelegateTool::new();
        let r = tool.execute(serde_json::json!({
            "agent_type": "coder"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("task"));
    }

    #[test]
    fn empty_task_returns_error() {
        let tool = DelegateTool::new();
        let r = tool.execute(serde_json::json!({
            "agent_type": "coder",
            "task": ""
        }));
        assert!(!r.success);
    }

    #[test]
    fn invalid_agent_type_returns_error() {
        let tool = DelegateTool::new();
        let r = tool.execute(serde_json::json!({
            "agent_type": "wizard",
            "task": "Cast a spell"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("Invalid agent type"));
    }

    #[test]
    fn callback_returns_result() {
        let tool = DelegateTool::new();
        tool.set_delegate_callback(|agent_type, task, _ctx| {
            Some(format!("[{agent_type}] completed: {task}"))
        });

        let r = tool.execute(serde_json::json!({
            "agent_type": "coder",
            "task": "Fix the login bug",
            "context": "User reported auth failures"
        }));
        assert!(r.success);
        assert!(r.output.contains("completed"));
        assert!(r.output.contains("coder"));

        let data = r.data.unwrap();
        assert_eq!(data.get("agent_type").and_then(|v| v.as_str()), Some("coder"));
    }

    #[test]
    fn callback_returns_none() {
        let tool = DelegateTool::new();
        tool.set_delegate_callback(|_agent_type, _task, _ctx| None);

        let r = tool.execute(serde_json::json!({
            "agent_type": "researcher",
            "task": "Search for info"
        }));
        assert!(!r.success);
    }

    #[test]
    fn all_agent_types_accepted() {
        let tool = DelegateTool::new();
        tool.set_delegate_callback(|agent_type, _task, _ctx| {
            Some(format!("ok from {agent_type}"))
        });

        for agent_type in &["coder", "researcher", "sysadmin", "file_manager", "analyst"] {
            let r = tool.execute(serde_json::json!({
                "agent_type": agent_type,
                "task": "test task"
            }));
            assert!(r.success, "agent_type {agent_type} should be accepted");
        }
    }

    #[test]
    fn context_is_optional() {
        let tool = DelegateTool::new();
        tool.set_delegate_callback(|_agent_type, _task, ctx| {
            Some(format!("context was: '{ctx}'"))
        });

        let r = tool.execute(serde_json::json!({
            "agent_type": "coder",
            "task": "do thing"
        }));
        assert!(r.success);
        assert!(r.output.contains("context was: ''"));
    }
}
