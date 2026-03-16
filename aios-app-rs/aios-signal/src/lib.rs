//! Signal messenger channel for AiOS.
//!
//! Integrates with `signal-cli` to receive and send Signal messages.
//! The AI can be reached via Signal — incoming messages switch the
//! active channel, and AI responses are sent back to the sender.

pub mod listener;
pub mod panel;
pub mod sender;

pub use listener::SignalListener;
pub use sender::SignalSender;
