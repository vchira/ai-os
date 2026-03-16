//! AiOS LLM provider abstraction layer.
//!
//! This crate provides a uniform interface for communicating with LLM APIs
//! (Claude, OpenAI) and implements the tool-call loop that powers AiOS.
//!
//! # Architecture
//!
//! * [`provider::LlmProvider`] — async trait implemented by each backend.
//! * [`claude::ClaudeProvider`] — Anthropic Messages API backend.
//! * [`openai::OpenAIProvider`] — OpenAI Chat Completions API backend.
//! * [`manager::LlmManager`] — provider registry + tool-call loop.
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
//! let response = manager.chat("Hello!", None, &[], None).await.unwrap();
//! ```

pub mod claude;
pub mod error;
pub mod manager;
pub mod openai;
pub mod provider;

// Re-export the most commonly used items at the crate root.
pub use claude::ClaudeProvider;
pub use error::{LlmError, Result};
pub use manager::{LlmManager, ToolExecutor};
pub use openai::OpenAIProvider;
pub use provider::{ChunkStream, LlmProvider};
