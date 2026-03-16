//! Claude (Anthropic) LLM provider.
//!
//! Communicates with the Anthropic Messages API using raw `reqwest` HTTP
//! calls.  Converts between the internal AiOS message types and the
//! Anthropic wire format.

use std::pin::Pin;

use async_trait::async_trait;
use futures::stream;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, error};

use aios_core::types::{LlmResponse, Message, Role, StreamChunk, ToolCall, ToolSchema, Usage};

use crate::error::{LlmError, Result};
use crate::provider::{ChunkStream, LlmProvider};

/// Base URL for the Anthropic Messages API.
const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Default model identifier.
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
/// Default maximum tokens in the response.
const DEFAULT_MAX_TOKENS: u32 = 8192;

/// LLM provider backed by the Anthropic Claude API.
pub struct ClaudeProvider {
    api_key: String,
    model: String,
    max_tokens: u32,
    client: Client,
}

impl ClaudeProvider {
    /// Create a new Claude provider.
    ///
    /// # Arguments
    ///
    /// * `api_key` — Anthropic API key (can be empty, set later via [`set_api_key`]).
    /// * `model` — Model identifier.  Pass `None` for the default (`claude-sonnet-4-20250514`).
    /// * `max_tokens` — Maximum tokens in the response.  Pass `None` for the default (8192).
    pub fn new(
        api_key: impl Into<String>,
        model: Option<String>,
        max_tokens: Option<u32>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            client: Client::new(),
        }
    }

    /// Convert internal AiOS tool schemas to Anthropic `tool_use` format.
    ///
    /// Anthropic uses `input_schema` instead of `parameters`.
    fn convert_tools(tools: &[ToolSchema]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect()
    }

    /// Convert internal [`Message`] list to the Anthropic wire format.
    ///
    /// System messages are dropped (the system prompt is sent as a separate
    /// top-level field).  Tool-result messages become `role: "user"` with a
    /// `tool_result` content block.
    fn convert_messages(messages: &[Message]) -> Vec<Value> {
        let mut out = Vec::with_capacity(messages.len());

        for msg in messages {
            match msg.role {
                Role::System => {
                    // Handled separately via the `system` field.
                    continue;
                }
                Role::Tool => {
                    // Anthropic expects tool results inside a user turn.
                    out.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": msg.tool_call_id.as_deref().unwrap_or(""),
                            "content": msg.content.as_deref().unwrap_or(""),
                        }]
                    }));
                }
                Role::Assistant if !msg.tool_calls.is_empty() => {
                    // Mixed content: optional text + tool_use blocks.
                    let mut content = Vec::new();
                    if let Some(text) = &msg.content {
                        content.push(serde_json::json!({
                            "type": "text",
                            "text": text,
                        }));
                    }
                    for tc in &msg.tool_calls {
                        content.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments,
                        }));
                    }
                    out.push(serde_json::json!({
                        "role": "assistant",
                        "content": content,
                    }));
                }
                _ => {
                    // Plain user or assistant text message.
                    out.push(serde_json::json!({
                        "role": msg.role.to_string(),
                        "content": msg.content.as_deref().unwrap_or(""),
                    }));
                }
            }
        }

        out
    }

    /// Parse the Anthropic response JSON into an [`LlmResponse`].
    fn parse_response(body: &Value) -> Result<LlmResponse> {
        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        let content = body
            .get("content")
            .and_then(|v| v.as_array())
            .ok_or_else(|| LlmError::ParseError("missing 'content' array in response".into()))?;

        for block in content {
            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        text_parts.push(text.to_string());
                    }
                }
                "tool_use" => {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = block
                        .get("input")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
                _ => {}
            }
        }

        let usage = Self::parse_usage(body.get("usage"));

        let combined_text = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        };

        Ok(LlmResponse {
            content: combined_text,
            tool_calls,
            usage,
        })
    }

    /// Extract [`Usage`] from an optional JSON value.
    fn parse_usage(usage_val: Option<&Value>) -> Usage {
        match usage_val {
            Some(u) => Usage {
                input_tokens: u
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                output_tokens: u
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            },
            None => Usage::default(),
        }
    }

    /// Safely parse tool-call arguments that may arrive as a JSON string or
    /// already-parsed value.
    fn safe_parse_arguments(raw: &str) -> Value {
        if raw.is_empty() {
            return serde_json::json!({});
        }
        serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({ "raw": raw }))
    }

    /// Build the request body for the Anthropic Messages API.
    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
        stream: bool,
    ) -> Value {
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": Self::convert_messages(messages),
        });

        if let Some(sp) = system_prompt {
            body["system"] = Value::String(sp.to_string());
        }

        if !tools.is_empty() {
            body["tools"] = Value::Array(Self::convert_tools(tools));
        }

        if stream {
            body["stream"] = Value::Bool(true);
        }

        body
    }

    /// Send the HTTP request to the Anthropic API and check for errors.
    async fn do_request(&self, body: &Value) -> Result<reqwest::Response> {
        if self.api_key.is_empty() {
            return Err(LlmError::NoApiKey("claude".into()));
        }

        let resp = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            error!("Claude API error (HTTP {}): {}", status.as_u16(), error_body);
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: error_body,
            });
        }

        Ok(resp)
    }
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    async fn send_message(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
    ) -> Result<LlmResponse> {
        let body = self.build_request_body(messages, tools, system_prompt, false);
        debug!("Claude request: model={}", self.model);

        let resp = self.do_request(&body).await?;
        let response_json: Value = resp.json().await?;

        Self::parse_response(&response_json)
    }

    async fn stream_message(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
    ) -> Result<ChunkStream> {
        let body = self.build_request_body(messages, tools, system_prompt, true);
        debug!("Claude stream request: model={}", self.model);

        let resp = self.do_request(&body).await?;

        // Process the SSE byte stream.
        let byte_stream = resp.bytes_stream();

        // We accumulate SSE lines from the byte stream and parse events.
        let event_stream = stream::unfold(
            SseState::new(byte_stream),
            |mut state| async move {
                loop {
                    // Try to consume the next complete SSE event from the buffer.
                    if let Some(event) = state.next_event() {
                        match event.event_type.as_str() {
                            "message_start" => {
                                // Extract initial usage from message_start.
                                if let Some(msg) = event.data_json.get("message") {
                                    if let Some(u) = msg.get("usage") {
                                        state.input_tokens = u
                                            .get("input_tokens")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0)
                                            as u32;
                                    }
                                }
                                continue;
                            }
                            "content_block_start" => {
                                // Check if this is a tool_use block.
                                if let Some(block) = event.data_json.get("content_block") {
                                    if block.get("type").and_then(|v| v.as_str())
                                        == Some("tool_use")
                                    {
                                        state.current_tool_id = block
                                            .get("id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        state.current_tool_name = block
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        state.current_tool_json.clear();
                                    }
                                }
                                continue;
                            }
                            "content_block_delta" => {
                                if let Some(delta) = event.data_json.get("delta") {
                                    let delta_type =
                                        delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                    match delta_type {
                                        "text_delta" => {
                                            if let Some(text) =
                                                delta.get("text").and_then(|v| v.as_str())
                                            {
                                                return Some((
                                                    Ok(StreamChunk {
                                                        text: Some(text.to_string()),
                                                        ..Default::default()
                                                    }),
                                                    state,
                                                ));
                                            }
                                        }
                                        "input_json_delta" => {
                                            if let Some(json_fragment) = delta
                                                .get("partial_json")
                                                .and_then(|v| v.as_str())
                                            {
                                                state
                                                    .current_tool_json
                                                    .push_str(json_fragment);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            "content_block_stop" => {
                                // Flush accumulated tool call, if any.
                                if !state.current_tool_id.is_empty() {
                                    let raw = if state.current_tool_json.is_empty() {
                                        "{}".to_string()
                                    } else {
                                        std::mem::take(&mut state.current_tool_json)
                                    };
                                    let args = ClaudeProvider::safe_parse_arguments(&raw);
                                    let tc = ToolCall {
                                        id: std::mem::take(&mut state.current_tool_id),
                                        name: std::mem::take(&mut state.current_tool_name),
                                        arguments: args,
                                    };
                                    return Some((
                                        Ok(StreamChunk {
                                            tool_call: Some(tc),
                                            ..Default::default()
                                        }),
                                        state,
                                    ));
                                }
                                continue;
                            }
                            "message_delta" => {
                                // Final usage info.
                                if let Some(u) = event.data_json.get("usage") {
                                    state.output_tokens = u
                                        .get("output_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u32;
                                }
                                continue;
                            }
                            "message_stop" => {
                                // End of stream — emit the done chunk.
                                return Some((
                                    Ok(StreamChunk {
                                        done: true,
                                        usage: Some(Usage {
                                            input_tokens: state.input_tokens,
                                            output_tokens: state.output_tokens,
                                        }),
                                        ..Default::default()
                                    }),
                                    state,
                                ));
                            }
                            "ping" | "error" => {
                                if event.event_type == "error" {
                                    let msg = event
                                        .data_json
                                        .get("error")
                                        .and_then(|e| e.get("message"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown SSE error");
                                    return Some((
                                        Err(LlmError::ApiError {
                                            status: 0,
                                            message: msg.to_string(),
                                        }),
                                        state,
                                    ));
                                }
                                continue;
                            }
                            _ => {
                                // Unknown event type — skip.
                                continue;
                            }
                        }
                    }

                    // Need more data from the byte stream.
                    match state.read_more().await {
                        Ok(true) => {
                            // Got more bytes — loop back to try parsing events.
                            continue;
                        }
                        Ok(false) => {
                            // Stream ended.  If we never saw message_stop, emit done.
                            if !state.done_emitted {
                                state.done_emitted = true;
                                return Some((
                                    Ok(StreamChunk {
                                        done: true,
                                        usage: Some(Usage {
                                            input_tokens: state.input_tokens,
                                            output_tokens: state.output_tokens,
                                        }),
                                        ..Default::default()
                                    }),
                                    state,
                                ));
                            }
                            return None;
                        }
                        Err(e) => {
                            return Some((Err(LlmError::NetworkError(e)), state));
                        }
                    }
                }
            },
        );

        Ok(Box::pin(event_stream))
    }

    fn set_api_key(&mut self, key: String) {
        self.api_key = key;
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }

    fn api_key(&self) -> &str {
        &self.api_key
    }

    fn model(&self) -> &str {
        &self.model
    }
}

// ---------------------------------------------------------------------------
// SSE parser state
// ---------------------------------------------------------------------------

/// A parsed SSE event.
struct SseEvent {
    event_type: String,
    data_json: Value,
}

/// Internal state machine for parsing an SSE byte stream from Anthropic.
struct SseState<S> {
    byte_stream: Pin<Box<S>>,
    buffer: String,
    /// Fully parsed events waiting to be consumed.
    pending_events: Vec<SseEvent>,
    // Streaming accumulators.
    input_tokens: u32,
    output_tokens: u32,
    current_tool_id: String,
    current_tool_name: String,
    current_tool_json: String,
    done_emitted: bool,
}

impl<S> SseState<S>
where
    S: Stream<Item = std::result::Result<::bytes::Bytes, reqwest::Error>> + Send + Unpin,
{
    fn new(byte_stream: S) -> Self {
        Self {
            byte_stream: Box::pin(byte_stream),
            buffer: String::new(),
            pending_events: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            current_tool_id: String::new(),
            current_tool_name: String::new(),
            current_tool_json: String::new(),
            done_emitted: false,
        }
    }

    /// Try to extract the next fully parsed SSE event from the buffer.
    fn next_event(&mut self) -> Option<SseEvent> {
        if !self.pending_events.is_empty() {
            return Some(self.pending_events.remove(0));
        }

        // Parse SSE lines from the buffer.
        self.parse_buffer();

        if !self.pending_events.is_empty() {
            Some(self.pending_events.remove(0))
        } else {
            None
        }
    }

    /// Parse as many complete SSE events as possible from the buffer.
    fn parse_buffer(&mut self) {
        // SSE events are separated by double newlines.
        while let Some(end) = self.buffer.find("\n\n") {
            let event_text = self.buffer[..end].to_string();
            self.buffer = self.buffer[end + 2..].to_string();

            let mut event_type = String::new();
            let mut data_parts: Vec<String> = Vec::new();

            for line in event_text.lines() {
                if let Some(stripped) = line.strip_prefix("event: ") {
                    event_type = stripped.trim().to_string();
                } else if let Some(stripped) = line.strip_prefix("data: ") {
                    data_parts.push(stripped.to_string());
                } else if line.starts_with("data:") {
                    // "data:" with no space — empty data line.
                    data_parts.push(line[5..].to_string());
                }
            }

            if event_type.is_empty() && data_parts.is_empty() {
                continue;
            }

            let data_str = data_parts.join("\n");
            let data_json = serde_json::from_str(&data_str).unwrap_or(Value::Null);

            self.pending_events.push(SseEvent {
                event_type,
                data_json,
            });
        }
    }

    /// Read more bytes from the underlying stream into the buffer.
    ///
    /// Returns `Ok(true)` if data was read, `Ok(false)` if the stream ended.
    async fn read_more(&mut self) -> std::result::Result<bool, reqwest::Error> {
        match self.byte_stream.next().await {
            Some(Ok(bytes)) => {
                // Append the raw bytes as UTF-8 (SSE is always UTF-8).
                self.buffer
                    .push_str(&String::from_utf8_lossy(&bytes));
                Ok(true)
            }
            Some(Err(e)) => Err(e),
            None => Ok(false),
        }
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
    fn convert_tools_maps_parameters_to_input_schema() {
        let tools = vec![ToolSchema {
            name: "memory_store".into(),
            description: "Store a value".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" },
                },
                "required": ["key"]
            }),
        }];

        let converted = ClaudeProvider::convert_tools(&tools);
        assert_eq!(converted.len(), 1);
        assert!(converted[0].get("input_schema").is_some());
        assert!(converted[0].get("parameters").is_none());
        assert_eq!(converted[0]["name"], "memory_store");
    }

    #[test]
    fn convert_messages_skips_system() {
        let messages = vec![
            Message::system("you are helpful"),
            Message::user("hello"),
        ];

        let converted = ClaudeProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["role"], "user");
    }

    #[test]
    fn convert_messages_tool_result() {
        let messages = vec![Message::tool_result("tc-1", "result text")];

        let converted = ClaudeProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["role"], "user");

        let content = converted[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[0]["tool_use_id"], "tc-1");
        assert_eq!(content[0]["content"], "result text");
    }

    #[test]
    fn convert_messages_assistant_with_tool_calls() {
        let messages = vec![Message::assistant_with_tools(
            Some("thinking...".into()),
            vec![ToolCall {
                id: "tc-1".into(),
                name: "memory_store".into(),
                arguments: serde_json::json!({"key": "name"}),
            }],
        )];

        let converted = ClaudeProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["role"], "assistant");

        let content = converted[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "thinking...");
        assert_eq!(content[1]["type"], "tool_use");
        assert_eq!(content[1]["id"], "tc-1");
        assert_eq!(content[1]["name"], "memory_store");
    }

    #[test]
    fn parse_response_text_only() {
        let body = serde_json::json!({
            "content": [
                { "type": "text", "text": "Hello!" }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5
            }
        });

        let resp = ClaudeProvider::parse_response(&body).unwrap();
        assert_eq!(resp.content.as_deref(), Some("Hello!"));
        assert!(resp.tool_calls.is_empty());
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
    }

    #[test]
    fn parse_response_with_tool_use() {
        let body = serde_json::json!({
            "content": [
                { "type": "text", "text": "Let me check." },
                {
                    "type": "tool_use",
                    "id": "tc-abc",
                    "name": "web_search",
                    "input": { "query": "rust lang" }
                }
            ],
            "usage": {
                "input_tokens": 20,
                "output_tokens": 15
            }
        });

        let resp = ClaudeProvider::parse_response(&body).unwrap();
        assert_eq!(resp.content.as_deref(), Some("Let me check."));
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].id, "tc-abc");
        assert_eq!(resp.tool_calls[0].name, "web_search");
        assert_eq!(resp.tool_calls[0].arguments["query"], "rust lang");
    }

    #[test]
    fn safe_parse_arguments_valid_json() {
        let result = ClaudeProvider::safe_parse_arguments(r#"{"key": "value"}"#);
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn safe_parse_arguments_invalid_json() {
        let result = ClaudeProvider::safe_parse_arguments("not json");
        assert_eq!(result["raw"], "not json");
    }

    #[test]
    fn safe_parse_arguments_empty() {
        let result = ClaudeProvider::safe_parse_arguments("");
        assert_eq!(result, serde_json::json!({}));
    }
}
