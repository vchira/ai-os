#![allow(dead_code)]
//! Desktop channel — Channel trait implementation for the GTK4 desktop UI.
//!
//! Bridges the message queue to the GTK ChatView. Renders messages as GTK
//! widgets and handles voice output (TTS).

use std::sync::Arc;

use aios_core::channel::traits::Channel;
use aios_core::channel::types::{ChannelCapabilities, ChannelKind};
use aios_core::config::ConfigManager;
use aios_core::queue::QueuedMessage;
use aios_core::types::Role;

use crate::tts;
use crate::ui::chat_view::ChatView;

/// GTK Desktop channel implementation.
///
/// **Thread safety:** This struct holds GTK widgets which are NOT `Send`.
/// All methods MUST be called from the GTK main thread. Queue event
/// dispatch should use `glib::idle_add_local` to marshal events.
pub struct DesktopChannel {
    chat_view: ChatView,
    capabilities: ChannelCapabilities,
    config: Arc<std::sync::Mutex<ConfigManager>>,
}

impl DesktopChannel {
    /// Create a new DesktopChannel.
    pub fn new(chat_view: ChatView, config: Arc<std::sync::Mutex<ConfigManager>>) -> Self {
        Self {
            chat_view,
            capabilities: ChannelCapabilities::desktop(),
            config,
        }
    }

    /// Get a reference to the underlying ChatView.
    pub fn chat_view(&self) -> &ChatView {
        &self.chat_view
    }

    /// Render a single QueuedMessage in the chat view.
    fn render_one(&self, msg: &QueuedMessage) {
        let role_str = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        };

        // System messages with a level get special formatting.
        if msg.role == Role::System {
            if let Some(level) = msg.level {
                if let Some(ref content) = msg.content {
                    self.chat_view.add_level_message(level, content);
                    return;
                }
            }
        }

        // Check for metadata-driven cards (setup cards).
        if let Some(ref metadata) = msg.metadata {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata) {
                if meta.get("card_type").is_some() {
                    // Render as interactive card based on card_type.
                    // For now, fall through to plain text rendering.
                    // TODO: Implement card rendering from metadata.
                }
            }
        }

        // Standard message rendering.
        if let Some(ref content) = msg.content {
            self.chat_view.add_message(role_str, content);
        }

        // Trigger TTS for assistant messages.
        if msg.role == Role::Assistant {
            if let Some(ref content) = msg.content {
                if let Ok(config) = self.config.lock() {
                    tts::speak_if_enabled(content, &config);
                }
            }
        }
    }
}

impl Channel for DesktopChannel {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Desktop
    }

    fn capabilities(&self) -> &ChannelCapabilities {
        &self.capabilities
    }

    fn render_message(&self, msg: &QueuedMessage) {
        self.render_one(msg);
    }

    fn render_batch(&self, msgs: &[QueuedMessage]) {
        for msg in msgs {
            self.render_one(msg);
        }
    }

    fn on_input(&self, raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_string())
    }

    fn is_active(&self) -> bool {
        // Desktop is always active.
        true
    }
}
