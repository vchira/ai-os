//! Abstract LLM provider trait.
//!
//! Every concrete provider (Claude, OpenAI, etc.) implements [`LlmProvider`],
//! which translates between the internal AiOS message types and the
//! provider-specific wire format.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use aios_core::types::{LlmResponse, Message, StreamChunk, ToolSchema};

use crate::error::Result;

/// A pinned, boxed, `Send` stream of [`StreamChunk`] items.
pub type ChunkStream = Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

/// Abstract interface that every LLM provider must implement.
///
/// The trait is object-safe (`async_trait`) so providers can be stored as
/// `Box<dyn LlmProvider>` inside the [`LlmManager`](crate::manager::LlmManager).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Short, unique identifier for this provider (e.g. `"claude"`, `"openai"`).
    fn name(&self) -> &str;

    /// Send a non-streaming request and await the full response.
    ///
    /// # Arguments
    ///
    /// * `messages` — Conversation history in internal AiOS format.
    /// * `tools` — Tool definitions available to the model.
    /// * `system_prompt` — Optional system-level instruction.
    async fn send_message(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
    ) -> Result<LlmResponse>;

    /// Stream a response, returning a stream of [`StreamChunk`] items.
    ///
    /// The final chunk has `done == true` and may include token usage info.
    async fn stream_message(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        system_prompt: Option<&str>,
    ) -> Result<ChunkStream>;

    /// Set the API key used to authenticate requests.
    fn set_api_key(&mut self, key: String);

    /// Set the model identifier used for subsequent requests.
    fn set_model(&mut self, model: String);

    /// Return the current API key (may be empty if not yet configured).
    fn api_key(&self) -> &str;

    /// Return the current model identifier.
    fn model(&self) -> &str;
}
