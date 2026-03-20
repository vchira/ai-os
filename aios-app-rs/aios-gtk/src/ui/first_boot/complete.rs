//! Completion step: summary display and finish callback.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align};
use tracing::info;

use aios_core::i18n::{t, t_fmt};

use super::{SetupConversation, SetupResult, PRETRAINED_WAKE_WORDS};

impl SetupConversation {
    // -- Step: Complete -----------------------------------------------------

    pub(super) fn show_complete(&self) {
        use aios_core::types::{BootStatus, StatusLine, MessageLevel};

        let s = self.state.borrow();
        let mut status = BootStatus::new();

        for (i, p) in s.providers.iter().enumerate() {
            let role = if i == 0 {
                t("setup.complete.primary_provider")
            } else {
                t("setup.complete.backup_provider")
            };
            let display = match p.name.as_str() {
                "claude" => "Claude".to_string(),
                "openai" => "ChatGPT".to_string(),
                other => other.to_string(),
            };
            status.add(StatusLine::new(
                &role,
                true,
                t_fmt("setup.complete.api_key_stored", &[("provider", &display)]),
            ));
        }
        let wake_display = if !s.wake_word_pretrained_id.is_empty() {
            PRETRAINED_WAKE_WORDS.iter()
                .find(|(id, _)| *id == s.wake_word_pretrained_id)
                .map(|(_, display)| display.to_string())
                .unwrap_or_else(|| s.assistant_name.clone())
        } else if !s.wake_word_custom.is_empty() {
            s.wake_word_custom.clone()
        } else {
            s.assistant_name.clone()
        };
        status.add(StatusLine::new(&t("setup.complete.assistant_name"), true, &s.assistant_name));
        status.add(StatusLine::new(&t("setup.complete.wake_word"), true, &wake_display));
        status.add(StatusLine::new(&t("setup.complete.network"), true, t_fmt("setup.complete.network_local", &[("name", &s.machine_name_custom)])));
        status.add(StatusLine::new(&t("setup.complete.master_password"), true, &t("setup.complete.master_password_set")));
        status.add(StatusLine::new(&t("setup.complete.vault"), true, &t("setup.complete.vault_created")));
        status.add(StatusLine::new(&t("setup.complete.web_channel"), true, &t("setup.complete.web_channel_url")));

        drop(s);

        // Show the INFO summary.
        self.chat_view.add_level_message(
            MessageLevel::Info,
            &format!("{}\n\n{}", t("setup.complete.title"), status.format()),
        );

        // "Start Chatting" button via a setup card (no description needed).
        let btn = gtk::Button::with_label(&t("setup.complete.button"));
        btn.add_css_class("suggested-action");
        btn.add_css_class("pill");
        btn.set_halign(Align::Start);
        btn.set_margin_top(8);

        let this = self.clone();
        btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.finish();
        });

        self.chat_view.add_setup_card(
            "emblem-default-symbolic",
            &t("setup.complete.all_set_title"),
            &t("setup.complete.all_set_description"),
            Some(btn.upcast_ref()),
        );

        self.speak(&t("setup.complete.tts"));
    }

    /// Invoke the completion callback with the accumulated setup result.
    pub(super) fn finish(&self) {
        self.chat_view.dismiss_last_card_input();
        let s = self.state.borrow();
        // Resolve the effective wake word:
        // - If a pretrained ID was chosen (non-empty), use the pretrained display name
        // - Otherwise fall back to the custom text entry value
        // - If use_same_name was never overridden and both are empty, fall back to assistant name
        let wake = if !s.wake_word_pretrained_id.is_empty() {
            // Find the display name for the pretrained ID
            PRETRAINED_WAKE_WORDS.iter()
                .find(|(id, _)| *id == s.wake_word_pretrained_id)
                .map(|(_, display)| display.to_string())
                .unwrap_or_else(|| s.assistant_name.clone())
        } else if !s.wake_word_custom.is_empty() {
            s.wake_word_custom.clone()
        } else {
            s.assistant_name.clone()
        };
        let machine = if s.use_same_name {
            s.assistant_name.to_lowercase().replace(' ', "-")
        } else {
            s.machine_name_custom.clone()
        };
        let result = SetupResult {
            providers: s.providers.clone(),
            master_password: s.master_password.clone(),
            assistant_name: s.assistant_name.clone(),
            wake_word: wake,
            machine_name: machine,
            country: s.country.clone(),
            installed_to_drive: s.install_to_drive,
        };
        drop(s);

        info!(
            "Setup conversation complete: {} provider(s)",
            result.providers.len()
        );

        if let Some(ref cb) = *self.on_complete_cb.borrow() {
            cb(result);
        }
    }
}
