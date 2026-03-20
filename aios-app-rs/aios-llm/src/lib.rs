//! AiOS LLM provider abstraction layer.
//!
//! This crate provides a uniform interface for communicating with LLM APIs
//! (Claude, OpenAI) and implements the tool-call loop that powers AiOS.
//!
//! # Architecture
//!
//! * [`provider::LlmProvider`] — async trait implemented by each backend.
//! * [`provider::CacheConfig`] — caching parameters returned by providers
//!   that support prompt caching.
//! * [`provider::CacheFingerprint`] — on-disk fingerprint for cache re-linking.
//! * [`claude::ClaudeProvider`] — Anthropic Messages API backend (with caching).
//! * [`openai::OpenAIProvider`] — OpenAI Chat Completions API backend.
//! * [`manager::LlmManager`] — provider registry + tool-call loop + cache lifecycle.
//! * [`cascade::CascadeConfig`] — cascading model routing configuration.
//! * [`cascade::CascadeRouter`] — escalation logic for cascading requests.
//! * [`semantic_cache::SemanticCache`] — keyword-based semantic response cache.
//! * [`error::LlmError`] — unified error type for all LLM operations.
//!
//! # Quick start
//!
//! ```ignore
//! use aios_llm::{LlmManager, ClaudeProvider, OpenAIProvider};
//!
//! let mut manager = LlmManager::new();
//! manager.register_provider(Box::new(ClaudeProvider::new("sk-ant-...", None, None)));
//! manager.set_active("claude").unwrap();
//!
//! // Warm up cache on boot.
//! manager.warmup_cache().await.ok();
//! let _keep_warm = manager.start_keep_warm();
//!
//! let response = manager.chat("Hello!", None, &[], None).await.unwrap();
//! ```

pub mod agents;
pub mod cascade;
pub mod claude;
pub mod context;
pub mod error;
pub mod local;
pub mod manager;
pub mod openai;
pub mod prefetch;
pub mod provider;
pub mod semantic_cache;

// Re-export the most commonly used items at the crate root.
pub use agents::{AgentOrchestrator, AgentTask, AgentType};
pub use cascade::{CascadeConfig, CascadeRouter, EscalationReason};
pub use claude::ClaudeProvider;
pub use context::ContextManager;
pub use error::{LlmError, Result};
pub use manager::{LlmManager, ToolExecutor};
pub use openai::OpenAIProvider;
pub use prefetch::Prefetcher;
pub use provider::{CacheConfig, CacheFingerprint, ChunkStream, LlmProvider};
pub use local::OllamaClient;
pub use semantic_cache::{SemanticCache, CacheStats};
