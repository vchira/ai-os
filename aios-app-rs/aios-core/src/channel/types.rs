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
    /// Internal system messages (boot, setup, status). Not a user-facing channel.
    System,
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
            Self::System => "system",
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
            "system" => Some(Self::System),
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

    /// Full-featured web capabilities (browser) — same as desktop.
    pub fn web() -> Self {
        Self::desktop()
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
            ChannelKind::System => Self::voice(), // System has no rendering surface
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

    // -- Additional edge-case tests --

    #[test]
    fn channel_kind_from_str_opt_all_four_variants() {
        // Verify every variant round-trips through from_str_opt.
        let cases = [
            ("desktop", ChannelKind::Desktop),
            ("web", ChannelKind::Web),
            ("signal", ChannelKind::Signal),
            ("voice", ChannelKind::Voice),
        ];
        for (input, expected) in &cases {
            assert_eq!(
                ChannelKind::from_str_opt(input),
                Some(*expected),
                "from_str_opt(\"{input}\") failed"
            );
        }
    }

    #[test]
    fn channel_kind_from_str_opt_case_insensitive_mixed() {
        assert_eq!(ChannelKind::from_str_opt("DeSKtoP"), Some(ChannelKind::Desktop));
        assert_eq!(ChannelKind::from_str_opt("wEb"), Some(ChannelKind::Web));
        assert_eq!(ChannelKind::from_str_opt("SIGNAL"), Some(ChannelKind::Signal));
        assert_eq!(ChannelKind::from_str_opt("VoIcE"), Some(ChannelKind::Voice));
    }

    #[test]
    fn channel_kind_from_str_opt_rejects_invalid_strings() {
        assert!(ChannelKind::from_str_opt("telegram").is_none());
        assert!(ChannelKind::from_str_opt("").is_none());
        assert!(ChannelKind::from_str_opt("  desktop  ").is_none()); // whitespace
        assert!(ChannelKind::from_str_opt("DESKTOP ").is_none());
    }

    #[test]
    fn capabilities_for_kind_desktop() {
        let caps = ChannelCapabilities::for_kind(ChannelKind::Desktop);
        assert!(caps.rich_panels);
        assert!(caps.images);
        assert!(caps.markdown);
        assert!(caps.notifications);
        assert!(caps.structured_input);
        assert!(caps.password_input);
        assert!(caps.max_text_length.is_none());
    }

    #[test]
    fn capabilities_for_kind_web() {
        let caps = ChannelCapabilities::for_kind(ChannelKind::Web);
        assert!(caps.rich_panels);
        assert!(caps.images);
        assert!(caps.markdown);
        assert!(caps.notifications);
        assert!(caps.structured_input);
        assert!(caps.password_input);
        assert!(caps.max_text_length.is_none());
    }

    #[test]
    fn capabilities_for_kind_signal() {
        let caps = ChannelCapabilities::for_kind(ChannelKind::Signal);
        assert!(!caps.rich_panels);
        assert!(caps.images);
        assert!(!caps.markdown);
        assert!(!caps.notifications);
        assert!(!caps.structured_input);
        assert!(!caps.password_input);
        assert_eq!(caps.max_text_length, Some(4096));
    }

    #[test]
    fn capabilities_for_kind_voice() {
        let caps = ChannelCapabilities::for_kind(ChannelKind::Voice);
        assert!(!caps.rich_panels);
        assert!(!caps.images);
        assert!(!caps.markdown);
        assert!(!caps.notifications);
        assert!(!caps.structured_input);
        assert!(!caps.password_input);
        assert!(caps.max_text_length.is_none());
    }

    #[test]
    fn channel_context_with_custom_capabilities() {
        let custom_caps = ChannelCapabilities {
            rich_panels: false,
            images: true,
            markdown: false,
            notifications: true,
            structured_input: false,
            password_input: false,
            max_text_length: Some(1000),
        };
        let ctx = ChannelContext::with_capabilities(ChannelKind::Signal, custom_caps);
        assert_eq!(ctx.kind, ChannelKind::Signal);
        // Custom caps override defaults.
        assert!(!ctx.capabilities.rich_panels);
        assert!(ctx.capabilities.images);
        assert!(ctx.capabilities.notifications); // overridden to true
        assert_eq!(ctx.capabilities.max_text_length, Some(1000));
    }

    #[test]
    fn incoming_message_is_not_serialize() {
        // IncomingMessage is Clone + Debug but NOT Serialize.
        // We verify this by constructing it and checking Debug output.
        let msg = IncomingMessage {
            channel: ChannelKind::Web,
            text: "test".to_string(),
            sender_id: Some("session-42".to_string()),
        };
        let debug = format!("{msg:?}");
        assert!(debug.contains("IncomingMessage"));
        assert!(debug.contains("Web"));
        assert!(debug.contains("session-42"));
    }

    #[test]
    fn incoming_message_clone_is_independent() {
        let msg1 = IncomingMessage {
            channel: ChannelKind::Voice,
            text: "original".to_string(),
            sender_id: None,
        };
        let mut msg2 = msg1.clone();
        msg2.text = "modified".to_string();
        assert_eq!(msg1.text, "original");
        assert_eq!(msg2.text, "modified");
    }

    #[test]
    fn channel_kind_hash_is_distinct() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ChannelKind::Desktop);
        set.insert(ChannelKind::Web);
        set.insert(ChannelKind::Signal);
        set.insert(ChannelKind::Voice);
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn channel_kind_deserialize_all_variants() {
        let cases = [
            ("\"desktop\"", ChannelKind::Desktop),
            ("\"web\"", ChannelKind::Web),
            ("\"signal\"", ChannelKind::Signal),
            ("\"voice\"", ChannelKind::Voice),
        ];
        for (json, expected) in &cases {
            let kind: ChannelKind = serde_json::from_str(json).unwrap();
            assert_eq!(kind, *expected, "deserialize {json} failed");
        }
    }

    #[test]
    fn channel_context_shorthand_constructors() {
        assert_eq!(ChannelContext::desktop().kind, ChannelKind::Desktop);
        assert_eq!(ChannelContext::web().kind, ChannelKind::Web);
        assert_eq!(ChannelContext::signal().kind, ChannelKind::Signal);
        assert_eq!(ChannelContext::voice().kind, ChannelKind::Voice);
    }

    // ========================================================================
    // Additional comprehensive tests — System variant & edge cases
    // ========================================================================

    // -- ChannelKind::System in Display -------------------------------------

    #[test]
    fn channel_kind_system_display() {
        assert_eq!(ChannelKind::System.to_string(), "system");
    }

    // -- ChannelKind::System in from_str_opt --------------------------------

    #[test]
    fn channel_kind_system_from_str_opt() {
        assert_eq!(ChannelKind::from_str_opt("system"), Some(ChannelKind::System));
        assert_eq!(ChannelKind::from_str_opt("SYSTEM"), Some(ChannelKind::System));
        assert_eq!(ChannelKind::from_str_opt("System"), Some(ChannelKind::System));
        assert_eq!(ChannelKind::from_str_opt("sYsTeM"), Some(ChannelKind::System));
    }

    // -- ChannelKind::System in serde ---------------------------------------

    #[test]
    fn channel_kind_system_serialize() {
        let json = serde_json::to_string(&ChannelKind::System).unwrap();
        assert_eq!(json, "\"system\"");
    }

    #[test]
    fn channel_kind_system_deserialize() {
        let kind: ChannelKind = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(kind, ChannelKind::System);
    }

    #[test]
    fn channel_kind_system_serde_roundtrip() {
        let json = serde_json::to_string(&ChannelKind::System).unwrap();
        let back: ChannelKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ChannelKind::System);
    }

    // -- ChannelCapabilities::for_kind(System) returns minimal caps ---------

    #[test]
    fn capabilities_for_kind_system_is_minimal() {
        let caps = ChannelCapabilities::for_kind(ChannelKind::System);
        // System has no rendering surface — should be same as voice (minimal)
        assert!(!caps.rich_panels);
        assert!(!caps.images);
        assert!(!caps.markdown);
        assert!(!caps.notifications);
        assert!(!caps.structured_input);
        assert!(!caps.password_input);
        assert!(caps.max_text_length.is_none());
    }

    #[test]
    fn capabilities_for_kind_system_matches_voice() {
        let system_caps = ChannelCapabilities::for_kind(ChannelKind::System);
        let voice_caps = ChannelCapabilities::for_kind(ChannelKind::Voice);
        assert_eq!(system_caps.rich_panels, voice_caps.rich_panels);
        assert_eq!(system_caps.images, voice_caps.images);
        assert_eq!(system_caps.markdown, voice_caps.markdown);
        assert_eq!(system_caps.notifications, voice_caps.notifications);
        assert_eq!(system_caps.structured_input, voice_caps.structured_input);
        assert_eq!(system_caps.password_input, voice_caps.password_input);
        assert_eq!(system_caps.max_text_length, voice_caps.max_text_length);
    }

    // -- ChannelKind hash includes System -----------------------------------

    #[test]
    fn channel_kind_hash_includes_system() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ChannelKind::Desktop);
        set.insert(ChannelKind::Web);
        set.insert(ChannelKind::Signal);
        set.insert(ChannelKind::Voice);
        set.insert(ChannelKind::System);
        assert_eq!(set.len(), 5);
    }

    // -- ChannelKind::System is not default ---------------------------------

    #[test]
    fn channel_kind_system_is_not_default() {
        assert_ne!(ChannelKind::default(), ChannelKind::System);
    }

    // -- ChannelContext for System ------------------------------------------

    #[test]
    fn channel_context_new_system() {
        let ctx = ChannelContext::new(ChannelKind::System);
        assert_eq!(ctx.kind, ChannelKind::System);
        assert!(!ctx.capabilities.rich_panels);
        assert!(!ctx.capabilities.images);
    }

    // -- ChannelKind serde all five variants roundtrip ----------------------

    #[test]
    fn channel_kind_serde_all_five_roundtrip() {
        let variants = [
            ChannelKind::Desktop,
            ChannelKind::Web,
            ChannelKind::Signal,
            ChannelKind::Voice,
            ChannelKind::System,
        ];
        for kind in &variants {
            let json = serde_json::to_string(kind).unwrap();
            let back: ChannelKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*kind, back, "serde roundtrip failed for {:?}", kind);
        }
    }

    // -- ChannelKind from_str_opt all five variants -------------------------

    #[test]
    fn channel_kind_from_str_opt_all_five() {
        let cases = [
            ("desktop", ChannelKind::Desktop),
            ("web", ChannelKind::Web),
            ("signal", ChannelKind::Signal),
            ("voice", ChannelKind::Voice),
            ("system", ChannelKind::System),
        ];
        for (input, expected) in &cases {
            assert_eq!(
                ChannelKind::from_str_opt(input),
                Some(*expected),
                "from_str_opt(\"{input}\") failed"
            );
        }
    }

    // -- ChannelKind deserialize invalid ------------------------------------

    #[test]
    fn channel_kind_deserialize_invalid() {
        let result: Result<ChannelKind, _> = serde_json::from_str("\"telegram\"");
        assert!(result.is_err());
    }

    #[test]
    fn channel_kind_deserialize_number_invalid() {
        let result: Result<ChannelKind, _> = serde_json::from_str("42");
        assert!(result.is_err());
    }

    // -- ChannelKind Display all five variants ------------------------------

    #[test]
    fn channel_kind_display_all_five() {
        assert_eq!(ChannelKind::Desktop.to_string(), "desktop");
        assert_eq!(ChannelKind::Web.to_string(), "web");
        assert_eq!(ChannelKind::Signal.to_string(), "signal");
        assert_eq!(ChannelKind::Voice.to_string(), "voice");
        assert_eq!(ChannelKind::System.to_string(), "system");
    }

    // -- ChannelCapabilities web equals desktop -----------------------------

    #[test]
    fn web_capabilities_equal_desktop() {
        let web = ChannelCapabilities::web();
        let desktop = ChannelCapabilities::desktop();
        assert_eq!(web.rich_panels, desktop.rich_panels);
        assert_eq!(web.images, desktop.images);
        assert_eq!(web.markdown, desktop.markdown);
        assert_eq!(web.notifications, desktop.notifications);
        assert_eq!(web.structured_input, desktop.structured_input);
        assert_eq!(web.password_input, desktop.password_input);
        assert_eq!(web.max_text_length, desktop.max_text_length);
    }

    // -- Signal has text limit, others don't --------------------------------

    #[test]
    fn only_signal_has_text_length_limit() {
        assert!(ChannelCapabilities::desktop().max_text_length.is_none());
        assert!(ChannelCapabilities::web().max_text_length.is_none());
        assert_eq!(ChannelCapabilities::signal().max_text_length, Some(4096));
        assert!(ChannelCapabilities::voice().max_text_length.is_none());
    }

    // -- Signal can display images ------------------------------------------

    #[test]
    fn signal_supports_images_but_not_rich() {
        let caps = ChannelCapabilities::signal();
        assert!(caps.images);
        assert!(!caps.rich_panels);
        assert!(!caps.markdown);
    }

    // -- IncomingMessage with empty text ------------------------------------

    #[test]
    fn incoming_message_with_empty_text() {
        let msg = IncomingMessage {
            channel: ChannelKind::System,
            text: String::new(),
            sender_id: None,
        };
        assert!(msg.text.is_empty());
        assert_eq!(msg.channel, ChannelKind::System);
    }

    // -- IncomingMessage from System channel --------------------------------

    #[test]
    fn incoming_message_from_system() {
        let msg = IncomingMessage {
            channel: ChannelKind::System,
            text: "Boot complete".to_string(),
            sender_id: None,
        };
        assert_eq!(msg.channel, ChannelKind::System);
        assert_eq!(msg.text, "Boot complete");
        assert!(msg.sender_id.is_none());
    }

    // ========================================================================
    // Further edge-case tests
    // ========================================================================

    #[test]
    fn channel_kind_from_str_opt_returns_none_for_numeric() {
        assert!(ChannelKind::from_str_opt("0").is_none());
        assert!(ChannelKind::from_str_opt("1").is_none());
        assert!(ChannelKind::from_str_opt("42").is_none());
    }

    #[test]
    fn channel_kind_from_str_opt_returns_none_for_special_chars() {
        assert!(ChannelKind::from_str_opt("@desktop").is_none());
        assert!(ChannelKind::from_str_opt("web!").is_none());
        assert!(ChannelKind::from_str_opt("signal.").is_none());
    }

    #[test]
    fn channel_context_system_has_no_password_input() {
        let ctx = ChannelContext::new(ChannelKind::System);
        assert!(!ctx.capabilities.password_input);
    }

    #[test]
    fn channel_context_desktop_has_password_input() {
        let ctx = ChannelContext::desktop();
        assert!(ctx.capabilities.password_input);
    }

    #[test]
    fn channel_context_web_has_password_input() {
        let ctx = ChannelContext::web();
        assert!(ctx.capabilities.password_input);
    }

    #[test]
    fn channel_context_signal_no_password_input() {
        let ctx = ChannelContext::signal();
        assert!(!ctx.capabilities.password_input);
    }

    #[test]
    fn channel_kind_all_five_are_copy() {
        // ChannelKind is Copy — verify by assigning without move
        let a = ChannelKind::System;
        let b = a;
        let c = a;
        assert_eq!(b, c);
        assert_eq!(a, ChannelKind::System);
    }

    #[test]
    fn channel_kind_debug_format_includes_variant_name() {
        let debug = format!("{:?}", ChannelKind::System);
        assert!(debug.contains("System"));

        let debug = format!("{:?}", ChannelKind::Desktop);
        assert!(debug.contains("Desktop"));
    }

    #[test]
    fn channel_context_clone_shares_capabilities_arc() {
        let ctx1 = ChannelContext::new(ChannelKind::System);
        let ctx2 = ctx1.clone();
        // Arc::ptr_eq checks that they share the same allocation
        assert!(Arc::ptr_eq(&ctx1.capabilities, &ctx2.capabilities));
    }

    #[test]
    fn incoming_message_with_unicode_text() {
        let msg = IncomingMessage {
            channel: ChannelKind::Web,
            text: "Hallo Welt! Salut! \u{1f600}".to_string(),
            sender_id: Some("user-42".to_string()),
        };
        assert!(msg.text.contains("Hallo"));
        assert!(msg.text.contains("\u{1f600}"));
    }

    #[test]
    fn incoming_message_with_very_long_text() {
        let long_text = "a".repeat(100_000);
        let msg = IncomingMessage {
            channel: ChannelKind::Desktop,
            text: long_text.clone(),
            sender_id: None,
        };
        assert_eq!(msg.text.len(), 100_000);
    }

    #[test]
    fn channel_capabilities_for_kind_covers_all_variants() {
        // Ensure for_kind does not panic for any variant
        let variants = [
            ChannelKind::Desktop,
            ChannelKind::Web,
            ChannelKind::Signal,
            ChannelKind::Voice,
            ChannelKind::System,
        ];
        for kind in &variants {
            let _caps = ChannelCapabilities::for_kind(*kind);
        }
    }

    #[test]
    fn channel_kind_serialize_all_five_are_distinct_strings() {
        let variants = [
            ChannelKind::Desktop,
            ChannelKind::Web,
            ChannelKind::Signal,
            ChannelKind::Voice,
            ChannelKind::System,
        ];
        let jsons: Vec<String> = variants.iter().map(|k| serde_json::to_string(k).unwrap()).collect();
        let mut unique = jsons.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "all 5 channel kinds should serialize to distinct strings");
    }
}
