//! Sentinel model selection and download step (mandatory).

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};

use tracing::{info, warn};

use super::{SetupConversation, SetupStep};

impl SetupConversation {
    /// Show the Sentinel model selection step.
    ///
    /// This step is MANDATORY — the user must install a local model for
    /// security sanitization before setup can complete.
    pub(super) fn show_sentinel_model(&self) {
        let data = aios_llm::local::build_model_selection();

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Description
        let desc = gtk::Label::new(Some(&data.description));
        desc.add_css_class("dim-label");
        desc.set_halign(Align::Start);
        desc.set_wrap(true);
        input_box.append(&desc);

        // Model dropdown
        let model_names: Vec<String> = data
            .models
            .iter()
            .map(|m| {
                let status = if m.installed {
                    " \u{2705}"
                } else if m.recommended {
                    " \u{2b50}"
                } else {
                    ""
                };
                format!("{} ({}, {}){}", m.display_name, m.download_size, m.speed, status)
            })
            .collect();
        let model_ids: Vec<String> = data.models.iter().map(|m| m.model_id.clone()).collect();
        let name_refs: Vec<&str> = model_names.iter().map(|s| s.as_str()).collect();

        let dropdown_list = gtk::StringList::new(&name_refs);
        let dropdown = gtk::DropDown::new(Some(dropdown_list), gtk::Expression::NONE);
        dropdown.set_hexpand(true);

        // Pre-select the recommended model.
        if let Some(idx) = data.models.iter().position(|m| m.recommended) {
            dropdown.set_selected(idx as u32);
        }
        input_box.append(&dropdown);

        // Download + Continue button
        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_halign(Align::End);

        let download_btn = gtk::Button::with_label("Download & Install");
        download_btn.add_css_class("suggested-action");
        btn_box.append(&download_btn);
        input_box.append(&btn_box);

        // Progress bar (hidden initially)
        let progress_bar = gtk::ProgressBar::new();
        progress_bar.set_show_text(true);
        progress_bar.set_visible(false);
        progress_bar.set_hexpand(true);
        input_box.append(&progress_bar);

        let status_label = gtk::Label::new(None);
        status_label.add_css_class("dim-label");
        status_label.set_halign(Align::Start);
        status_label.set_visible(false);
        input_box.append(&status_label);

        let handle = self.chat_view.add_setup_card(
            "security-high-symbolic",
            "Install Sentinel Security Model",
            "A local AI model is required to check every response for security \
             violations. This model runs entirely on your device — no data leaves \
             your machine. This step cannot be skipped.",
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let ids = model_ids.clone();
        let progress = progress_bar.clone();
        let status = status_label.clone();
        let dd = dropdown.clone();
        let btn = download_btn.clone();
        download_btn.connect_clicked(move |_| {
            let sel = dd.selected() as usize;
            let model_id = match ids.get(sel) {
                Some(id) => id.clone(),
                None => return,
            };

            btn.set_sensitive(false);
            dd.set_sensitive(false);
            progress.set_visible(true);
            status.set_visible(true);
            status.set_text(&format!("Downloading {model_id}..."));
            progress.set_fraction(0.0);
            progress.set_text(Some("Starting download..."));

            // Start Ollama if not running.
            let client = aios_llm::OllamaClient::new();
            if let Err(e) = client.ensure_running() {
                warn!("Failed to start Ollama: {e}");
                status.set_text(&format!("Error: {e}"));
                btn.set_sensitive(true);
                dd.set_sensitive(true);
                return;
            }

            // Download in background thread, poll progress from GTK.
            let (tx, rx) = std::sync::mpsc::channel::<DownloadMsg>();

            let model_for_thread = model_id.clone();
            let tx_clone = tx.clone();
            std::thread::spawn(move || {
                let client = aios_llm::OllamaClient::new();
                match client.pull_model(&model_for_thread, |p| {
                    let _ = tx_clone.send(DownloadMsg::Progress {
                        status: p.status.clone(),
                        completed: p.completed,
                        total: p.total,
                    });
                }) {
                    Ok(()) => {
                        let _ = tx.send(DownloadMsg::Done);
                    }
                    Err(e) => {
                        let _ = tx.send(DownloadMsg::Error(e));
                    }
                }
            });

            // Poll for progress updates.
            let progress_ref = progress.clone();
            let status_ref = status.clone();
            let btn_ref = btn.clone();
            let dd_ref = dd.clone();
            let this_ref = this.clone();
            let model_for_complete = model_id.clone();
            let handle_ref = handle.clone();
            gtk4::glib::timeout_add_local(
                std::time::Duration::from_millis(100),
                move || {
                    while let Ok(msg) = rx.try_recv() {
                        match msg {
                            DownloadMsg::Progress { status: s, completed, total } => {
                                if total > 0 {
                                    let frac = completed as f64 / total as f64;
                                    progress_ref.set_fraction(frac);
                                    let mb_done = completed / 1_048_576;
                                    let mb_total = total / 1_048_576;
                                    let pct = (frac * 100.0) as u32;
                                    progress_ref.set_text(Some(&format!(
                                        "{}MB / {}MB ({}%)",
                                        mb_done, mb_total, pct
                                    )));
                                }
                                status_ref.set_text(&s);
                            }
                            DownloadMsg::Done => {
                                progress_ref.set_fraction(1.0);
                                progress_ref.set_text(Some("Download complete!"));
                                status_ref.set_text("Sentinel model installed successfully.");

                                // Store the selection.
                                this_ref.state.borrow_mut().sentinel_model =
                                    model_for_complete.clone();

                                info!("Sentinel model installed: {model_for_complete}");

                                // Dismiss and advance.
                                if let Some(ref h) = handle_ref {
                                    h.dismiss(&format!("Sentinel: {model_for_complete}"));
                                }

                                let installing = this_ref.state.borrow().install_to_drive;
                                if installing {
                                    this_ref.advance(SetupStep::InstallConfirm);
                                } else {
                                    this_ref.advance(SetupStep::Complete);
                                }

                                return gtk4::glib::ControlFlow::Break;
                            }
                            DownloadMsg::Error(e) => {
                                status_ref.set_text(&format!("Error: {e}"));
                                status_ref.add_css_class("error");
                                btn_ref.set_sensitive(true);
                                dd_ref.set_sensitive(true);
                                return gtk4::glib::ControlFlow::Break;
                            }
                        }
                    }
                    gtk4::glib::ControlFlow::Continue
                },
            );
        });

        self.speak(
            "Please choose a local AI model for security. \
             This model will check every response before you see it. \
             Select the recommended option and click Download.",
        );
    }
}

/// Internal message type for download progress communication.
enum DownloadMsg {
    Progress {
        status: String,
        completed: u64,
        total: u64,
    },
    Done,
    Error(String),
}
