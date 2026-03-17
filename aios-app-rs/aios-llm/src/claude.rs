//! Claude (Anthropic) LLM provider.
//!
//! Communicates with the Anthropic Messages API using raw `reqwest` HTTP
//! calls.  Converts between the internal AiOS message types and the
//! Anthropic wire format.
//!
//! ## Prompt caching
//!
//! Anthropic supports prompt caching via `cache_control` annotations on the
//! system prompt and the last tool definition.  When the same prefix hits the
//! cloud cache, input tokens are billed at ~10% of the normal rate and
//! latency drops significantly.
//!
//! This module:
//! - Annotates the system prompt and last tool with
//!   `cache_control: {"type": "ephemeral", "ttl": "1h"}`.
//! - Sends the `anthropic-beta: prompt-caching-2024-07-31` header.
//! - Implements `warmup()` to re-link the cache on startup with a
//!   minimal `max_tokens=1` ping.
//! - Tracks the last request time so the keep-warm timer can skip
//!   unnecessary pings.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::stream;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, error, info};

use aios_core::types::{EffortLevel, LlmResponse, Message, Role, StreamChunk, ToolCall, ToolSchema, Usage};

use crate::error::{LlmError, Result};
use crate::provider::{CacheConfig, CacheFingerprint, ChunkStream, LlmProvider};

/// Base URL for the Anthropic Messages API.
const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Beta header required for prompt-caching support.
const ANTHROPIC_BETA: &str = "prompt-caching-2024-07-31";
/// Default model identifier.
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
/// Haiku model for low-effort requests.
const HAIKU_MODEL: &str = "claude-3-haiku-20240307";
/// Default maximum tokens in the response.
const DEFAULT_MAX_TOKENS: u32 = 8192;
/// Max tokens for low-effort requests.
const LOW_EFFORT_MAX_TOKENS: u32 = 2048;
/// Max tokens for high-effort requests.
const HIGH_EFFORT_MAX_TOKENS: u32 = 16384;
/// Extended thinking budget for high-effort requests.
const HIGH_EFFORT_THINKING_BUDGET: u32 = 10000;

/// Cloud cache TTL — 1 hour.
const CACHE_TTL: Duration = Duration::from_secs(3600);
/// Keep-warm interval — 55 minutes (slightly less than the 1h TTL).
const CACHE_KEEP_WARM_INTERVAL: Duration = Duration::from_secs(3300);
/// TTL value sent in the API request body.
const CACHE_TTL_API_VALUE: &str = "1h";

/// LLM provider backed by the Anthropic Claude API.
pub struct ClaudeProvider {
    api_key: String,
    model: String,
    max_tokens: u32,
    client: Client,
    /// Current effort level controlling model selection and token budget.
    effort: EffortLevel,
    /// Tracks the last time a request was sent, so the keep-warm task can
    /// decide whether a ping is necessary.
    last_request_at: Arc<Mutex<Option<Instant>>>,
    /// The most recently computed fingerprint, kept in memory so
    /// `save_fingerprint` can write it to disk without recomputing.
    current_fingerprint: Arc<Mutex<Option<CacheFingerprint>>>,
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
            effort: EffortLevel::Medium,
            last_request_at: Arc::new(Mutex::new(None)),
            current_fingerprint: Arc::new(Mutex::new(None)),
        }
    }

    /// Return the last time a request was sent, if any.
    ///
    /// Used by the keep-warm task to decide whether a ping is necessary.
    pub fn last_request_at(&self) -> Option<Instant> {
        *self.last_request_at.lock().unwrap()
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

    /// Convert internal AiOS tool schemas to Anthropic format *with*
    /// `cache_control` on the last element.
    ///
    /// Anthropic caches the request prefix up to and including the last
    /// block annotated with `cache_control`.  By placing the annotation on
    /// both the system prompt and the last tool, we maximise the cached
    /// prefix.
    fn convert_tools_with_cache(tools: &[ToolSchema]) -> Vec<Value> {
        let len = tools.len();
        tools
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut val = serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                });
                if i == len - 1 {
                    val["cache_control"] = serde_json::json!({
                        "type": "ephemeral",
                        "ttl": CACHE_TTL_API_VALUE
                    });
                }
                val
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

    /// Build the system prompt value with `cache_control` annotation.
    ///
    /// When prompt caching is active, the system field is sent as an array
    /// of content blocks rather than a plain string:
    ///
    /// ```json
    /// "system": [{
    ///     "type": "text",
    ///     "text": "You are AiOS...",
    ///     "cache_control": {"type": "ephemeral", "ttl": "1h"}
    /// }]
    /// ```
    fn build_system_value_cached(system_prompt: &str) -> Value {
        serde_json::json!([{
            "type": "text",
            "text": system_prompt,
            "cache_control": {
                "type": "ephemeral",
                "ttl": CACHE_TTL_API_VALUE
            }
        }])
    }

    /// Build the request body for the Anthropic Messages API.
    ///
    /// Always uses cache annotations (system prompt array format + cache_control
    /// on the last tool).
    fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
        stream: bool,
    ) -> Value {
        self.build_request_body_inner(messages, tools, system_prompt, stream, true)
    }

    /// Inner body builder with explicit `use_cache` toggle.
    fn build_request_body_inner(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
        stream: bool,
        use_cache: bool,
    ) -> Value {
        let effective_model = self.effective_model();
        let effective_max_tokens = self.effective_max_tokens();

        let mut body = serde_json::json!({
            "model": effective_model,
            "max_tokens": effective_max_tokens,
            "messages": Self::convert_messages(messages),
        });

        // High effort: enable extended thinking.
        if self.effort == EffortLevel::High {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": HIGH_EFFORT_THINKING_BUDGET
            });
        }

        if let Some(sp) = system_prompt {
            if use_cache {
                body["system"] = Self::build_system_value_cached(sp);
            } else {
                body["system"] = Value::String(sp.to_string());
            }
        }

        if !tools.is_empty() {
            if use_cache {
                body["tools"] = Value::Array(Self::convert_tools_with_cache(tools));
            } else {
                body["tools"] = Value::Array(Self::convert_tools(tools));
            }
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
            .header("anthropic-beta", ANTHROPIC_BETA)
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

    /// Record the current instant as the last request time.
    fn touch_last_request(&self) {
        *self.last_request_at.lock().unwrap() = Some(Instant::now());
    }

    /// Return the effective model for the current effort level.
    fn effective_model(&self) -> &str {
        match self.effort {
            EffortLevel::Low => HAIKU_MODEL,
            EffortLevel::Medium | EffortLevel::High => &self.model,
        }
    }

    /// Return the effective max_tokens for the current effort level.
    fn effective_max_tokens(&self) -> u32 {
        match self.effort {
            EffortLevel::Low => LOW_EFFORT_MAX_TOKENS,
            EffortLevel::Medium => self.max_tokens,
            EffortLevel::High => HIGH_EFFORT_MAX_TOKENS,
        }
    }

    /// Update the in-memory fingerprint from the given system prompt + tools.
    fn update_fingerprint(&self, system_prompt: Option<&str>, tools: &[ToolSchema]) {
        let fp = CacheFingerprint::new(
            system_prompt.unwrap_or(""),
            tools,
            &self.model,
        );
        *self.current_fingerprint.lock().unwrap() = Some(fp);
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

        self.touch_last_request();
        self.update_fingerprint(system_prompt, tools);

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

        self.touch_last_request();
        self.update_fingerprint(system_prompt, tools);

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

    // -- Effort level -------------------------------------------------------

    fn set_effort(&mut self, level: EffortLevel) {
        debug!("Claude effort level set to: {level}");
        self.effort = level;
    }

    fn effort(&self) -> EffortLevel {
        self.effort
    }

    // -- Cache support ------------------------------------------------------

    fn cache_config(&self) -> Option<CacheConfig> {
        Some(CacheConfig {
            ttl: CACHE_TTL,
            keep_warm_interval: CACHE_KEEP_WARM_INTERVAL,
            ttl_api_value: CACHE_TTL_API_VALUE.to_string(),
        })
    }

    async fn warmup(
        &self,
        system_prompt: Option<&str>,
        tools: Option<&[ToolSchema]>,
    ) -> Result<()> {
        if self.api_key.is_empty() {
            debug!("Claude warmup skipped — no API key configured");
            return Ok(());
        }

        let tools = tools.unwrap_or(&[]);
        let messages = &[Message::user("ping")];

        // Build a minimal request with max_tokens=1 to save cost.
        let mut body = self.build_request_body_inner(
            messages, tools, system_prompt, false, true,
        );
        body["max_tokens"] = serde_json::json!(1);

        info!(
            "Claude cache warmup: model={}, system_len={}, tools={}",
            self.model,
            system_prompt.map_or(0, |s| s.len()),
            tools.len(),
        );

        self.touch_last_request();

        // Fire the request — we only care that it succeeds, not the response content.
        let resp = self.do_request(&body).await?;
        let _body: Value = resp.json().await?;

        info!("Claude cache warmup complete");
        Ok(())
    }

    fn save_fingerprint(&self, path: &std::path::Path) -> Result<()> {
        let guard = self.current_fingerprint.lock().unwrap();
        let fp = match guard.as_ref() {
            Some(fp) => fp,
            None => {
                debug!("No fingerprint to save");
                return Ok(());
            }
        };

        // Ensure the parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(fp)?;
        std::fs::write(path, json)?;

        debug!("Saved cache fingerprint to {:?} (hash={})", path, fp.hash);
        Ok(())
    }

    fn load_fingerprint(&self, path: &std::path::Path) -> Result<Option<CacheFingerprint>> {
        if !path.exists() {
            return Ok(None);
        }

        let data = std::fs::read_to_string(path)?;
        let fp: CacheFingerprint = serde_json::from_str(&data)?;

        debug!(
            "Loaded cache fingerprint from {:?} (hash={}, model={})",
            path, fp.hash, fp.model
        );
        Ok(Some(fp))
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
    fn convert_tools_with_cache_adds_cache_control_to_last() {
        let tools = vec![
            ToolSchema {
                name: "tool_a".into(),
                description: "First tool".into(),
                parameters: serde_json::json!({}),
            },
            ToolSchema {
                name: "tool_b".into(),
                description: "Second tool".into(),
                parameters: serde_json::json!({}),
            },
        ];

        let converted = ClaudeProvider::convert_tools_with_cache(&tools);
        assert_eq!(converted.len(), 2);

        // First tool should NOT have cache_control.
        assert!(converted[0].get("cache_control").is_none());

        // Last tool SHOULD have cache_control.
        let cc = converted[1].get("cache_control").expect("missing cache_control on last tool");
        assert_eq!(cc["type"], "ephemeral");
        assert_eq!(cc["ttl"], "1h");
    }

    #[test]
    fn convert_tools_with_cache_single_tool() {
        let tools = vec![ToolSchema {
            name: "only_tool".into(),
            description: "The only tool".into(),
            parameters: serde_json::json!({}),
        }];

        let converted = ClaudeProvider::convert_tools_with_cache(&tools);
        assert_eq!(converted.len(), 1);

        let cc = converted[0].get("cache_control").expect("missing cache_control on single tool");
        assert_eq!(cc["type"], "ephemeral");
        assert_eq!(cc["ttl"], "1h");
    }

    #[test]
    fn convert_tools_with_cache_empty() {
        let converted = ClaudeProvider::convert_tools_with_cache(&[]);
        assert!(converted.is_empty());
    }

    #[test]
    fn build_system_value_cached_has_cache_control() {
        let val = ClaudeProvider::build_system_value_cached("You are AiOS.");
        let arr = val.as_array().expect("should be an array");
        assert_eq!(arr.len(), 1);

        let block = &arr[0];
        assert_eq!(block["type"], "text");
        assert_eq!(block["text"], "You are AiOS.");

        let cc = block.get("cache_control").expect("missing cache_control on system");
        assert_eq!(cc["type"], "ephemeral");
        assert_eq!(cc["ttl"], "1h");
    }

    #[test]
    fn build_request_body_uses_cached_system() {
        let provider = ClaudeProvider::new("test-key", None, None);
        let messages = vec![Message::user("hello")];
        let tools = vec![ToolSchema {
            name: "t1".into(),
            description: "a tool".into(),
            parameters: serde_json::json!({}),
        }];

        let body = provider.build_request_body(&messages, &tools, Some("sys prompt"), false);

        // System should be an array (cached format), not a plain string.
        let system = body.get("system").expect("missing system");
        assert!(system.is_array(), "system should be array for cached format");
        let arr = system.as_array().unwrap();
        assert_eq!(arr[0]["cache_control"]["type"], "ephemeral");

        // Last tool should have cache_control.
        let api_tools = body["tools"].as_array().unwrap();
        assert!(api_tools.last().unwrap().get("cache_control").is_some());
    }

    #[test]
    fn build_request_body_no_cache_mode() {
        let provider = ClaudeProvider::new("test-key", None, None);
        let messages = vec![Message::user("hello")];

        let body = provider.build_request_body_inner(
            &messages,
            &[],
            Some("sys prompt"),
            false,
            false,
        );

        // System should be a plain string (no cache).
        let system = body.get("system").expect("missing system");
        assert!(system.is_string(), "system should be plain string without cache");
    }

    #[test]
    fn cache_config_returns_some() {
        let provider = ClaudeProvider::new("", None, None);
        let config = provider.cache_config();
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(config.ttl, Duration::from_secs(3600));
        assert_eq!(config.keep_warm_interval, Duration::from_secs(3300));
        assert_eq!(config.ttl_api_value, "1h");
    }

    #[test]
    fn last_request_at_initially_none() {
        let provider = ClaudeProvider::new("", None, None);
        assert!(provider.last_request_at().is_none());
    }

    #[test]
    fn touch_last_request_sets_instant() {
        let provider = ClaudeProvider::new("", None, None);
        provider.touch_last_request();
        assert!(provider.last_request_at().is_some());
    }

    #[test]
    fn fingerprint_save_and_load_roundtrip() {
        let provider = ClaudeProvider::new("", None, None);
        let tools = vec![ToolSchema {
            name: "web_search".into(),
            description: "Search the web".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];

        // Update the internal fingerprint.
        provider.update_fingerprint(Some("You are AiOS."), &tools);

        // Save to a temp file.
        let dir = tempfile::tempdir().unwrap();
        let fp_path = dir.path().join("fingerprint.json");

        provider.save_fingerprint(&fp_path).unwrap();
        assert!(fp_path.exists());

        // Load it back.
        let loaded = provider.load_fingerprint(&fp_path).unwrap();
        let loaded = loaded.expect("should have loaded a fingerprint");
        assert_eq!(loaded.system_prompt, "You are AiOS.");
        assert_eq!(loaded.model, DEFAULT_MODEL);
        assert!(!loaded.tools_json.is_empty());

        // Verify tools round-trip.
        let restored_tools = loaded.tools();
        assert_eq!(restored_tools.len(), 1);
        assert_eq!(restored_tools[0].name, "web_search");
    }

    #[test]
    fn load_fingerprint_missing_file_returns_none() {
        let provider = ClaudeProvider::new("", None, None);
        let result = provider
            .load_fingerprint(std::path::Path::new("/tmp/nonexistent_fp_12345.json"))
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn fingerprint_hash_deterministic() {
        let tools = vec![ToolSchema {
            name: "t1".into(),
            description: "d1".into(),
            parameters: serde_json::json!({}),
        }];
        let fp1 = CacheFingerprint::new("sys", &tools, "model");
        let fp2 = CacheFingerprint::new("sys", &tools, "model");
        assert_eq!(fp1.hash, fp2.hash);
    }

    #[test]
    fn fingerprint_hash_changes_with_prompt() {
        let tools = vec![];
        let fp1 = CacheFingerprint::new("prompt A", &tools, "model");
        let fp2 = CacheFingerprint::new("prompt B", &tools, "model");
        assert_ne!(fp1.hash, fp2.hash);
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
