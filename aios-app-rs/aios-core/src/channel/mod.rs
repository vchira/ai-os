//! Multi-channel communication system.
//!
//! AiOS supports multiple output channels (Desktop, Web, Signal, Voice).
//! The AI has one conversation and one brain — the channel is just where
//! the conversation is happening right now.
//!
//! # Key types
//!
//! - [`ChannelKind`] — enum of available channels
//! - [`ChannelCapabilities`] — what a channel can render
//! - [`ChannelContext`] — lightweight snapshot passed to tools
//! - [`ChannelSwitcher`] — thread-safe state machine for the active channel
//! - [`IncomingMessage`] — unified message from any channel

pub mod runtime;
pub mod switcher;
pub mod traits;
pub mod types;

pub use runtime::AppRuntime;
pub use switcher::{ChannelSwitcher, OnSwitchCallback};
pub use traits::{Channel, ChannelBase};
pub use types::{ChannelCapabilities, ChannelContext, ChannelKind, IncomingMessage};
