//! Channel types — identifies where the user is interacting from.
//!
//! The AI has one conversation and one brain.  The channel determines
//! which surface is currently active (Desktop, Web, Signal, Voice).
//! Tools receive a [`ChannelContext`] so they can adapt their behaviour
//! (e.g. `ui_panel` degrades to text choices on Signal).

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ChannelKind
// ---------------------------------------------------------------------------

/// Identifies which communication channel is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    /// Local GTK4/libadwaita desktop UI.
    Desktop,
    /// Browser-based client connected via WebSocket.
    Web,
    /// Signal messenger.
    Signal,
    /// Voice-only (TTS/STT, no visual UI).
    Voice,
}

impl Default for ChannelKind {
    fn default() -> Self {
        Self::Desktop
    }
}

impl fmt::Display for ChannelKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Desktop => "desktop",
            Self::Web => "web",
            Self::Signal => "signal",
            Self::Voice => "voice",
        };
        f.write_str(label)
    }
}

impl ChannelKind {
    /// Parse a channel kind from a string (case-insensitive).
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "desktop" => Some(Self::Desktop),
            "web" => Some(Self::Web),
            "signal" => Some(Self::Signal),
            "voice" => Some(Self::Voice),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelCapabilities
// ---------------------------------------------------------------------------

/// Capability flags indicating what a channel can render.
///
/// Tools inspect these to decide their degradation strategy.
/// For example, `ui_panel` checks `rich_panels` to decide whether
/// to show a GTK dialog or fall back to numbered text choices.
#[derive(Debug, Clone)]
pub struct ChannelCapabilities {
    /// Can render rich UI panels (GTK dialogs, HTML forms).
    pub rich_panels: bool,
    /// Can display images.
    pub images: bool,
    /// Can render markdown formatting.
    pub markdown: bool,
    /// Can show popup/toast notifications.
    pub notifications: bool,
    /// Can accept multi-field structured input in one interaction.
    pub structured_input: bool,
    /// Supports masked password entry.
    pub password_input: bool,
    /// Maximum text message length (`None` = unlimited).
    pub max_text_length: Option<usize>,
}

impl ChannelCapabilities {
    /// Full-featured desktop capabilities (GTK4).
    pub fn desktop() -> Self {
        Self {
            rich_panels: true,
            images: true,
            markdown: true,
            notifications: true,
            structured_input: true,
            password_input: true,
            max_text_length: None,
        }
    }

    /// Full-featured web capabilities (browser).
    pub fn web() -> Self {
        Self {
            rich_panels: true,
            images: true,
            markdown: true,
            notifications: true,
            structured_input: true,
            password_input: true,
            max_text_length: None,
        }
    }

    /// Limited Signal messenger capabilities.
    pub fn signal() -> Self {
        Self {
            rich_panels: false,
            images: true, // Signal supports image attachments
            markdown: false,
            notifications: false,
            structured_input: false,
            password_input: false,
            max_text_length: Some(4096),
        }
    }

    /// Voice-only capabilities (no visual UI).
    pub fn voice() -> Self {
        Self {
            rich_panels: false,
            images: false,
            markdown: false,
            notifications: false,
            structured_input: false,
            password_input: false,
            max_text_length: None,
        }
    }

    /// Get capabilities for a given channel kind.
    pub fn for_kind(kind: ChannelKind) -> Self {
        match kind {
            ChannelKind::Desktop => Self::desktop(),
            ChannelKind::Web => Self::web(),
            ChannelKind::Signal => Self::signal(),
            ChannelKind::Voice => Self::voice(),
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelContext
// ---------------------------------------------------------------------------

/// Lightweight snapshot of the current channel state.
///
/// Passed to tools via [`Tool::execute_on_channel`] so they can
/// adapt their behaviour.  Cheap to clone (kind is `Copy`,
/// capabilities are behind `Arc`).
#[derive(Debug, Clone)]
pub struct ChannelContext {
    /// Which channel is active.
    pub kind: ChannelKind,
    /// What the channel can render.
    pub capabilities: Arc<ChannelCapabilities>,
}

impl ChannelContext {
    /// Create a context for a given channel kind with default capabilities.
    pub fn new(kind: ChannelKind) -> Self {
        Self {
            kind,
            capabilities: Arc::new(ChannelCapabilities::for_kind(kind)),
        }
    }

    /// Create a context with custom capabilities.
    pub fn with_capabilities(kind: ChannelKind, caps: ChannelCapabilities) -> Self {
        Self {
            kind,
            capabilities: Arc::new(caps),
        }
    }

    /// Shorthand for `ChannelContext::new(ChannelKind::Desktop)`.
    pub fn desktop() -> Self {
        Self::new(ChannelKind::Desktop)
    }

    /// Shorthand for `ChannelContext::new(ChannelKind::Web)`.
    pub fn web() -> Self {
        Self::new(ChannelKind::Web)
    }

    /// Shorthand for `ChannelContext::new(ChannelKind::Signal)`.
    pub fn signal() -> Self {
        Self::new(ChannelKind::Signal)
    }

    /// Shorthand for `ChannelContext::new(ChannelKind::Voice)`.
    pub fn voice() -> Self {
        Self::new(ChannelKind::Voice)
    }
}

impl Default for ChannelContext {
    fn default() -> Self {
        Self::desktop()
    }
}

// ---------------------------------------------------------------------------
// IncomingMessage
// ---------------------------------------------------------------------------

/// A message arriving from any channel, fed into the unified message loop.
///
/// All frontends (Desktop, Web, Signal) produce these.  The orchestrator
/// consumes them, switches the active channel, and routes them to the AI.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// Which channel the message came from.
    pub channel: ChannelKind,
    /// The text content of the message.
    pub text: String,
    /// Optional sender identifier (Signal phone number, web session id, etc.).
    /// `None` for the local desktop user.
    pub sender_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_kind_default_is_desktop() {
        assert_eq!(ChannelKind::default(), ChannelKind::Desktop);
    }

    #[test]
    fn channel_kind_display() {
        assert_eq!(ChannelKind::Desktop.to_string(), "desktop");
        assert_eq!(ChannelKind::Web.to_string(), "web");
        assert_eq!(ChannelKind::Signal.to_string(), "signal");
        assert_eq!(ChannelKind::Voice.to_string(), "voice");
    }

    #[test]
    fn channel_kind_from_str_opt() {
        assert_eq!(ChannelKind::from_str_opt("desktop"), Some(ChannelKind::Desktop));
        assert_eq!(ChannelKind::from_str_opt("WEB"), Some(ChannelKind::Web));
        assert_eq!(ChannelKind::from_str_opt("Signal"), Some(ChannelKind::Signal));
        assert_eq!(ChannelKind::from_str_opt("voice"), Some(ChannelKind::Voice));
        assert_eq!(ChannelKind::from_str_opt("telegram"), None);
    }

    #[test]
    fn channel_kind_serializes_lowercase() {
        let json = serde_json::to_string(&ChannelKind::Signal).unwrap();
        assert_eq!(json, "\"signal\"");
    }

    #[test]
    fn channel_kind_deserializes_lowercase() {
        let kind: ChannelKind = serde_json::from_str("\"web\"").unwrap();
        assert_eq!(kind, ChannelKind::Web);
    }

    #[test]
    fn channel_context_default_is_desktop() {
        let ctx = ChannelContext::default();
        assert_eq!(ctx.kind, ChannelKind::Desktop);
        assert!(ctx.capabilities.rich_panels);
    }

    #[test]
    fn signal_capabilities_are_limited() {
        let ctx = ChannelContext::signal();
        assert!(!ctx.capabilities.rich_panels);
        assert!(ctx.capabilities.images);
        assert!(!ctx.capabilities.structured_input);
        assert_eq!(ctx.capabilities.max_text_length, Some(4096));
    }

    #[test]
    fn web_capabilities_are_full() {
        let ctx = ChannelContext::web();
        assert!(ctx.capabilities.rich_panels);
        assert!(ctx.capabilities.images);
        assert!(ctx.capabilities.markdown);
        assert!(ctx.capabilities.max_text_length.is_none());
    }

    #[test]
    fn voice_capabilities_are_minimal() {
        let ctx = ChannelContext::voice();
        assert!(!ctx.capabilities.rich_panels);
        assert!(!ctx.capabilities.images);
        assert!(!ctx.capabilities.markdown);
    }

    #[test]
    fn channel_context_clone_is_cheap() {
        let ctx = ChannelContext::desktop();
        let cloned = ctx.clone();
        assert_eq!(ctx.kind, cloned.kind);
        // Arc means capabilities share the same allocation
        assert!(Arc::ptr_eq(&ctx.capabilities, &cloned.capabilities));
    }

    #[test]
    fn incoming_message_from_signal() {
        let msg = IncomingMessage {
            channel: ChannelKind::Signal,
            text: "Hello AI".to_string(),
            sender_id: Some("+1234567890".to_string()),
        };
        assert_eq!(msg.channel, ChannelKind::Signal);
        assert_eq!(msg.sender_id.as_deref(), Some("+1234567890"));
    }

    #[test]
    fn incoming_message_from_desktop() {
        let msg = IncomingMessage {
            channel: ChannelKind::Desktop,
            text: "Hello".to_string(),
            sender_id: None,
        };
        assert!(msg.sender_id.is_none());
    }
}
