//! AiOS application orchestrator.
//!
//! [`AiosApp`] is a plain Rust struct (no GObject subclassing) that holds
//! all subsystem managers and wires them together via closures and
//! [`glib::MainContext::channel`] for async communication from Tokio back
//! to the GTK main thread.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use tracing::{info, warn};

use aios_core::config::commands::{CommandHandler, CommandResult};
use aios_core::config::ConfigManager;
use aios_core::secure::Vault;
use aios_core::secure::vault::{SecretEntry, SecretKind};
use aios_llm::{ClaudeProvider, LlmManager, OpenAIProvider};
use aios_tools::ToolRegistry;
use aios_tools::builtin::ui_panel::UiPanelTool;

use crate::ui::channel_overlay::ChannelOverlay;
use crate::ui::chat_view::ChatView;
use crate::ui::first_boot::SetupConversation;
use crate::ui::main_window;
use crate::ui::panel_renderer::PanelRenderer;
use crate::ui::prompt_input::PromptInput;
use crate::ui::settings_dialog;

// ---------------------------------------------------------------------------
// AiosApp
// ---------------------------------------------------------------------------

/// Application-level state shared across signal handlers.
///
/// Wrapped in `Rc<RefCell<...>>` for the GTK main-thread parts, and
/// `Arc<tokio::sync::Mutex<...>>` for anything shared with the Tokio runtime.
pub struct AiosApp {
    config: ConfigManager,
    llm: Arc<tokio::sync::Mutex<LlmManager>>,
    #[allow(dead_code)]
    tools: ToolRegistry,
    conversation: Vec<aios_core::types::Message>,
    rt: tokio::runtime::Handle,
}

impl AiosApp {
    /// Called from `Application::connect_activate`. Builds the entire UI and
    /// wires up signals.
    pub fn activate(app: &adw::Application, rt: tokio::runtime::Handle) {
        // Check if the vault exists. If not, run the first-boot setup.
        let vault_path = ConfigManager::default_config_dir().join("vault.enc");
        let vault = Vault::new(vault_path);

        if !vault.exists() {
            info!("No vault found — launching first-boot setup conversation");
            Self::run_first_boot_setup(app, rt);
            return;
        }

        // Vault exists — proceed with normal startup.
        Self::activate_main(app, rt);
    }

    /// Run the first-boot setup as a conversation in the main chat view.
    ///
    /// Builds the normal main window (fullscreen) but starts a
    /// [`SetupConversation`] that drives setup cards through the chat view.
    /// Once setup completes, the vault is created, secrets are stored, the
    /// LLM manager is configured, and normal chat mode begins.
    ///
    /// **Channel restriction**: First-boot setup can only happen on the
    /// Desktop (GTK) or Web channel. Remote channels (Signal, Voice, etc.)
    /// are not started until `activate_main()`, which runs *after* the vault
    /// has been created. This means the setup wizard is never reachable from
    /// remote channels — by the time they are online, the vault already
    /// exists and `activate()` takes the normal startup path.
    fn run_first_boot_setup(app: &adw::Application, rt: tokio::runtime::Handle) {
        // Build the main UI — same window as normal mode.
        let chat_view = ChatView::new();
        let prompt_input = PromptInput::new();
        let channel_overlay = ChannelOverlay::new();

        let window = main_window::build_main_window(app, &chat_view, &prompt_input, &channel_overlay);

        // Create the setup conversation.
        let setup = SetupConversation::new(chat_view.clone());

        // On completion: create vault, store secrets, transition to normal mode.
        let app_ref = app.clone();
        let rt_ref = rt.clone();
        let chat_view_ref = chat_view.clone();
        let prompt_ref = prompt_input.clone();
        let window_ref = window.clone();
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
                // Continue anyway — the user can set keys later.
            } else {
                for provider in &result.providers {
                    let key_name = format!("{}_api_key", provider.name);
                    let label = match provider.name.as_str() {
                        "claude" => "Claude API Key",
                        "openai" => "OpenAI API Key",
                        other => other,
                    };
                    let entry = SecretEntry {
                        kind: SecretKind::ApiKey,
                        value: provider.api_key.clone(),
                        label: label.to_string(),
                        created: chrono::Utc::now(),
                        last_accessed: None,
                    };
                    if let Err(e) = vault.set(&key_name, entry) {
                        warn!("Failed to store {key_name} in vault: {e}");
                    }
                }
                info!("Vault created with {} secret(s)", result.providers.len());
            }

            // Store provider/key info in the config so the LLM manager
            // can pick them up immediately.
            if let Ok(mut config) = ConfigManager::new() {
                if let Some(primary) = result.providers.first() {
                    let _ = config.set(
                        "llm.provider",
                        serde_json::json!(primary.name),
                    );
                }
                for p in &result.providers {
                    match p.name.as_str() {
                        "claude" => {
                            let _ = config.set(
                                "llm.claude_api_key",
                                serde_json::json!(p.api_key),
                            );
                        }
                        "openai" => {
                            let _ = config.set(
                                "llm.openai_api_key",
                                serde_json::json!(p.api_key),
                            );
                        }
                        _ => {}
                    }
                }
            }

            // Transition to normal mode: initialize LLM, wire up the real
            // prompt handler, and show the ready message.
            Self::transition_to_normal_mode(
                &app_ref,
                rt_ref.clone(),
                &chat_view_ref,
                &prompt_ref,
                &window_ref,
            );
        });

        // During setup, the prompt input feeds into the setup conversation
        // as voice-like text input (for steps that accept it).
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

        // Start the conversation.
        setup.start();

        window.present();
    }

    /// After first-boot setup completes, configure the app for normal chat mode.
    ///
    /// This initializes the LLM manager, creates the shared application state,
    /// and re-wires the prompt input for real chat.
    fn transition_to_normal_mode(
        _app: &adw::Application,
        rt: tokio::runtime::Handle,
        chat_view: &ChatView,
        prompt_input: &PromptInput,
        window: &adw::ApplicationWindow,
    ) {
        // Load configuration (now includes the keys we just stored).
        let config = match ConfigManager::new() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to load config, using defaults: {e}");
                ConfigManager::with_path(std::path::PathBuf::from("/tmp/.aios/config.json"))
                    .expect("fallback config path must work")
            }
        };

        // Initialize tool registry.
        let mut tools = ToolRegistry::new();
        tools.load_builtins();
        info!("Loaded {} built-in tools", tools.len());

        // Initialize LLM providers.
        let mut llm = LlmManager::new();
        Self::init_llm(&config, &mut llm);

        // Create a UiPanelTool with the GTK panel renderer callback.
        let ui_panel_tool = Self::create_ui_panel_tool(window);

        // Wire tool executor into LLM manager.
        let tool_registry = Arc::new(std::sync::Mutex::new(ToolRegistry::new()));
        {
            let mut tr = tool_registry.lock().unwrap();
            tr.load_builtins();
            // Replace the callback-less builtin with the GTK-wired one.
            let _ = tr.unregister("ui_panel");
            let _ = tr.register(Box::new(UiPanelToolWrapper(ui_panel_tool.clone())));
        }
        let tr_for_executor = tool_registry.clone();
        llm.set_tool_executor(Arc::new(move |name, args, channel| {
            let registry = tr_for_executor.lock().unwrap();
            let result = registry.execute_on_channel(&name, args, &channel);
            if result.success {
                result.output
            } else {
                format!("Tool error: {}", result.output)
            }
        }));

        // Create shared application state.
        let state = Rc::new(RefCell::new(AiosApp {
            config,
            llm: Arc::new(tokio::sync::Mutex::new(llm)),
            tools,
            conversation: Vec::new(),
            rt,
        }));

        // Show the transition message.
        chat_view.add_message(
            "system",
            "Setup complete. How can I help?",
        );

        // --- Connect signals ---

        // Provider dropdown changed.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        main_window::connect_provider_dropdown(window, move |provider_name| {
            let mut s = state_ref.borrow_mut();
            let name = provider_name.to_lowercase();
            let _ = s.config.set("llm.provider", serde_json::json!(name));
            if let Ok(mut llm) = s.llm.try_lock() {
                if llm.set_active(&name).is_err() {
                    chat_view_ref
                        .add_message("system", &format!("Provider '{name}' not available"));
                } else {
                    chat_view_ref.add_message("system", &format!("Switched to {name}"));
                }
            }
        });

        // Settings button.
        let state_ref = state.clone();
        let win_ref = window.clone();
        main_window::connect_settings_button(window, move || {
            let s = state_ref.borrow();
            settings_dialog::show_settings(&win_ref, &s.config);
        });

        // Mic toggle.
        let state_ref = state.clone();
        main_window::connect_mic_toggle(window, move |active| {
            let mut s = state_ref.borrow_mut();
            let _ = s.config.set("voice.stt_enabled", serde_json::json!(active));
            info!("Mic toggled: {active}");
        });

        // Speaker toggle.
        let state_ref = state.clone();
        main_window::connect_speaker_toggle(window, move |active| {
            let mut s = state_ref.borrow_mut();
            let _ = s.config.set("voice.tts_enabled", serde_json::json!(active));
            info!("Speaker toggled: {active}");
        });

        // Re-wire message submission for normal chat mode.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        let prompt_ref = prompt_input.clone();
        prompt_input.on_submit(move |text| {
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }

            // Check if it's a slash command.
            if text.starts_with('/') {
                Self::handle_command(&state_ref, &chat_view_ref, &text);
                return;
            }

            // Display the user message immediately.
            chat_view_ref.add_message("user", &text);

            // Send to LLM asynchronously.
            Self::send_to_llm(&state_ref, &chat_view_ref, &prompt_ref, text);
        });
    }

    /// Normal application startup — builds the UI and wires up signals.
    fn activate_main(app: &adw::Application, rt: tokio::runtime::Handle) {
        // Load configuration.
        let mut config = match ConfigManager::new() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to load config, using defaults: {e}");
                ConfigManager::with_path(std::path::PathBuf::from("/tmp/.aios/config.json"))
                    .expect("fallback config path must work")
            }
        };

        // Initialize tool registry with built-in tools.
        let mut tools = ToolRegistry::new();
        tools.load_builtins();
        info!("Loaded {} built-in tools", tools.len());

        // Initialize LLM providers.
        let mut llm = LlmManager::new();
        Self::init_llm(&config, &mut llm);

        // Build the UI first so we can wire the UiPanelTool callback.
        let chat_view = ChatView::new();
        let prompt_input = PromptInput::new();
        let channel_overlay = ChannelOverlay::new();

        let window = main_window::build_main_window(
            app,
            &chat_view,
            &prompt_input,
            &channel_overlay,
        );

        // Create a UiPanelTool with the GTK panel renderer callback.
        let ui_panel_tool = Self::create_ui_panel_tool(&window);

        // Wire tool executor into LLM manager.
        // The LLM executor gets its own registry that includes the
        // ui_panel tool with the GTK callback already set.
        let tool_registry = Arc::new(std::sync::Mutex::new(ToolRegistry::new()));
        {
            let mut tr = tool_registry.lock().unwrap();
            tr.load_builtins();
            // The builtin UiPanelTool has no callback — replace it with
            // the one that has the GTK callback wired in.
            let _ = tr.unregister("ui_panel");
            let _ = tr.register(Box::new(UiPanelToolWrapper(ui_panel_tool.clone())));
        }
        let tr_for_executor = tool_registry.clone();
        llm.set_tool_executor(Arc::new(move |name, args, channel| {
            let registry = tr_for_executor.lock().unwrap();
            let result = registry.execute_on_channel(&name, args, &channel);
            if result.success {
                result.output
            } else {
                format!("Tool error: {}", result.output)
            }
        }));

        // Read channel config (used by both boot status and channel startup).
        let web_enabled = config.get_bool("channels.web.enabled", false);
        let web_port = config.get_str("channels.web.port", "80");
        let signal_enabled = config.get_bool("channels.signal.enabled", false);
        let signal_phone = config.get_str("channels.signal.phone", "");

        // Build boot status text (used by GTK + Web).
        let boot_status_text = {
            use aios_core::types::{BootStatus, StatusLine};

            let mut status = BootStatus::new();
            status.add(StatusLine::new("Desktop", true, "GTK4/libadwaita"));
            if web_enabled {
                status.add(StatusLine::new("Web Channel", true, format!("http://aios.local:{web_port}")));
            } else {
                status.add(StatusLine::new("Web Channel", false, "disabled (/channel web on)"));
            }
            if signal_enabled && !signal_phone.is_empty() {
                status.add(StatusLine::new("Signal", true, &signal_phone));
            } else if signal_enabled {
                status.add(StatusLine::new("Signal", false, "enabled but no phone configured"));
            } else {
                status.add(StatusLine::new("Signal", false, "disabled (/channel signal on)"));
            }
            let provider = config.get_str("llm.provider", "claude");
            let has_key = match provider.as_str() {
                "claude" => !config.get_str("llm.claude_api_key", "").is_empty(),
                "openai" => !config.get_str("llm.openai_api_key", "").is_empty(),
                _ => false,
            };
            if has_key {
                status.add(StatusLine::new("LLM Provider", true, &provider));
            } else {
                status.add(StatusLine::new("LLM Provider", false, format!("{provider} (no API key — use /key)")));
            }
            let stt = config.get_bool("voice.stt_enabled", true);
            let tts = config.get_bool("voice.tts_enabled", true);
            status.add(StatusLine::new("Voice", stt || tts,
                format!("STT: {} | TTS: {}", if stt { "on" } else { "off" }, if tts { "on" } else { "off" })));
            status.format()
        };

        // Show boot status on Desktop.
        chat_view.add_level_message(aios_core::types::MessageLevel::Info, &boot_status_text);
        chat_view.add_message("system", "Type a message or use /help to see available commands.");

        // --- Channel infrastructure ---

        // Create the shared AppRuntime for multi-channel orchestration.
        let runtime = aios_core::channel::AppRuntime::new();

        // Register Desktop channel (always available).
        runtime.switcher.register_channel(
            aios_core::channel::ChannelKind::Desktop,
            aios_core::channel::ChannelContext::desktop(),
        );

        // Start Web server if enabled.
        let web_server = if web_enabled {
            let port: u16 = web_port.parse().unwrap_or(80);
            let web_tx = runtime.message_sender();
            // Generate or load auth token for web access.
            let web_token = config.get_str("channels.web.token", "");
            let web_token = if web_token.is_empty() {
                let token = uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_string();
                let _ = config.set("channels.web.token", serde_json::json!(token));
                info!("Generated web auth token: {token}");
                Some(token)
            } else {
                Some(web_token)
            };
            let server = aios_web::server::WebServer::new(
                port, web_tx, web_token,
                Some(runtime.switcher.clone()),
                Some(boot_status_text.clone()),
            );
            let response_tx = server.response_tx.clone();
            server.start();
            runtime.switcher.register_channel(
                aios_core::channel::ChannelKind::Web,
                aios_core::channel::ChannelContext::web(),
            );
            info!("Web channel started on port {port}");
            Some(response_tx)
        } else {
            None
        };

        // Start Signal listener if enabled.
        let signal_sender = if signal_enabled && !signal_phone.is_empty() {
            let contacts_str = config.get_str("channels.signal.allowed_contacts", "");
            let allowed: Vec<String> = contacts_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let listener = aios_signal::SignalListener::new(
                signal_phone.clone(),
                allowed,
            );
            let sig_tx = runtime.message_sender();
            listener.start(sig_tx);
            runtime.switcher.register_channel(
                aios_core::channel::ChannelKind::Signal,
                aios_core::channel::ChannelContext::signal(),
            );
            info!("Signal channel started for {signal_phone}");
            Some(Arc::new(aios_signal::SignalSender::new(signal_phone)))
        } else {
            None
        };

        // Wire channel switcher to the overlay (item 5).
        // GTK widgets aren't Send, so we use a std::sync::mpsc channel to
        // bridge from the switcher callback (any thread) to the GTK thread.
        {
            let (overlay_tx, overlay_rx) =
                std::sync::mpsc::channel::<aios_core::channel::ChannelKind>();

            runtime.switcher.on_switch(Arc::new(move |_old, new| {
                let _ = overlay_tx.send(new);
            }));

            // GTK-side: poll the channel and update the overlay.
            let overlay_for_poll = channel_overlay.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                while let Ok(new_kind) = overlay_rx.try_recv() {
                    if new_kind == aios_core::channel::ChannelKind::Desktop {
                        overlay_for_poll.hide();
                    } else {
                        overlay_for_poll.show(new_kind);
                    }
                }
                glib::ControlFlow::Continue
            });

            // "Switch back here" button → switch to Desktop.
            let switcher_for_btn = runtime.switcher.clone();
            channel_overlay.on_switch_back(move || {
                let _ = switcher_for_btn.switch_to(aios_core::channel::ChannelKind::Desktop);
            });
        }

        // Warn if no API key is configured.
        {
            let provider = config.get_str("llm.provider", "claude");
            let has_key = match provider.as_str() {
                "claude" => !config.get_str("llm.claude_api_key", "").is_empty(),
                "openai" => !config.get_str("llm.openai_api_key", "").is_empty(),
                _ => false,
            };
            if !has_key {
                chat_view.add_level_message(
                    aios_core::types::MessageLevel::Warning,
                    "No API key configured. Use **/key claude sk-ant-...** or **/key openai sk-...** to set one.",
                );
            }
        }

        // Create shared application state.
        let llm_arc = Arc::new(tokio::sync::Mutex::new(llm));
        let state = Rc::new(RefCell::new(AiosApp {
            config,
            llm: llm_arc.clone(),
            tools,
            conversation: Vec::new(),
            rt: rt.clone(),
        }));

        // --- Unified message loop (item 3) ---
        // Poll incoming messages from ALL channels (Web, Signal, Desktop)
        // and route them through the LLM.
        {
            let msg_rx_holder = runtime.clone();
            let state_for_loop = state.clone();
            let chat_for_loop = chat_view.clone();
            let switcher_for_loop = runtime.switcher.clone();
            let llm_for_loop = llm_arc.clone();
            let rt_for_loop = rt.clone();
            let web_tx = web_server.clone();
            let sig_sender = signal_sender.clone();

            // We take the receiver on the Tokio side and poll it from GTK.
            let (bridge_tx, bridge_rx) =
                std::sync::mpsc::channel::<aios_core::channel::IncomingMessage>();

            // Tokio task: drain AppRuntime's message channel into the std bridge.
            rt.spawn(async move {
                if let Some(mut rx) = msg_rx_holder.take_message_rx().await {
                    while let Some(msg) = rx.recv().await {
                        if bridge_tx.send(msg).is_err() {
                            break;
                        }
                    }
                }
            });

            // GTK poll: check for incoming messages from remote channels.
            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                while let Ok(msg) = bridge_rx.try_recv() {
                    // Switch channel if needed.
                    if switcher_for_loop.active_kind() != msg.channel {
                        let _ = switcher_for_loop.switch_to(msg.channel);
                    }

                    // Always show in Desktop chat (conversation continuity).
                    chat_for_loop.add_message("user", &msg.text);

                    // Record in conversation history.
                    {
                        let mut s = state_for_loop.borrow_mut();
                        s.conversation.push(aios_core::types::Message::user(&msg.text));
                    }

                    // Handle slash commands from remote channels.
                    if msg.text.starts_with('/') {
                        Self::handle_command(&state_for_loop, &chat_for_loop, &msg.text);
                        continue;
                    }

                    // Send to LLM and route response to active channel (item 4).
                    let llm = llm_for_loop.clone();
                    let history = state_for_loop.borrow().conversation.clone();
                    let text = msg.text.clone();
                    let sender_id = msg.sender_id.clone();
                    let active_channel = switcher_for_loop.active_kind();
                    let (resp_tx, resp_rx) = std::sync::mpsc::channel::<LlmResult>();
                    let web_tx_inner = web_tx.clone();
                    let sig_inner = sig_sender.clone();

                    rt_for_loop.spawn(async move {
                        let mut llm_guard = llm.lock().await;
                        let mut history = history;
                        if history.last().is_some_and(|m| m.role == aios_core::types::Role::User) {
                            history.pop();
                        }
                        let result = llm_guard.chat(&text, Some(&mut history), &[], None).await;
                        drop(llm_guard);

                        match result {
                            Ok(response) => {
                                let content = response.content.unwrap_or_default();
                                // Route to active channel.
                                match active_channel {
                                    aios_core::channel::ChannelKind::Web => {
                                        if let Some(ref tx) = web_tx_inner {
                                            let msg = aios_web::protocol::ServerMessage::Message {
                                                role: "assistant".into(),
                                                content: content.clone(),
                                                level: None,
                                            };
                                            if let Ok(json) = serde_json::to_string(&msg) {
                                                let _ = tx.send(json);
                                            }
                                        }
                                    }
                                    aios_core::channel::ChannelKind::Signal => {
                                        if let Some(ref sender) = sig_inner {
                                            if let Some(ref recipient) = sender_id {
                                                let _ = sender.send_text(recipient, &content).await;
                                            }
                                        }
                                    }
                                    _ => {} // Desktop is handled below via resp_tx.
                                }
                                let _ = resp_tx.send(LlmResult::Success {
                                    content,
                                    updated_history: history,
                                });
                            }
                            Err(e) => {
                                let _ = resp_tx.send(LlmResult::Error(format!("{e}")));
                            }
                        }
                    });

                    // Poll for this response too (displays on Desktop).
                    let chat_for_resp = chat_for_loop.clone();
                    let state_for_resp = state_for_loop.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                        match resp_rx.try_recv() {
                            Ok(LlmResult::Success { content, updated_history }) => {
                                chat_for_resp.add_message("assistant", &content);
                                let mut s = state_for_resp.borrow_mut();
                                s.conversation = updated_history;
                                s.conversation.push(aios_core::types::Message::assistant(&content));
                                glib::ControlFlow::Break
                            }
                            Ok(LlmResult::Error(err)) => {
                                chat_for_resp.add_message("system", &format!("Error: {err}"));
                                glib::ControlFlow::Break
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                        }
                    });
                }
                glib::ControlFlow::Continue
            });
        }

        // --- Connect GTK signals ---

        // Provider dropdown changed.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        main_window::connect_provider_dropdown(&window, move |provider_name| {
            let mut s = state_ref.borrow_mut();
            let name = provider_name.to_lowercase();
            let _ = s.config.set("llm.provider", serde_json::json!(name));
            if let Ok(mut llm) = s.llm.try_lock() {
                if llm.set_active(&name).is_err() {
                    chat_view_ref
                        .add_message("system", &format!("Provider '{name}' not available"));
                } else {
                    chat_view_ref.add_message("system", &format!("Switched to {name}"));
                }
            }
        });

        // Settings button.
        let state_ref = state.clone();
        let win_ref = window.clone();
        main_window::connect_settings_button(&window, move || {
            let s = state_ref.borrow();
            settings_dialog::show_settings(&win_ref, &s.config);
        });

        // Mic toggle.
        let state_ref = state.clone();
        main_window::connect_mic_toggle(&window, move |active| {
            let mut s = state_ref.borrow_mut();
            let _ = s.config.set("voice.stt_enabled", serde_json::json!(active));
            info!("Mic toggled: {active}");
        });

        // Speaker toggle.
        let state_ref = state.clone();
        main_window::connect_speaker_toggle(&window, move |active| {
            let mut s = state_ref.borrow_mut();
            let _ = s.config.set("voice.tts_enabled", serde_json::json!(active));
            info!("Speaker toggled: {active}");
        });

        // Message submission (Enter key or send button) — Desktop channel.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        let prompt_ref = prompt_input.clone();
        prompt_input.on_submit(move |text| {
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }

            // Check if it's a slash command.
            if text.starts_with('/') {
                Self::handle_command(&state_ref, &chat_view_ref, &text);
                return;
            }

            // Display the user message immediately.
            chat_view_ref.add_message("user", &text);

            // Send to LLM asynchronously.
            Self::send_to_llm(&state_ref, &chat_view_ref, &prompt_ref, text);
        });

        window.present();
    }

    /// Initialize LLM providers from config.
    fn init_llm(config: &ConfigManager, llm: &mut LlmManager) {
        let claude_key = config.get_str("llm.claude_api_key", "");
        let claude_model = config.get_str("llm.claude_model", "claude-sonnet-4-20250514");
        llm.register_provider(Box::new(ClaudeProvider::new(
            claude_key,
            Some(claude_model),
            None,
        )));

        let openai_key = config.get_str("llm.openai_api_key", "");
        let openai_model = config.get_str("llm.openai_model", "gpt-4o");
        llm.register_provider(Box::new(OpenAIProvider::new(
            openai_key,
            Some(openai_model),
            None,
        )));

        let active = config.get_str("llm.provider", "claude");
        if let Err(e) = llm.set_active(&active) {
            warn!("Failed to set active provider to '{active}': {e}");
        }
        info!("LLM providers initialized, active: {active}");
    }

    /// Create a [`UiPanelTool`] with its callback wired to the GTK
    /// [`PanelRenderer`].
    ///
    /// The returned tool can be used from any thread.  When `execute()` is
    /// called, it marshals the panel display to the GTK main thread via
    /// `glib::idle_add_local_once` and blocks until the user responds.
    ///
    /// GTK widgets are not `Send`/`Sync`, but the tool callback must be.
    /// We use a `std::sync::mpsc` channel protected by a mutex to route
    /// panel requests from the worker thread to the GTK main thread:
    ///
    /// 1. Worker thread sends `(PanelRequest, reply_tx)` into a shared queue.
    /// 2. A periodic `glib::timeout_add_local` polls the queue on the GTK
    ///    main thread and calls `PanelRenderer::build_and_show_panel`.
    /// 3. Worker thread blocks on `reply_rx.recv()`.
    fn create_ui_panel_tool(parent_window: &adw::ApplicationWindow) -> Arc<UiPanelTool> {
        use aios_tools::builtin::ui_panel::{PanelRequest, PanelResponse};

        type PanelMsg = (
            PanelRequest,
            std::sync::mpsc::Sender<Option<PanelResponse>>,
        );

        // Shared queue: worker pushes, GTK main thread pops.
        let queue: Arc<std::sync::Mutex<Vec<PanelMsg>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        // Poll the queue from the GTK main thread.
        let queue_for_gtk = queue.clone();
        let window: adw::ApplicationWindow = parent_window.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            let msg = {
                let mut q = queue_for_gtk.lock().unwrap();
                if q.is_empty() {
                    None
                } else {
                    Some(q.remove(0))
                }
            };
            if let Some((request, reply_tx)) = msg {
                let parent: gtk4::Window = window.clone().upcast();
                PanelRenderer::build_and_show_panel(request, &parent, reply_tx);
            }
            glib::ControlFlow::Continue
        });

        // The tool callback: Send-safe, captures only the Arc<Mutex<Vec>>.
        let queue_for_tool = queue.clone();
        let tool = Arc::new(UiPanelTool::new());
        tool.set_panel_callback(move |request, _channel| {
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            {
                let mut q = queue_for_tool.lock().unwrap();
                q.push((request, reply_tx));
            }
            // Block the worker thread until the GTK main thread responds.
            reply_rx.recv().ok().flatten()
        });

        tool
    }

    /// Handle a slash command.
    fn handle_command(
        state: &Rc<RefCell<AiosApp>>,
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
                let info = aios_core::system_monitor::SystemInfo::gather();
                chat_view.add_level_message(
                    aios_core::types::MessageLevel::Info,
                    &info.format_text(),
                );
            }
            CommandResult::ClosePanel => {
                drop(s);
                let closed = Self::close_topmost_dialog();
                if closed {
                    chat_view.add_message("system", "Panel closed.");
                } else {
                    chat_view.add_message("system", "No open panel or dialog to close.");
                }
                return;
            }
            CommandResult::SelfTest(filter) => {
                drop(s);
                Self::run_selftest(state, chat_view, &filter);
                return;
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
    }

    /// Run the self-test suite on the current channel.
    fn run_selftest(
        _state: &Rc<RefCell<AiosApp>>,
        chat_view: &ChatView,
        filter: &str,
    ) {
        use aios_core::selftest::SelfTestRunner;
        use aios_core::selftest::runner::TestContext;

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

    /// Close the topmost modal/transient dialog window.
    ///
    /// Iterates all windows registered with the GTK application and looks
    /// for visible windows that are not the main `ApplicationWindow`.
    /// Closes the last one found (topmost) and returns `true` if a window
    /// was closed.
    fn close_topmost_dialog() -> bool {
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

    /// Send a user message to the LLM on the Tokio runtime.
    fn send_to_llm(
        state: &Rc<RefCell<AiosApp>>,
        chat_view: &ChatView,
        _prompt: &PromptInput,
        text: String,
    ) {
        let s = state.borrow();
        let llm = s.llm.clone();
        let rt = s.rt.clone();

        // Record user message in conversation history.
        drop(s);
        state
            .borrow_mut()
            .conversation
            .push(aios_core::types::Message::user(&text));

        // Use mpsc channel: tokio task sends result, GTK polls via idle_add.
        let (tx, rx) = std::sync::mpsc::channel::<LlmResult>();
        let history = state.borrow().conversation.clone();

        // Spawn LLM call on Tokio (no GTK types captured — all Send-safe).
        rt.spawn(async move {
            let mut llm_guard = llm.lock().await;
            let mut history = history;

            if history
                .last()
                .is_some_and(|m| m.role == aios_core::types::Role::User)
            {
                history.pop();
            }

            let result = llm_guard
                .chat(&text, Some(&mut history), &[], None)
                .await;
            drop(llm_guard);

            match result {
                Ok(response) => {
                    let content = response.content.unwrap_or_else(|| {
                        "I received your message but have no response text.".to_string()
                    });
                    let _ = tx.send(LlmResult::Success {
                        content,
                        updated_history: history,
                    });
                }
                Err(e) => {
                    let _ = tx.send(LlmResult::Error(format!("{e}")));
                }
            }
        });

        // Poll the channel from the GTK main thread.
        let chat_view_ref = chat_view.clone();
        let state_ref = state.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            match rx.try_recv() {
                Ok(LlmResult::Success { content, updated_history }) => {
                    chat_view_ref.add_message("assistant", &content);
                    let mut s = state_ref.borrow_mut();
                    s.conversation = updated_history;
                    s.conversation
                        .push(aios_core::types::Message::assistant(&content));
                    glib::ControlFlow::Break
                }
                Ok(LlmResult::Error(err)) => {
                    chat_view_ref.add_message("system", &format!("Error: {err}"));
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    }
}

/// Internal result type for async LLM communication.
enum LlmResult {
    Success {
        content: String,
        updated_history: Vec<aios_core::types::Message>,
    },
    Error(String),
}

// ---------------------------------------------------------------------------
// UiPanelToolWrapper
// ---------------------------------------------------------------------------

/// Wraps an `Arc<UiPanelTool>` as a `Tool` so it can be registered in the
/// [`ToolRegistry`].
///
/// This allows us to register a pre-configured `UiPanelTool` (with the GTK
/// panel callback already set) into the registry used by the LLM executor.
struct UiPanelToolWrapper(Arc<UiPanelTool>);

impl aios_tools::Tool for UiPanelToolWrapper {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn description(&self) -> &str {
        self.0.description()
    }

    fn parameters(&self) -> serde_json::Value {
        self.0.parameters()
    }

    fn execute(&self, args: serde_json::Value) -> aios_core::types::ToolResult {
        self.0.execute(args)
    }

    fn execute_on_channel(
        &self,
        args: serde_json::Value,
        channel: &aios_core::channel::ChannelContext,
    ) -> aios_core::types::ToolResult {
        self.0.execute_on_channel(args, channel)
    }
}
