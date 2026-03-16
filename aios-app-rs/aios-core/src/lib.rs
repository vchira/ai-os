//! AiOS core library — shared types, configuration, and command handling.
//!
//! This crate provides the foundational types used by every other AiOS crate:
//!
//! * [`types`] — LLM messages, tool results, voice definitions
//! * [`config`] — Persistent JSON configuration and slash-command handling
//! * [`error`] — Unified error type ([`AiosError`])

pub mod config;
pub mod error;
pub mod types;

// Re-export the most commonly used items at the crate root.
pub use error::{AiosError, Result};
