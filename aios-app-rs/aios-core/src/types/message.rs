//! LLM conversation types — messages, tool calls, streaming chunks.
//!
//! Ported from the Python `aios.llm.base` module.

use std::fmt;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EffortLevel
// ---------------------------------------------------------------------------

/// Controls how much computational effort the AI puts into processing a request.
///
/// Each level adjusts the underlying model, token budget, and optional
/// extended-thinking features to trade off latency vs. quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffortLevel {
    /// Fast response, minimal thinking. For simple queries, greetings,
    /// file listings, and other lightweight tasks.
    Low,
    /// Standard balanced mode. Suitable for most interactions.
    Medium,
    /// Extended thinking, self-checking. For complex reasoning, security
    /// audits, refactoring, and destructive operations.
    High,
}

impl Default for EffortLevel {
    fn default() -> Self {
        Self::Medium
    }
}

impl fmt::Display for EffortLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        };
        f.write_str(label)
    }
}

impl EffortLevel {
    /// Parse an effort level from a string (case-insensitive).
    ///
    /// Returns `None` for unrecognised strings.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// QualityMode
// ---------------------------------------------------------------------------

/// Controls the cost-vs-quality tradeoff for model routing.
///
/// Unlike [`EffortLevel`] (which maps to a specific model tier),
/// `QualityMode` determines the *strategy* for how effort escalation
/// is handled:
///
/// - **Saver** — Start with the cheapest model and escalate aggressively
///   based on heuristic quality checks.  Saves money but may give worse
///   results.
/// - **Balanced** — Use auto-detected effort and only escalate on
///   reliable signals (empty response, tool errors, explicit user retry).
/// - **Thorough** — Always use the most capable model with extended
///   thinking enabled.  No cascading needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QualityMode {
    /// Maximum cost savings.  Cheapest model first, heuristic escalation.
    Saver,
    /// Good results with reasonable cost.  Auto-detected effort, reliable
    /// escalation only.
    Balanced,
    /// Best possible results.  Always uses the most capable model.
    Thorough,
}

impl Default for QualityMode {
    fn default() -> Self {
        Self::Balanced
    }
}

impl fmt::Display for QualityMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Saver => "saver",
            Self::Balanced => "balanced",
            Self::Thorough => "thorough",
        };
        f.write_str(label)
    }
}

impl QualityMode {
    /// Parse a quality mode from a string (case-insensitive).
    ///
    /// Returns `None` for unrecognised strings.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "saver" => Some(Self::Saver),
            "balanced" => Some(Self::Balanced),
            "thorough" => Some(Self::Thorough),
            _ => None,
        }
    }
}

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
    fn effort_level_default_is_medium() {
        assert_eq!(EffortLevel::default(), EffortLevel::Medium);
    }

    #[test]
    fn effort_level_display() {
        assert_eq!(EffortLevel::Low.to_string(), "low");
        assert_eq!(EffortLevel::Medium.to_string(), "medium");
        assert_eq!(EffortLevel::High.to_string(), "high");
    }

    #[test]
    fn effort_level_from_str_opt() {
        assert_eq!(EffortLevel::from_str_opt("low"), Some(EffortLevel::Low));
        assert_eq!(EffortLevel::from_str_opt("MEDIUM"), Some(EffortLevel::Medium));
        assert_eq!(EffortLevel::from_str_opt("High"), Some(EffortLevel::High));
        assert_eq!(EffortLevel::from_str_opt("unknown"), None);
    }

    #[test]
    fn effort_level_serializes_lowercase() {
        let json = serde_json::to_string(&EffortLevel::High).unwrap();
        assert_eq!(json, "\"high\"");
    }

    #[test]
    fn effort_level_deserializes_lowercase() {
        let level: EffortLevel = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(level, EffortLevel::Low);
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

    // -- QualityMode tests ----------------------------------------------------

    #[test]
    fn quality_mode_default_is_balanced() {
        assert_eq!(QualityMode::default(), QualityMode::Balanced);
    }

    #[test]
    fn quality_mode_display() {
        assert_eq!(QualityMode::Saver.to_string(), "saver");
        assert_eq!(QualityMode::Balanced.to_string(), "balanced");
        assert_eq!(QualityMode::Thorough.to_string(), "thorough");
    }

    #[test]
    fn quality_mode_from_str_opt() {
        assert_eq!(QualityMode::from_str_opt("saver"), Some(QualityMode::Saver));
        assert_eq!(QualityMode::from_str_opt("BALANCED"), Some(QualityMode::Balanced));
        assert_eq!(QualityMode::from_str_opt("Thorough"), Some(QualityMode::Thorough));
        assert_eq!(QualityMode::from_str_opt("unknown"), None);
    }

    #[test]
    fn quality_mode_serializes_lowercase() {
        let json = serde_json::to_string(&QualityMode::Thorough).unwrap();
        assert_eq!(json, "\"thorough\"");
    }

    #[test]
    fn quality_mode_deserializes_lowercase() {
        let mode: QualityMode = serde_json::from_str("\"saver\"").unwrap();
        assert_eq!(mode, QualityMode::Saver);
    }
}
