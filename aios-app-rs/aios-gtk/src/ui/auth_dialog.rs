//! Reusable authentication dialog.
//!
//! [`AuthDialog`] presents a modal window that collects a password and an
//! optional "don't ask again" duration.  It is a pure UI component — it does
//! **not** verify the password itself.  The caller receives the entered
//! credentials via the [`AuthDialog::on_authenticate`] callback and decides
//! what to do with them.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};
use libadwaita as adw;

// ---------------------------------------------------------------------------
// AuthDialog
// ---------------------------------------------------------------------------

/// A reusable, modal authentication dialog.
///
/// Usage:
/// ```ignore
/// let dialog = AuthDialog::new(&parent_window, "Unlock the vault");
/// dialog.on_authenticate(|password, dont_ask| { /* verify */ });
/// dialog.on_cancel(|| { /* handle cancel */ });
/// dialog.show();
/// ```
#[derive(Clone)]
pub struct AuthDialog {
    window: adw::Window,
    reason_label: gtk::Label,
    password_entry: gtk::PasswordEntry,
    dont_ask_check: gtk::CheckButton,
    dont_ask_spin: gtk::SpinButton,
    error_label: gtk::Label,
    authenticate_button: gtk::Button,
    cancel_button: gtk::Button,

    on_authenticate_cb: Rc<RefCell<Option<Box<dyn Fn(String, Option<Duration>)>>>>,
    on_cancel_cb: Rc<RefCell<Option<Box<dyn Fn()>>>>,
}

impl AuthDialog {
    /// Create a new authentication dialog.
    ///
    /// * `parent` — The parent window; the dialog is modal over it.
    /// * `reason` — Human-readable explanation shown to the user (e.g.
    ///   "Enter your password to unlock the vault").
    pub fn new(parent: &impl IsA<gtk::Window>, reason: &str) -> Self {
        // --- Widgets ---------------------------------------------------------

        let reason_label = gtk::Label::new(Some(reason));
        reason_label.set_wrap(true);
        reason_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        reason_label.set_xalign(0.0);
        reason_label.set_margin_bottom(12);

        let password_entry = gtk::PasswordEntry::builder()
            .placeholder_text("Password")
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        password_entry.set_margin_bottom(12);

        // "Don't ask for [N] minutes" row.
        let dont_ask_check = gtk::CheckButton::with_label("Don't ask for password for");
        let adjustment = gtk::Adjustment::new(
            15.0, // default
            1.0,  // min
            120.0, // max
            1.0,  // step
            5.0,  // page step
            0.0,  // page size
        );
        let dont_ask_spin = gtk::SpinButton::new(Some(&adjustment), 1.0, 0);
        dont_ask_spin.set_sensitive(false);
        let minutes_label = gtk::Label::new(Some("minutes"));

        let dont_ask_row = gtk::Box::new(Orientation::Horizontal, 6);
        dont_ask_row.set_margin_bottom(16);
        dont_ask_row.append(&dont_ask_check);
        dont_ask_row.append(&dont_ask_spin);
        dont_ask_row.append(&minutes_label);

        // Enable/disable the spin button based on the check button state.
        {
            let spin = dont_ask_spin.clone();
            dont_ask_check.connect_toggled(move |cb| {
                spin.set_sensitive(cb.is_active());
            });
        }

        // Error label (hidden by default).
        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_wrap(true);
        error_label.set_xalign(0.0);
        error_label.set_margin_bottom(8);

        // Buttons.
        let cancel_button = gtk::Button::with_label("Cancel");
        let authenticate_button = gtk::Button::with_label("Authenticate");
        authenticate_button.add_css_class("suggested-action");

        let button_box = gtk::Box::new(Orientation::Horizontal, 8);
        button_box.set_halign(Align::End);
        button_box.append(&cancel_button);
        button_box.append(&authenticate_button);

        // --- Layout ----------------------------------------------------------

        let content = gtk::Box::new(Orientation::Vertical, 0);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.append(&reason_label);
        content.append(&password_entry);
        content.append(&dont_ask_row);
        content.append(&error_label);
        content.append(&button_box);

        let header = adw::HeaderBar::new();
        header.set_show_end_title_buttons(false);
        header.set_show_start_title_buttons(false);

        let outer = gtk::Box::new(Orientation::Vertical, 0);
        outer.append(&header);
        outer.append(&content);

        let window = adw::Window::builder()
            .title("Authentication Required")
            .transient_for(parent)
            .modal(true)
            .resizable(false)
            .default_width(420)
            .content(&outer)
            .build();

        // --- Callbacks -------------------------------------------------------

        let on_authenticate_cb: Rc<RefCell<Option<Box<dyn Fn(String, Option<Duration>)>>>> =
            Rc::new(RefCell::new(None));
        let on_cancel_cb: Rc<RefCell<Option<Box<dyn Fn()>>>> =
            Rc::new(RefCell::new(None));

        let dialog = Self {
            window,
            reason_label,
            password_entry,
            dont_ask_check,
            dont_ask_spin,
            error_label,
            authenticate_button,
            cancel_button,
            on_authenticate_cb,
            on_cancel_cb,
        };

        dialog.connect_internal_signals();

        dialog
    }

    /// Update the reason text displayed to the user.
    pub fn set_reason(&self, reason: &str) {
        self.reason_label.set_text(reason);
    }

    /// Register a callback for successful authentication attempts.
    ///
    /// The callback receives the entered password and, if the "don't ask"
    /// checkbox was checked, the chosen duration.
    pub fn on_authenticate(&self, callback: impl Fn(String, Option<Duration>) + 'static) {
        *self.on_authenticate_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Register a callback invoked when the user cancels.
    pub fn on_cancel(&self, callback: impl Fn() + 'static) {
        *self.on_cancel_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Present the dialog.
    pub fn show(&self) {
        self.password_entry.set_text("");
        self.error_label.set_visible(false);
        self.window.present();
    }

    /// Hide (close) the dialog.
    pub fn hide(&self) {
        self.window.close();
    }

    /// Display an error message (e.g. "Wrong password").
    pub fn show_error(&self, msg: &str) {
        self.error_label.set_text(msg);
        self.error_label.set_visible(true);
    }

    /// Return the underlying `adw::Window` (for advanced use).
    pub fn widget(&self) -> &adw::Window {
        &self.window
    }

    // -- Internal signal wiring -----------------------------------------------

    fn connect_internal_signals(&self) {
        // Authenticate button clicked.
        let pwd_entry = self.password_entry.clone();
        let check = self.dont_ask_check.clone();
        let spin = self.dont_ask_spin.clone();
        let cb = self.on_authenticate_cb.clone();
        self.authenticate_button.connect_clicked(move |_| {
            let password = pwd_entry.text().to_string();
            let dont_ask = if check.is_active() {
                let minutes = spin.value() as u64;
                Some(Duration::from_secs(minutes * 60))
            } else {
                None
            };
            if let Some(ref f) = *cb.borrow() {
                f(password, dont_ask);
            }
        });

        // Enter key in the password entry triggers authenticate.
        let auth_btn = self.authenticate_button.clone();
        self.password_entry.connect_activate(move |_| {
            auth_btn.emit_clicked();
        });

        // Cancel button clicked.
        let window = self.window.clone();
        let cb = self.on_cancel_cb.clone();
        self.cancel_button.connect_clicked(move |_| {
            if let Some(ref f) = *cb.borrow() {
                f();
            }
            window.close();
        });

        // Window close request (e.g. Escape key or window manager close).
        let cb = self.on_cancel_cb.clone();
        self.window.connect_close_request(move |_| {
            if let Some(ref f) = *cb.borrow() {
                f();
            }
            gtk4::glib::Propagation::Proceed
        });
    }
}
