//! Tool result and schema types for the AiOS plugin system.
//!
//! Ported from the Python `aios.tools.base` module.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ToolResult
// ---------------------------------------------------------------------------

/// Result returned by a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool executed successfully.
    pub success: bool,
    /// Human-readable output text.
    pub output: String,
    /// Optional structured data (e.g. image paths, tables) for the UI layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Error description when `success` is `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    /// Create a successful result.
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            data: None,
            error: None,
        }
    }

    /// Create a successful result with attached structured data.
    pub fn ok_with_data(output: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            success: true,
            output: output.into(),
            data: Some(data),
            error: None,
        }
    }

    /// Create a failed result.
    pub fn fail(error: impl Into<String>) -> Self {
        let error_str = error.into();
        Self {
            success: false,
            output: String::new(),
            data: None,
            error: Some(error_str),
        }
    }
}

// ---------------------------------------------------------------------------
// ToolSchema
// ---------------------------------------------------------------------------

/// Schema description of a tool, suitable for LLM function-calling APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Unique tool name (lowercase, underscores allowed).
    pub name: String,
    /// One-line human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the keyword arguments accepted by the tool.
    pub parameters: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_result_ok() {
        let r = ToolResult::ok("done");
        assert!(r.success);
        assert_eq!(r.output, "done");
        assert!(r.error.is_none());
    }

    #[test]
    fn tool_result_fail() {
        let r = ToolResult::fail("file not found");
        assert!(!r.success);
        assert!(r.output.is_empty());
        assert_eq!(r.error.as_deref(), Some("file not found"));
    }

    #[test]
    fn tool_schema_roundtrips_json() {
        let schema = ToolSchema {
            name: "memory_store".into(),
            description: "Persist a key-value pair".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "key":   { "type": "string" },
                    "value": { "type": "string" }
                },
                "required": ["key", "value"]
            }),
        };
        let json = serde_json::to_string(&schema).unwrap();
        let back: ToolSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "memory_store");
    }
}
