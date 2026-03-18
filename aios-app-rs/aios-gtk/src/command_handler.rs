//! Extracted command handler for slash commands.
//!
//! This module processes [`CommandResult`] variants produced by
//! [`aios_core::config::commands::CommandHandler`] and drives the corresponding
//! GTK-side actions: showing panels, running selftest, spawning upgrades, etc.
//!
//! The main entry point is [`handle_command`], which is called from `AiosApp`
//! whenever the user enters a `/`-prefixed command.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use tracing::{info, warn};

use aios_core::config::commands::{
    BackgroundTaskKind, CommandHandler, CommandResult, PanelFieldKind,
};
use aios_core::config::ConfigManager;
use aios_core::i18n::{t, t_fmt};
use aios_core::types::MessageLevel;

use crate::ui::chat_view::ChatView;

// ---------------------------------------------------------------------------
// Shared state needed by the command handler
// ---------------------------------------------------------------------------

/// Minimal application state required by the command handler.
///
/// This is a subset of `AiosApp` fields, passed by reference so the handler
/// does not need access to the full application struct.
pub(crate) struct CommandHandlerState {
    pub config: ConfigManager,
    pub llm: Arc<tokio::sync::Mutex<aios_llm::LlmManager>>,
    pub conversation: Vec<aios_core::types::Message>,
    pub rt: tokio::runtime::Handle,
    pub kws_engine: Option<Arc<std::sync::Mutex<Option<aios_voice::KwsEngine>>>>,
    pub wake_training_in_progress: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub wake_enabled: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub kws_models_dir: std::path::PathBuf,
}

// ---------------------------------------------------------------------------
// Internal enums for async result channels
// ---------------------------------------------------------------------------

/// Result of an async upgrade version check (sent from Tokio to GTK thread).
enum UpgradeCheckResult {
    UpToDate,
    Available(aios_core::upgrade::ReleaseInfo),
    Error(String),
}

/// Result of an async upgrade install (download + verify + install).
enum UpgradeInstallResult {
    Success,
    Error(String),
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Process a slash command and perform the appropriate GTK-side action.
///
/// `state` is wrapped in `Rc<RefCell<..>>` so that async callbacks (timers,
/// background tasks) can hold a reference without lifetime issues.
pub(crate) fn handle_command(
    state: &Rc<RefCell<CommandHandlerState>>,
    chat_view: &ChatView,
    input: &str,
) {
    let mut s = state.borrow_mut();
    let mut handler = CommandHandler::new(&mut s.config);
    let result = handler.execute(input);

    match result {
        CommandResult::Response(text) => {
            chat_view.add_message("system", &text);
        }
        CommandResult::Clear => {
            chat_view.clear();
            s.conversation.clear();
            chat_view.add_message("system", "Chat history cleared.");
        }
        CommandResult::Configure => {
            chat_view.add_message(
                "system",
                "Use the settings button (gear icon) to configure AiOS.",
            );
        }
        CommandResult::SysInfo => {
            drop(s);
            let info = aios_core::system_monitor::SystemInfo::gather();
            chat_view.add_level_message(MessageLevel::Info, &info.format_text());
            return;
        }
        CommandResult::ClosePanel => {
            drop(s);
            let closed = close_topmost_dialog();
            if closed {
                chat_view.add_message("system", "Panel closed.");
            } else {
                chat_view.add_message("system", "No open panel or dialog to close.");
            }
            return;
        }
        CommandResult::SelfTest(filter) => {
            drop(s);
            run_selftest(chat_view, &filter);
            return;
        }
        CommandResult::Update(url) => {
            let url = url.trim().to_string();
            if url.is_empty() {
                chat_view.add_message(
                    "system",
                    "Usage: /update <url>\n\
                     Example: /update https://example.com/aios\n\n\
                     Or from your dev machine:\n\
                     ./deploy.sh aios.local",
                );
            } else {
                chat_view.add_level_message(
                    MessageLevel::Warning,
                    &format!("Updating AiOS from: **{url}**\nThis will restart the app..."),
                );
                // Run aios-update in the background.
                let rt = s.rt.clone();
                drop(s);
                rt.spawn(async move {
                    let output = tokio::process::Command::new("aios-update")
                        .arg(&url)
                        .output()
                        .await;
                    match output {
                        Ok(o) => {
                            let stdout = String::from_utf8_lossy(&o.stdout);
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            tracing::info!("aios-update: {stdout}{stderr}");
                        }
                        Err(e) => {
                            tracing::error!("aios-update failed: {e}");
                        }
                    }
                });
                return;
            }
        }
        CommandResult::Upgrade => {
            // Show a "Checking..." message immediately on the GTK thread.
            chat_view.add_message("system", &t("cmd.upgrade.checking"));

            // Pull the update URL from config before dropping the borrow.
            let update_url = s.config.get_str(
                "system.update_url",
                "https://api.github.com/repos/aios-dev/aios/releases/latest",
            );
            let rt = s.rt.clone();
            drop(s);

            // Use mpsc channel: tokio task sends result, GTK polls via timeout_add_local.
            let (tx, rx) = std::sync::mpsc::channel::<UpgradeCheckResult>();

            // Spawn version check on Tokio (no GTK types captured).
            rt.spawn(async move {
                let result = aios_core::upgrade::check_for_update(&update_url).await;
                let _ = tx.send(match result {
                    Err(e) => UpgradeCheckResult::Error(e.to_string()),
                    Ok(None) => UpgradeCheckResult::UpToDate,
                    Ok(Some(info)) => UpgradeCheckResult::Available(info),
                });
            });

            // Poll for result on the GTK thread.
            let chat = chat_view.clone();
            let state_clone = state.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                match rx.try_recv() {
                    Ok(UpgradeCheckResult::Error(e)) => {
                        let msg = t_fmt("cmd.upgrade.check_failed", &[("error", &e)]);
                        chat.add_level_message(MessageLevel::Warning, &msg);
                        glib::ControlFlow::Break
                    }
                    Ok(UpgradeCheckResult::UpToDate) => {
                        let version = aios_core::upgrade::CURRENT_VERSION.trim();
                        let msg =
                            t_fmt("cmd.upgrade.up_to_date", &[("version", version)]);
                        chat.add_message("system", &msg);
                        glib::ControlFlow::Break
                    }
                    Ok(UpgradeCheckResult::Available(info)) => {
                        let current = aios_core::upgrade::CURRENT_VERSION.trim();
                        let msg = t_fmt(
                            "cmd.upgrade.available",
                            &[("latest", &info.version), ("current", current)],
                        );
                        chat.add_message("system", &msg);
                        if !info.changelog.is_empty() {
                            let label = t("cmd.upgrade.changelog_label");
                            chat.add_message(
                                "system",
                                &format!("{label} {}", info.changelog),
                            );
                        }
                        show_upgrade_confirm_dialog(&state_clone, &chat, info);
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        glib::ControlFlow::Continue
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        glib::ControlFlow::Break
                    }
                }
            });
            return;
        }
        CommandResult::Panel {
            title,
            description,
            fields,
            config_key,
        } => {
            // Render the panel as an interactive card in the chat view.
            let input_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
            input_box.set_margin_top(8);

            if !description.is_empty() {
                let desc = gtk4::Label::new(Some(&description));
                desc.set_halign(gtk4::Align::Start);
                desc.set_opacity(0.7);
                input_box.append(&desc);
            }

            for field in &fields {
                match &field.kind {
                    PanelFieldKind::Dropdown { options, selected } => {
                        let label = gtk4::Label::new(Some(&field.label));
                        label.set_halign(gtk4::Align::Start);
                        label.add_css_class("heading");
                        input_box.append(&label);

                        let string_list = gtk4::StringList::new(
                            &options.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                        );
                        let dropdown = gtk4::DropDown::new(
                            Some(string_list),
                            gtk4::Expression::NONE,
                        );

                        // Set the currently selected value.
                        if let Some(sel) = selected {
                            if let Some(idx) = options.iter().position(|o| o == sel) {
                                dropdown.set_selected(idx as u32);
                            }
                        }
                        input_box.append(&dropdown);

                        // Apply button.
                        let apply_btn = gtk4::Button::with_label("Apply");
                        apply_btn.add_css_class("suggested-action");
                        apply_btn.set_halign(gtk4::Align::Start);
                        apply_btn.set_margin_top(4);

                        let options_clone = options.clone();
                        let config_key_clone = config_key.clone();
                        let state_for_panel = state.clone();
                        let chat_for_panel = chat_view.clone();
                        let dd_ref = dropdown.clone();
                        let field_id = field.id.clone();
                        apply_btn.connect_clicked(move |b| {
                            b.set_sensitive(false);
                            let idx = dd_ref.selected() as usize;
                            if let Some(value) = options_clone.get(idx) {
                                let mut s = state_for_panel.borrow_mut();
                                let _ = s.config.set(
                                    &config_key_clone,
                                    serde_json::json!(value),
                                );

                                // Apply theme change immediately via libadwaita.
                                if config_key_clone == "ui.theme" {
                                    apply_theme(value);
                                }

                                // Apply resolution change.
                                if field_id == "resolution" {
                                    aios_core::config::commands::CommandHandler::apply_resolution(value);
                                }

                                chat_for_panel.add_message(
                                    "system",
                                    &format!("Set to: {value}"),
                                );
                            }
                        });
                        input_box.append(&apply_btn);
                    }
                }
            }

            drop(s);
            chat_view.add_setup_card(
                "preferences-system-symbolic",
                &title,
                "",
                Some(input_box.upcast_ref()),
            );
            return;
        }
        CommandResult::BackgroundTask { description, task } => {
            chat_view.add_message("system", &description);

            match task {
                BackgroundTaskKind::WakeWordTraining {
                    phrase,
                    output_dir,
                } => {
                    // Get shared state for training.
                    let training_flag = s.wake_training_in_progress.clone();
                    let kws_engine_arc = s.kws_engine.clone();
                    let _kws_models_dir = s.kws_models_dir.clone();
                    let wake_enabled_flag = s.wake_enabled.clone();
                    drop(s);

                    if let Some(ref flag) = training_flag {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }

                    // Spawn training in a background thread.
                    let chat_for_train = chat_view.clone();
                    let (train_tx, train_rx) =
                        std::sync::mpsc::channel::<Result<std::path::PathBuf, String>>();

                    let phrase_clone = phrase.clone();
                    std::thread::spawn(move || {
                        let result =
                            aios_voice::KwsTrainer::train(&phrase_clone, &output_dir);
                        let _ = train_tx.send(result.map_err(|e| e.to_string()));
                    });

                    // Poll for training completion on the GTK thread.
                    let state_for_train = state.clone();
                    glib::timeout_add_local(
                        std::time::Duration::from_millis(500),
                        move || {
                            match train_rx.try_recv() {
                                Ok(Ok(model_path)) => {
                                    info!(
                                        "KWS training complete: {}",
                                        model_path.display()
                                    );
                                    chat_for_train.add_message(
                                        "system",
                                        &format!(
                                            "Wake word training complete for \
                                             \"{phrase}\". Model saved."
                                        ),
                                    );

                                    // Hot-swap the model into the KWS engine.
                                    if let Some(ref engine_arc) = kws_engine_arc {
                                        if let Ok(mut guard) = engine_arc.lock() {
                                            if let Some(ref mut engine) = *guard {
                                                match engine
                                                    .load_wake_model(&model_path, &phrase)
                                                {
                                                    Ok(()) => {
                                                        info!(
                                                            "KWS: hot-swapped model for \
                                                             \"{}\"",
                                                            phrase
                                                        );
                                                        chat_for_train.add_message(
                                                            "system",
                                                            &format!(
                                                                "Wake word \"{phrase}\" \
                                                                 is now active."
                                                            ),
                                                        );
                                                    }
                                                    Err(e) => {
                                                        warn!(
                                                            "KWS: failed to load trained \
                                                             model: {e}"
                                                        );
                                                        chat_for_train.add_message(
                                                            "system",
                                                            &format!(
                                                                "Model trained but failed \
                                                                 to load: {e}"
                                                            ),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Enable wake word detection.
                                    if let Some(ref flag) = wake_enabled_flag {
                                        flag.store(
                                            true,
                                            std::sync::atomic::Ordering::Relaxed,
                                        );
                                    }

                                    // Update config.
                                    if let Ok(mut cfg) =
                                        aios_core::config::ConfigManager::new()
                                    {
                                        let _ = cfg.set(
                                            "voice.wake_word_source",
                                            serde_json::json!("custom"),
                                        );
                                    }

                                    // Clear training flag.
                                    let st = state_for_train.borrow();
                                    if let Some(ref flag) =
                                        st.wake_training_in_progress
                                    {
                                        flag.store(
                                            false,
                                            std::sync::atomic::Ordering::Relaxed,
                                        );
                                    }

                                    glib::ControlFlow::Break
                                }
                                Ok(Err(e)) => {
                                    warn!("KWS training failed: {e}");
                                    chat_for_train.add_level_message(
                                        MessageLevel::Warning,
                                        &format!("Wake word training failed: {e}"),
                                    );

                                    // Clear training flag.
                                    let st = state_for_train.borrow();
                                    if let Some(ref flag) =
                                        st.wake_training_in_progress
                                    {
                                        flag.store(
                                            false,
                                            std::sync::atomic::Ordering::Relaxed,
                                        );
                                    }

                                    glib::ControlFlow::Break
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
                                    glib::ControlFlow::Continue
                                }
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    // Training thread crashed.
                                    let st = state_for_train.borrow();
                                    if let Some(ref flag) =
                                        st.wake_training_in_progress
                                    {
                                        flag.store(
                                            false,
                                            std::sync::atomic::Ordering::Relaxed,
                                        );
                                    }
                                    glib::ControlFlow::Break
                                }
                            }
                        },
                    );
                    return;
                }
            }
        }
        CommandResult::Unknown(cmd) => {
            chat_view.add_message(
                "system",
                &format!("Unknown command: {cmd}\nType /help for available commands."),
            );
        }
    }

    // Re-apply any provider/key changes to the LLM manager.
    let provider = s.config.get_str("llm.provider", "claude");
    if let Ok(mut llm) = s.llm.try_lock() {
        let _ = llm.set_active(&provider);

        let claude_key = s.config.get_str("llm.claude_api_key", "");
        if !claude_key.is_empty() {
            let _ = llm.set_api_key("claude", claude_key);
        }
        let openai_key = s.config.get_str("llm.openai_api_key", "");
        if !openai_key.is_empty() {
            let _ = llm.set_api_key("openai", openai_key);
        }
    }

    // Re-apply theme from config (handles /theme dark, /theme light, etc.).
    let theme = s.config.get_str("ui.theme", "dark");
    apply_theme(&theme);
}

// ---------------------------------------------------------------------------
// Helper: selftest
// ---------------------------------------------------------------------------

/// Run the self-test suite and display results in the chat view.
fn run_selftest(chat_view: &ChatView, filter: &str) {
    use aios_core::selftest::runner::TestContext;
    use aios_core::selftest::SelfTestRunner;

    let chat = chat_view.clone();
    chat.add_message("system", "Starting AiOS self-test...");

    let runner = SelfTestRunner::new();

    // Build context factory — each test gets a fresh context.
    let chat_for_ctx = chat_view.clone();
    let ctx_factory = move || -> TestContext {
        let c = chat_for_ctx.clone();
        TestContext {
            display: Box::new(move |role, content| {
                c.add_message(role, content);
            }),
            // Panel support: not wired yet (would need the UiPanelTool callback).
            // For now, interactive tests will show "skipped".
            show_panel: None,
            channel_kind: aios_core::channel::ChannelKind::Desktop,
        }
    };

    let results = match filter.trim() {
        "" => runner.run_all(ctx_factory),
        "quick" => runner.run_quick(ctx_factory),
        tag => runner.run_tagged(tag, ctx_factory),
    };

    let report = SelfTestRunner::format_report(&results);
    chat.add_message("system", &report);
}

// ---------------------------------------------------------------------------
// Helper: close topmost dialog
// ---------------------------------------------------------------------------

/// Close the topmost modal/transient dialog window.
///
/// Iterates all windows registered with the GTK application and looks for
/// visible windows that are not the main `ApplicationWindow`. Closes the last
/// one found (topmost) and returns `true` if a window was closed.
pub(crate) fn close_topmost_dialog() -> bool {
    // Get the running GtkApplication via gio::Application::default().
    let gio_app = match gtk4::gio::Application::default() {
        Some(a) => a,
        None => return false,
    };
    let gtk_app = match gio_app.downcast::<gtk4::Application>() {
        Ok(a) => a,
        Err(_) => return false,
    };

    // Iterate windows registered with the application.
    // The list is ordered; the last matching window is the topmost.
    let mut candidate: Option<gtk4::Window> = None;

    for win in gtk_app.windows() {
        // Skip the main application window.
        if win.downcast_ref::<adw::ApplicationWindow>().is_some() {
            continue;
        }
        if win.is_visible() {
            candidate = Some(win);
        }
    }

    if let Some(win) = candidate {
        win.close();
        true
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Helper: apply theme
// ---------------------------------------------------------------------------

/// Apply a theme setting via libadwaita's StyleManager.
pub(crate) fn apply_theme(theme: &str) {
    let style_manager = adw::StyleManager::default();
    match theme {
        "dark" => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
        "light" => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
        "auto" | "system" => {
            style_manager.set_color_scheme(adw::ColorScheme::Default)
        }
        _ => {
            warn!("Unknown theme: {theme}, defaulting to dark");
            style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
        }
    }
    info!("Theme applied: {theme}");
}

// ---------------------------------------------------------------------------
// Helper: upgrade flow
// ---------------------------------------------------------------------------

/// Show an "Install update?" confirm dialog for the upgrade flow.
fn show_upgrade_confirm_dialog(
    state: &Rc<RefCell<CommandHandlerState>>,
    chat_view: &ChatView,
    info: aios_core::upgrade::ReleaseInfo,
) {
    use adw::prelude::*;

    // Find the top-level window from the chat view widget.
    let widget = chat_view.widget();
    let window = widget
        .root()
        .and_then(|r| r.downcast::<adw::ApplicationWindow>().ok());
    let win_ref: Option<&gtk4::Window> =
        window.as_ref().map(|w| w.upcast_ref::<gtk4::Window>());

    let dialog = adw::MessageDialog::new(
        win_ref,
        Some(&t("cmd.upgrade.install_prompt")),
        Some(&format!("Install AiOS v{}?", info.version)),
    );
    dialog.add_response("skip", "Skip");
    dialog.add_response("install", "Install");
    dialog.set_response_appearance("install", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("skip"));

    let chat = chat_view.clone();
    let state_clone = state.clone();

    dialog.connect_response(None, move |_dlg, response| {
        if response != "install" {
            return;
        }
        run_upgrade(&state_clone, &chat, info.clone());
    });

    dialog.present();
}

/// Drive the download + verify + install sequence, reporting progress via
/// the chat view, then show the reboot dialog.
fn run_upgrade(
    state: &Rc<RefCell<CommandHandlerState>>,
    chat_view: &ChatView,
    info: aios_core::upgrade::ReleaseInfo,
) {
    let rt = state.borrow().rt.clone();
    let chat = chat_view.clone();
    let state_clone = state.clone();

    chat.add_message("system", &t("cmd.upgrade.downloading"));

    // Use mpsc channel: tokio task sends result, GTK polls via timeout_add_local.
    let (tx, rx) = std::sync::mpsc::channel::<UpgradeInstallResult>();

    let sha256 = info.sha256.clone();

    // Spawn download + verify + install on Tokio (no GTK types captured).
    rt.spawn(async move {
        // --- Download ---
        let dl_result =
            aios_core::upgrade::download_binary(&info, |_downloaded, _total| {
                // Progress reporting intentionally omitted to avoid Send issues.
                // The "Downloading..." message is already shown.
            })
            .await;

        if let Err(e) = dl_result {
            let _ = tx.send(UpgradeInstallResult::Error(e.to_string()));
            return;
        }

        // --- Verify checksum ---
        if let Err(e) = aios_core::upgrade::verify_checksum(&sha256).await {
            let _ = tx.send(UpgradeInstallResult::Error(e.to_string()));
            return;
        }

        // --- Install ---
        if let Err(e) = aios_core::upgrade::install_binary().await {
            let _ = tx.send(UpgradeInstallResult::Error(e.to_string()));
            return;
        }

        let _ = tx.send(UpgradeInstallResult::Success);
    });

    // Poll for result on the GTK thread.
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        match rx.try_recv() {
            Ok(UpgradeInstallResult::Error(e)) => {
                let msg = t_fmt("cmd.upgrade.install_failed", &[("error", &e)]);
                chat.add_level_message(MessageLevel::Warning, &msg);
                glib::ControlFlow::Break
            }
            Ok(UpgradeInstallResult::Success) => {
                chat.add_message("system", &t("cmd.upgrade.install_ok"));
                show_reboot_dialog(&state_clone, &chat);
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                glib::ControlFlow::Break
            }
        }
    });
}

/// Show a "Reboot now?" dialog after a successful upgrade install.
fn show_reboot_dialog(
    state: &Rc<RefCell<CommandHandlerState>>,
    chat_view: &ChatView,
) {
    use adw::prelude::*;

    // Find the top-level window from the chat view widget.
    let widget = chat_view.widget();
    let window = widget
        .root()
        .and_then(|r| r.downcast::<adw::ApplicationWindow>().ok());
    let win_ref: Option<&gtk4::Window> =
        window.as_ref().map(|w| w.upcast_ref::<gtk4::Window>());

    let dialog = adw::MessageDialog::new(
        win_ref,
        Some(&t("cmd.upgrade.reboot_prompt")),
        Some("Reboot now to apply the update?"),
    );
    dialog.add_response("later", "Later");
    dialog.add_response("reboot", "Reboot");
    dialog.set_response_appearance("reboot", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("reboot"));

    let chat = chat_view.clone();
    let state_clone = state.clone();

    dialog.connect_response(None, move |_dlg, response| {
        if response != "reboot" {
            let msg = t("cmd.upgrade.reboot_later");
            chat.add_message("system", &msg);
            return;
        }
        let rt = state_clone.borrow().rt.clone();
        rt.spawn(async {
            let _ = aios_core::upgrade::reboot().await;
        });
    });

    dialog.present();
}
