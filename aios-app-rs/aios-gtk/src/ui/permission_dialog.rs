//! Permission + authentication combined dialog.
//!
//! [`PermissionDialog`] is shown when the AI needs to access a secret or
//! perform a sensitive action.  It displays what is being accessed and why,
//! then lets the user **Allow** or **Deny**.  If the underlying
//! [`PermissionRequest`] also requires authentication, an inline password
//! section is shown.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};
use libadwaita as adw;

use aios_core::secure::permission::{PermissionGrant, PermissionRequest};
use aios_core::secure::auth::AuthResult;

// ---------------------------------------------------------------------------
// PermissionDialog
// ---------------------------------------------------------------------------

/// A modal dialog that asks the user for permission to access a secret,
/// optionally collecting a password if re-authentication is required.
#[derive(Clone)]
pub struct PermissionDialog {
    window: adw::Window,
    on_grant_cb: Rc<RefCell<Option<Box<dyn Fn(PermissionGrant)>>>>,
    on_deny_cb: Rc<RefCell<Option<Box<dyn Fn()>>>>,

    // Keep references to widgets we need to read on button click.
    password_entry: Option<gtk::PasswordEntry>,
    perm_dont_ask_check: gtk::CheckButton,
    perm_dont_ask_spin: gtk::SpinButton,
    auth_dont_ask_check: Option<gtk::CheckButton>,
    auth_dont_ask_spin: Option<gtk::SpinButton>,
    error_label: gtk::Label,
}

impl PermissionDialog {
    /// Create a new permission dialog.
    ///
    /// * `parent` — The parent window (dialog is modal).
    /// * `request` — Describes what secret is being accessed and why.
    pub fn new(parent: &impl IsA<gtk::Window>, request: &PermissionRequest) -> Self {
        // --- What & Why labels -----------------------------------------------

        let what_label = gtk::Label::new(None);
        what_label.set_markup(&format!(
            "AiOS wants to access: <b>{}</b>",
            glib_escape(&request.label)
        ));
        what_label.set_wrap(true);
        what_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        what_label.set_xalign(0.0);
        what_label.set_margin_bottom(4);

        let why_label = gtk::Label::new(Some(&request.reason));
        why_label.set_wrap(true);
        why_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        why_label.set_xalign(0.0);
        why_label.set_margin_bottom(16);

        // --- Permission "don't ask" row --------------------------------------

        let perm_dont_ask_check =
            gtk::CheckButton::with_label("Don't ask permission for");
        let perm_adjustment = gtk::Adjustment::new(15.0, 1.0, 120.0, 1.0, 5.0, 0.0);
        let perm_dont_ask_spin = gtk::SpinButton::new(Some(&perm_adjustment), 1.0, 0);
        perm_dont_ask_spin.set_sensitive(false);
        let perm_minutes_label = gtk::Label::new(Some("minutes"));

        let perm_row = gtk::Box::new(Orientation::Horizontal, 6);
        perm_row.set_margin_bottom(12);
        perm_row.append(&perm_dont_ask_check);
        perm_row.append(&perm_dont_ask_spin);
        perm_row.append(&perm_minutes_label);

        {
            let spin = perm_dont_ask_spin.clone();
            perm_dont_ask_check.connect_toggled(move |cb| {
                spin.set_sensitive(cb.is_active());
            });
        }

        // --- Optional auth section -------------------------------------------

        let (password_entry, auth_dont_ask_check, auth_dont_ask_spin) =
            if request.requires_auth {
                let separator = gtk::Separator::new(Orientation::Horizontal);
                separator.set_margin_top(4);
                separator.set_margin_bottom(12);

                let auth_title = gtk::Label::new(Some("Authentication required"));
                auth_title.set_xalign(0.0);
                auth_title.add_css_class("heading");
                auth_title.set_margin_bottom(8);

                let pwd = gtk::PasswordEntry::builder()
                    .placeholder_text("Password")
                    .show_peek_icon(true)
                    .hexpand(true)
                    .build();
                pwd.set_margin_bottom(8);

                let auth_check =
                    gtk::CheckButton::with_label("Don't ask password for");
                let auth_adj = gtk::Adjustment::new(15.0, 1.0, 120.0, 1.0, 5.0, 0.0);
                let auth_spin = gtk::SpinButton::new(Some(&auth_adj), 1.0, 0);
                auth_spin.set_sensitive(false);
                let auth_min_label = gtk::Label::new(Some("minutes"));

                let auth_row = gtk::Box::new(Orientation::Horizontal, 6);
                auth_row.set_margin_bottom(12);
                auth_row.append(&auth_check);
                auth_row.append(&auth_spin);
                auth_row.append(&auth_min_label);

                {
                    let s = auth_spin.clone();
                    auth_check.connect_toggled(move |cb| {
                        s.set_sensitive(cb.is_active());
                    });
                }

                // We return the widgets; they will be packed into the content box
                // below via a vec of "extra" widgets.
                (
                    Some((separator, auth_title, pwd.clone())),
                    Some(auth_check),
                    Some(auth_spin),
                )
            } else {
                (None, None, None)
            };

        // --- Error label (hidden) -------------------------------------------

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_wrap(true);
        error_label.set_xalign(0.0);
        error_label.set_margin_bottom(8);

        // --- Buttons ---------------------------------------------------------

        let deny_button = gtk::Button::with_label("Deny");
        let allow_button = gtk::Button::with_label("Allow");
        allow_button.add_css_class("suggested-action");

        let button_box = gtk::Box::new(Orientation::Horizontal, 8);
        button_box.set_halign(Align::End);
        button_box.append(&deny_button);
        button_box.append(&allow_button);

        // --- Layout ----------------------------------------------------------

        let content = gtk::Box::new(Orientation::Vertical, 0);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.append(&what_label);
        content.append(&why_label);
        content.append(&perm_row);

        // Auth section (conditional).
        let pwd_widget: Option<gtk::PasswordEntry>;
        if let Some((sep, title, pwd)) = password_entry {
            content.append(&sep);
            content.append(&title);
            content.append(&pwd);
            pwd_widget = Some(pwd);

            // Auth "don't ask" row.
            if let (Some(check), Some(spin)) =
                (&auth_dont_ask_check, &auth_dont_ask_spin)
            {
                let auth_da_row = gtk::Box::new(Orientation::Horizontal, 6);
                auth_da_row.set_margin_bottom(12);
                auth_da_row.append(check);
                auth_da_row.append(spin);
                auth_da_row.append(&gtk::Label::new(Some("minutes")));
                content.append(&auth_da_row);
            }
        } else {
            pwd_widget = None;
        }

        content.append(&error_label);
        content.append(&button_box);

        let header = adw::HeaderBar::new();
        header.set_show_end_title_buttons(false);
        header.set_show_start_title_buttons(false);

        let outer = gtk::Box::new(Orientation::Vertical, 0);
        outer.append(&header);
        outer.append(&content);

        let window = adw::Window::builder()
            .title("Permission Required")
            .transient_for(parent)
            .modal(true)
            .resizable(false)
            .default_width(460)
            .content(&outer)
            .build();

        // --- Callback storage ------------------------------------------------

        let on_grant_cb: Rc<RefCell<Option<Box<dyn Fn(PermissionGrant)>>>> =
            Rc::new(RefCell::new(None));
        let on_deny_cb: Rc<RefCell<Option<Box<dyn Fn()>>>> =
            Rc::new(RefCell::new(None));

        let dialog = Self {
            window,
            on_grant_cb,
            on_deny_cb,
            password_entry: pwd_widget,
            perm_dont_ask_check,
            perm_dont_ask_spin,
            auth_dont_ask_check,
            auth_dont_ask_spin,
            error_label,
        };

        // Wire signals.
        dialog.connect_signals(allow_button, deny_button);

        dialog
    }

    /// Register a callback invoked when the user clicks "Allow".
    pub fn on_grant(&self, callback: impl Fn(PermissionGrant) + 'static) {
        *self.on_grant_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Register a callback invoked when the user clicks "Deny".
    pub fn on_deny(&self, callback: impl Fn() + 'static) {
        *self.on_deny_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Present the dialog.
    pub fn show(&self) {
        if let Some(ref pwd) = self.password_entry {
            pwd.set_text("");
        }
        self.error_label.set_visible(false);
        self.window.present();
    }

    /// Hide (close) the dialog.
    pub fn hide(&self) {
        self.window.close();
    }

    /// Display an error (e.g. authentication failure).
    pub fn show_error(&self, msg: &str) {
        self.error_label.set_text(msg);
        self.error_label.set_visible(true);
    }

    /// Return the underlying `adw::Window`.
    pub fn widget(&self) -> &adw::Window {
        &self.window
    }

    // -- Internal signal wiring -----------------------------------------------

    fn connect_signals(&self, allow_button: gtk::Button, deny_button: gtk::Button) {
        // Allow button.
        let perm_check = self.perm_dont_ask_check.clone();
        let perm_spin = self.perm_dont_ask_spin.clone();
        let auth_check = self.auth_dont_ask_check.clone();
        let auth_spin = self.auth_dont_ask_spin.clone();
        let pwd = self.password_entry.clone();
        let cb = self.on_grant_cb.clone();
        allow_button.connect_clicked(move |_| {
            let dont_ask_permission = if perm_check.is_active() {
                let minutes = perm_spin.value() as u64;
                Some(Duration::from_secs(minutes * 60))
            } else {
                None
            };

            let auth_result = pwd.as_ref().map(|_entry| {
                let dont_ask_auth = if let (Some(ac), Some(asp)) =
                    (&auth_check, &auth_spin)
                {
                    if ac.is_active() {
                        let m = asp.value() as u64;
                        Some(Duration::from_secs(m * 60))
                    } else {
                        None
                    }
                } else {
                    None
                };

                AuthResult {
                    success: true, // will be verified by the caller
                    method: "password".to_owned(),
                    dont_ask_duration: dont_ask_auth,
                }
            });

            let grant = PermissionGrant {
                allowed: true,
                auth_result,
                dont_ask_permission_duration: dont_ask_permission,
            };

            if let Some(ref f) = *cb.borrow() {
                f(grant);
            }
        });

        // Enter key in password entry triggers Allow.
        if let Some(ref pwd) = self.password_entry {
            let btn = allow_button.clone();
            pwd.connect_activate(move |_| {
                btn.emit_clicked();
            });
        }

        // Deny button.
        let window = self.window.clone();
        let cb = self.on_deny_cb.clone();
        deny_button.connect_clicked(move |_| {
            if let Some(ref f) = *cb.borrow() {
                f();
            }
            window.close();
        });

        // Window close request.
        let cb = self.on_deny_cb.clone();
        self.window.connect_close_request(move |_| {
            if let Some(ref f) = *cb.borrow() {
                f();
            }
            gtk4::glib::Propagation::Proceed
        });
    }
}

/// Return the entered password text, if the auth section is present.
impl PermissionDialog {
    pub fn password_text(&self) -> Option<String> {
        self.password_entry
            .as_ref()
            .map(|e| e.text().to_string())
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Escape a string for safe use in Pango markup.
fn glib_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
