//! GTK panel renderer — renders [`PanelRequest`] as a modal dialog.
//!
//! The [`PanelRenderer`] creates an `adw::Window` for each panel request,
//! builds the appropriate GTK widgets for every field type, and collects
//! the user's responses when they click Submit or Cancel.
//!
//! This module is the bridge between the UI-agnostic [`UiPanelTool`] in
//! `aios-tools` and the GTK4/libadwaita frontend.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};
use libadwaita as adw;
use libadwaita::prelude::AdwWindowExt;

use aios_tools::builtin::ui_panel::{
    FieldType, PanelField, PanelRequest, PanelResponse,
};

// ---------------------------------------------------------------------------
// CSS for panel styling
// ---------------------------------------------------------------------------

const PANEL_CSS: &str = r#"
.panel-window {
    background-color: @window_bg_color;
}

.panel-header {
    padding: 4px 0;
}

.panel-icon {
    color: @accent_bg_color;
    min-width: 32px;
    min-height: 32px;
}

.panel-title {
    font-size: 1.2em;
    font-weight: bold;
}

.panel-description {
    color: alpha(@window_fg_color, 0.7);
}

.panel-field-label {
    font-weight: 600;
    margin-top: 8px;
}

.panel-required-star {
    color: @error_bg_color;
    font-weight: bold;
}

/* Secure field — orange/red border + badge */
.panel-field-secure {
    border-left: 3px solid #e5534b;
    padding-left: 8px;
    margin-top: 4px;
}

.panel-secure-badge {
    color: #e5534b;
    font-size: 0.75em;
    font-weight: 700;
    margin-left: 8px;
    padding: 1px 6px;
    border: 1px solid #e5534b;
    border-radius: 4px;
}

/* Private field — blue border + badge */
.panel-field-private {
    border-left: 3px solid #3584e4;
    padding-left: 8px;
    margin-top: 4px;
}

.panel-private-badge {
    color: #3584e4;
    font-size: 0.75em;
    font-weight: 700;
    margin-left: 8px;
    padding: 1px 6px;
    border: 1px solid #3584e4;
    border-radius: 4px;
}

.panel-choice-card {
    padding: 10px 14px;
    border-radius: 10px;
    background-color: alpha(@window_fg_color, 0.04);
    border: 1px solid alpha(@window_fg_color, 0.1);
}

.panel-choice-card:checked {
    background-color: alpha(@accent_bg_color, 0.15);
    border-color: @accent_bg_color;
}

.panel-choice-label {
    font-weight: 600;
}

.panel-choice-description {
    color: alpha(@window_fg_color, 0.6);
    font-size: 0.9em;
}

.panel-multiline {
    min-height: 100px;
    border-radius: 8px;
    border: 1px solid alpha(@window_fg_color, 0.15);
    padding: 8px;
}

.panel-error {
    color: @error_bg_color;
    font-size: 0.85em;
}

.panel-button-row {
    margin-top: 12px;
}
"#;

// ---------------------------------------------------------------------------
// PanelRenderer
// ---------------------------------------------------------------------------

/// Renders a [`PanelRequest`] as a GTK modal dialog and collects user input.
pub struct PanelRenderer;

impl PanelRenderer {
    /// Show a panel dialog on the GTK main thread and block the calling
    /// (worker) thread until the user responds.
    ///
    /// This function:
    /// 1. Creates an `std::sync::mpsc` channel.
    /// 2. Schedules the panel creation on the GTK main thread via
    ///    `glib::idle_add_local_once`.
    /// 3. Blocks on `channel.recv()` waiting for the user's response.
    /// 4. Returns the response.
    ///
    /// # Arguments
    ///
    /// * `request` — The panel definition (title, fields, etc.).
    /// * `window` — The parent window for modality. GTK widget clones are
    ///   cheap reference-counted pointers.
    pub fn show_panel_blocking(
        request: PanelRequest,
        window: gtk::Window,
    ) -> Option<PanelResponse> {
        let (tx, rx) = std::sync::mpsc::channel::<Option<PanelResponse>>();

        // Schedule panel creation on the GTK main thread.
        gtk4::glib::idle_add_local_once(move || {
            Self::build_and_show_panel(request, &window, tx);
        });

        // Block the worker thread until the UI responds.
        match rx.recv() {
            Ok(response) => response,
            Err(_) => None, // Channel disconnected — treat as cancelled.
        }
    }

    /// Build and display the panel dialog. Called on the GTK main thread.
    ///
    /// When the user clicks Submit or Cancel, the response is sent through
    /// `tx`.  This is public so that `app.rs` can call it from a glib
    /// channel receiver.
    pub fn build_and_show_panel(
        request: PanelRequest,
        parent: &gtk::Window,
        tx: std::sync::mpsc::Sender<Option<PanelResponse>>,
    ) {
        // Load panel CSS.
        let css_provider = gtk::CssProvider::new();
        #[allow(deprecated)]
        css_provider.load_from_data(PANEL_CSS);
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("Could not get default display"),
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // Create the dialog window.
        let dialog = adw::Window::builder()
            .modal(true)
            .default_width(480)
            .resizable(true)
            .build();
        dialog.set_transient_for(Some(parent));
        dialog.add_css_class("panel-window");

        // Outer vertical box: header bar + scrolled content.
        let outer_box = gtk::Box::new(Orientation::Vertical, 0);

        let header = adw::HeaderBar::new();
        header.set_show_end_title_buttons(true);
        header.set_show_start_title_buttons(false);
        header.set_title_widget(Some(&gtk::Label::new(Some(&request.title))));
        outer_box.append(&header);

        // Scrollable content area.
        let scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(600)
            .build();

        let content = gtk::Box::new(Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(24);
        content.set_margin_end(24);

        // Icon + title header.
        let header_box = gtk::Box::new(Orientation::Horizontal, 12);
        header_box.add_css_class("panel-header");

        if let Some(ref icon_name) = request.icon {
            let icon = gtk::Image::from_icon_name(icon_name);
            icon.set_pixel_size(32);
            icon.add_css_class("panel-icon");
            header_box.append(&icon);
        }

        let title_label = gtk::Label::new(Some(&request.title));
        title_label.add_css_class("panel-title");
        title_label.set_halign(Align::Start);
        title_label.set_hexpand(true);
        title_label.set_wrap(true);
        header_box.append(&title_label);
        content.append(&header_box);

        // Description.
        if let Some(ref desc) = request.description {
            let desc_label = gtk::Label::new(Some(desc));
            desc_label.add_css_class("panel-description");
            desc_label.set_halign(Align::Start);
            desc_label.set_xalign(0.0);
            desc_label.set_wrap(true);
            desc_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            content.append(&desc_label);
        }

        // Build field widgets and collect value getters.
        let field_getters: Rc<RefCell<Vec<FieldGetter>>> = Rc::new(RefCell::new(Vec::new()));

        for field in &request.fields {
            let (widget, getter) = Self::build_field(field);
            content.append(&widget);
            field_getters.borrow_mut().push(getter);
        }

        // Error label (hidden initially).
        let error_label = gtk::Label::new(None);
        error_label.add_css_class("panel-error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        content.append(&error_label);

        // Button row.
        let button_row = gtk::Box::new(Orientation::Horizontal, 8);
        button_row.add_css_class("panel-button-row");
        button_row.set_halign(Align::End);
        button_row.set_hexpand(true);

        let cancel_btn = gtk::Button::with_label("Cancel");
        button_row.append(&cancel_btn);

        let submit_btn = gtk::Button::with_label("Submit");
        submit_btn.add_css_class("suggested-action");
        button_row.append(&submit_btn);

        content.append(&button_row);

        scrolled.set_child(Some(&content));
        outer_box.append(&scrolled);
        dialog.set_content(Some(&outer_box));

        // --- Connect signals ---

        // Cancel button.
        let tx_cancel = tx.clone();
        let dialog_cancel = dialog.clone();
        cancel_btn.connect_clicked(move |_| {
            let _ = tx_cancel.send(Some(PanelResponse {
                values: serde_json::Map::new(),
                cancelled: true,
            }));
            dialog_cancel.close();
        });

        // Window close (X button or Escape).
        let tx_close = tx.clone();
        dialog.connect_close_request(move |_| {
            // Try to send — if already sent (via Submit/Cancel), this is a no-op.
            let _ = tx_close.send(Some(PanelResponse {
                values: serde_json::Map::new(),
                cancelled: true,
            }));
            gtk4::glib::Propagation::Proceed
        });

        // Submit button.
        let fields_for_validation = request.fields.clone();
        let dialog_submit = dialog.clone();
        submit_btn.connect_clicked(move |_| {
            let getters = field_getters.borrow();
            let mut values = serde_json::Map::new();

            // Collect values and validate required fields.
            for (i, getter) in getters.iter().enumerate() {
                let field_def = &fields_for_validation[i];
                let value = (getter.get_value)();

                if field_def.required && is_empty_value(&value) {
                    error_label.set_text(&format!("'{}' is required.", field_def.label));
                    error_label.set_visible(true);
                    return;
                }

                values.insert(field_def.id.clone(), value);
            }

            error_label.set_visible(false);

            let _ = tx.send(Some(PanelResponse {
                values,
                cancelled: false,
            }));
            dialog_submit.close();
        });

        dialog.present();
    }

    // -----------------------------------------------------------------------
    // Field builders
    // -----------------------------------------------------------------------

    /// Build a GTK widget and value getter for a single field.
    ///
    /// Returns a container widget (label + input) and a [`FieldGetter`] that
    /// can retrieve the current value as `serde_json::Value`.
    fn build_field(field: &PanelField) -> (gtk::Widget, FieldGetter) {
        let container = gtk::Box::new(Orientation::Vertical, 4);

        // Label row (with optional required star).
        let label_box = gtk::Box::new(Orientation::Horizontal, 4);
        let label = gtk::Label::new(Some(&field.label));
        label.add_css_class("panel-field-label");
        label.set_halign(Align::Start);
        label_box.append(&label);

        if field.required {
            let star = gtk::Label::new(Some("*"));
            star.add_css_class("panel-required-star");
            label_box.append(&star);
        }

        // Protection badge — visual indicator for secure/private fields.
        use aios_tools::builtin::ui_panel::FieldProtection;
        match &field.protection {
            FieldProtection::Secure => {
                let badge = gtk::Label::new(Some("\u{1f512} SECURE"));
                badge.add_css_class("panel-secure-badge");
                badge.set_tooltip_text(Some(
                    "This value is encrypted and never visible to the AI or in logs"
                ));
                label_box.append(&badge);
                container.add_css_class("panel-field-secure");
            }
            FieldProtection::Private => {
                let badge = gtk::Label::new(Some("\u{1f464} PRIVATE"));
                badge.add_css_class("panel-private-badge");
                badge.set_tooltip_text(Some(
                    "This value is stored privately. The AI cannot read it directly."
                ));
                label_box.append(&badge);
                container.add_css_class("panel-field-private");
            }
            FieldProtection::None => {}
        }

        container.append(&label_box);

        // Build the actual input widget based on field type.
        let getter = match field.field_type {
            FieldType::Text => Self::build_text_field(&container, field),
            FieldType::Password => Self::build_password_field(&container, field),
            FieldType::Number => Self::build_number_field(&container, field),
            FieldType::Date => Self::build_date_field(&container, field),
            FieldType::Choice => Self::build_choice_field(&container, field),
            FieldType::Dropdown => Self::build_dropdown_field(&container, field),
            FieldType::Toggle => Self::build_toggle_field(&container, field),
            FieldType::Multiline => Self::build_multiline_field(&container, field),
        };

        (container.upcast(), getter)
    }

    /// Build a single-line text entry.
    fn build_text_field(container: &gtk::Box, field: &PanelField) -> FieldGetter {
        let entry = gtk::Entry::new();
        if let Some(ref ph) = field.placeholder {
            entry.set_placeholder_text(Some(ph));
        }
        if let Some(ref default) = field.default_value {
            if let Some(s) = default.as_str() {
                entry.set_text(s);
            }
        }
        entry.set_hexpand(true);
        container.append(&entry);

        let entry_ref = entry.clone();
        FieldGetter {
            get_value: Box::new(move || {
                let text = entry_ref.text().to_string();
                serde_json::Value::String(text)
            }),
        }
    }

    /// Build a password entry (masked input).
    fn build_password_field(container: &gtk::Box, field: &PanelField) -> FieldGetter {
        let entry = gtk::PasswordEntry::builder()
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        if let Some(ref ph) = field.placeholder {
            entry.set_placeholder_text(Some(ph));
        }
        container.append(&entry);

        let entry_ref = entry.clone();
        FieldGetter {
            get_value: Box::new(move || {
                let text = entry_ref.text().to_string();
                serde_json::Value::String(text)
            }),
        }
    }

    /// Build a numeric spin button.
    fn build_number_field(container: &gtk::Box, field: &PanelField) -> FieldGetter {
        let min = field.min.unwrap_or(0.0);
        let max = field.max.unwrap_or(999999.0);
        let step = field.step.unwrap_or(1.0);
        let default = field
            .default_value
            .as_ref()
            .and_then(|v| v.as_f64())
            .unwrap_or(min);

        let adjustment = gtk::Adjustment::new(default, min, max, step, step * 10.0, 0.0);
        let spin = gtk::SpinButton::new(Some(&adjustment), step, 2);
        spin.set_hexpand(true);
        container.append(&spin);

        let spin_ref = spin.clone();
        FieldGetter {
            get_value: Box::new(move || {
                let val = spin_ref.value();
                serde_json::json!(val)
            }),
        }
    }

    /// Build a date entry (simple text with YYYY-MM-DD placeholder).
    fn build_date_field(container: &gtk::Box, field: &PanelField) -> FieldGetter {
        let entry = gtk::Entry::new();
        let placeholder = field
            .placeholder
            .as_deref()
            .unwrap_or("YYYY-MM-DD");
        entry.set_placeholder_text(Some(placeholder));
        if let Some(ref default) = field.default_value {
            if let Some(s) = default.as_str() {
                entry.set_text(s);
            }
        }
        entry.set_hexpand(true);
        container.append(&entry);

        let entry_ref = entry.clone();
        FieldGetter {
            get_value: Box::new(move || {
                let text = entry_ref.text().to_string();
                serde_json::Value::String(text)
            }),
        }
    }

    /// Build radio-style choice buttons (styled cards with optional descriptions).
    fn build_choice_field(container: &gtk::Box, field: &PanelField) -> FieldGetter {
        let options_box = gtk::Box::new(Orientation::Vertical, 6);
        options_box.set_margin_top(4);

        let selected: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        // Set default if provided.
        if let Some(ref default) = field.default_value {
            if let Some(s) = default.as_str() {
                *selected.borrow_mut() = Some(s.to_string());
            }
        }

        // Create a group for radio behavior using CheckButton.
        let mut group_member: Option<gtk::CheckButton> = None;

        for option in &field.options {
            let radio = gtk::CheckButton::new();

            // Join the radio group.
            if let Some(ref group) = group_member {
                radio.set_group(Some(group));
            } else {
                group_member = Some(radio.clone());
            }

            // Set active if this is the default.
            if selected.borrow().as_deref() == Some(&option.value) {
                radio.set_active(true);
            }

            // Build the card content: label + optional description.
            let card_content = gtk::Box::new(Orientation::Vertical, 2);
            card_content.set_margin_top(6);
            card_content.set_margin_bottom(6);
            card_content.set_margin_start(8);
            card_content.set_margin_end(8);

            let name_label = gtk::Label::new(Some(&option.label));
            name_label.add_css_class("panel-choice-label");
            name_label.set_halign(Align::Start);
            card_content.append(&name_label);

            if let Some(ref desc) = option.description {
                let desc_label = gtk::Label::new(Some(desc));
                desc_label.add_css_class("panel-choice-description");
                desc_label.set_halign(Align::Start);
                desc_label.set_wrap(true);
                desc_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                card_content.append(&desc_label);
            }

            // Wrap in a clickable card.
            let card = gtk::Box::new(Orientation::Horizontal, 10);
            card.add_css_class("panel-choice-card");
            card.set_margin_start(0);
            card.set_margin_end(0);
            card.append(&radio);
            card.append(&card_content);

            // Make the entire card clickable.
            let radio_ref = radio.clone();
            let click = gtk::GestureClick::new();
            click.connect_released(move |_, _, _, _| {
                radio_ref.set_active(true);
            });
            card.add_controller(click);

            // Track selection.
            let selected_ref = selected.clone();
            let value = option.value.clone();
            radio.connect_toggled(move |rb| {
                if rb.is_active() {
                    *selected_ref.borrow_mut() = Some(value.clone());
                }
            });

            options_box.append(&card);
        }

        container.append(&options_box);

        let selected_for_getter = selected.clone();
        FieldGetter {
            get_value: Box::new(move || {
                match selected_for_getter.borrow().as_deref() {
                    Some(val) => serde_json::Value::String(val.to_string()),
                    None => serde_json::Value::Null,
                }
            }),
        }
    }

    /// Build a dropdown (combo box).
    fn build_dropdown_field(container: &gtk::Box, field: &PanelField) -> FieldGetter {
        let labels: Vec<&str> = field.options.iter().map(|o| o.label.as_str()).collect();
        let string_list = gtk::StringList::new(&labels);
        let dropdown = gtk::DropDown::new(Some(string_list), gtk::Expression::NONE);
        dropdown.set_hexpand(true);

        // Set default selection.
        if let Some(ref default) = field.default_value {
            if let Some(default_val) = default.as_str() {
                for (i, opt) in field.options.iter().enumerate() {
                    if opt.value == default_val {
                        dropdown.set_selected(i as u32);
                        break;
                    }
                }
            }
        }

        container.append(&dropdown);

        let options = field.options.clone();
        let dropdown_ref = dropdown.clone();
        FieldGetter {
            get_value: Box::new(move || {
                let idx = dropdown_ref.selected() as usize;
                if idx < options.len() {
                    serde_json::Value::String(options[idx].value.clone())
                } else {
                    serde_json::Value::Null
                }
            }),
        }
    }

    /// Build a toggle switch.
    fn build_toggle_field(container: &gtk::Box, field: &PanelField) -> FieldGetter {
        let toggle_box = gtk::Box::new(Orientation::Horizontal, 12);
        toggle_box.set_margin_top(4);

        let switch = gtk::Switch::new();
        switch.set_halign(Align::Start);

        // Set default.
        if let Some(ref default) = field.default_value {
            if let Some(b) = default.as_bool() {
                switch.set_active(b);
            }
        }

        toggle_box.append(&switch);
        container.append(&toggle_box);

        let switch_ref = switch.clone();
        FieldGetter {
            get_value: Box::new(move || {
                serde_json::Value::Bool(switch_ref.is_active())
            }),
        }
    }

    /// Build a multiline text area.
    fn build_multiline_field(container: &gtk::Box, field: &PanelField) -> FieldGetter {
        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(false)
            .min_content_height(100)
            .max_content_height(300)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scrolled.add_css_class("panel-multiline");

        let text_view = gtk::TextView::new();
        text_view.set_wrap_mode(gtk::WrapMode::WordChar);
        text_view.set_top_margin(4);
        text_view.set_bottom_margin(4);
        text_view.set_left_margin(4);
        text_view.set_right_margin(4);

        if let Some(ref default) = field.default_value {
            if let Some(s) = default.as_str() {
                text_view.buffer().set_text(s);
            }
        }

        scrolled.set_child(Some(&text_view));
        container.append(&scrolled);

        let buffer = text_view.buffer();
        FieldGetter {
            get_value: Box::new(move || {
                let start = buffer.start_iter();
                let end = buffer.end_iter();
                let text = buffer.text(&start, &end, false).to_string();
                serde_json::Value::String(text)
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// FieldGetter
// ---------------------------------------------------------------------------

/// Holds a closure that retrieves the current value from a field widget.
struct FieldGetter {
    get_value: Box<dyn Fn() -> serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check whether a JSON value should be considered "empty" for required field
/// validation.
fn is_empty_value(val: &serde_json::Value) -> bool {
    match val {
        serde_json::Value::Null => true,
        serde_json::Value::String(s) => s.trim().is_empty(),
        _ => false,
    }
}
