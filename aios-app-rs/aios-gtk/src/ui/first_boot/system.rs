//! System installation steps: country selection, install decision, drive
//! selection, partition plan, install confirmation, and install progress.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};
use tracing::info;

use aios_core::i18n::{t, t_fmt};

use super::{SetupConversation, SetupStep};

impl SetupConversation {
    // -- Step: Country Selection --------------------------------------------

    pub(super) fn show_country(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let countries = aios_core::installer::locale::all_countries();

        // Build a StringList model for the dropdown.
        let names: Vec<&str> = countries.iter().map(|c| c.country_name).collect();
        let model = gtk::StringList::new(&names);

        let dropdown = gtk::DropDown::new(Some(model), gtk::Expression::NONE);
        dropdown.set_hexpand(true);
        input_box.append(&dropdown);

        // Summary label showing derived settings.
        let summary_label = gtk::Label::new(None);
        summary_label.set_halign(Align::Start);
        summary_label.set_wrap(true);
        summary_label.add_css_class("dim-label");
        summary_label.set_margin_top(4);
        input_box.append(&summary_label);

        // Update summary whenever the dropdown selection changes.
        {
            let label_ref = summary_label.clone();
            dropdown.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                let all = aios_core::installer::locale::all_countries();
                if idx < all.len() {
                    let c = &all[idx];
                    let time_fmt = if c.time_format_24h { "24h" } else { "12h" };
                    label_ref.set_text(&format!(
                        "Language: {} | Keyboard: {} | Timezone: {} | Time: {}",
                        c.language, c.keyboard, c.timezone, time_fmt
                    ));
                }
            });
        }

        // Trigger initial summary update.
        dropdown.notify("selected");

        let next_btn = gtk::Button::with_label(&t("setup.country.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(8);
        input_box.append(&next_btn);

        let handle = self.chat_view.add_setup_card(
            "preferences-desktop-locale-symbolic",
            &t("setup.country.title"),
            &t("setup.country.description"),
            Some(input_box.upcast_ref()),
        );

        // Wire the Next button.
        let this = self.clone();
        let dd_ref = dropdown.clone();
        next_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let idx = dd_ref.selected() as usize;
            let all = aios_core::installer::locale::all_countries();
            if idx < all.len() {
                let country = all[idx].clone();
                let name = country.country_name.to_string();
                {
                    let mut s = this.state.borrow_mut();
                    s.country = Some(country);
                }
                this.chat_view.add_message("user", &name);
                if let Some(ref h) = handle {
                    h.dismiss(&name);
                }

                // If running from live ISO, show install decision; otherwise skip to NameAssistant.
                if aios_core::installer::is_live_iso() {
                    this.advance(SetupStep::InstallDecision);
                } else {
                    this.advance(SetupStep::NameAssistant);
                }
            }
        });

        // Background country auto-detection via IP geolocation.
        // We cannot move the DropDown into a background thread (it's not Send),
        // so we send the detected index via a channel and poll from GTK.
        let (detect_tx, detect_rx) = std::sync::mpsc::channel::<u32>();
        std::thread::spawn(move || {
            // Use the blocking detect_country function from the locale module.
            // It internally uses reqwest::blocking with a 5s timeout.
            if let Some(detected) = aios_core::installer::locale::detect_country_sync_pub() {
                let all = aios_core::installer::locale::all_countries();
                if let Some(idx) = all.iter().position(|c| c.country_code == detected.country_code) {
                    let _ = detect_tx.send(idx as u32);
                }
            }
        });

        let dd_for_detect = dropdown.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            match detect_rx.try_recv() {
                Ok(idx) => {
                    dd_for_detect.set_selected(idx);
                    gtk::glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    gtk::glib::ControlFlow::Continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    gtk::glib::ControlFlow::Break
                }
            }
        });

        self.speak(&t("setup.country.tts"));
    }

    // -- Step: Install Decision ---------------------------------------------

    pub(super) fn show_install_decision(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);

        let install_btn = gtk::Button::with_label(&t("setup.install_decision.install"));
        install_btn.add_css_class("suggested-action");
        btn_box.append(&install_btn);

        let usb_btn = gtk::Button::with_label(&t("setup.install_decision.usb"));
        btn_box.append(&usb_btn);

        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "drive-harddisk-symbolic",
            &t("setup.install_decision.title"),
            &t("setup.install_decision.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        install_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.state.borrow_mut().install_to_drive = true;
            this.chat_view.add_message("user", &t("setup.install_decision.user_yes"));
            if let Some(ref h) = h { h.dismiss(&t("setup.install_decision.user_yes")); }
            this.advance(SetupStep::DriveSelection);
        });

        let this = self.clone();
        usb_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.state.borrow_mut().install_to_drive = false;
            this.chat_view.add_message("user", &t("setup.install_decision.user_no"));
            if let Some(ref h) = handle { h.dismiss(&t("setup.install_decision.user_no")); }
            this.advance(SetupStep::NameAssistant);
        });

        self.speak(&t("setup.install_decision.tts"));
    }

    // -- Step: Drive Selection ----------------------------------------------

    pub(super) fn show_drive_selection(&self) {
        let drives = aios_core::installer::drives::list_drives();

        // Filter out live media.
        let eligible: Vec<_> = drives.iter()
            .filter(|d| !d.is_live_media)
            .collect();

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        if eligible.is_empty() {
            let error_label = gtk::Label::new(Some(&t("setup.drive_selection.no_drives")));
            error_label.add_css_class("error");
            error_label.set_halign(Align::Start);
            error_label.set_wrap(true);
            input_box.append(&error_label);

            let back_btn = gtk::Button::with_label(&t("setup.drive_selection.back"));
            back_btn.set_halign(Align::Start);
            back_btn.set_margin_top(4);
            input_box.append(&back_btn);

            self.chat_view.add_setup_card(
                "drive-harddisk-symbolic",
                &t("setup.drive_selection.title"),
                &t("setup.drive_selection.no_drives"),
                Some(input_box.upcast_ref()),
            );

            let this = self.clone();
            back_btn.connect_clicked(move |_| {
                this.state.borrow_mut().install_to_drive = false;
                this.advance(SetupStep::NameAssistant);
            });
            return;
        }

        // Build dropdown with drive display names.
        let display_names: Vec<String> = eligible.iter()
            .map(|d| aios_core::installer::drives::drive_display_name(d))
            .collect();
        let name_refs: Vec<&str> = display_names.iter().map(|s| s.as_str()).collect();
        let model = gtk::StringList::new(&name_refs);
        let dropdown = gtk::DropDown::new(Some(model), gtk::Expression::NONE);
        dropdown.set_hexpand(true);
        input_box.append(&dropdown);

        // Drive details label.
        let detail_label = gtk::Label::new(None);
        detail_label.set_halign(Align::Start);
        detail_label.set_wrap(true);
        detail_label.add_css_class("dim-label");
        detail_label.set_margin_top(4);
        input_box.append(&detail_label);

        // Update details when selection changes.
        let eligible_clone: Vec<aios_core::installer::drives::DriveInfo> =
            eligible.iter().map(|d| (*d).clone()).collect();
        {
            let drives_for_notify = eligible_clone.clone();
            let label_ref = detail_label.clone();
            dropdown.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                if idx < drives_for_notify.len() {
                    let d = &drives_for_notify[idx];
                    let mut parts = Vec::new();
                    for p in &d.partitions {
                        let label = p.label.as_deref().unwrap_or("");
                        let mount = p.mount_point.as_deref().unwrap_or("");
                        parts.push(format!("  {} {} {} {}", p.device, p.size_human, p.fstype, if !label.is_empty() { label } else { mount }));
                    }
                    if parts.is_empty() {
                        label_ref.set_text(&t("setup.drive_selection.no_partitions"));
                    } else {
                        label_ref.set_text(&parts.join("\n"));
                    }
                }
            });
        }
        dropdown.notify("selected");

        let next_btn = gtk::Button::with_label(&t("setup.drive_selection.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(8);
        input_box.append(&next_btn);

        let handle = self.chat_view.add_setup_card(
            "drive-harddisk-symbolic",
            &t("setup.drive_selection.title"),
            &t("setup.drive_selection.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let dd_ref = dropdown.clone();
        let drives_for_click = eligible_clone;
        next_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let idx = dd_ref.selected() as usize;
            if idx < drives_for_click.len() {
                let drive = drives_for_click[idx].clone();
                let display = aios_core::installer::drives::drive_display_name(&drive);
                {
                    let mut s = this.state.borrow_mut();
                    s.target_drive = Some(drive);
                }
                this.chat_view.add_message("user", &display);
                if let Some(ref h) = handle { h.dismiss(&display); }
                this.advance(SetupStep::PartitionPlan);
            }
        });

        self.speak(&t("setup.drive_selection.tts"));
    }

    // -- Step: Partition Plan -----------------------------------------------

    pub(super) fn show_partition_plan(&self) {
        let drive = self.state.borrow().target_drive.clone();
        let drive = match drive {
            Some(d) => d,
            None => {
                self.advance(SetupStep::NameAssistant);
                return;
            }
        };

        let plan = match aios_core::installer::partition::plan_partitions(&drive.device, drive.size_bytes) {
            Ok(p) => p,
            Err(e) => {
                self.chat_view.add_message("system", &format!("Partition planning failed: {e}"));
                self.state.borrow_mut().install_to_drive = false;
                self.advance(SetupStep::NameAssistant);
                return;
            }
        };

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Show planned partitions.
        for p in &plan.partitions {
            let role_label = gtk::Label::new(Some(&format!(
                "{}: {} ({})", p.role, p.size_human, p.filesystem
            )));
            role_label.set_halign(Align::Start);
            input_box.append(&role_label);
        }

        // Warning text.
        let warning = gtk::Label::new(Some(&t_fmt(
            "setup.partition_plan.warning",
            &[("drive", if drive.model.is_empty() { &drive.device } else { &drive.model })],
        )));
        warning.add_css_class("error");
        warning.set_halign(Align::Start);
        warning.set_wrap(true);
        warning.set_margin_top(8);
        input_box.append(&warning);

        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_margin_top(8);

        let continue_btn = gtk::Button::with_label(&t("setup.partition_plan.continue"));
        continue_btn.add_css_class("destructive-action");
        btn_box.append(&continue_btn);

        let cancel_btn = gtk::Button::with_label(&t("setup.partition_plan.cancel"));
        btn_box.append(&cancel_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "drive-harddisk-symbolic",
            &t("setup.partition_plan.title"),
            &t("setup.partition_plan.description"),
            Some(input_box.upcast_ref()),
        );

        // Store the plan in state.
        self.state.borrow_mut().partition_plan = Some(plan);

        let this = self.clone();
        let h = handle.clone();
        continue_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.chat_view.add_message("user", &t("setup.partition_plan.user_continue"));
            if let Some(ref h) = h { h.dismiss(&t("setup.partition_plan.user_continue")); }
            this.advance(SetupStep::NameAssistant);
        });

        let this = self.clone();
        cancel_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.state.borrow_mut().install_to_drive = false;
            this.state.borrow_mut().target_drive = None;
            this.state.borrow_mut().partition_plan = None;
            this.chat_view.add_message("user", &t("setup.partition_plan.user_cancel"));
            if let Some(ref h) = handle { h.dismiss(&t("setup.partition_plan.user_cancel")); }
            this.advance(SetupStep::NameAssistant);
        });
    }

    // -- Step: Install Confirmation -----------------------------------------

    pub(super) fn show_install_confirm(&self) {
        use aios_core::types::{BootStatus, StatusLine, MessageLevel};

        let s = self.state.borrow();
        let mut status = BootStatus::new();

        if let Some(ref country) = s.country {
            status.add(StatusLine::new(&t("setup.install_confirm.country"), true, country.country_name));
            let locale_info = format!(
                "{} | Keyboard: {} | Timezone: {}",
                country.language, country.keyboard, country.timezone
            );
            status.add(StatusLine::new(&t("setup.install_confirm.locale"), true, &locale_info));
        }

        if let Some(ref drive) = s.target_drive {
            let display = aios_core::installer::drives::drive_display_name(drive);
            status.add(StatusLine::new(&t("setup.install_confirm.target"), true, &display));
        }

        if let Some(ref plan) = s.partition_plan {
            let parts: Vec<String> = plan.partitions.iter()
                .map(|p| format!("{} {}", p.role, p.size_human))
                .collect();
            status.add(StatusLine::new(&t("setup.install_confirm.partitions"), true, &parts.join(" + ")));
        }

        status.add(StatusLine::new(&t("setup.complete.assistant_name"), true, &s.assistant_name));
        status.add(StatusLine::new(&t("setup.complete.master_password"), true, &t("setup.complete.master_password_set")));

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
            status.add(StatusLine::new(&role, true, &display));
        }

        drop(s);

        let summary_text = status.format();
        self.chat_view.add_level_message(
            MessageLevel::Info,
            &format!("{}\n\n{}", t("setup.install_confirm.title"), summary_text),
        );

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);

        let install_btn = gtk::Button::with_label(&t("setup.install_confirm.install"));
        install_btn.add_css_class("destructive-action");
        btn_box.append(&install_btn);

        let cancel_btn = gtk::Button::with_label(&t("setup.install_confirm.cancel"));
        btn_box.append(&cancel_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "emblem-important-symbolic",
            &t("setup.install_confirm.card_title"),
            &t("setup.install_confirm.card_description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        install_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.chat_view.add_message("user", &t("setup.install_confirm.user_install"));
            if let Some(ref h) = h { h.dismiss(&t("setup.install_confirm.user_install")); }
            this.advance(SetupStep::InstallProgress);
        });

        let this = self.clone();
        cancel_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.state.borrow_mut().install_to_drive = false;
            this.chat_view.add_message("user", &t("setup.install_confirm.user_cancel"));
            if let Some(ref h) = handle { h.dismiss(&t("setup.install_confirm.user_cancel")); }
            this.finish();
        });
    }

    // -- Step: Install Progress ---------------------------------------------

    pub(super) fn show_install_progress(&self) {
        let install_config = self.build_install_config();

        self.chat_view.add_message("system", &t("setup.install_progress.starting"));

        // Use std::sync::mpsc for thread-safe communication, then poll from GTK.
        let (tx, rx) = std::sync::mpsc::channel::<(String, String, bool)>();

        // Background thread runs the installation.
        let config_clone = install_config.clone();
        std::thread::spawn(move || {
            let result = aios_core::installer::run_install(&config_clone, |phase, msg| {
                let _ = tx.send((phase.description().to_string(), msg.to_string(), false));
            });
            match result {
                Ok(()) => {
                    let _ = tx.send(("complete".to_string(), String::new(), true));
                }
                Err(e) => {
                    let _ = tx.send(("error".to_string(), e, true));
                }
            }
        });

        // Poll for progress messages from the GTK main thread.
        let chat = self.chat_view.clone();
        let this = self.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            loop {
                match rx.try_recv() {
                    Ok((desc, msg, is_final)) => {
                        if is_final {
                            if msg.is_empty() {
                                // Success!
                                this.state.borrow_mut().install_to_drive = true;
                                this.show_reboot_dialog();
                            } else {
                                // Error — show message and recovery options.
                                chat.add_message("system",
                                    &format!("{} {msg}", t("setup.install_progress.failed")));
                                this.show_install_error_dialog(&msg);
                            }
                            return gtk::glib::ControlFlow::Break;
                        }
                        if msg.is_empty() {
                            chat.add_message("system", &desc);
                        } else {
                            chat.add_message("system", &format!("{desc} {msg}"));
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        return gtk::glib::ControlFlow::Continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return gtk::glib::ControlFlow::Break;
                    }
                }
            }
        });
    }

    /// Assemble an `InstallConfig` from the current `SetupState`.
    pub(super) fn build_install_config(&self) -> aios_core::installer::InstallConfig {
        let s = self.state.borrow();
        let hostname = if s.use_same_name {
            s.assistant_name.to_lowercase().replace(' ', "-")
        } else {
            s.machine_name_custom.clone()
        };
        let wake_word = if s.use_same_name {
            s.assistant_name.clone()
        } else {
            s.wake_word_custom.clone()
        };

        aios_core::installer::InstallConfig {
            target_device: s.target_drive.as_ref().map(|d| d.device.clone()).unwrap_or_default(),
            target_size_bytes: s.target_drive.as_ref().map(|d| d.size_bytes).unwrap_or(0),
            country: s.country.clone().unwrap_or_else(|| {
                aios_core::installer::locale::country_by_code("US").unwrap().clone()
            }),
            hostname,
            master_password: s.master_password.clone(),
            assistant_name: s.assistant_name.clone(),
            wake_word,
            providers: s.providers.iter().map(|p| aios_core::installer::ProviderConfig {
                name: p.name.clone(),
                api_key: p.api_key.clone(),
            }).collect(),
        }
    }

    /// Show the modal error dialog after a failed installation.
    pub(super) fn show_install_error_dialog(&self, error: &str) {
        use libadwaita as adw;
        use adw::prelude::*;

        let widget = self.chat_view.widget();
        let window = widget.root()
            .and_then(|r| r.downcast::<adw::ApplicationWindow>().ok());
        let win_ref: Option<&gtk::Window> = window.as_ref().map(|w| w.upcast_ref::<gtk::Window>());

        let dialog = adw::MessageDialog::new(
            win_ref,
            Some("Installation Failed"),
            Some(error),
        );
        dialog.add_response("retry", "Retry Installation");
        dialog.add_response("continue", "Continue to Chat");
        dialog.add_response("reboot", "Reboot");
        dialog.set_default_response(Some("retry"));
        dialog.set_close_response("continue");

        let this = self.clone();
        dialog.connect_response(None, move |_, response| {
            match response {
                "retry" => {
                    this.show_install_progress();
                }
                "reboot" => {
                    let _ = std::process::Command::new("sudo").args(["reboot"]).status();
                }
                _ => {
                    // Continue to chat — skip installation, proceed with setup
                    this.advance(SetupStep::NameAssistant);
                }
            }
        });
        dialog.present();
    }

    /// Show the modal reboot dialog after successful installation.
    pub(super) fn show_reboot_dialog(&self) {
        use libadwaita as adw;
        use adw::prelude::*;

        // Find the top-level window.
        let widget = self.chat_view.widget();
        let window = widget.root()
            .and_then(|r| r.downcast::<adw::ApplicationWindow>().ok());

        let win_ref: Option<&gtk::Window> = window.as_ref().map(|w| w.upcast_ref::<gtk::Window>());

        let dialog = adw::MessageDialog::new(
            win_ref,
            Some(&t("setup.reboot.title")),
            Some(&t("setup.reboot.body")),
        );
        dialog.add_response("reboot", &t("setup.reboot.button"));
        dialog.set_default_response(Some("reboot"));
        dialog.set_close_response("reboot"); // Cannot dismiss without rebooting.
        dialog.connect_response(None, |_, _| {
            let _ = std::process::Command::new("sudo").args(["reboot"]).status();
        });
        dialog.present();
    }
}
