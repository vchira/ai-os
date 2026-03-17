//! AiOS core library — shared types, configuration, and command handling.
//!
//! This crate provides the foundational types used by every other AiOS crate:
//!
//! * [`types`] — LLM messages, tool results, voice definitions
//! * [`config`] — Persistent JSON configuration and slash-command handling
//! * [`channel`] — Multi-channel communication (Desktop, Web, Signal, Voice)
//! * [`secure`] — Encrypted storage, authentication, and permission management
//! * [`error`] — Unified error type ([`AiosError`])

pub mod channel;
pub mod config;
pub mod error;
pub mod i18n;
pub mod installer;
pub mod memory;
pub mod secure;
pub mod selftest;
pub mod system_monitor;
pub mod types;

// Re-export the most commonly used items at the crate root.
pub use channel::{AppRuntime, ChannelCapabilities, ChannelContext, ChannelKind, ChannelSwitcher, IncomingMessage};
pub use error::{AiosError, Result};
pub use secure::{AuthManager, PermissionManager, Vault};
