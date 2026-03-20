//! First-boot setup and autoconfig flow.
//!
//! Extracted from `app.rs`. Contains the first-boot wizard (card-based
//! setup conversation) and the unattended autoconfig path.
//!
//! After either path completes, [`crate::boot_context::finalize_boot`] wires up
//! the prompt input, connects voice, and shows the ready message.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use tracing::{error, info, warn};

use aios_core::config::ConfigManager;
use aios_core::i18n::{t, t_fmt};
use aios_core::secure::Vault;
use aios_core::secure::vault::{SecretEntry, SecretKind};

use crate::app::{AiosApp, SharedQueue};
use crate::boot_status;
use crate::llm_handler;
use crate::tts::stop_tts;
use crate::ui::channel_overlay::ChannelOverlay;
use crate::ui::chat_view::ChatView;
use crate::ui::first_boot::SetupConversation;
use crate::ui::main_window;
use crate::ui::prompt_input::PromptInput;
use crate::ui::settings_dialog;

// ---------------------------------------------------------------------------
// First-boot interactive setup
// ---------------------------------------------------------------------------

/// Run the first-boot setup as a conversation in the main chat view.
///
/// Builds the normal main window (fullscreen) but starts a
/// [`SetupConversation`] that drives setup cards through the chat view.
/// Once setup completes, the vault is created, secrets are stored, the
/// LLM manager is configured, and normal chat mode begins.
pub(crate) fn run_first_boot_setup(
    app: &adw::Application,
    rt: tokio::runtime::Handle,
    queue: SharedQueue,
) {
    let chat_view = ChatView::new();
    let prompt_input = PromptInput::new();
    let channel_overlay = ChannelOverlay::new();

    let mut config = load_config();

    // Initialize i18n.
    init_i18n(&config);

    let mw = main_window::build_main_window(
        app,
        &chat_view,
        &prompt_input,
        &channel_overlay,
        &[&t("setup.window_title")],
        &t("setup.window_title"),
    );
    let window = mw.window;
    let vu_meter_ref = mw.vu_meter;

    // Hide the prompt input during setup.
    prompt_input.widget().set_visible(false);

    // Apply assistant display name from config.
    let assistant_name = config.get_str("assistant.name", "Assistant");
    crate::ui::chat_view::set_assistant_display_name(&assistant_name);

    // Create the shared AppRuntime for multi-channel orchestration.
    let runtime = aios_core::channel::AppRuntime::new();
    runtime.switcher.register_channel(
        aios_core::channel::ChannelKind::Desktop,
        aios_core::channel::ChannelContext::desktop(),
    );

    // Build and show boot status.
    let boot_status_text = boot_status::build_boot_status(&config);

    // Start the web server (setup wizard also available via browser).
    let _guard = rt.enter();
    let _web_server = AiosApp::start_web_server(
        &mut config,
        &runtime,
        Some(boot_status_text.clone()),
    );

    chat_view.add_level_message(aios_core::types::MessageLevel::Info, &boot_status_text);

    // Push boot status to queue.
    {
        let mut q = queue.lock().unwrap();
        q.push(aios_core::queue::QueuedMessage {
            role: aios_core::types::Role::System,
            channel: aios_core::channel::ChannelKind::System,
            source: Some("system".into()),
            content: Some(boot_status_text),
            level: Some(aios_core::types::MessageLevel::Info),
            ..Default::default()
        });
    }

    // Apply theme.
    AiosApp::apply_theme(&config.get_str("ui.theme", "dark"));

    // Check for hostname collision.
    show_hostname_conflict_card(&chat_view);

    // Create the setup conversation with config for pre-filling API keys.
    let setup = SetupConversation::new(chat_view.clone(), Some(config));

    // On completion: create vault, store secrets, finalize boot.
    let rt_ref = rt.clone();
    let chat_view_ref = chat_view.clone();
    let prompt_ref = prompt_input.clone();
    let window_ref = window.clone();
    let channel_overlay_ref = channel_overlay.clone();
    let vu_meter_for_transition = vu_meter_ref.clone();
    let queue_for_transition = queue.clone();
    setup.on_complete(move |result| {
        info!(
            "First-boot setup complete: {} provider(s) configured",
            result.providers.len()
        );

        // Create the vault and store the API keys.
        let vault_path = ConfigManager::default_config_dir().join("vault.enc");
        let mut vault = Vault::new(vault_path);

        if let Err(e) = vault.create(&result.master_password) {
            warn!("Failed to create vault: {e}");
        } else {
            for provider in &result.providers {
                let key_name = format!("{}_api_key", provider.name);
                let label = crate::providers::find_by_id(&provider.name)
                    .map(|p| format!("{} API Key", p.display_name))
                    .unwrap_or_else(|| format!("{} API Key", provider.name));
                let entry = SecretEntry {
                    kind: SecretKind::ApiKey,
                    value: provider.api_key.clone(),
                    label,
                    created: chrono::Utc::now(),
                    last_accessed: None,
                };
                if let Err(e) = vault.set(&key_name, entry) {
                    warn!("Failed to store {key_name} in vault: {e}");
                }
            }
            info!("Vault created with {} secret(s)", result.providers.len());
        }

        // Store provider/key info in config.
        if let Ok(mut config) = ConfigManager::new() {
            if let Some(primary) = result.providers.first() {
                let _ = config.set("llm.provider", serde_json::json!(primary.name));
            }
            for p in &result.providers {
                if let Some(def) = crate::providers::find_by_id(&p.name) {
                    if def.needs_api_key && !def.api_key_config.is_empty() {
                        let _ = config.set(def.api_key_config, serde_json::json!(p.api_key));
                    }
                }
            }

            // Save assistant identity.
            let _ = config.set("assistant.name", serde_json::json!(result.assistant_name));
            let _ = config.set("voice.wake_word", serde_json::json!(result.wake_word));
            let _ = config.set("system.machine_name", serde_json::json!(result.machine_name));

            // Save Sentinel model.
            if !result.sentinel_model.is_empty() {
                let _ = config.set("llm.sentinel_model", serde_json::json!(result.sentinel_model));
            }

            // Determine wake word source.
            let normalized = result.wake_word.to_lowercase().replace(' ', "_");
            let pretrained_ids = [
                "hey_assistant",
                "hey_jarvis",
                "computer",
                "ok_computer",
                "hey_friday",
                "jarvis",
                "ok_jarvis",
                "skynet",
                "terminator",
                "hey_house",
                "ok_home",
                "home_assistant",
                "mr_anderson",
                "mr_smith",
                "hey_dick_head",
                "oi_fuckwhit",
                "yo_homie",
            ];
            if pretrained_ids.contains(&normalized.as_str()) {
                let _ = config.set("voice.wake_word_source", serde_json::json!("pretrained"));
            } else {
                let _ = config.set("voice.wake_word_source", serde_json::json!("training"));
            }
        }

        // Apply country-derived settings.
        if let Some(ref country) = result.country {
            let _ = std::process::Command::new("sudo")
                .args(["localectl", "set-x11-keymap", country.keyboard])
                .status();
            let _ = std::process::Command::new("sudo")
                .args(["timedatectl", "set-timezone", country.timezone])
                .status();

            if let Ok(mut cfg) = ConfigManager::new() {
                let _ = cfg.set("system.keyboard_layout", serde_json::json!(country.keyboard));
                let _ = cfg.set("system.timezone", serde_json::json!(country.timezone));
                let _ = cfg.set("system.language", serde_json::json!(country.language));
                let _ = cfg.set(
                    "system.time_format_24h",
                    serde_json::json!(country.time_format_24h),
                );
            }
        }

        // Update SSH password from default to master password.
        let _ = std::process::Command::new("sudo")
            .args(["chpasswd"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin
                        .write_all(format!("aios:{}\n", result.master_password).as_bytes());
                }
                child.wait()
            });

        // Update display name and hostname.
        crate::ui::chat_view::set_assistant_display_name(&result.assistant_name);
        let _ = std::process::Command::new("sudo")
            .args(["hostnamectl", "set-hostname", &result.machine_name])
            .status();
        let _ = std::process::Command::new("sudo")
            .args(["systemctl", "restart", "avahi-daemon"])
            .status();

        // If installation was done, reboot dialog is showing -- skip transition.
        if result.installed_to_drive {
            info!("Installation completed -- reboot dialog showing, skipping normal mode");
            return;
        }

        // Finalize boot in an idle callback to avoid panicking inside
        // the on_complete GTK closure (GTK signal handlers abort on panic).
        let rt_for_boot = rt_ref.clone();
        let queue_for_boot = queue_for_transition.clone();
        let cv = chat_view_ref.clone();
        let pi = prompt_ref.clone();
        let co = channel_overlay_ref.clone();
        let win = window_ref.clone();
        let vu = vu_meter_for_transition.clone();
        gtk4::glib::idle_add_local_once(move || {
            let config = load_config();
            let ui = crate::boot_context::BootUi {
                chat_view: cv.clone(),
                prompt_input: pi,
                channel_overlay: co,
                window: win.clone(),
                vu_meter: vu,
            };
            crate::boot_context::finalize_boot(config, rt_for_boot, queue_for_boot, &ui, None);
            cv.add_message("system", &t("setup.type_message"));
            win.queue_draw();
        });
    });

    // During setup, prompt feeds into the setup conversation.
    let setup_ref = setup.clone();
    prompt_input.on_submit(move |text| {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        if setup_ref.is_active() {
            setup_ref.on_voice_input(&text);
        }
    });

    setup.start();
    window.present();
}

// ---------------------------------------------------------------------------
// Autoconfig path
// ---------------------------------------------------------------------------

/// Apply autoconfig -- show status, create vault, store keys, configure system,
/// then boot normally.
pub(crate) fn apply_autoconfig(
    app: &adw::Application,
    rt: tokio::runtime::Handle,
    auto: aios_core::config::autoconfig::AutoConfig,
    queue: SharedQueue,
) {
    use aios_core::secure::{SecretEntry, SecretKind, Vault};
    use aios_core::types::MessageLevel;

    let chat_view = ChatView::new();
    let prompt_input = PromptInput::new();
    let channel_overlay = ChannelOverlay::new();

    let mut config = ConfigManager::new().unwrap_or_else(|e| {
        warn!("Config load failed: {e}");
        ConfigManager::with_path(std::path::PathBuf::from("/tmp/.aios/config.json")).unwrap()
    });

    aios_core::i18n::init();
    aios_core::i18n::set_language(&auto.system.language);

    let active_display = crate::providers::find_by_id(&auto.provider.primary)
        .map(|p| p.display_name)
        .unwrap_or("AiOS");
    let mw = main_window::build_main_window(
        app,
        &chat_view,
        &prompt_input,
        &channel_overlay,
        &["AiOS"],
        active_display,
    );
    let window = mw.window;
    let vu_meter_autoconfig = mw.vu_meter;

    // Hide prompt + settings until downloads and initialization are complete.
    prompt_input.widget().set_visible(false);
    main_window::set_settings_button_visible(&window, false);
    // DON'T hide the prompt in autoconfig — it will be needed immediately
    // after the transition to normal mode. Hiding + showing in an idle
    // callback causes GTK layout issues where the widget never reappears.

    // Web server will be started in transition_to_normal_mode (needs Tokio runtime).

    // Build and show boot status.
    let boot_status_text = boot_status::build_boot_status(&config);
    chat_view.add_level_message(MessageLevel::Info, &boot_status_text);

    // Push boot status to queue.
    {
        let mut q = queue.lock().unwrap();
        q.push(aios_core::queue::QueuedMessage {
            role: aios_core::types::Role::System,
            channel: aios_core::channel::ChannelKind::System,
            source: Some("system".into()),
            content: Some(boot_status_text),
            level: Some(MessageLevel::Info),
            ..Default::default()
        });
    }

    // Show autoconfig detection message with masked values.
    let mask = |s: &str| -> String {
        if s.is_empty() {
            "(empty)".to_string()
        } else if s.len() <= 8 {
            "****".to_string()
        } else {
            format!("{}****{}", &s[..4], &s[s.len() - 4..])
        }
    };

    // Build API key lines for all configured providers.
    let api_key_lines: String = [
        ("Claude", &auto.provider.claude_api_key),
        ("OpenAI", &auto.provider.openai_api_key),
        ("DeepSeek", &auto.provider.deepseek_api_key),
        ("Mistral", &auto.provider.mistral_api_key),
        ("Groq", &auto.provider.groq_api_key),
        ("Gemini", &auto.provider.gemini_api_key),
    ]
    .iter()
    .filter(|(_, key)| !key.is_empty())
    .map(|(name, key)| format!("**{name} API Key:** {}", mask(key)))
    .collect::<Vec<_>>()
    .join("\n");

    // Resolve Main AI and Sentinel display info.
    let main_prov_display = crate::providers::find_by_id(&auto.ai.main_provider)
        .map(|p| p.display_name)
        .unwrap_or(auto.ai.main_provider.as_str());
    let main_model_human = crate::providers::model_human_name(&auto.ai.main_model);
    let sentinel_model_display = if auto.ai.sentinel_model.is_empty() {
        "not configured".to_string()
    } else {
        auto.ai.sentinel_model.clone()
    };

    let autoconfig_msg = format!(
        "**Autoconfig detected** \u{2014} applying unattended configuration:\n\n\
         **Main AI:** {main_prov_display} \u{2014} {main_model_human}\n\
         **Sentinel:** {sentinel_model_display}\n\
         {api_key_lines}\n\
         **Keyboard:** {}\n\
         **Language:** {}\n\
         **Timezone:** {}\n\
         **Hostname:** {}\n\
         **Password:** ****\n\
         **Install to disk:** {}\n\
         **Assistant name:** {}",
        auto.system.keyboard,
        auto.system.language,
        if auto.system.timezone.is_empty() {
            "auto"
        } else {
            &auto.system.timezone
        },
        auto.system.hostname,
        if auto.install.enabled {
            "yes"
        } else {
            "no (live mode)"
        },
        auto.assistant.name,
    );
    chat_view.add_level_message(MessageLevel::Info, &autoconfig_msg);

    // 1. Create vault.
    let vault_path = ConfigManager::default_config_dir().join("vault.enc");
    let mut vault = Vault::new(vault_path);
    if let Err(e) = vault.create(&auto.system.master_password) {
        error!("Autoconfig: failed to create vault: {e}");
        chat_view.add_level_message(
            MessageLevel::Error,
            &format!("Autoconfig failed: {e}\nFalling back to interactive setup."),
        );
        window.present();
        return;
    }

    // 2. Store API keys — helper to avoid duplication.
    let store_key = |vault: &mut Vault, id: &str, label: &str, value: &str| {
        if !value.is_empty() {
            let entry = SecretEntry {
                kind: SecretKind::ApiKey,
                value: value.to_string(),
                label: label.to_string(),
                created: chrono::Utc::now(),
                last_accessed: None,
            };
            let _ = vault.set(id, entry);
        }
    };
    store_key(&mut vault, "claude_api_key", "Claude API Key", &auto.provider.claude_api_key);
    store_key(&mut vault, "openai_api_key", "OpenAI API Key", &auto.provider.openai_api_key);
    store_key(&mut vault, "deepseek_api_key", "DeepSeek API Key", &auto.provider.deepseek_api_key);
    store_key(&mut vault, "mistral_api_key", "Mistral API Key", &auto.provider.mistral_api_key);
    store_key(&mut vault, "groq_api_key", "Groq API Key", &auto.provider.groq_api_key);
    store_key(&mut vault, "gemini_api_key", "Gemini API Key", &auto.provider.gemini_api_key);

    // 3. Write config.
    let _ = config.set(
        "llm.claude_api_key",
        serde_json::json!(auto.provider.claude_api_key),
    );
    let _ = config.set("llm.openai_api_key", serde_json::json!(auto.provider.openai_api_key));
    let _ = config.set("llm.deepseek_api_key", serde_json::json!(auto.provider.deepseek_api_key));
    let _ = config.set("llm.mistral_api_key", serde_json::json!(auto.provider.mistral_api_key));
    let _ = config.set("llm.groq_api_key", serde_json::json!(auto.provider.groq_api_key));
    let _ = config.set("llm.gemini_api_key", serde_json::json!(auto.provider.gemini_api_key));

    // AI model selection — main AI and Sentinel.
    let main_provider = if auto.ai.main_provider.is_empty() {
        &auto.provider.primary
    } else {
        &auto.ai.main_provider
    };
    let _ = config.set("llm.provider", serde_json::json!(main_provider));
    if main_provider == "ollama" {
        let _ = config.set("llm.ollama_enabled", serde_json::json!(true));
    }
    if !auto.ai.main_model.is_empty() {
        let model_key = format!("llm.{main_provider}_model");
        let _ = config.set(&model_key, serde_json::json!(auto.ai.main_model));
    }
    // Set sentinel model — if empty, leave empty (user must install during setup).
    let _ = config.set("llm.sentinel_model", serde_json::json!(auto.ai.sentinel_model));

    let _ = config.set("assistant.name", serde_json::json!(auto.assistant.name));
    let _ = config.set(
        "assistant.language",
        serde_json::json!(auto.system.language),
    );
    let _ = config.set("llm.effort", serde_json::json!(auto.assistant.effort));

    // 4. Show success.
    chat_view.add_level_message(
        MessageLevel::Success,
        &format!(
            "**Autoconfig applied successfully!**\n\n\
             Vault created, API keys stored, system configured.\n\
             Main AI: **{main_prov_display} \u{2014} {main_model_human}** | \
             Sentinel: **{sentinel_model_display}** | \
             Keyboard: **{}** | Mode: **{}**",
            auto.system.keyboard,
            if auto.install.enabled {
                "hard drive install"
            } else {
                "live ISO"
            },
        ),
    );

    // Present window now so the user sees boot status + download progress.
    window.present();

    // 4b. Download Ollama models (sentinel + main AI if local provider).
    if !auto.ai.sentinel_model.is_empty() {
        autoconfig_download_model(&chat_view, "Sentinel", &auto.ai.sentinel_model);
    }
    if main_provider == "ollama"
        && !auto.ai.main_model.is_empty()
        && auto.ai.main_model != auto.ai.sentinel_model
    {
        autoconfig_download_model(&chat_view, "Main AI", &auto.ai.main_model);
    }

    info!("Autoconfig applied -- finalizing boot");

    // 5. Finalize boot — init LLM, tools, channels, voice.
    // Run synchronously (not idle_add) so we can catch panics.
    let ui = crate::boot_context::BootUi {
        chat_view: chat_view.clone(),
        prompt_input,
        channel_overlay,
        window: window.clone(),
        vu_meter: vu_meter_autoconfig,
    };

    let config = load_config();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::boot_context::finalize_boot(config, rt, queue, &ui, None);
    }));
    match result {
        Ok(()) => {
            info!("Boot finalized successfully");

            // Warm up the LLM if it's a local model (Ollama needs to load weights).
            let provider = load_config().get_str("llm.provider", "");
            if provider == "ollama" {
                let model = load_config().get_str("llm.ollama_model", "llama3.2");
                chat_view.update_or_add_progress(
                    &format!("Loading AI model **{model}** into memory... (first time takes ~20s)")
                );
                while gtk4::glib::MainContext::default().iteration(false) {}

                let (tx, rx) = std::sync::mpsc::channel::<bool>();
                let model_c = model.clone();
                std::thread::spawn(move || {
                    // Send a tiny prompt to force model loading.
                    let _ = std::process::Command::new("curl")
                        .args([
                            "-s", "http://localhost:11434/api/generate",
                            "-d", &format!("{{\"model\":\"{model_c}\",\"prompt\":\"hi\",\"stream\":false}}"),
                        ])
                        .output();
                    let _ = tx.send(true);
                });

                loop {
                    while gtk4::glib::MainContext::default().iteration(false) {}
                    match rx.try_recv() {
                        Ok(_) => {
                            chat_view.update_or_add_progress(
                                &format!("AI model **{model}** ready.")
                            );
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                    }
                }
            }

            // Only now show the prompt input + settings button.
            ui.prompt_input.widget().set_visible(true);
            ui.prompt_input.widget().set_sensitive(true);
            main_window::set_settings_button_visible(&window, true);
        }
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!("finalize_boot panicked: {msg}");
            chat_view.add_level_message(
                aios_core::types::MessageLevel::Error,
                &format!("Boot error: {msg}"),
            );
            // Show prompt even on error so user can type /help.
            ui.prompt_input.widget().set_visible(true);
            ui.prompt_input.widget().set_sensitive(true);
        }
    }
    ui.chat_view.add_message("system", &t("setup.type_message"));
    ui.window.queue_draw();
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Connect signal handlers shared by all boot paths
/// (provider dropdown, settings, mic, speaker, prompt submit).
pub(crate) fn connect_common_signals(
    state: &Rc<RefCell<AiosApp>>,
    chat_view: &ChatView,
    prompt_input: &PromptInput,
    window: &adw::ApplicationWindow,
) {
    // Provider dropdown.
    let state_ref = state.clone();
    let chat_view_ref = chat_view.clone();
    main_window::connect_provider_dropdown(window, move |provider_name| {
        let mut s = state_ref.borrow_mut();
        let name = crate::providers::display_name_to_id(provider_name);
        let _ = s.config.set("llm.provider", serde_json::json!(name));
        if let Ok(mut llm) = s.llm.try_lock() {
            if llm.set_active(&name).is_err() {
                chat_view_ref.add_message("system", &format!("Provider '{name}' not available"));
            } else {
                chat_view_ref.add_message("system", &format!("Switched to {name}"));
            }
        }
    });

    // Settings button.
    let state_ref = state.clone();
    let win_ref = window.clone();
    let win_for_refresh = window.clone();
    main_window::connect_settings_button(window, move || {
        let s = state_ref.borrow();
        let refresh_win = win_for_refresh.clone();
        settings_dialog::show_settings(&win_ref, &s.config, move || {
            // Refresh title bar provider dropdown after settings close.
            let config_dir = ConfigManager::default_config_dir();
            if let Ok(cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                let names = crate::providers::configured_display_names(&cfg);
                let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                main_window::update_provider_dropdown(&refresh_win, &name_refs);
            }
        });
    });

    // Info button — opens tabbed info dialog (Costs + About).
    let win_ref = window.clone();
    main_window::connect_info_button(window, move || {
        crate::ui::info_dialog::show_info_dialog(&win_ref);
    });

    // Speaker toggle.
    let state_ref = state.clone();
    main_window::connect_speaker_toggle(window, move |active| {
        let mut s = state_ref.borrow_mut();
        let _ = s.config.set("voice.tts_enabled", serde_json::json!(active));
        if !active {
            stop_tts();
        }
        info!("Speaker toggled: {active}");
    });

    // Message submission (Enter key or send button) -- Desktop channel.
    let state_ref = state.clone();
    let chat_view_ref = chat_view.clone();
    let prompt_ref = prompt_input.clone();
    prompt_input.on_submit(move |text| {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }

        if text.starts_with('/') {
            crate::app::AiosApp::handle_command(&state_ref, &chat_view_ref, &text);
            return;
        }

        chat_view_ref.add_message("user", &text);
        while gtk4::glib::MainContext::default().iteration(false) {}
        llm_handler::send_to_llm(&state_ref, &chat_view_ref, &prompt_ref, text);
    });
}

// ---------------------------------------------------------------------------
// Hostname conflict card (shared between first-boot and normal boot)
// ---------------------------------------------------------------------------

/// Check for hostname collision and show a rename card if found.
pub(crate) fn show_hostname_conflict_card(chat_view: &ChatView) {
    if !std::path::Path::new("/tmp/aios-name-conflict").exists() {
        return;
    }

    let conflicting = match std::fs::read_to_string("/tmp/aios-name-conflict") {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };

    let random_suffix = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        % 10000) as u16;
    let suggestion = format!("{conflicting}-{random_suffix}");

    chat_view.add_level_message(
        aios_core::types::MessageLevel::Warning,
        &t_fmt("hostname.conflict.warning", &[("name", &conflicting)]),
    );

    let input_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    input_box.set_margin_top(8);

    let entry = gtk4::Entry::builder()
        .placeholder_text(&t("hostname.conflict.placeholder"))
        .text(&suggestion)
        .hexpand(true)
        .build();
    input_box.append(&entry);

    let hint = gtk4::Label::new(Some(&t("hostname.conflict.hint")));
    hint.add_css_class("dim-label");
    hint.set_halign(gtk4::Align::Start);
    hint.set_wrap(true);
    input_box.append(&hint);

    let error_label = gtk4::Label::new(None);
    error_label.add_css_class("error");
    error_label.set_visible(false);
    error_label.set_halign(gtk4::Align::Start);
    input_box.append(&error_label);

    let apply_btn = gtk4::Button::with_label(&t("hostname.conflict.apply"));
    apply_btn.add_css_class("suggested-action");
    apply_btn.set_halign(gtk4::Align::Start);
    apply_btn.set_margin_top(4);
    input_box.append(&apply_btn);

    let entry_ref = entry.clone();
    let error_ref = error_label.clone();
    let chat_ref = chat_view.clone();
    apply_btn.connect_clicked(move |b| {
        let name = entry_ref.text().to_string().trim().to_lowercase();

        if !aios_core::hostname::is_valid_hostname(&name) {
            error_ref.set_text(&t("hostname.conflict.error_invalid"));
            error_ref.set_visible(true);
            return;
        }

        b.set_sensitive(false);
        error_ref.set_visible(false);

        let _ = std::process::Command::new("sudo")
            .args(["hostnamectl", "set-hostname", &name])
            .status();
        let _ = std::process::Command::new("sudo")
            .args(["systemctl", "restart", "avahi-daemon"])
            .status();

        if let Ok(mut cfg) = ConfigManager::new() {
            let _ = cfg.set("system.machine_name", serde_json::json!(name));
        }

        chat_ref.add_message(
            "system",
            &t_fmt("hostname.conflict.changed", &[("name", &name)]),
        );
        let _ = std::fs::remove_file("/tmp/aios-name-conflict");
    });

    chat_view.add_setup_card(
        "network-server-symbolic",
        &t("hostname.conflict.title"),
        &t_fmt(
            "hostname.conflict.description",
            &[("name", &conflicting)],
        ),
        Some(input_box.upcast_ref()),
    );
}

// ---------------------------------------------------------------------------
// Autoconfig model download helper
// ---------------------------------------------------------------------------

/// Download an Ollama model during autoconfig, showing status in the chat view.
///
/// `label` is a human-readable role (e.g. "Sentinel", "Main AI") used in status messages.
fn autoconfig_download_model(
    chat_view: &crate::ui::chat_view::ChatView,
    label: &str,
    model: &str,
) {
    use aios_core::types::MessageLevel;

    chat_view.add_level_message(
        MessageLevel::Info,
        &format!("Downloading {label} model: **{model}**...\nThis may take a few minutes."),
    );
    while gtk4::glib::MainContext::default().iteration(false) {}

    let client = aios_llm::OllamaClient::new();
    if let Err(e) = client.ensure_running() {
        warn!("Failed to start Ollama for {label}: {e}");
        chat_view.add_level_message(
            MessageLevel::Warning,
            &format!("Ollama not available: {e}. {label} model not installed."),
        );
        return;
    }
    if client.is_model_installed(model) {
        info!("{label} model {model} already installed");
        chat_view.add_level_message(
            MessageLevel::Success,
            &format!("{label} model **{model}** already installed."),
        );
        return;
    }

    // Channel for completion, plus a shared progress string for the UI.
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let progress_text = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let progress_writer = progress_text.clone();
    let model_owned = model.to_string();
    let label_owned = label.to_string();
    std::thread::spawn(move || {
        let client = aios_llm::OllamaClient::new();
        let lbl = label_owned;
        let pw = progress_writer;
        let result = client.pull_model(&model_owned, |p| {
            if p.total > 0 {
                let pct = (p.completed as f64 / p.total as f64 * 100.0) as u32;
                let mb_done = p.completed / (1024 * 1024);
                let mb_total = p.total / (1024 * 1024);
                let msg = format!("{lbl}: {pct}% ({mb_done}/{mb_total} MB) — {}", p.status);
                if let Ok(mut guard) = pw.lock() {
                    *guard = msg;
                }
            }
        });
        let _ = tx.send(result);
    });

    // Poll the channel without blocking the GTK main loop.
    // Show live download progress in the chat.
    let label_str = label.to_string();
    let model_str = model.to_string();
    let cv = chat_view.clone();
    let mut last_progress = String::new();
    loop {
        // Process pending GTK events so the UI stays responsive.
        while gtk4::glib::MainContext::default().iteration(false) {}

        // Update progress display.
        if let Ok(guard) = progress_text.lock() {
            if !guard.is_empty() && *guard != last_progress {
                last_progress = guard.clone();
                cv.update_or_add_progress(&last_progress);
            }
        }

        match rx.try_recv() {
            Ok(Ok(())) => {
                // Verify the model is actually installed before claiming success.
                let verify_client = aios_llm::OllamaClient::new();
                if verify_client.is_model_installed(&model_str) {
                    info!("{label_str} model {model_str} downloaded and verified");
                    cv.update_or_add_progress(&format!(
                        "{label_str} model **{model_str}** installed."
                    ));
                } else {
                    warn!("{label_str} model {model_str}: pull reported OK but model not found!");
                    cv.add_level_message(
                        MessageLevel::Warning,
                        &format!("{label_str} model **{model_str}** download incomplete. Retrying..."),
                    );
                    // Retry once.
                    let retry_client = aios_llm::OllamaClient::new();
                    let _ = retry_client.pull_model(&model_str, |_| {});
                    if retry_client.is_model_installed(&model_str) {
                        cv.update_or_add_progress(&format!(
                            "{label_str} model **{model_str}** installed (retry)."
                        ));
                    } else {
                        cv.add_level_message(
                            MessageLevel::Error,
                            &format!("{label_str} model **{model_str}** failed to install."),
                        );
                    }
                }
                break;
            }
            Ok(Err(e)) => {
                warn!("{label_str} model download failed: {e}");
                cv.add_level_message(
                    MessageLevel::Warning,
                    &format!("{label_str} download failed: {e}"),
                );
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Still downloading — sleep briefly and keep pumping GTK events.
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                warn!("{label_str} download thread disconnected");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Load configuration, falling back to a temp path if needed.
pub(crate) fn load_config() -> ConfigManager {
    match ConfigManager::new() {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to load config, using defaults: {e}");
            match ConfigManager::with_path(std::path::PathBuf::from("/tmp/.aios/config.json")) {
                Ok(c) => c,
                Err(e2) => {
                    error!("Fallback config also failed: {e2}");
                    panic!("Cannot initialize configuration from any path");
                }
            }
        }
    }
}

/// Initialize the i18n system from config.
pub(crate) fn init_i18n(config: &ConfigManager) {
    aios_core::i18n::init();
    let lang = config.get_str("assistant.language", "");
    if lang.is_empty() {
        let detected = aios_core::i18n::detect_system_language();
        aios_core::i18n::set_language(&detected);
    } else {
        aios_core::i18n::set_language(&lang);
    }
}
