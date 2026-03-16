//! Abstract LLM provider trait.
//!
//! Every concrete provider (Claude, OpenAI, etc.) implements [`LlmProvider`],
//! which translates between the internal AiOS message types and the
//! provider-specific wire format.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

use aios_core::types::{EffortLevel, LlmResponse, Message, StreamChunk, ToolSchema};

use crate::error::Result;

/// A pinned, boxed, `Send` stream of [`StreamChunk`] items.
pub type ChunkStream = Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

/// Configuration for a provider's prompt-cache support.
///
/// Returned by [`LlmProvider::cache_config`] when the provider supports
/// caching.  Contains all the timing parameters the [`LlmManager`] needs to
/// drive the keep-warm background task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// How long the cloud cache entry lives.
    #[serde(with = "humantime_serde")]
    pub ttl: Duration,
    /// How often to send keep-warm pings.  Should be slightly less than
    /// `ttl` (e.g. ttl=1h -> interval=55min).
    #[serde(with = "humantime_serde")]
    pub keep_warm_interval: Duration,
    /// The TTL string sent in the API request body (e.g. `"1h"`, `"5m"`).
    pub ttl_api_value: String,
}

/// Fingerprint of a cached system prompt + tools configuration.
///
/// Stored on disk so the cache can be re-linked on application restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheFingerprint {
    /// The full system prompt text, so it can be replayed during warmup.
    pub system_prompt: String,
    /// Serialized JSON of the tool schemas, so they can be replayed during warmup.
    pub tools_json: String,
    /// The model identifier that was active when the fingerprint was created.
    pub model: String,
    /// ISO 8601 timestamp of when the fingerprint was created.
    pub created_at: String,
    /// A hash digest of `system_prompt + tools_json` for quick comparison.
    pub hash: u64,
}

impl CacheFingerprint {
    /// Create a new fingerprint from a system prompt, tools, and model.
    pub fn new(system_prompt: &str, tools: &[ToolSchema], model: &str) -> Self {
        let tools_json = serde_json::to_string(tools).unwrap_or_default();

        let mut hasher = DefaultHasher::new();
        system_prompt.hash(&mut hasher);
        tools_json.hash(&mut hasher);
        let hash = hasher.finish();

        Self {
            system_prompt: system_prompt.to_string(),
            tools_json,
            model: model.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            hash,
        }
    }

    /// Deserialize the stored tools JSON back into a `Vec<ToolSchema>`.
    ///
    /// Returns an empty vector on parse failure.
    pub fn tools(&self) -> Vec<ToolSchema> {
        serde_json::from_str(&self.tools_json).unwrap_or_default()
    }
}

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

    // -- Effort level -------------------------------------------------------

    /// Set the effort level for the next request.
    ///
    /// Providers translate effort levels into model selection, token budgets,
    /// and optional extended-thinking parameters.
    fn set_effort(&mut self, _level: EffortLevel) {
        // Default: no-op.  Providers override this to adjust behaviour.
    }

    /// Return the current effort level.
    fn effort(&self) -> EffortLevel {
        EffortLevel::Medium
    }

    // -- Cache support (default: no-op) -------------------------------------

    /// Returns cache configuration if this provider supports prompt caching.
    ///
    /// Returns `None` if caching is not supported.  When `Some`, the
    /// [`LlmManager`] uses the returned [`CacheConfig`] to drive the
    /// keep-warm background task.
    fn cache_config(&self) -> Option<CacheConfig> {
        None
    }

    /// Send a minimal request to warm up / re-link the cloud cache.
    ///
    /// Called on startup if a fingerprint is found, and periodically by the
    /// keep-warm background task.  The default implementation is a no-op.
    async fn warmup(
        &self,
        _system_prompt: Option<&str>,
        _tools: Option<&[ToolSchema]>,
    ) -> Result<()> {
        Ok(())
    }

    /// Save the cache fingerprint to disk.
    ///
    /// The default implementation is a no-op.
    fn save_fingerprint(&self, _path: &std::path::Path) -> Result<()> {
        Ok(())
    }

    /// Load a previously-saved cache fingerprint from disk.
    ///
    /// The default implementation always returns `None`.
    fn load_fingerprint(&self, _path: &std::path::Path) -> Result<Option<CacheFingerprint>> {
        Ok(None)
    }
}

/// Serialization helpers for [`Duration`] via `humantime`.
mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}
