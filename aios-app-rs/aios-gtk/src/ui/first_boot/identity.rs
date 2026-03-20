//! Identity steps: name the assistant, create and confirm the master password.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};

use aios_core::i18n::t;

use super::{SetupConversation, SetupStep, PRETRAINED_WAKE_WORDS};

impl SetupConversation {
    // -- Step 1d: Name Your Assistant ----------------------------------------

    pub(super) fn show_name_assistant(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Assistant name
        let name_label = gtk::Label::new(Some(&t("setup.name.label")));
        name_label.set_halign(Align::Start);
        input_box.append(&name_label);

        let name_entry = gtk::Entry::builder()
            .text(&t("setup.name.default"))
            .hexpand(true)
            .build();
        name_entry.add_css_class("setup-input");
        input_box.append(&name_entry);

        // ---------------------------------------------------------------------------
        // Wake word selection — pretrained dropdown + custom entry
        // ---------------------------------------------------------------------------

        // Section separator and label
        let wake_section_label = gtk::Label::new(Some("Wake Word"));
        wake_section_label.set_halign(Align::Start);
        wake_section_label.add_css_class("heading");
        wake_section_label.set_margin_top(12);
        input_box.append(&wake_section_label);

        // Pretrained dropdown
        let pretrained_names: Vec<&str> = PRETRAINED_WAKE_WORDS.iter()
            .map(|(_, display)| *display)
            .collect();
        // Append "Custom…" as the last entry so users can type their own phrase.
        let mut dropdown_names = pretrained_names.clone();
        dropdown_names.push("Custom\u{2026}");
        let wake_model = gtk::StringList::new(&dropdown_names);
        let wake_dropdown = gtk::DropDown::new(Some(wake_model), gtk::Expression::NONE);
        wake_dropdown.set_hexpand(true);
        input_box.append(&wake_dropdown);

        // Custom wake word entry (hidden unless "Custom…" is selected)
        let custom_box = gtk::Box::new(Orientation::Vertical, 4);
        custom_box.set_visible(false);
        custom_box.set_margin_top(4);

        let custom_wake_label = gtk::Label::new(Some(
            "Custom wake phrase \u{2014} a model will be trained on first boot (~5 min). May be less accurate.",
        ));
        custom_wake_label.set_halign(Align::Start);
        custom_wake_label.set_wrap(true);
        custom_wake_label.add_css_class("dim-label");
        custom_box.append(&custom_wake_label);

        let custom_wake_entry = gtk::Entry::builder()
            .placeholder_text("e.g. Hey Computer")
            .hexpand(true)
            .build();
        custom_wake_entry.add_css_class("setup-input");
        custom_box.append(&custom_wake_entry);

        input_box.append(&custom_box);

        // Show/hide the custom entry when "Custom…" is selected
        {
            let custom_box_ref = custom_box.clone();
            let pretrained_count = PRETRAINED_WAKE_WORDS.len() as u32;
            wake_dropdown.connect_selected_notify(move |dd| {
                // The last entry (index == pretrained_count) is "Custom…"
                custom_box_ref.set_visible(dd.selected() == pretrained_count);
            });
        }

        // ---------------------------------------------------------------------------
        // Extra fields (hidden by default) — hostname only
        // ---------------------------------------------------------------------------

        // Same-for-all toggle
        let same_check = gtk::CheckButton::with_label(
            &t("setup.name.same_toggle"),
        );
        same_check.set_active(true);
        same_check.set_margin_top(8);
        input_box.append(&same_check);

        // Extra fields (hidden by default) — hostname only
        let extra_box = gtk::Box::new(Orientation::Vertical, 6);
        extra_box.set_visible(false);
        extra_box.set_margin_top(8);

        let host_label = gtk::Label::new(Some(&t("setup.name.host_label")));
        host_label.set_halign(Align::Start);
        extra_box.append(&host_label);

        let host_entry = gtk::Entry::builder()
            .text(&t("setup.name.default_lowercase"))
            .hexpand(true)
            .build();
        host_entry.add_css_class("setup-input");
        extra_box.append(&host_entry);

        input_box.append(&extra_box);

        // Toggle visibility of extra fields
        {
            let extra_ref = extra_box.clone();
            same_check.connect_toggled(move |t| {
                extra_ref.set_visible(!t.is_active());
            });
        }

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        input_box.append(&error_label);

        let next_btn = gtk::Button::with_label(&t("setup.name.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(4);
        input_box.append(&next_btn);

        let handle = self.chat_view.add_setup_card(
            "avatar-default-symbolic",
            &t("setup.name.title"),
            &t("setup.name.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let name_ref = name_entry.clone();
        let same_ref = same_check.clone();
        let wake_dd_ref = wake_dropdown.clone();
        let custom_wake_ref = custom_wake_entry.clone();
        let host_ref = host_entry.clone();
        let error_ref = error_label.clone();
        let pretrained_count_click = PRETRAINED_WAKE_WORDS.len() as u32;
        next_btn.connect_clicked(move |b| {
            let name = name_ref.text().to_string().trim().to_string();
            if name.is_empty() {
                error_ref.set_text(&t("setup.name.error_empty"));
                error_ref.set_visible(true);
                return;
            }

            // Resolve wake word selection
            let selected_idx = wake_dd_ref.selected();
            let (pretrained_id, wake_display) = if selected_idx < pretrained_count_click {
                // A pretrained model was selected
                let (id, display) = PRETRAINED_WAKE_WORDS[selected_idx as usize];
                (id.to_string(), display.to_string())
            } else {
                // "Custom…" was selected
                let custom = custom_wake_ref.text().to_string().trim().to_string();
                if custom.is_empty() {
                    error_ref.set_text("Please enter a custom wake phrase.");
                    error_ref.set_visible(true);
                    return;
                }
                (String::new(), custom)
            };

            let use_same = same_ref.is_active();
            let machine = if use_same {
                name.to_lowercase().replace(' ', "-")
            } else {
                host_ref.text().to_string().trim().to_lowercase()
            };

            // Validate machine name
            if !aios_core::hostname::is_valid_hostname(&machine) {
                error_ref.set_text(&t("setup.name.error_hostname"));
                error_ref.set_visible(true);
                return;
            }

            b.set_sensitive(false);
            error_ref.set_visible(false);

            {
                let mut s = this.state.borrow_mut();
                s.assistant_name = name.clone();
                s.wake_word_pretrained_id = pretrained_id;
                s.wake_word_custom = wake_display;
                s.machine_name_custom = machine;
                s.use_same_name = use_same;
            }

            this.chat_view.add_message("user", &name);
            if let Some(ref h) = handle {
                h.dismiss(&name);
            }
            this.advance(SetupStep::ChooseProvider);
        });

        self.speak(&t("setup.name.tts"));
    }

    // -- Step 4: Create Password -------------------------------------------

    pub(super) fn show_create_password(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 6);
        input_box.set_margin_top(8);

        let entry = gtk::PasswordEntry::builder()
            .placeholder_text(&t("setup.password.placeholder"))
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        entry.add_css_class("setup-input");
        input_box.append(&entry);

        // Strength indicator.
        let strength_box = gtk::Box::new(Orientation::Horizontal, 6);
        strength_box.set_margin_top(2);
        let strength_label = gtk::Label::new(Some(&t("setup.password.strength")));
        strength_label.add_css_class("dim-label");
        strength_box.append(&strength_label);

        let strength_bar = gtk::LevelBar::builder()
            .min_value(0.0)
            .max_value(4.0)
            .value(0.0)
            .hexpand(true)
            .build();
        strength_box.append(&strength_bar);
        input_box.append(&strength_box);

        // Update strength as user types.
        {
            let bar = strength_bar.clone();
            entry.connect_changed(move |e| {
                let text = e.text();
                let score = password_strength_score(&text);
                bar.set_value(score);
            });
        }

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        input_box.append(&error_label);

        let next_btn = gtk::Button::with_label(&t("setup.password.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(4);
        input_box.append(&next_btn);

        {
            let this = self.clone();
            let entry_ref = entry.clone();
            let error_ref = error_label.clone();
            next_btn.connect_clicked(move |b| {
                let password = entry_ref.text().to_string();
                if password.len() < 8 {
                    error_ref.set_text(&t("setup.password.error_short"));
                    error_ref.set_visible(true);
                    return;
                }
                b.set_sensitive(false);
                error_ref.set_visible(false);

                {
                    let mut s = this.state.borrow_mut();
                    s.pending_password = password;
                }

                this.chat_view.add_message("user", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
                this.advance(SetupStep::ConfirmPassword);
            });
        }

        {
            let this = self.clone();
            let entry_ref = entry.clone();
            let error_ref = error_label.clone();
            entry.connect_activate(move |_| {
                let password = entry_ref.text().to_string();
                if password.len() < 8 {
                    error_ref.set_text(&t("setup.password.error_short"));
                    error_ref.set_visible(true);
                    return;
                }
                error_ref.set_visible(false);

                {
                    let mut s = this.state.borrow_mut();
                    s.pending_password = password;
                }

                this.chat_view.add_message("user", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
                this.advance(SetupStep::ConfirmPassword);
            });
        }

        self.chat_view.add_setup_card(
            "channel-secure-symbolic",
            &t("setup.password.title"),
            &t("setup.password.description"),
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.password.tts"));

        let entry_focus = entry.clone();
        gtk4::glib::idle_add_local_once(move || {
            entry_focus.grab_focus();
        });
    }

    // -- Step 5: Confirm Password ------------------------------------------

    pub(super) fn show_confirm_password(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 6);
        input_box.set_margin_top(8);

        let entry = gtk::PasswordEntry::builder()
            .placeholder_text(&t("setup.confirm_password.placeholder"))
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        entry.add_css_class("setup-input");
        input_box.append(&entry);

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        input_box.append(&error_label);

        let next_btn = gtk::Button::with_label(&t("setup.confirm_password.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(4);
        input_box.append(&next_btn);

        {
            let this = self.clone();
            let entry_ref = entry.clone();
            let error_ref = error_label.clone();
            next_btn.connect_clicked(move |b| {
                let confirm = entry_ref.text().to_string();
                let pending = this.state.borrow().pending_password.clone();

                if confirm != pending {
                    error_ref.set_text(&t("setup.confirm_password.error_mismatch"));
                    error_ref.set_visible(true);
                    return;
                }
                b.set_sensitive(false);
                error_ref.set_visible(false);

                {
                    let mut s = this.state.borrow_mut();
                    s.master_password = s.pending_password.clone();
                    s.pending_password.clear();
                }

                this.chat_view.add_message("user", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
                this.advance(SetupStep::AddBackup);
            });
        }

        {
            let this = self.clone();
            let entry_ref = entry.clone();
            let error_ref = error_label.clone();
            entry.connect_activate(move |_| {
                let confirm = entry_ref.text().to_string();
                let pending = this.state.borrow().pending_password.clone();

                if confirm != pending {
                    error_ref.set_text(&t("setup.confirm_password.error_mismatch"));
                    error_ref.set_visible(true);
                    return;
                }
                error_ref.set_visible(false);

                {
                    let mut s = this.state.borrow_mut();
                    s.master_password = s.pending_password.clone();
                    s.pending_password.clear();
                }

                this.chat_view.add_message("user", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
                this.advance(SetupStep::AddBackup);
            });
        }

        self.chat_view.add_setup_card(
            "emblem-ok-symbolic",
            &t("setup.confirm_password.title"),
            &t("setup.confirm_password.description"),
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.confirm_password.tts"));

        let entry_focus = entry.clone();
        gtk4::glib::idle_add_local_once(move || {
            entry_focus.grab_focus();
        });
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Simple password strength score (0.0 to 4.0) based on length and
/// character class diversity.
fn password_strength_score(password: &str) -> f64 {
    if password.is_empty() {
        return 0.0;
    }

    let len = password.len();
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| !c.is_alphanumeric());

    let classes = [has_lower, has_upper, has_digit, has_special]
        .iter()
        .filter(|&&b| b)
        .count();

    let length_score = if len >= 16 {
        2.0
    } else if len >= 12 {
        1.5
    } else if len >= 8 {
        1.0
    } else {
        0.5
    };

    let class_score = classes as f64 * 0.5;

    (length_score + class_score).min(4.0)
}
