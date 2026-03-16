//! Text input area with send and push-to-talk buttons.
//!
//! [`PromptInput`] wraps a horizontal `gtk::Box` containing a text entry,
//! a send button, and a push-to-talk button. It exposes an `on_submit`
//! method to register a callback that fires when the user presses Enter
//! or clicks Send.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{self as gtk, Orientation};

// ---------------------------------------------------------------------------
// PromptInput
// ---------------------------------------------------------------------------

/// Text input area at the bottom of the main window.
#[derive(Clone)]
pub struct PromptInput {
    container: gtk::Box,
    entry: gtk::Entry,
    send_button: gtk::Button,
    ptt_button: gtk::Button,
    /// Shared callback invoked when the user submits text.
    on_submit_cb: Rc<RefCell<Option<Box<dyn Fn(String)>>>>,
}

impl PromptInput {
    /// Create a new prompt input widget.
    pub fn new() -> Self {
        let container = gtk::Box::new(Orientation::Horizontal, 6);
        container.add_css_class("prompt-area");
        container.set_margin_start(8);
        container.set_margin_end(8);
        container.set_margin_top(4);
        container.set_margin_bottom(8);

        // Text entry.
        let entry = gtk::Entry::new();
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some("Type a message or /command..."));
        entry.add_css_class("prompt-entry");
        container.append(&entry);

        // Send button.
        let send_button = gtk::Button::from_icon_name("paper-plane-symbolic");
        send_button.add_css_class("suggested-action");
        send_button.set_tooltip_text(Some("Send message"));
        container.append(&send_button);

        // Push-to-talk button.
        let ptt_button = gtk::Button::from_icon_name("microphone-symbolic");
        ptt_button.add_css_class("circular");
        ptt_button.set_tooltip_text(Some("Push to talk"));
        container.append(&ptt_button);

        let on_submit_cb: Rc<RefCell<Option<Box<dyn Fn(String)>>>> =
            Rc::new(RefCell::new(None));

        let prompt = Self {
            container,
            entry,
            send_button,
            ptt_button,
            on_submit_cb,
        };

        // Wire internal signals.
        prompt.connect_internal_signals();

        prompt
    }

    /// Return the underlying GTK widget (for adding to a container).
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// Register a callback that is called when the user submits text
    /// (via Enter key or Send button click).
    ///
    /// The entry is cleared after submission.
    pub fn on_submit(&self, callback: impl Fn(String) + 'static) {
        *self.on_submit_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Wire up Enter key and Send button to the submit handler.
    fn connect_internal_signals(&self) {
        // Enter key in the entry.
        let cb = self.on_submit_cb.clone();
        let entry = self.entry.clone();
        self.entry.connect_activate(move |e| {
            let text = e.text().to_string();
            if text.trim().is_empty() {
                return;
            }
            if let Some(ref f) = *cb.borrow() {
                f(text);
            }
            entry.set_text("");
        });

        // Send button click.
        let cb = self.on_submit_cb.clone();
        let entry = self.entry.clone();
        self.send_button.connect_clicked(move |_| {
            let text = entry.text().to_string();
            if text.trim().is_empty() {
                return;
            }
            if let Some(ref f) = *cb.borrow() {
                f(text);
            }
            entry.set_text("");
        });

        // Push-to-talk: placeholder — just log for now.
        self.ptt_button.connect_clicked(|_| {
            tracing::info!("Push-to-talk clicked (not yet implemented)");
        });
    }
}
