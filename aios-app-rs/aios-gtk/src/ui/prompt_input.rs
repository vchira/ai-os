//! Text input area with send, push-to-talk, and slash-command autocomplete.
//!
//! [`PromptInput`] wraps a horizontal `gtk::Box` containing a text entry,
//! a send button, and a push-to-talk button. It exposes an `on_submit`
//! method to register a callback that fires when the user presses Enter
//! or clicks Send.
//!
//! When the user types `/` followed by letters, an autocomplete popover
//! appears showing matching slash commands.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};

use aios_core::config::commands::command_list;

// ---------------------------------------------------------------------------
// PromptInput
// ---------------------------------------------------------------------------

/// Text input area at the bottom of the main window.
#[derive(Clone)]
pub struct PromptInput {
    container: gtk::Box,
    entry: gtk::Entry,
    send_button: gtk::Button,
    /// Push-to-talk button: hold to record, release to transcribe + send.
    ptt_button: gtk::Button,
    popover: gtk::Popover,
    list_box: gtk::ListBox,
    /// Shared callback invoked when the user submits text.
    on_submit_cb: Rc<RefCell<Option<Box<dyn Fn(String)>>>>,
    /// PTT press callback.
    on_ptt_press_cb: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    /// PTT release callback.
    on_ptt_release_cb: Rc<RefCell<Option<Box<dyn Fn()>>>>,
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

        // Push-to-talk button: hold to record, release to send through STT.
        let ptt_button = gtk::Button::from_icon_name("audio-input-microphone-symbolic");
        ptt_button.add_css_class("circular");
        ptt_button.set_tooltip_text(Some("Hold to talk (or press Super key)"));
        container.append(&ptt_button);

        let on_submit_cb: Rc<RefCell<Option<Box<dyn Fn(String)>>>> =
            Rc::new(RefCell::new(None));
        let on_ptt_press_cb: Rc<RefCell<Option<Box<dyn Fn()>>>> =
            Rc::new(RefCell::new(None));
        let on_ptt_release_cb: Rc<RefCell<Option<Box<dyn Fn()>>>> =
            Rc::new(RefCell::new(None));

        // --- Autocomplete popover ---
        let list_box = gtk::ListBox::new();
        list_box.set_selection_mode(gtk::SelectionMode::Single);

        let scrolled = gtk::ScrolledWindow::builder()
            .max_content_height(250)
            .propagate_natural_height(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scrolled.set_child(Some(&list_box));

        let popover = gtk::Popover::new();
        popover.set_parent(&entry);
        popover.set_child(Some(&scrolled));
        popover.set_autohide(false); // We manage visibility ourselves
        popover.set_position(gtk::PositionType::Top);

        let prompt = Self {
            container,
            entry,
            send_button,
            ptt_button,
            popover,
            list_box,
            on_submit_cb,
            on_ptt_press_cb,
            on_ptt_release_cb,
        };

        prompt.connect_internal_signals();
        prompt.connect_autocomplete();

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

    /// Enable or disable the input field and send button.
    #[allow(dead_code)]
    ///
    /// When disabled, the entry shows a hint and the user cannot type or send.
    /// Call this with `false` when no API key is configured.
    pub fn set_enabled(&self, enabled: bool) {
        self.entry.set_sensitive(enabled);
        self.send_button.set_sensitive(enabled);
        if enabled {
            self.entry
                .set_placeholder_text(Some("Type a message or /command..."));
        } else {
            self.entry
                .set_placeholder_text(Some("Configure an API key to start chatting (/key)"));
        }
    }

    /// Register a callback for PTT press (start recording).
    pub fn on_ptt_press(&self, callback: impl Fn() + 'static) {
        *self.on_ptt_press_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Register a callback for PTT release (stop recording + transcribe).
    pub fn on_ptt_release(&self, callback: impl Fn() + 'static) {
        *self.on_ptt_release_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Get the PTT button widget (for external Super key trigger).
    pub fn ptt_button(&self) -> &gtk::Button {
        &self.ptt_button
    }

    /// Commands that take no arguments and should execute immediately on selection.
    const IMMEDIATE_COMMANDS: &'static [&'static str] = &[
        "/help", "/tools", "/info", "/clear", "/configure", "/selftest", "/sysinfo", "/close",
    ];

    /// Wire up Enter key and Send button to the submit handler.
    fn connect_internal_signals(&self) {
        // Enter key in the entry.
        let cb = self.on_submit_cb.clone();
        let entry = self.entry.clone();
        let popover = self.popover.clone();
        self.entry.connect_activate(move |e| {
            // If popover is showing, don't submit — let autocomplete handle it.
            if popover.is_visible() {
                return;
            }
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

        // Push-to-talk: press to start recording, release to stop + transcribe.
        let press_gesture = gtk::GestureClick::new();
        press_gesture.set_button(1); // Left mouse button

        let press_cb = self.on_ptt_press_cb.clone();
        let ptt_btn_press = self.ptt_button.clone();
        press_gesture.connect_pressed(move |_, _, _, _| {
            ptt_btn_press.add_css_class("recording");
            ptt_btn_press.set_icon_name("media-record-symbolic");
            if let Some(ref f) = *press_cb.borrow() {
                f();
            }
        });

        let release_cb = self.on_ptt_release_cb.clone();
        let ptt_btn_release = self.ptt_button.clone();
        press_gesture.connect_released(move |_, _, _, _| {
            ptt_btn_release.remove_css_class("recording");
            ptt_btn_release.set_icon_name("audio-input-microphone-symbolic");
            if let Some(ref f) = *release_cb.borrow() {
                f();
            }
        });

        self.ptt_button.add_controller(press_gesture);
    }

    /// Set up the slash-command autocomplete popover.
    fn connect_autocomplete(&self) {
        let entry = self.entry.clone();
        let popover = self.popover.clone();
        let list_box = self.list_box.clone();
        let commands = command_list();

        // When the entry text changes, filter and show/hide the popover.
        let entry_for_changed = entry.clone();
        let popover_for_changed = popover.clone();
        let list_box_for_changed = list_box.clone();
        entry.connect_changed(move |_| {
            let text = entry_for_changed.text().to_string();

            // Only show when text starts with '/' and has no spaces.
            if !text.starts_with('/') || text.contains(' ') || text.len() < 1 {
                popover_for_changed.popdown();
                return;
            }

            let filter = text.to_lowercase();

            // Filter matching commands.
            let matches: Vec<_> = commands
                .iter()
                .filter(|c| c.command.starts_with(&filter))
                .collect();

            if matches.is_empty() {
                popover_for_changed.popdown();
                return;
            }

            // Rebuild list box rows.
            while let Some(child) = list_box_for_changed.first_child() {
                list_box_for_changed.remove(&child);
            }

            for cmd_info in &matches {
                let row_box = gtk::Box::new(Orientation::Horizontal, 10);
                row_box.set_margin_start(8);
                row_box.set_margin_end(8);
                row_box.set_margin_top(4);
                row_box.set_margin_bottom(4);

                let cmd_label = gtk::Label::new(Some(cmd_info.command));
                cmd_label.set_halign(Align::Start);
                cmd_label.add_css_class("heading");
                row_box.append(&cmd_label);

                let desc_label = gtk::Label::new(Some(&cmd_info.description));
                desc_label.set_halign(Align::Start);
                desc_label.set_hexpand(true);
                desc_label.set_opacity(0.6);
                row_box.append(&desc_label);

                list_box_for_changed.append(&row_box);
            }

            // Select first row.
            if let Some(first) = list_box_for_changed.row_at_index(0) {
                list_box_for_changed.select_row(Some(&first));
            }

            popover_for_changed.popup();
        });

        // When user clicks a row, execute immediately for no-arg commands
        // or insert the command text for commands that take arguments.
        let entry_for_activate = entry.clone();
        let popover_for_activate = popover.clone();
        let cb_for_activate = self.on_submit_cb.clone();
        list_box.connect_row_activated(move |lb, row| {
            let idx = row.index();
            let text = entry_for_activate.text().to_string();
            let filter = text.to_lowercase();
            let cmds = command_list();
            let matches: Vec<_> = cmds
                .iter()
                .filter(|c| c.command.starts_with(&filter))
                .collect();

            if let Some(cmd) = matches.get(idx as usize) {
                if Self::IMMEDIATE_COMMANDS.contains(&cmd.command) {
                    // Execute immediately — submit the command.
                    entry_for_activate.set_text("");
                    popover_for_activate.popdown();
                    if let Some(ref f) = *cb_for_activate.borrow() {
                        f(cmd.command.to_string());
                    }
                } else {
                    // Commands that take arguments — insert with trailing space.
                    let insert = format!("{} ", cmd.command);
                    entry_for_activate.set_text(&insert);
                    entry_for_activate.set_position(insert.len() as i32);
                    popover_for_activate.popdown();
                }
            } else {
                popover_for_activate.popdown();
            }

            let _ = lb; // suppress unused
        });

        // Keyboard navigation for popover.
        let popover_for_key = popover.clone();
        let entry_for_key = entry.clone();
        let entry_for_controller = entry.clone();
        let list_box_for_key = list_box.clone();
        let cb_for_key = self.on_submit_cb.clone();
        let key_controller = gtk::EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if !popover_for_key.is_visible() {
                return gtk::glib::Propagation::Proceed;
            }

            match key {
                gtk::gdk::Key::Escape => {
                    popover_for_key.popdown();
                    gtk::glib::Propagation::Stop
                }
                gtk::gdk::Key::Return | gtk::gdk::Key::Tab => {
                    // Accept selected row — execute immediately for no-arg
                    // commands, insert text for commands that take arguments.
                    if let Some(row) = list_box_for_key.selected_row() {
                        let idx = row.index();
                        let text = entry_for_key.text().to_string();
                        let filter = text.to_lowercase();
                        let cmds = command_list();
                        let matches: Vec<_> = cmds.iter()
                            .filter(|c| c.command.starts_with(&filter))
                            .collect();
                        if let Some(cmd) = matches.get(idx as usize) {
                            if Self::IMMEDIATE_COMMANDS.contains(&cmd.command) {
                                // Execute immediately.
                                entry_for_key.set_text("");
                                popover_for_key.popdown();
                                if let Some(ref f) = *cb_for_key.borrow() {
                                    f(cmd.command.to_string());
                                }
                                return gtk::glib::Propagation::Stop;
                            } else {
                                let insert = format!("{} ", cmd.command);
                                entry_for_key.set_text(&insert);
                                entry_for_key.set_position(insert.len() as i32);
                            }
                        }
                    }
                    popover_for_key.popdown();
                    gtk::glib::Propagation::Stop
                }
                gtk::gdk::Key::Down => {
                    // Move selection down.
                    if let Some(row) = list_box_for_key.selected_row() {
                        let next_idx = row.index() + 1;
                        if let Some(next) = list_box_for_key.row_at_index(next_idx) {
                            list_box_for_key.select_row(Some(&next));
                        }
                    }
                    gtk::glib::Propagation::Stop
                }
                gtk::gdk::Key::Up => {
                    // Move selection up.
                    if let Some(row) = list_box_for_key.selected_row() {
                        let prev_idx = row.index() - 1;
                        if prev_idx >= 0 {
                            if let Some(prev) = list_box_for_key.row_at_index(prev_idx) {
                                list_box_for_key.select_row(Some(&prev));
                            }
                        }
                    }
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
        entry_for_controller.add_controller(key_controller);
    }
}
