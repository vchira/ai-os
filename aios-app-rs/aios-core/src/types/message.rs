//! LLM conversation types — messages, tool calls, streaming chunks.
//!
//! Ported from the Python `aios.llm.base` module.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Role
// ---------------------------------------------------------------------------

/// Role of a participant in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System-level instruction prepended to the conversation.
    System,
    /// Message from the human user.
    User,
    /// Response from the AI assistant.
    Assistant,
    /// Result of a tool invocation.
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        };
        f.write_str(label)
    }
}

// ---------------------------------------------------------------------------
// ToolCall
// ---------------------------------------------------------------------------

/// A tool invocation requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-assigned identifier for correlating tool results.
    pub id: String,
    /// Name of the tool to execute.
    pub name: String,
    /// Parsed argument dictionary for the tool.
    pub arguments: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

/// Token usage statistics for a single LLM request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens consumed by the prompt.
    pub input_tokens: u32,
    /// Tokens produced in the response.
    pub output_tokens: u32,
}

impl Usage {
    /// Total token count (input + output).
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// One of system / user / assistant / tool.
    pub role: Role,
    /// Text content of the message.  May be `None` when the assistant message
    /// consists entirely of tool calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// For tool-result messages, the id of the originating [`ToolCall`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// For assistant messages, any tool calls the model wants to make.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl Message {
    /// Create a user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(text.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    /// Create an assistant message with optional text.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Some(text.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    /// Create an assistant message that includes tool calls.
    pub fn assistant_with_tools(
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
    ) -> Self {
        Self {
            role: Role::Assistant,
            content: text,
            tool_call_id: None,
            tool_calls,
        }
    }

    /// Create a tool-result message.
    pub fn tool_result(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_call_id: Some(id.into()),
            tool_calls: Vec::new(),
        }
    }

    /// Create a system message.
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(text.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// LlmResponse
// ---------------------------------------------------------------------------

/// Complete (non-streaming) response from an LLM provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmResponse {
    /// The text portion of the response, if any.
    pub content: Option<String>,
    /// Tool invocations the model wants to make.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// Token counts for billing / diagnostics.
    #[serde(default)]
    pub usage: Usage,
}

impl LlmResponse {
    /// Whether the response contains any tool calls.
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

// ---------------------------------------------------------------------------
// StreamChunk
// ---------------------------------------------------------------------------

/// A single piece of a streaming LLM response.
///
/// Exactly one of `text` or `tool_call` is set per chunk.  When `done` is
/// `true` it signals the end of the stream.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Incremental text delta (may be an empty string for heartbeats).
    pub text: Option<String>,
    /// A completed tool-call object once the provider has finished emitting it.
    pub tool_call: Option<ToolCall>,
    /// Whether this chunk marks the end of the stream.
    pub done: bool,
    /// Final token usage, typically only present on the last chunk.
    pub usage: Option<Usage>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_serializes_lowercase() {
        let json = serde_json::to_string(&Role::Assistant).unwrap();
        assert_eq!(json, "\"assistant\"");
    }

    #[test]
    fn role_deserializes_lowercase() {
        let role: Role = serde_json::from_str("\"tool\"").unwrap();
        assert_eq!(role, Role::Tool);
    }

    #[test]
    fn usage_total_tokens() {
        let u = Usage {
            input_tokens: 100,
            output_tokens: 50,
        };
        assert_eq!(u.total_tokens(), 150);
    }

    #[test]
    fn message_user_convenience() {
        let m = Message::user("hello");
        assert_eq!(m.role, Role::User);
        assert_eq!(m.content.as_deref(), Some("hello"));
        assert!(m.tool_calls.is_empty());
    }

    #[test]
    fn message_tool_result_convenience() {
        let m = Message::tool_result("call-1", "result text");
        assert_eq!(m.role, Role::Tool);
        assert_eq!(m.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(m.content.as_deref(), Some("result text"));
    }

    #[test]
    fn llm_response_has_tool_calls() {
        let empty = LlmResponse::default();
        assert!(!empty.has_tool_calls());

        let with_tools = LlmResponse {
            tool_calls: vec![ToolCall {
                id: "1".into(),
                name: "test".into(),
                arguments: serde_json::json!({}),
            }],
            ..Default::default()
        };
        assert!(with_tools.has_tool_calls());
    }

    #[test]
    fn message_roundtrips_json() {
        let msg = Message::assistant_with_tools(
            Some("thinking...".into()),
            vec![ToolCall {
                id: "tc-1".into(),
                name: "memory_store".into(),
                arguments: serde_json::json!({"key": "name", "value": "Alice"}),
            }],
        );
        let json = serde_json::to_string(&msg).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, Role::Assistant);
        assert_eq!(back.tool_calls.len(), 1);
        assert_eq!(back.tool_calls[0].name, "memory_store");
    }
}
