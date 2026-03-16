//! Reflect tool — record episodic reflections about what just happened.
//!
//! The AI uses this tool after completing a task, encountering an error, or
//! learning something about the user's preferences.  The reflection is stored
//! as an [`Episode`](aios_core::memory::Episode) in the episodic memory store.
//!
//! Like [`UiPanelTool`](super::UiPanelTool), this tool uses a callback pattern:
//! the application layer registers a callback that writes to the
//! [`EpisodicMemory`](aios_core::memory::EpisodicMemory) instance.

use std::sync::{Arc, Mutex};

use aios_core::types::ToolResult;
use tracing::warn;

use crate::tool::Tool;

// ---------------------------------------------------------------------------
// Callback type
// ---------------------------------------------------------------------------

/// Callback signature for the reflect tool.
///
/// Receives the parsed reflection data as a JSON value and returns
/// `Ok(episode_id)` on success or `Err(error_message)` on failure.
pub type ReflectCallback =
    Arc<dyn Fn(serde_json::Value) -> Result<String, String> + Send + Sync>;

// ---------------------------------------------------------------------------
// ReflectTool
// ---------------------------------------------------------------------------

/// Record a reflection about what just happened.
///
/// The callback is set by the application layer at startup via
/// [`set_reflect_callback`](ReflectTool::set_reflect_callback).
pub struct ReflectTool {
    reflect_callback: Arc<Mutex<Option<ReflectCallback>>>,
}

impl ReflectTool {
    /// Create a new `ReflectTool` with no callback registered.
    pub fn new() -> Self {
        Self {
            reflect_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Register the callback that stores a reflection in episodic memory.
    ///
    /// The callback receives the parsed reflection arguments as a JSON value
    /// and should create an [`Episode`] and add it to the
    /// [`EpisodicMemory`] store.
    pub fn set_reflect_callback(
        &self,
        cb: impl Fn(serde_json::Value) -> Result<String, String> + Send + Sync + 'static,
    ) {
        let mut guard = self.reflect_callback.lock().unwrap();
        *guard = Some(Arc::new(cb));
    }
}

impl Default for ReflectTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ReflectTool {
    fn name(&self) -> &str {
        "reflect"
    }

    fn description(&self) -> &str {
        "Record a reflection about what just happened. Use this after completing \
         a task, encountering an error, or learning something about the user's \
         preferences. This builds episodic memory for future reference."
    }

    fn category(&self) -> &str {
        "memory"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "Brief summary of what happened."
                },
                "category": {
                    "type": "string",
                    "enum": [
                        "task_completion",
                        "error_resolution",
                        "user_preference",
                        "system_configuration",
                        "file_modification",
                        "web_research",
                        "conversation"
                    ],
                    "description": "Category of the episode."
                },
                "outcome": {
                    "type": "string",
                    "enum": ["success", "failure", "partial"],
                    "description": "Whether the task succeeded, failed, or partially succeeded."
                },
                "details": {
                    "type": "string",
                    "description": "Detailed description of what happened."
                },
                "lessons": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Lessons learned from this episode."
                },
                "failure_reason": {
                    "type": "string",
                    "description": "If outcome is failure, why it failed."
                },
                "partial_worked": {
                    "type": "string",
                    "description": "If outcome is partial, what part worked."
                },
                "partial_failed": {
                    "type": "string",
                    "description": "If outcome is partial, what part failed."
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Searchable tags for this episode."
                },
                "related_files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths involved in this episode."
                }
            },
            "required": ["summary", "category", "outcome"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        // Validate required fields.
        let summary = match args.get("summary").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s,
            _ => return ToolResult::fail("'summary' is required and must not be empty."),
        };

        let category_str = match args.get("category").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::fail("'category' is required."),
        };

        let valid_categories = [
            "task_completion", "error_resolution", "user_preference",
            "system_configuration", "file_modification", "web_research",
            "conversation",
        ];
        if !valid_categories.contains(&category_str) {
            return ToolResult::fail(format!(
                "Invalid category: {category_str:?}. Valid categories: {}",
                valid_categories.join(", ")
            ));
        }

        let outcome_str = match args.get("outcome").and_then(|v| v.as_str()) {
            Some(o) => o,
            None => return ToolResult::fail("'outcome' is required."),
        };

        let valid_outcomes = ["success", "failure", "partial"];
        if !valid_outcomes.contains(&outcome_str) {
            return ToolResult::fail(format!(
                "Invalid outcome: {outcome_str:?}. Valid outcomes: {}",
                valid_outcomes.join(", ")
            ));
        }

        // Invoke the callback.
        let callback = {
            let guard = self.reflect_callback.lock().unwrap();
            guard.clone()
        };

        match callback {
            Some(cb) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cb(args.clone())
                }));
                match result {
                    Ok(Ok(episode_id)) => ToolResult::ok_with_data(
                        format!("Reflection recorded: {summary}"),
                        serde_json::json!({ "episode_id": episode_id }),
                    ),
                    Ok(Err(e)) => ToolResult::fail(format!("Failed to record reflection: {e}")),
                    Err(_) => ToolResult::fail("Reflect callback panicked."),
                }
            }
            None => {
                warn!("reflect callback not registered; cannot record reflection");
                ToolResult::fail(
                    "No reflect callback registered. Cannot record reflections without \
                     episodic memory.",
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
        let tool = ReflectTool::new();
        let r = tool.execute(serde_json::json!({
            "summary": "did thing",
            "category": "task_completion",
            "outcome": "success"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("callback"));
    }

    #[test]
    fn missing_summary_returns_error() {
        let tool = ReflectTool::new();
        let r = tool.execute(serde_json::json!({
            "category": "task_completion",
            "outcome": "success"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("summary"));
    }

    #[test]
    fn empty_summary_returns_error() {
        let tool = ReflectTool::new();
        let r = tool.execute(serde_json::json!({
            "summary": "",
            "category": "task_completion",
            "outcome": "success"
        }));
        assert!(!r.success);
    }

    #[test]
    fn missing_category_returns_error() {
        let tool = ReflectTool::new();
        let r = tool.execute(serde_json::json!({
            "summary": "did thing",
            "outcome": "success"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("category"));
    }

    #[test]
    fn invalid_category_returns_error() {
        let tool = ReflectTool::new();
        let r = tool.execute(serde_json::json!({
            "summary": "did thing",
            "category": "magic",
            "outcome": "success"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("Invalid category"));
    }

    #[test]
    fn missing_outcome_returns_error() {
        let tool = ReflectTool::new();
        let r = tool.execute(serde_json::json!({
            "summary": "did thing",
            "category": "task_completion"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("outcome"));
    }

    #[test]
    fn invalid_outcome_returns_error() {
        let tool = ReflectTool::new();
        let r = tool.execute(serde_json::json!({
            "summary": "did thing",
            "category": "task_completion",
            "outcome": "maybe"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("Invalid outcome"));
    }

    #[test]
    fn callback_success() {
        let tool = ReflectTool::new();
        tool.set_reflect_callback(|_args| Ok("ep-123".to_string()));

        let r = tool.execute(serde_json::json!({
            "summary": "fixed the login bug",
            "category": "error_resolution",
            "outcome": "success",
            "details": "found null pointer",
            "lessons": ["always check for null"],
            "tags": ["bug", "login"]
        }));
        assert!(r.success);
        assert!(r.output.contains("fixed the login bug"));
        let data = r.data.unwrap();
        assert_eq!(data.get("episode_id").and_then(|v| v.as_str()), Some("ep-123"));
    }

    #[test]
    fn callback_failure() {
        let tool = ReflectTool::new();
        tool.set_reflect_callback(|_args| Err("disk full".to_string()));

        let r = tool.execute(serde_json::json!({
            "summary": "test",
            "category": "conversation",
            "outcome": "success"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("disk full"));
    }

    #[test]
    fn all_categories_accepted() {
        let tool = ReflectTool::new();
        tool.set_reflect_callback(|_args| Ok("ok".to_string()));

        let categories = [
            "task_completion", "error_resolution", "user_preference",
            "system_configuration", "file_modification", "web_research",
            "conversation",
        ];

        for cat in &categories {
            let r = tool.execute(serde_json::json!({
                "summary": "test",
                "category": cat,
                "outcome": "success"
            }));
            assert!(r.success, "category {cat} should be accepted");
        }
    }

    #[test]
    fn all_outcomes_accepted() {
        let tool = ReflectTool::new();
        tool.set_reflect_callback(|_args| Ok("ok".to_string()));

        for outcome in &["success", "failure", "partial"] {
            let r = tool.execute(serde_json::json!({
                "summary": "test",
                "category": "conversation",
                "outcome": outcome
            }));
            assert!(r.success, "outcome {outcome} should be accepted");
        }
    }
}
