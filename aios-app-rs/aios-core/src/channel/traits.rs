//! Channel trait — unified interface for all communication channels.
//!
//! Every channel (Desktop, Web, Signal) implements this trait to receive
//! messages from the queue and render them in its own format.

use crate::channel::types::{ChannelCapabilities, ChannelKind};
use crate::queue::{QueueEvent, QueuedMessage};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// ChannelBase
// ---------------------------------------------------------------------------

/// Shared state for all channel implementations.
pub struct ChannelBase {
    /// Which channel this is.
    pub kind: ChannelKind,
    /// Receiver for queue events.
    pub queue_rx: mpsc::UnboundedReceiver<QueueEvent>,
    /// ID of the last message rendered by this channel.
    pub last_rendered_id: Option<i64>,
}

impl ChannelBase {
    /// Create a new ChannelBase with the given kind and event receiver.
    pub fn new(kind: ChannelKind, queue_rx: mpsc::UnboundedReceiver<QueueEvent>) -> Self {
        Self {
            kind,
            queue_rx,
            last_rendered_id: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Channel trait
// ---------------------------------------------------------------------------

/// Trait that every communication channel implements.
///
/// **Thread safety:** No `Send` bound — `DesktopChannel` holds GTK widgets
/// which are not `Send`. Queue event dispatch is handled per-channel:
/// Desktop events are marshaled to the GTK main thread via
/// `glib::idle_add_local`, Web/Signal events run on the Tokio runtime.
pub trait Channel {
    /// Channel identity.
    fn kind(&self) -> ChannelKind;

    /// What this channel can render.
    fn capabilities(&self) -> &ChannelCapabilities;

    /// Render a single message in this channel's format.
    fn render_message(&self, msg: &QueuedMessage);

    /// Render a batch of messages (initial load or scroll-back).
    fn render_batch(&self, msgs: &[QueuedMessage]);

    /// Handle incoming user input from this channel.
    /// Returns the text to push to the queue, or None to ignore.
    fn on_input(&self, raw: &str) -> Option<String>;

    /// Whether this channel is currently active/visible.
    fn is_active(&self) -> bool;
}
