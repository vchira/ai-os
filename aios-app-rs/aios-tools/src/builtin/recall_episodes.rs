//! Recall episodes tool — search past experiences and reflections.
//!
//! The AI uses this tool to learn from previous interactions by searching
//! the episodic memory store.  It can search by keyword, filter by category,
//! and limit results.
//!
//! Like [`UiPanelTool`](super::UiPanelTool), this tool uses a callback pattern:
//! the application layer registers a callback that queries the
//! [`EpisodicMemory`](aios_core::memory::EpisodicMemory) instance.

use std::sync::{Arc, Mutex};

use aios_core::types::ToolResult;
use tracing::warn;

use crate::tool::Tool;

// ---------------------------------------------------------------------------
// Callback type
// ---------------------------------------------------------------------------

/// Callback signature for recall_episodes.
///
/// Receives the parsed query parameters as a JSON value and returns
/// `Ok(results_json)` on success or `Err(error_message)` on failure.
pub type RecallCallback =
    Arc<dyn Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync>;

// ---------------------------------------------------------------------------
// RecallEpisodesTool
// ---------------------------------------------------------------------------

/// Search past experiences and reflections.
///
/// The callback is set by the application layer at startup via
/// [`set_recall_callback`](RecallEpisodesTool::set_recall_callback).
pub struct RecallEpisodesTool {
    recall_callback: Arc<Mutex<Option<RecallCallback>>>,
}

impl RecallEpisodesTool {
    /// Create a new `RecallEpisodesTool` with no callback registered.
    pub fn new() -> Self {
        Self {
            recall_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Register the callback that queries episodic memory.
    ///
    /// The callback receives the tool arguments as a JSON value containing
    /// optional `query`, `category`, and `limit` fields.  It should return
    /// a JSON value containing the matching episodes.
    pub fn set_recall_callback(
        &self,
        cb: impl Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync + 'static,
    ) {
        let mut guard = self.recall_callback.lock().unwrap();
        *guard = Some(Arc::new(cb));
    }
}

impl Default for RecallEpisodesTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for RecallEpisodesTool {
    fn name(&self) -> &str {
        "recall_episodes"
    }

    fn description(&self) -> &str {
        "Search past experiences and reflections. Use this to learn from \
         previous interactions, find what worked or failed before, and recall \
         lessons learned."
    }

    fn category(&self) -> &str {
        "memory"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to match against episode summaries, details, tags, and lessons."
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
                    "description": "Filter by episode category (optional)."
                },
                "outcome": {
                    "type": "string",
                    "enum": ["success", "failure", "partial"],
                    "description": "Filter by outcome type (optional)."
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of results to return (default 5)."
                }
            }
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        // At least one of query/category/outcome should be provided, or we
        // return recent episodes.
        let has_query = args.get("query").and_then(|v| v.as_str()).is_some();
        let has_category = args.get("category").and_then(|v| v.as_str()).is_some();
        let has_outcome = args.get("outcome").and_then(|v| v.as_str()).is_some();

        if !has_query && !has_category && !has_outcome {
            // No filter — return recent episodes.
        }

        // Validate category if provided.
        if let Some(cat) = args.get("category").and_then(|v| v.as_str()) {
            let valid = [
                "task_completion", "error_resolution", "user_preference",
                "system_configuration", "file_modification", "web_research",
                "conversation",
            ];
            if !valid.contains(&cat) {
                return ToolResult::fail(format!(
                    "Invalid category: {cat:?}. Valid categories: {}",
                    valid.join(", ")
                ));
            }
        }

        // Validate outcome if provided.
        if let Some(outcome) = args.get("outcome").and_then(|v| v.as_str()) {
            let valid = ["success", "failure", "partial"];
            if !valid.contains(&outcome) {
                return ToolResult::fail(format!(
                    "Invalid outcome: {outcome:?}. Valid outcomes: {}",
                    valid.join(", ")
                ));
            }
        }

        // Invoke the callback.
        let callback = {
            let guard = self.recall_callback.lock().unwrap();
            guard.clone()
        };

        match callback {
            Some(cb) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cb(args.clone())
                }));
                match result {
                    Ok(Ok(episodes)) => {
                        let count = episodes
                            .as_array()
                            .map(|a| a.len())
                            .unwrap_or(0);
                        let output = if count == 0 {
                            "No matching episodes found.".to_string()
                        } else {
                            serde_json::to_string_pretty(&episodes)
                                .unwrap_or_else(|_| format!("{count} episodes found"))
                        };
                        ToolResult::ok_with_data(output, episodes)
                    }
                    Ok(Err(e)) => ToolResult::fail(format!("Failed to recall episodes: {e}")),
                    Err(_) => ToolResult::fail("Recall callback panicked."),
                }
            }
            None => {
                warn!("recall_episodes callback not registered; cannot recall");
                ToolResult::fail(
                    "No recall callback registered. Cannot search episodes without \
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
        let tool = RecallEpisodesTool::new();
        let r = tool.execute(serde_json::json!({
            "query": "login"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("callback"));
    }

    #[test]
    fn invalid_category_returns_error() {
        let tool = RecallEpisodesTool::new();
        let r = tool.execute(serde_json::json!({
            "category": "magic"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("Invalid category"));
    }

    #[test]
    fn invalid_outcome_returns_error() {
        let tool = RecallEpisodesTool::new();
        let r = tool.execute(serde_json::json!({
            "outcome": "maybe"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("Invalid outcome"));
    }

    #[test]
    fn callback_returns_results() {
        let tool = RecallEpisodesTool::new();
        tool.set_recall_callback(|_args| {
            Ok(serde_json::json!([
                {
                    "summary": "fixed login bug",
                    "category": "error_resolution",
                    "outcome": "success"
                }
            ]))
        });

        let r = tool.execute(serde_json::json!({
            "query": "login"
        }));
        assert!(r.success);
        assert!(r.output.contains("fixed login bug"));
        let data = r.data.unwrap();
        assert_eq!(data.as_array().unwrap().len(), 1);
    }

    #[test]
    fn callback_returns_empty() {
        let tool = RecallEpisodesTool::new();
        tool.set_recall_callback(|_args| Ok(serde_json::json!([])));

        let r = tool.execute(serde_json::json!({
            "query": "nonexistent"
        }));
        assert!(r.success);
        assert!(r.output.contains("No matching episodes"));
    }

    #[test]
    fn callback_error() {
        let tool = RecallEpisodesTool::new();
        tool.set_recall_callback(|_args| Err("disk error".to_string()));

        let r = tool.execute(serde_json::json!({
            "query": "test"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("disk error"));
    }

    #[test]
    fn no_filters_still_works() {
        let tool = RecallEpisodesTool::new();
        tool.set_recall_callback(|_args| Ok(serde_json::json!([])));

        let r = tool.execute(serde_json::json!({}));
        assert!(r.success);
    }

    #[test]
    fn valid_category_filter() {
        let tool = RecallEpisodesTool::new();
        tool.set_recall_callback(|args| {
            let cat = args.get("category").and_then(|v| v.as_str()).unwrap_or("");
            Ok(serde_json::json!([{ "category": cat }]))
        });

        let r = tool.execute(serde_json::json!({
            "category": "error_resolution"
        }));
        assert!(r.success);
    }

    #[test]
    fn valid_outcome_filter() {
        let tool = RecallEpisodesTool::new();
        tool.set_recall_callback(|args| {
            let outcome = args.get("outcome").and_then(|v| v.as_str()).unwrap_or("");
            Ok(serde_json::json!([{ "outcome": outcome }]))
        });

        let r = tool.execute(serde_json::json!({
            "outcome": "failure"
        }));
        assert!(r.success);
    }
}
