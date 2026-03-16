//! OpenAI LLM provider.
//!
//! Communicates with the OpenAI Chat Completions API using raw `reqwest`
//! HTTP calls.  Converts between the internal AiOS message types and the
//! OpenAI wire format.

use std::collections::HashMap;
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

/// Base URL for the OpenAI Chat Completions API.
const API_URL: &str = "https://api.openai.com/v1/chat/completions";
/// Default model identifier.
const DEFAULT_MODEL: &str = "gpt-4o";
/// Default maximum tokens in the response.
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// LLM provider backed by the OpenAI Chat Completions API.
pub struct OpenAIProvider {
    api_key: String,
    model: String,
    max_tokens: u32,
    client: Client,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider.
    ///
    /// # Arguments
    ///
    /// * `api_key` — OpenAI API key (can be empty, set later via [`set_api_key`]).
    /// * `model` — Model identifier.  Pass `None` for the default (`gpt-4o`).
    /// * `max_tokens` — Maximum tokens in the response.  Pass `None` for the default (4096).
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

    /// Convert internal AiOS tool schemas to OpenAI function-calling format.
    ///
    /// OpenAI wraps each tool in `{ "type": "function", "function": { ... } }`.
    fn convert_tools(tools: &[ToolSchema]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect()
    }

    /// Convert internal [`Message`] list to the OpenAI wire format.
    ///
    /// System messages become `role: "system"`, tool results become
    /// `role: "tool"` with `tool_call_id`, and assistant messages with
    /// tool calls include a `tool_calls` array.
    fn convert_messages(messages: &[Message]) -> Vec<Value> {
        let mut out = Vec::with_capacity(messages.len());

        for msg in messages {
            match msg.role {
                Role::System => {
                    out.push(serde_json::json!({
                        "role": "system",
                        "content": msg.content.as_deref().unwrap_or(""),
                    }));
                }
                Role::Tool => {
                    out.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": msg.tool_call_id.as_deref().unwrap_or(""),
                        "content": msg.content.as_deref().unwrap_or(""),
                    }));
                }
                Role::Assistant if !msg.tool_calls.is_empty() => {
                    // Build the assistant message with tool_calls array.
                    let api_tool_calls: Vec<Value> = msg
                        .tool_calls
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string(),
                                }
                            })
                        })
                        .collect();

                    let mut entry = serde_json::json!({
                        "role": "assistant",
                        "tool_calls": api_tool_calls,
                    });

                    if let Some(text) = &msg.content {
                        entry["content"] = Value::String(text.clone());
                    }

                    out.push(entry);
                }
                _ => {
                    // Plain user or assistant text.
                    out.push(serde_json::json!({
                        "role": msg.role.to_string(),
                        "content": msg.content.as_deref().unwrap_or(""),
                    }));
                }
            }
        }

        out
    }

    /// Parse the OpenAI response JSON into an [`LlmResponse`].
    fn parse_response(body: &Value) -> Result<LlmResponse> {
        let choices = body
            .get("choices")
            .and_then(|v| v.as_array())
            .ok_or_else(|| LlmError::ParseError("missing 'choices' array in response".into()))?;

        let message = choices
            .first()
            .and_then(|c| c.get("message"))
            .ok_or_else(|| {
                LlmError::ParseError("missing 'choices[0].message' in response".into())
            })?;

        let content = message
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut tool_calls: Vec<ToolCall> = Vec::new();
        if let Some(tcs) = message.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tcs {
                let id = tc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let function = tc.get("function");
                let name = function
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let raw_args = function
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");
                let arguments = Self::safe_parse_arguments(raw_args);

                tool_calls.push(ToolCall {
                    id,
                    name,
                    arguments,
                });
            }
        }

        let usage = Self::parse_usage(body.get("usage"));

        Ok(LlmResponse {
            content,
            tool_calls,
            usage,
        })
    }

    /// Extract [`Usage`] from an optional JSON value.
    fn parse_usage(usage_val: Option<&Value>) -> Usage {
        match usage_val {
            Some(u) => Usage {
                input_tokens: u
                    .get("prompt_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                output_tokens: u
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            },
            None => Usage::default(),
        }
    }

    /// Safely parse tool-call arguments that may arrive as a JSON string.
    fn safe_parse_arguments(raw: &str) -> Value {
        if raw.is_empty() {
            return serde_json::json!({});
        }
        serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({ "raw": raw }))
    }

    /// Build the request body for the OpenAI Chat Completions API.
    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
        stream: bool,
    ) -> Value {
        // Prepend system prompt as the first message if provided.
        let mut api_messages = Self::convert_messages(messages);
        if let Some(sp) = system_prompt {
            api_messages.insert(
                0,
                serde_json::json!({
                    "role": "system",
                    "content": sp,
                }),
            );
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": api_messages,
        });

        if !tools.is_empty() {
            body["tools"] = Value::Array(Self::convert_tools(tools));
        }

        if stream {
            body["stream"] = Value::Bool(true);
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }

        body
    }

    /// Send the HTTP request to the OpenAI API and check for errors.
    async fn do_request(&self, body: &Value) -> Result<reqwest::Response> {
        if self.api_key.is_empty() {
            return Err(LlmError::NoApiKey("openai".into()));
        }

        let resp = self
            .client
            .post(API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            error!(
                "OpenAI API error (HTTP {}): {}",
                status.as_u16(),
                error_body
            );
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: error_body,
            });
        }

        Ok(resp)
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn send_message(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
    ) -> Result<LlmResponse> {
        let body = self.build_request_body(messages, tools, system_prompt, false);
        debug!("OpenAI request: model={}", self.model);

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
        debug!("OpenAI stream request: model={}", self.model);

        let resp = self.do_request(&body).await?;

        // Process the SSE byte stream.
        let byte_stream = resp.bytes_stream();

        let event_stream = stream::unfold(
            OpenAISseState::new(byte_stream),
            |mut state| async move {
                loop {
                    // Try to get the next SSE event from the buffer.
                    if let Some(data_str) = state.next_data_line() {
                        if data_str == "[DONE]" {
                            // Stream finished — emit the final done chunk.
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

                        // Parse the chunk JSON.
                        let chunk_json: Value = match serde_json::from_str(&data_str) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        // Extract usage from the final chunk (OpenAI includes it when
                        // stream_options.include_usage is set).
                        if let Some(u) = chunk_json.get("usage") {
                            if !u.is_null() {
                                state.input_tokens = u
                                    .get("prompt_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0) as u32;
                                state.output_tokens = u
                                    .get("completion_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0) as u32;
                            }
                        }

                        // Process choices.
                        let choices = match chunk_json.get("choices").and_then(|v| v.as_array()) {
                            Some(c) => c,
                            None => continue,
                        };

                        if choices.is_empty() {
                            continue;
                        }

                        let choice = &choices[0];
                        let delta = match choice.get("delta") {
                            Some(d) => d,
                            None => continue,
                        };
                        let finish_reason = choice
                            .get("finish_reason")
                            .and_then(|v| v.as_str());

                        // Text delta.
                        if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                return Some((
                                    Ok(StreamChunk {
                                        text: Some(text.to_string()),
                                        ..Default::default()
                                    }),
                                    state,
                                ));
                            }
                        }

                        // Tool-call deltas.
                        if let Some(tool_calls) =
                            delta.get("tool_calls").and_then(|v| v.as_array())
                        {
                            for tc_delta in tool_calls {
                                let idx = tc_delta
                                    .get("index")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0) as usize;

                                let entry = state
                                    .pending_calls
                                    .entry(idx)
                                    .or_insert_with(PendingToolCall::default);

                                if let Some(id) =
                                    tc_delta.get("id").and_then(|v| v.as_str())
                                {
                                    entry.id = id.to_string();
                                }

                                if let Some(function) = tc_delta.get("function") {
                                    if let Some(name) =
                                        function.get("name").and_then(|v| v.as_str())
                                    {
                                        entry.name = name.to_string();
                                    }
                                    if let Some(args) =
                                        function.get("arguments").and_then(|v| v.as_str())
                                    {
                                        entry.arguments_parts.push(args.to_string());
                                    }
                                }
                            }
                        }

                        // Finish: flush accumulated tool calls.
                        if finish_reason.is_some() && !state.pending_calls.is_empty() {
                            let mut indices: Vec<usize> =
                                state.pending_calls.keys().copied().collect();
                            indices.sort();

                            // We yield them one at a time by storing remaining
                            // in the pending queue and returning the first.
                            if let Some(&first_idx) = indices.first() {
                                let entry = state.pending_calls.remove(&first_idx).unwrap();
                                let raw_args = if entry.arguments_parts.is_empty() {
                                    "{}".to_string()
                                } else {
                                    entry.arguments_parts.join("")
                                };
                                let args = OpenAIProvider::safe_parse_arguments(&raw_args);

                                // Store remaining calls for subsequent iterations.
                                // They'll be flushed via state.flush_queue.
                                for &idx in &indices[1..] {
                                    if let Some(remaining) = state.pending_calls.remove(&idx) {
                                        state.flush_queue.push(remaining);
                                    }
                                }

                                return Some((
                                    Ok(StreamChunk {
                                        tool_call: Some(ToolCall {
                                            id: entry.id,
                                            name: entry.name,
                                            arguments: args,
                                        }),
                                        ..Default::default()
                                    }),
                                    state,
                                ));
                            }
                        }

                        continue;
                    }

                    // Flush any remaining tool calls queued from a previous finish.
                    if let Some(entry) = state.flush_queue.pop() {
                        let raw_args = if entry.arguments_parts.is_empty() {
                            "{}".to_string()
                        } else {
                            entry.arguments_parts.join("")
                        };
                        let args = OpenAIProvider::safe_parse_arguments(&raw_args);
                        return Some((
                            Ok(StreamChunk {
                                tool_call: Some(ToolCall {
                                    id: entry.id,
                                    name: entry.name,
                                    arguments: args,
                                }),
                                ..Default::default()
                            }),
                            state,
                        ));
                    }

                    // Need more data from the byte stream.
                    match state.read_more().await {
                        Ok(true) => continue,
                        Ok(false) => {
                            // Stream ended.
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
// SSE parser state for OpenAI
// ---------------------------------------------------------------------------

/// Accumulated state for an in-progress tool call from streamed deltas.
#[derive(Default, Clone)]
struct PendingToolCall {
    id: String,
    name: String,
    arguments_parts: Vec<String>,
}

/// Internal state machine for parsing an OpenAI SSE byte stream.
struct OpenAISseState<S> {
    byte_stream: Pin<Box<S>>,
    buffer: String,
    pending_data: Vec<String>,
    // Tool-call accumulators keyed by delta index.
    pending_calls: HashMap<usize, PendingToolCall>,
    // Tool calls queued for flushing after a finish event.
    flush_queue: Vec<PendingToolCall>,
    // Token accumulators.
    input_tokens: u32,
    output_tokens: u32,
    done_emitted: bool,
}

impl<S> OpenAISseState<S>
where
    S: Stream<Item = std::result::Result<::bytes::Bytes, reqwest::Error>> + Send + Unpin,
{
    fn new(byte_stream: S) -> Self {
        Self {
            byte_stream: Box::pin(byte_stream),
            buffer: String::new(),
            pending_data: Vec::new(),
            pending_calls: HashMap::new(),
            flush_queue: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            done_emitted: false,
        }
    }

    /// Try to extract the next data line from the SSE buffer.
    ///
    /// OpenAI sends each chunk as:
    /// ```text
    /// data: { ... JSON ... }
    /// ```
    /// followed by a blank line.  The final message is `data: [DONE]`.
    fn next_data_line(&mut self) -> Option<String> {
        if !self.pending_data.is_empty() {
            return Some(self.pending_data.remove(0));
        }

        self.parse_buffer();

        if !self.pending_data.is_empty() {
            Some(self.pending_data.remove(0))
        } else {
            None
        }
    }

    /// Parse as many complete SSE data lines as possible from the buffer.
    fn parse_buffer(&mut self) {
        // Look for complete lines (terminated by \n).
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].trim_end_matches('\r').to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();

            if let Some(stripped) = line.strip_prefix("data: ") {
                self.pending_data.push(stripped.to_string());
            } else if line.starts_with("data:") {
                self.pending_data.push(line[5..].trim_start().to_string());
            }
            // Skip blank lines, event: lines, etc.
        }
    }

    /// Read more bytes from the underlying stream.
    ///
    /// Returns `Ok(true)` if data was read, `Ok(false)` if the stream ended.
    async fn read_more(&mut self) -> std::result::Result<bool, reqwest::Error> {
        match self.byte_stream.next().await {
            Some(Ok(bytes)) => {
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
    fn convert_tools_wraps_in_function() {
        let tools = vec![ToolSchema {
            name: "web_search".into(),
            description: "Search the web".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                },
                "required": ["query"]
            }),
        }];

        let converted = OpenAIProvider::convert_tools(&tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["type"], "function");
        assert_eq!(converted[0]["function"]["name"], "web_search");
        assert!(converted[0]["function"]["parameters"].is_object());
    }

    #[test]
    fn convert_messages_includes_system() {
        let messages = vec![
            Message::system("you are helpful"),
            Message::user("hello"),
        ];

        let converted = OpenAIProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0]["role"], "system");
        assert_eq!(converted[1]["role"], "user");
    }

    #[test]
    fn convert_messages_tool_result() {
        let messages = vec![Message::tool_result("tc-1", "done")];

        let converted = OpenAIProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["role"], "tool");
        assert_eq!(converted[0]["tool_call_id"], "tc-1");
        assert_eq!(converted[0]["content"], "done");
    }

    #[test]
    fn convert_messages_assistant_with_tool_calls() {
        let messages = vec![Message::assistant_with_tools(
            Some("thinking...".into()),
            vec![ToolCall {
                id: "tc-1".into(),
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "rust"}),
            }],
        )];

        let converted = OpenAIProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["role"], "assistant");
        assert_eq!(converted[0]["content"], "thinking...");

        let tool_calls = converted[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "tc-1");
        assert_eq!(tool_calls[0]["type"], "function");
        assert_eq!(tool_calls[0]["function"]["name"], "web_search");
    }

    #[test]
    fn parse_response_text_only() {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello!",
                    "tool_calls": null
                }
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5
            }
        });

        let resp = OpenAIProvider::parse_response(&body).unwrap();
        assert_eq!(resp.content.as_deref(), Some("Hello!"));
        assert!(resp.tool_calls.is_empty());
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
    }

    #[test]
    fn parse_response_with_tool_calls() {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "tc-abc",
                        "type": "function",
                        "function": {
                            "name": "web_search",
                            "arguments": "{\"query\": \"rust lang\"}"
                        }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 15
            }
        });

        let resp = OpenAIProvider::parse_response(&body).unwrap();
        assert!(resp.content.is_none());
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].id, "tc-abc");
        assert_eq!(resp.tool_calls[0].name, "web_search");
        assert_eq!(resp.tool_calls[0].arguments["query"], "rust lang");
    }

    #[test]
    fn safe_parse_arguments_valid_json() {
        let result = OpenAIProvider::safe_parse_arguments(r#"{"key": "value"}"#);
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn safe_parse_arguments_invalid_json() {
        let result = OpenAIProvider::safe_parse_arguments("not json");
        assert_eq!(result["raw"], "not json");
    }

    #[test]
    fn safe_parse_arguments_empty() {
        let result = OpenAIProvider::safe_parse_arguments("");
        assert_eq!(result, serde_json::json!({}));
    }
}
