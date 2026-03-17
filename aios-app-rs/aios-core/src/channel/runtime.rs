//! Shared application runtime for multi-channel orchestration.
//!
//! [`AppRuntime`] holds the shared state that all channel frontends
//! (Desktop, Web, Signal) connect to.  It provides:
//!
//! - A unified message ingestion point ([`send_message`](AppRuntime::send_message))
//! - Channel switching via the embedded [`ChannelSwitcher`]
//! - Response routing callbacks per channel
//!
//! Each frontend creates or receives an `Arc<AppRuntime>` and uses it
//! to feed messages and receive responses.

use std::sync::Arc;

use tokio::sync::mpsc;

use super::switcher::ChannelSwitcher;
use super::types::{ChannelKind, IncomingMessage};

/// Callback invoked when the AI produces a response.
///
/// Arguments: `(channel_kind, role, content)`.
/// - `channel_kind`: which channel was active when the response was generated.
/// - `role`: "assistant", "system", or "tool".
/// - `content`: the response text.
pub type ResponseCallback = Arc<dyn Fn(ChannelKind, &str, &str) + Send + Sync>;

/// Shared runtime state for multi-channel AI orchestration.
///
/// All frontends share a single `Arc<AppRuntime>` which provides:
/// - Message ingestion from any channel
/// - Channel switching
/// - Response routing to registered callbacks
///
/// The actual LLM processing is driven by whoever consumes messages
/// from [`message_rx`](AppRuntime::take_message_rx) (typically the
/// GTK app or a dedicated orchestrator task).
pub struct AppRuntime {
    /// Channel switcher — tracks the active channel.
    pub switcher: ChannelSwitcher,
    /// Sender half — any channel can send messages here.
    message_tx: mpsc::UnboundedSender<IncomingMessage>,
    /// Receiver half — consumed by the main orchestration loop.
    /// Wrapped in a Mutex so it can be taken once.
    message_rx: tokio::sync::Mutex<Option<mpsc::UnboundedReceiver<IncomingMessage>>>,
    /// Registered response callbacks (one per channel kind).
    response_callbacks: tokio::sync::Mutex<Vec<(ChannelKind, ResponseCallback)>>,
}

impl AppRuntime {
    /// Create a new runtime with Desktop as the default channel.
    pub fn new() -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            switcher: ChannelSwitcher::new(),
            message_tx: tx,
            message_rx: tokio::sync::Mutex::new(Some(rx)),
            response_callbacks: tokio::sync::Mutex::new(Vec::new()),
        })
    }

    /// Get a clone of the message sender.
    ///
    /// Each channel frontend clones this to feed messages into the runtime.
    pub fn message_sender(&self) -> mpsc::UnboundedSender<IncomingMessage> {
        self.message_tx.clone()
    }

    /// Take the message receiver (can only be called once).
    ///
    /// The main orchestration loop consumes messages from this receiver.
    pub async fn take_message_rx(&self) -> Option<mpsc::UnboundedReceiver<IncomingMessage>> {
        self.message_rx.lock().await.take()
    }

    /// Send a message from a specific channel.
    ///
    /// This is a convenience wrapper that creates an [`IncomingMessage`]
    /// and sends it through the internal channel.
    pub fn send_message(&self, channel: ChannelKind, text: String, sender_id: Option<String>) {
        let msg = IncomingMessage {
            channel,
            text,
            sender_id,
        };
        let _ = self.message_tx.send(msg);
    }

    /// Register a callback for receiving AI responses on a specific channel.
    ///
    /// When the AI produces a response and the given channel is active,
    /// the callback is invoked with `(role, content)`.
    pub async fn on_response(&self, channel: ChannelKind, cb: ResponseCallback) {
        let mut callbacks = self.response_callbacks.lock().await;
        callbacks.push((channel, cb));
    }

    /// Dispatch a response to all registered callbacks for the active channel.
    ///
    /// Also dispatches to Desktop if Desktop is not the active channel
    /// (for conversation continuity — Desktop always shows all messages).
    pub async fn dispatch_response(&self, role: &str, content: &str) {
        let active = self.switcher.active_kind();

        // Clone callbacks and drop the lock before invoking them to avoid
        // deadlocks if a callback tries to call on_response/dispatch_response.
        let snapshot: Vec<_> = {
            let callbacks = self.response_callbacks.lock().await;
            callbacks.clone()
        };

        for (kind, cb) in snapshot.iter() {
            // Always send to the active channel.
            if *kind == active {
                cb(active, role, content);
            }
            // Desktop always gets a copy for conversation continuity.
            if *kind == ChannelKind::Desktop && active != ChannelKind::Desktop {
                cb(active, role, content);
            }
        }
    }

    /// Switch the active channel (delegates to the embedded switcher).
    ///
    /// If the message came from a different channel than the active one,
    /// the channel is switched automatically.
    pub fn switch_if_needed(&self, incoming_channel: ChannelKind) {
        if self.switcher.active_kind() != incoming_channel {
            let _ = self.switcher.switch_to(incoming_channel);
        }
    }
}

impl Default for AppRuntime {
    fn default() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            switcher: ChannelSwitcher::new(),
            message_tx: tx,
            message_rx: tokio::sync::Mutex::new(Some(rx)),
            response_callbacks: tokio::sync::Mutex::new(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::ChannelContext;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn runtime_message_roundtrip() {
        let rt = AppRuntime::new();
        let mut rx = rt.take_message_rx().await.unwrap();

        rt.send_message(ChannelKind::Signal, "Hello".into(), Some("+1234".into()));

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.channel, ChannelKind::Signal);
        assert_eq!(msg.text, "Hello");
        assert_eq!(msg.sender_id.as_deref(), Some("+1234"));
    }

    #[tokio::test]
    async fn runtime_take_rx_only_once() {
        let rt = AppRuntime::new();
        assert!(rt.take_message_rx().await.is_some());
        assert!(rt.take_message_rx().await.is_none());
    }

    #[tokio::test]
    async fn runtime_switch_if_needed() {
        let rt = AppRuntime::new();
        rt.switcher.register_channel(ChannelKind::Web, ChannelContext::web());

        assert_eq!(rt.switcher.active_kind(), ChannelKind::Desktop);
        rt.switch_if_needed(ChannelKind::Web);
        assert_eq!(rt.switcher.active_kind(), ChannelKind::Web);
    }

    #[tokio::test]
    async fn runtime_switch_if_needed_noop() {
        let rt = AppRuntime::new();
        rt.switch_if_needed(ChannelKind::Desktop);
        assert_eq!(rt.switcher.active_kind(), ChannelKind::Desktop);
    }

    #[tokio::test]
    async fn runtime_dispatch_to_active_channel() {
        let rt = AppRuntime::new();
        let count = Arc::new(AtomicUsize::new(0));

        let c = count.clone();
        rt.on_response(
            ChannelKind::Desktop,
            Arc::new(move |_kind, _role, _content| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .await;

        rt.dispatch_response("assistant", "Hello!").await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn runtime_desktop_always_gets_copy() {
        let rt = AppRuntime::new();
        rt.switcher
            .register_channel(ChannelKind::Signal, ChannelContext::signal());
        rt.switcher.switch_to(ChannelKind::Signal).unwrap();

        let desktop_count = Arc::new(AtomicUsize::new(0));
        let signal_count = Arc::new(AtomicUsize::new(0));

        let d = desktop_count.clone();
        rt.on_response(
            ChannelKind::Desktop,
            Arc::new(move |_kind, _role, _content| {
                d.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .await;

        let s = signal_count.clone();
        rt.on_response(
            ChannelKind::Signal,
            Arc::new(move |_kind, _role, _content| {
                s.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .await;

        rt.dispatch_response("assistant", "Hi from Signal").await;

        // Both should get the message.
        assert_eq!(signal_count.load(Ordering::SeqCst), 1);
        assert_eq!(desktop_count.load(Ordering::SeqCst), 1);
    }

    // -- Integration / stress tests --

    #[tokio::test]
    async fn send_100_messages_and_receive_all() {
        let rt = AppRuntime::new();
        let mut rx = rt.take_message_rx().await.unwrap();

        for i in 0..100 {
            rt.send_message(
                ChannelKind::Desktop,
                format!("msg-{i}"),
                None,
            );
        }

        for i in 0..100 {
            let msg = rx.recv().await.unwrap();
            assert_eq!(msg.text, format!("msg-{i}"));
            assert_eq!(msg.channel, ChannelKind::Desktop);
        }
    }

    #[tokio::test]
    async fn multiple_senders_web_and_signal() {
        let rt = AppRuntime::new();
        rt.switcher
            .register_channel(ChannelKind::Web, ChannelContext::web());
        rt.switcher
            .register_channel(ChannelKind::Signal, ChannelContext::signal());
        let mut rx = rt.take_message_rx().await.unwrap();

        let tx_web = rt.message_sender();
        let tx_signal = rt.message_sender();

        // Simulate web client sending messages.
        for i in 0..5 {
            let _ = tx_web.send(IncomingMessage {
                channel: ChannelKind::Web,
                text: format!("web-{i}"),
                sender_id: Some("browser-1".to_string()),
            });
        }

        // Simulate signal client sending messages.
        for i in 0..5 {
            let _ = tx_signal.send(IncomingMessage {
                channel: ChannelKind::Signal,
                text: format!("signal-{i}"),
                sender_id: Some("+1234567890".to_string()),
            });
        }

        let mut received = Vec::new();
        for _ in 0..10 {
            received.push(rx.recv().await.unwrap());
        }

        let web_msgs: Vec<_> = received
            .iter()
            .filter(|m| m.channel == ChannelKind::Web)
            .collect();
        let signal_msgs: Vec<_> = received
            .iter()
            .filter(|m| m.channel == ChannelKind::Signal)
            .collect();

        assert_eq!(web_msgs.len(), 5);
        assert_eq!(signal_msgs.len(), 5);
    }

    #[tokio::test]
    async fn dispatch_response_with_no_callbacks_does_not_panic() {
        let rt = AppRuntime::new();
        // No callbacks registered — should not panic.
        rt.dispatch_response("assistant", "Hello, nobody").await;
        // If we reach here, the test passes.
    }

    #[tokio::test]
    async fn switch_if_needed_with_unregistered_channel_is_noop() {
        let rt = AppRuntime::new();
        // Voice is not registered — switch_if_needed should silently fail.
        rt.switch_if_needed(ChannelKind::Voice);
        // Active channel should still be Desktop.
        assert_eq!(rt.switcher.active_kind(), ChannelKind::Desktop);
    }

    #[tokio::test]
    async fn dispatch_response_content_is_passed_correctly() {
        let rt = AppRuntime::new();
        let captured_role = Arc::new(std::sync::Mutex::new(String::new()));
        let captured_content = Arc::new(std::sync::Mutex::new(String::new()));

        let r = captured_role.clone();
        let c = captured_content.clone();
        rt.on_response(
            ChannelKind::Desktop,
            Arc::new(move |_kind, role, content| {
                *r.lock().unwrap() = role.to_string();
                *c.lock().unwrap() = content.to_string();
            }),
        )
        .await;

        rt.dispatch_response("tool", "result data").await;

        assert_eq!(*captured_role.lock().unwrap(), "tool");
        assert_eq!(*captured_content.lock().unwrap(), "result data");
    }

    #[tokio::test]
    async fn multiple_callbacks_on_same_channel() {
        let rt = AppRuntime::new();
        let count = Arc::new(AtomicUsize::new(0));

        for _ in 0..5 {
            let c = count.clone();
            rt.on_response(
                ChannelKind::Desktop,
                Arc::new(move |_, _, _| {
                    c.fetch_add(1, Ordering::SeqCst);
                }),
            )
            .await;
        }

        rt.dispatch_response("system", "test").await;
        assert_eq!(count.load(Ordering::SeqCst), 5);
    }
}
