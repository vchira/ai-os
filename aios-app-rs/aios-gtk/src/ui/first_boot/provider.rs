//! Provider selection, API key entry, backup provider, and provider ordering steps.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};

use aios_core::i18n::{t, t_fmt};

use super::{ProviderSetup, SetupConversation, SetupStep};

impl SetupConversation {
    // -- Step 2: Choose Provider --------------------------------------------

    pub(super) fn show_choose_provider(&self) {
        use crate::providers::PROVIDERS;

        // If only one provider has a key in config, auto-select it.
        if let Some(ref config) = *self.config.borrow() {
            let configured: Vec<&str> = PROVIDERS
                .iter()
                .filter(|p| p.needs_api_key && {
                    let k = config.get_str(p.api_key_config, "");
                    !k.is_empty() && k != "your-api-key-here"
                })
                .map(|p| p.id)
                .collect();
            if configured.len() == 1 {
                let id = configured[0];
                let display = crate::providers::find_by_id(id)
                    .map(|p| p.display_name)
                    .unwrap_or(id);
                self.chat_view.add_message("system",
                    &format!("Auto-selected {display} (API key found in config)"));
                self.select_provider(id);
                return;
            }
        }

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Build dropdown with all providers that need API keys (exclude Ollama).
        let provider_names: Vec<String> = PROVIDERS
            .iter()
            .filter(|p| p.needs_api_key)
            .map(|p| format!("{} — {}", p.display_name, Self::provider_description(p.id)))
            .collect();
        let provider_ids: Vec<&str> = PROVIDERS
            .iter()
            .filter(|p| p.needs_api_key)
            .map(|p| p.id)
            .collect();
        let name_refs: Vec<&str> = provider_names.iter().map(|s| s.as_str()).collect();

        let dropdown_list = gtk::StringList::new(&name_refs);
        let dropdown = gtk::DropDown::new(Some(dropdown_list), gtk::Expression::NONE);
        dropdown.set_hexpand(true);
        input_box.append(&dropdown);

        let select_btn = gtk::Button::with_label("Select");
        select_btn.add_css_class("suggested-action");
        select_btn.set_halign(Align::End);
        input_box.append(&select_btn);

        let handle = self.chat_view.add_setup_card(
            "network-server-symbolic",
            &t("setup.provider.title"),
            &t("setup.provider.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let ids = provider_ids.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        select_btn.connect_clicked(move |_| {
            let sel = dropdown.selected() as usize;
            if let Some(id) = ids.get(sel) {
                let display = crate::providers::find_by_id(id)
                    .map(|p| p.display_name)
                    .unwrap_or(id.as_str());
                if let Some(ref h) = handle { h.dismiss(display); }
                this.select_provider(id);
            }
        });

        self.speak(&t("setup.provider.tts"));
    }

    /// Short description for each provider (used in the setup dropdown).
    pub(super) fn provider_description(id: &str) -> &'static str {
        match id {
            "claude" => "Advanced reasoning and analysis",
            "openai" => "GPT-4o with broad general knowledge",
            "deepseek" => "Strong reasoning, very affordable",
            "mistral" => "European AI, fast and multilingual",
            "groq" => "Ultra-fast inference (Llama, Mixtral)",
            "gemini" => "Google AI with large context window",
            _ => "LLM provider",
        }
    }

    /// Handle provider selection (from button click or voice).
    pub(super) fn select_provider(&self, provider: &str) {
        {
            let mut s = self.state.borrow_mut();
            s.primary_provider = provider.to_owned();
        }

        // Show a user-style confirmation message.
        let display = crate::providers::find_by_id(provider)
            .map(|p| p.display_name.to_string())
            .unwrap_or_else(|| provider.to_string());
        self.chat_view.add_message("user", &display);

        let next = SetupStep::EnterApiKey {
            provider: provider.to_owned(),
        };
        self.advance_with_choice(next, Some(&display));
    }

    // -- Step 3 / Step 7: Enter API Key ------------------------------------

    pub(super) fn show_enter_api_key(&self, provider: String, is_backup: bool) {
        let display_name = crate::providers::find_by_id(&provider)
            .map(|p| p.display_name)
            .unwrap_or(provider.as_str());
        let (title, tutorial) = match provider.as_str() {
            "claude" => (
                t("setup.api_key.title_claude"),
                t("setup.api_key.tutorial_claude"),
            ),
            "openai" => (
                t("setup.api_key.title_chatgpt"),
                t("setup.api_key.tutorial_chatgpt"),
            ),
            _ => (
                t_fmt("setup.api_key.title_generic", &[("provider", display_name)]),
                t("setup.api_key.tutorial_generic"),
            ),
        };

        let input_box = gtk::Box::new(Orientation::Vertical, 6);
        input_box.set_margin_top(8);

        let entry = gtk::PasswordEntry::builder()
            .placeholder_text(&t("setup.api_key.placeholder"))
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        entry.add_css_class("setup-input");

        // Pre-fill from config if an API key already exists (using PROVIDERS lookup).
        if let Some(ref config) = *self.config.borrow() {
            if let Some(prov_def) = crate::providers::find_by_id(&provider) {
                if prov_def.needs_api_key && !prov_def.api_key_config.is_empty() {
                    let existing = config.get_str(prov_def.api_key_config, "");
                    if !existing.is_empty() && existing != "your-api-key-here" {
                        entry.set_text(&existing);
                    }
                }
            }
        }

        input_box.append(&entry);

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        input_box.append(&error_label);

        let next_btn = gtk::Button::with_label(&t("setup.api_key.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(4);
        input_box.append(&next_btn);

        let this = self.clone();
        let entry_ref = entry.clone();
        let error_ref = error_label.clone();
        let provider_clone = provider.clone();
        next_btn.connect_clicked(move |b| {
            let api_key = entry_ref.text().to_string().trim().to_owned();
            if api_key.is_empty() {
                error_ref.set_text(&t("setup.api_key.error_empty"));
                error_ref.set_visible(true);
                return;
            }
            b.set_sensitive(false);
            error_ref.set_visible(false);

            this.store_api_key(&provider_clone, api_key, is_backup);
        });

        // Allow Enter key to submit.
        let this = self.clone();
        let entry_ref2 = entry.clone();
        let error_ref2 = error_label.clone();
        let provider_clone2 = provider.clone();
        entry.connect_activate(move |_| {
            let api_key = entry_ref2.text().to_string().trim().to_owned();
            if api_key.is_empty() {
                error_ref2.set_text(&t("setup.api_key.error_empty"));
                error_ref2.set_visible(true);
                return;
            }
            error_ref2.set_visible(false);

            this.store_api_key(&provider_clone2, api_key, is_backup);
        });

        self.chat_view.add_setup_card(
            "dialog-password-symbolic",
            &title,
            &tutorial,
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.api_key.tts"));

        // Focus the entry after a brief delay so the card is rendered.
        let entry_focus = entry.clone();
        gtk4::glib::idle_add_local_once(move || {
            entry_focus.grab_focus();
        });
    }

    /// Store an API key and advance to the next step.
    pub(super) fn store_api_key(&self, provider: &str, api_key: String, is_backup: bool) {
        // Show masked key as user response.
        let masked = if api_key.len() > 8 {
            format!(
                "{}...{}",
                &api_key[..4],
                &api_key[api_key.len() - 4..]
            )
        } else {
            t("setup.api_key.masked_fallback")
        };
        self.chat_view
            .add_message("user", &t_fmt("setup.api_key.user_masked", &[("masked", &masked)]));

        {
            let mut s = self.state.borrow_mut();
            s.providers.push(ProviderSetup {
                name: provider.to_owned(),
                api_key,
            });
        }

        if is_backup {
            // Backup key entered — go to provider order.
            self.advance(SetupStep::ProviderOrder);
        } else {
            // Primary key entered — go to create password.
            self.advance(SetupStep::CreatePassword);
        }
    }

    // -- Step 6: Add Backup Provider? --------------------------------------

    pub(super) fn show_add_backup(&self) {
        let primary = self.state.borrow().primary_provider.clone();
        let other_name = if primary == "claude" {
            t("setup.provider.chatgpt_name")
        } else {
            t("setup.provider.claude_name")
        };

        // Always show the backup provider question — the user can enter
        // the key manually if one isn't pre-configured.

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let yes_btn = gtk::Button::with_label(&t_fmt("setup.backup.yes", &[("provider", &other_name)]));
        yes_btn.add_css_class("suggested-action");
        yes_btn.set_halign(Align::Start);

        let this = self.clone();
        yes_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.handle_add_backup(true);
        });
        input_box.append(&yes_btn);

        let no_btn = gtk::Button::with_label(&t("setup.backup.no"));
        no_btn.set_halign(Align::Start);

        let this = self.clone();
        no_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.handle_add_backup(false);
        });
        input_box.append(&no_btn);

        self.chat_view.add_setup_card(
            "list-add-symbolic",
            &t("setup.backup.title"),
            &t("setup.backup.description"),
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.backup.tts"));
    }

    /// Handle the add-backup decision (from button or voice).
    pub(super) fn handle_add_backup(&self, add: bool) {
        if add {
            let primary = self.state.borrow().primary_provider.clone();
            let backup = if primary == "claude" {
                "openai"
            } else {
                "claude"
            };

            let display = if backup == "claude" {
                t("setup.provider.claude_name")
            } else {
                t("setup.provider.chatgpt_name")
            };
            self.chat_view.add_message("user",
                &t_fmt("setup.backup.user_yes", &[("provider", &display)]));

            let dismiss = t_fmt("setup.backup.dismiss_add", &[("provider", &display)]);
            self.advance_with_choice(SetupStep::EnterBackupKey {
                provider: backup.to_owned(),
            }, Some(&dismiss));
        } else {
            self.chat_view.add_message("user", &t("setup.backup.user_no"));
            let installing = self.state.borrow().install_to_drive;
            if installing {
                self.advance_with_choice(SetupStep::InstallConfirm, Some(&t("setup.backup.dismiss_none")));
            } else {
                self.advance_with_choice(SetupStep::Complete, Some(&t("setup.backup.dismiss_none")));
            }
        }
    }

    // -- Step 8: Provider Order --------------------------------------------

    pub(super) fn show_provider_order(&self) {
        let providers = self.state.borrow().providers.clone();
        if providers.len() < 2 {
            // Only one provider — skip ordering.
            self.advance_to_complete_or_install();
            return;
        }

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        for p in &providers {
            let display = match p.name.as_str() {
                "claude" => t("setup.provider.claude_name"),
                "openai" => t("setup.provider.chatgpt_name"),
                other => other.to_string(),
            };

            let btn = gtk::Button::with_label(&t_fmt("setup.order.primary", &[("provider", &display)]));
            btn.add_css_class("setup-provider-button");
            btn.set_halign(Align::Start);

            let this = self.clone();
            let name = p.name.clone();
            btn.connect_clicked(move |b| {
                b.set_sensitive(false);
                this.set_primary_order(&name);
            });
            input_box.append(&btn);
        }

        self.chat_view.add_setup_card(
            "view-sort-descending-symbolic",
            &t("setup.order.title"),
            &t("setup.order.description"),
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.order.tts"));
    }

    /// Set the primary provider order and advance.
    pub(super) fn set_primary_order(&self, primary: &str) {
        {
            let mut s = self.state.borrow_mut();
            s.providers.sort_by(|a, _b| {
                if a.name == primary {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            });
        }

        let display = match primary {
            "claude" => t("setup.provider.claude_name"),
            "openai" => t("setup.provider.chatgpt_name"),
            other => other.to_string(),
        };
        self.chat_view
            .add_message("user", &t_fmt("setup.order.primary", &[("provider", &display)]));

        self.advance_to_complete_or_install();
    }

    /// Advance to `InstallConfirm` if the user chose to install, otherwise `Complete`.
    pub(super) fn advance_to_complete_or_install(&self) {
        let installing = self.state.borrow().install_to_drive;
        if installing {
            self.advance(SetupStep::InstallConfirm);
        } else {
            self.advance(SetupStep::Complete);
        }
    }
}
