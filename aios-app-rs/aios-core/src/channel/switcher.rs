//! Channel switcher — thread-safe state machine for the active channel.
//!
//! The AI has one conversation, one brain, multiple output surfaces.
//! The [`ChannelSwitcher`] tracks which channel is currently active
//! (whoever sent a message last owns it) and notifies listeners when
//! the channel changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::types::{ChannelContext, ChannelKind};

// ---------------------------------------------------------------------------
// Callback type
// ---------------------------------------------------------------------------

/// Callback invoked when the active channel changes.
///
/// Arguments: `(old_channel, new_channel)`.
pub type OnSwitchCallback = Arc<dyn Fn(ChannelKind, ChannelKind) + Send + Sync>;

// ---------------------------------------------------------------------------
// ChannelSwitcher
// ---------------------------------------------------------------------------

/// Thread-safe state machine that tracks the active output channel.
///
/// All frontends (Desktop, Web, Signal, Voice) share a single
/// `ChannelSwitcher` via `Arc`.  When a message arrives on a new
/// channel, the sender calls [`switch_to`](Self::switch_to) and
/// all registered listeners are notified.
///
/// # Example
///
/// ```
/// use aios_core::channel::{ChannelSwitcher, ChannelContext, ChannelKind};
/// use std::sync::Arc;
///
/// let switcher = ChannelSwitcher::new();
/// switcher.register_channel(ChannelKind::Desktop, ChannelContext::desktop());
/// switcher.register_channel(ChannelKind::Signal, ChannelContext::signal());
///
/// assert_eq!(switcher.active_kind(), ChannelKind::Desktop);
///
/// switcher.switch_to(ChannelKind::Signal).unwrap();
/// assert_eq!(switcher.active_kind(), ChannelKind::Signal);
/// ```
#[derive(Clone)]
pub struct ChannelSwitcher {
    inner: Arc<Mutex<SwitcherInner>>,
}

struct SwitcherInner {
    /// Currently active channel.
    active: ChannelKind,
    /// Registered channels with their contexts.
    channels: HashMap<ChannelKind, ChannelContext>,
    /// Listeners notified on channel switch.
    on_switch: Vec<OnSwitchCallback>,
}

impl ChannelSwitcher {
    /// Create a new switcher with Desktop as the default active channel.
    ///
    /// Desktop is automatically registered.
    pub fn new() -> Self {
        let mut channels = HashMap::new();
        channels.insert(ChannelKind::Desktop, ChannelContext::desktop());

        Self {
            inner: Arc::new(Mutex::new(SwitcherInner {
                active: ChannelKind::Desktop,
                channels,
                on_switch: Vec::new(),
            })),
        }
    }

    /// Register a channel as available.
    ///
    /// A channel must be registered before it can be switched to.
    pub fn register_channel(&self, kind: ChannelKind, ctx: ChannelContext) {
        let mut inner = self.inner.lock().unwrap();
        inner.channels.insert(kind, ctx);
    }

    /// Unregister a channel (e.g. when a web client disconnects).
    ///
    /// If the removed channel was active, switches back to Desktop.
    pub fn unregister_channel(&self, kind: ChannelKind) {
        let mut inner = self.inner.lock().unwrap();
        inner.channels.remove(&kind);

        if inner.active == kind {
            let old = inner.active;
            inner.active = ChannelKind::Desktop;
            let callbacks: Vec<_> = inner.on_switch.clone();
            drop(inner);
            for cb in &callbacks {
                cb(old, ChannelKind::Desktop);
            }
        }
    }

    /// Get the currently active channel kind.
    pub fn active_kind(&self) -> ChannelKind {
        self.inner.lock().unwrap().active
    }

    /// Get the full context for the currently active channel.
    pub fn active_context(&self) -> ChannelContext {
        let inner = self.inner.lock().unwrap();
        inner
            .channels
            .get(&inner.active)
            .cloned()
            .unwrap_or_default()
    }

    /// Switch to a different channel.
    ///
    /// Returns `Err` if the channel is not registered.
    /// No-op if already on the requested channel.
    pub fn switch_to(&self, kind: ChannelKind) -> Result<(), String> {
        let (old, callbacks) = {
            let mut inner = self.inner.lock().unwrap();

            if inner.active == kind {
                return Ok(()); // Already on this channel
            }

            if !inner.channels.contains_key(&kind) {
                return Err(format!("Channel {kind} is not registered"));
            }

            let old = inner.active;
            inner.active = kind;

            (old, inner.on_switch.clone())
        };

        // Fire callbacks outside the lock to avoid deadlocks.
        for cb in &callbacks {
            cb(old, kind);
        }

        Ok(())
    }

    /// Register a callback that fires when the active channel changes.
    ///
    /// The callback receives `(old_kind, new_kind)`.
    pub fn on_switch(&self, cb: OnSwitchCallback) {
        let mut inner = self.inner.lock().unwrap();
        inner.on_switch.push(cb);
    }

    /// List all registered channel kinds.
    pub fn registered_channels(&self) -> Vec<ChannelKind> {
        let inner = self.inner.lock().unwrap();
        inner.channels.keys().copied().collect()
    }

    /// Check whether a channel is registered.
    pub fn is_registered(&self, kind: ChannelKind) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.channels.contains_key(&kind)
    }
}

impl Default for ChannelSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    #[test]
    fn default_channel_is_desktop() {
        let sw = ChannelSwitcher::new();
        assert_eq!(sw.active_kind(), ChannelKind::Desktop);
    }

    #[test]
    fn desktop_is_auto_registered() {
        let sw = ChannelSwitcher::new();
        assert!(sw.is_registered(ChannelKind::Desktop));
    }

    #[test]
    fn switch_to_registered_channel() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());

        assert!(sw.switch_to(ChannelKind::Signal).is_ok());
        assert_eq!(sw.active_kind(), ChannelKind::Signal);
    }

    #[test]
    fn switch_to_unregistered_channel_fails() {
        let sw = ChannelSwitcher::new();
        let result = sw.switch_to(ChannelKind::Signal);
        assert!(result.is_err());
        assert_eq!(sw.active_kind(), ChannelKind::Desktop); // unchanged
    }

    #[test]
    fn switch_to_same_channel_is_noop() {
        let sw = ChannelSwitcher::new();
        let called = Arc::new(AtomicBool::new(false));
        let called_ref = called.clone();
        sw.on_switch(Arc::new(move |_, _| {
            called_ref.store(true, Ordering::SeqCst);
        }));

        assert!(sw.switch_to(ChannelKind::Desktop).is_ok());
        assert!(!called.load(Ordering::SeqCst)); // callback NOT fired
    }

    #[test]
    fn on_switch_callback_fires() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Web, ChannelContext::web());

        let fired = Arc::new(AtomicBool::new(false));
        let fired_ref = fired.clone();
        sw.on_switch(Arc::new(move |old, new| {
            assert_eq!(old, ChannelKind::Desktop);
            assert_eq!(new, ChannelKind::Web);
            fired_ref.store(true, Ordering::SeqCst);
        }));

        sw.switch_to(ChannelKind::Web).unwrap();
        assert!(fired.load(Ordering::SeqCst));
    }

    #[test]
    fn multiple_callbacks_all_fire() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());

        let count = Arc::new(AtomicU32::new(0));
        for _ in 0..3 {
            let count_ref = count.clone();
            sw.on_switch(Arc::new(move |_, _| {
                count_ref.fetch_add(1, Ordering::SeqCst);
            }));
        }

        sw.switch_to(ChannelKind::Signal).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn unregister_channel_switches_back_to_desktop() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Web, ChannelContext::web());
        sw.switch_to(ChannelKind::Web).unwrap();

        let fired = Arc::new(AtomicBool::new(false));
        let fired_ref = fired.clone();
        sw.on_switch(Arc::new(move |old, new| {
            assert_eq!(old, ChannelKind::Web);
            assert_eq!(new, ChannelKind::Desktop);
            fired_ref.store(true, Ordering::SeqCst);
        }));

        sw.unregister_channel(ChannelKind::Web);
        assert_eq!(sw.active_kind(), ChannelKind::Desktop);
        assert!(fired.load(Ordering::SeqCst));
    }

    #[test]
    fn unregister_inactive_channel_is_quiet() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());

        let fired = Arc::new(AtomicBool::new(false));
        let fired_ref = fired.clone();
        sw.on_switch(Arc::new(move |_, _| {
            fired_ref.store(true, Ordering::SeqCst);
        }));

        sw.unregister_channel(ChannelKind::Signal);
        assert!(!fired.load(Ordering::SeqCst));
        assert!(!sw.is_registered(ChannelKind::Signal));
    }

    #[test]
    fn registered_channels_list() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Web, ChannelContext::web());
        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());

        let mut registered = sw.registered_channels();
        registered.sort_by_key(|k| format!("{k}"));
        assert_eq!(registered.len(), 3); // Desktop + Web + Signal
    }

    #[test]
    fn active_context_returns_correct_capabilities() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());

        let ctx = sw.active_context();
        assert!(ctx.capabilities.rich_panels); // Desktop

        sw.switch_to(ChannelKind::Signal).unwrap();
        let ctx = sw.active_context();
        assert!(!ctx.capabilities.rich_panels); // Signal
    }

    #[test]
    fn switcher_is_clone_and_shared() {
        let sw1 = ChannelSwitcher::new();
        let sw2 = sw1.clone();
        sw1.register_channel(ChannelKind::Web, ChannelContext::web());

        // sw2 sees the registration because they share the inner Arc
        assert!(sw2.is_registered(ChannelKind::Web));
        sw2.switch_to(ChannelKind::Web).unwrap();
        assert_eq!(sw1.active_kind(), ChannelKind::Web);
    }

    // -- Stress / edge-case tests --

    #[test]
    fn switch_to_same_channel_multiple_times_is_noop() {
        let sw = ChannelSwitcher::new();
        let call_count = Arc::new(AtomicU32::new(0));
        let c = call_count.clone();
        sw.on_switch(Arc::new(move |_, _| {
            c.fetch_add(1, Ordering::SeqCst);
        }));

        // Switching to Desktop (already active) 10 times should be a no-op.
        for _ in 0..10 {
            assert!(sw.switch_to(ChannelKind::Desktop).is_ok());
        }
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn register_same_channel_twice_overwrites() {
        let sw = ChannelSwitcher::new();

        // First registration with default signal caps.
        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());
        assert!(sw.is_registered(ChannelKind::Signal));

        // Second registration with custom caps — should overwrite.
        let custom_caps = super::super::types::ChannelCapabilities {
            rich_panels: true, // unusual for signal
            images: false,
            markdown: false,
            notifications: false,
            structured_input: false,
            password_input: false,
            max_text_length: Some(100),
        };
        let custom_ctx = super::super::types::ChannelContext::with_capabilities(
            ChannelKind::Signal,
            custom_caps,
        );
        sw.register_channel(ChannelKind::Signal, custom_ctx);

        // Switch to signal and verify the overwritten context.
        sw.switch_to(ChannelKind::Signal).unwrap();
        let ctx = sw.active_context();
        assert!(ctx.capabilities.rich_panels); // custom value
        assert_eq!(ctx.capabilities.max_text_length, Some(100));
    }

    #[test]
    fn switch_rapidly_between_three_channels() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Web, ChannelContext::web());
        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());

        let history = Arc::new(std::sync::Mutex::new(Vec::new()));
        let h = history.clone();
        sw.on_switch(Arc::new(move |old, new| {
            h.lock().unwrap().push((old, new));
        }));

        // Rapidly switch.
        sw.switch_to(ChannelKind::Web).unwrap();
        sw.switch_to(ChannelKind::Signal).unwrap();
        sw.switch_to(ChannelKind::Desktop).unwrap();
        sw.switch_to(ChannelKind::Web).unwrap();
        sw.switch_to(ChannelKind::Signal).unwrap();

        let switches = history.lock().unwrap();
        assert_eq!(switches.len(), 5);
        assert_eq!(switches[0], (ChannelKind::Desktop, ChannelKind::Web));
        assert_eq!(switches[1], (ChannelKind::Web, ChannelKind::Signal));
        assert_eq!(switches[2], (ChannelKind::Signal, ChannelKind::Desktop));
        assert_eq!(switches[3], (ChannelKind::Desktop, ChannelKind::Web));
        assert_eq!(switches[4], (ChannelKind::Web, ChannelKind::Signal));
    }

    #[test]
    fn callbacks_fire_in_registration_order() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Web, ChannelContext::web());

        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        for i in 0..5u32 {
            let o = order.clone();
            sw.on_switch(Arc::new(move |_, _| {
                o.lock().unwrap().push(i);
            }));
        }

        sw.switch_to(ChannelKind::Web).unwrap();
        let fired = order.lock().unwrap();
        assert_eq!(*fired, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn unregister_all_channels_except_desktop() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Web, ChannelContext::web());
        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());
        sw.register_channel(ChannelKind::Voice, ChannelContext::voice());

        // Unregister all non-Desktop channels.
        sw.unregister_channel(ChannelKind::Web);
        sw.unregister_channel(ChannelKind::Signal);
        sw.unregister_channel(ChannelKind::Voice);

        assert!(!sw.is_registered(ChannelKind::Web));
        assert!(!sw.is_registered(ChannelKind::Signal));
        assert!(!sw.is_registered(ChannelKind::Voice));
        assert!(sw.is_registered(ChannelKind::Desktop));
        assert_eq!(sw.registered_channels().len(), 1);
        assert_eq!(sw.active_kind(), ChannelKind::Desktop);
    }

    #[test]
    fn unregister_active_with_multiple_callbacks() {
        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Voice, ChannelContext::voice());
        sw.switch_to(ChannelKind::Voice).unwrap();

        let count = Arc::new(AtomicU32::new(0));
        for _ in 0..3 {
            let c = count.clone();
            sw.on_switch(Arc::new(move |_, _| {
                c.fetch_add(1, Ordering::SeqCst);
            }));
        }

        sw.unregister_channel(ChannelKind::Voice);
        assert_eq!(sw.active_kind(), ChannelKind::Desktop);
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn default_impl_same_as_new() {
        let sw = ChannelSwitcher::default();
        assert_eq!(sw.active_kind(), ChannelKind::Desktop);
        assert!(sw.is_registered(ChannelKind::Desktop));
    }
}
