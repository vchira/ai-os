//! Shared boot context — eliminates duplication across the 3 startup paths.
//!
//! All startup paths (interactive first-boot, autoconfig, normal boot) share
//! the same UI creation, boot status display, and finalization steps. This
//! module extracts those into reusable functions.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use libadwaita as adw;
use tracing::info;

use aios_core::config::ConfigManager;
use aios_llm::LlmManager;
use aios_tools::ToolRegistry;

use crate::app::{AiosApp, SharedQueue};
use crate::boot_status;
use crate::first_boot_flow;
use crate::ui::channel_overlay::ChannelOverlay;
use crate::ui::chat_view::ChatView;
use crate::ui::main_window;
use crate::ui::prompt_input::PromptInput;
use crate::voice_setup;

// ---------------------------------------------------------------------------
// BootUi — shared UI widgets created by all paths
// ---------------------------------------------------------------------------

/// UI widgets created during boot, shared across all startup paths.
pub(crate) struct BootUi {
    pub chat_view: ChatView,
    pub prompt_input: PromptInput,
    pub channel_overlay: ChannelOverlay,
    pub window: adw::ApplicationWindow,
    pub vu_meter: gtk4::LevelBar,
}

/// Create the shared UI widgets and main window.
///
/// All 3 startup paths create the same widgets — this extracts that.
pub(crate) fn build_ui(
    app: &adw::Application,
    provider_names: &[&str],
    active_provider: &str,
) -> BootUi {
    let chat_view = ChatView::new();
    let prompt_input = PromptInput::new();
    let channel_overlay = ChannelOverlay::new();

    let mw = main_window::build_main_window(
        app,
        &chat_view,
        &prompt_input,
        &channel_overlay,
        provider_names,
        active_provider,
    );

    BootUi {
        chat_view,
        prompt_input,
        channel_overlay,
        window: mw.window,
        vu_meter: mw.vu_meter,
    }
}

// ---------------------------------------------------------------------------
// Boot status + theme — common to all paths
// ---------------------------------------------------------------------------

/// Show boot status in the chat view, push to queue, apply theme, show
/// hostname conflict card, and set assistant display name.
pub(crate) fn show_boot_status(
    config: &ConfigManager,
    chat_view: &ChatView,
    queue: &SharedQueue,
) -> String {
    let boot_status_text = boot_status::build_boot_status(config);
    chat_view.add_level_message(aios_core::types::MessageLevel::Info, &boot_status_text);

    {
        let mut q = queue.lock().unwrap();
        q.push(aios_core::queue::QueuedMessage {
            role: aios_core::types::Role::System,
            channel: aios_core::channel::ChannelKind::System,
            source: Some("system".into()),
            content: Some(boot_status_text.clone()),
            level: Some(aios_core::types::MessageLevel::Info),
            ..Default::default()
        });
    }

    AiosApp::apply_theme(&config.get_str("ui.theme", "dark"));
    first_boot_flow::show_hostname_conflict_card(chat_view);

    let assistant_name = config.get_str("assistant.name", "Assistant");
    crate::ui::chat_view::set_assistant_display_name(&assistant_name);

    boot_status_text
}

// ---------------------------------------------------------------------------
// Finalize boot — init LLM, tools, signals, voice, channels
// ---------------------------------------------------------------------------

/// Initialize LLM + tools + channels + signals + voice.
///
/// This is the "tail" that all paths call after setup-specific logic is done.
/// Replaces the old `transition_to_normal_mode()`.
pub(crate) fn finalize_boot(
    config: ConfigManager,
    rt: tokio::runtime::Handle,
    queue: SharedQueue,
    ui: &BootUi,
    boot_status_text: Option<String>,
) {
    // Migrate legacy "auto" summary provider.
    let mut config = config;
    {
        let summary_prov = config.get_str("llm.tts_summary_provider", "");
        if summary_prov == "auto" || summary_prov.is_empty() {
            let main = config.get_str("llm.provider", "claude");
            let main_model_key = format!("llm.{main}_model");
            let main_model = config.get_str(&main_model_key, "");
            let _ = config.set("llm.tts_summary_provider", serde_json::json!(main));
            if !main_model.is_empty() {
                let _ = config.set("llm.tts_summary_model", serde_json::json!(main_model));
            }
        }
    }

    // Initialize tools and LLM.
    let mut tools = ToolRegistry::new();
    tools.load_builtins();
    tools.register_queue_tools(queue.clone());
    info!("Loaded {} built-in tools", tools.len());

    let mut llm = LlmManager::new();
    AiosApp::init_llm(&config, &mut llm);

    // Wire UiPanelTool and tool executor into LLM.
    AiosApp::wire_tool_executor(&mut llm, &ui.window, queue.clone());

    // Channel infrastructure.
    let runtime = aios_core::channel::AppRuntime::new();
    runtime.switcher.register_channel(
        aios_core::channel::ChannelKind::Desktop,
        aios_core::channel::ChannelContext::desktop(),
    );

    let web_server = AiosApp::start_web_server(&mut config, &runtime, boot_status_text);

    let signal_enabled = config.get_bool("channels.signal.enabled", false);
    let signal_phone = config.get_str("channels.signal.phone", "");
    let signal_sender = AiosApp::start_signal_listener(
        &config, signal_enabled, &signal_phone, &runtime,
    );

    AiosApp::wire_channel_overlay(&runtime, &ui.channel_overlay);
    AiosApp::warn_if_no_api_key(&config, &ui.chat_view);

    // Resolve KWS models directory.
    let system_kws = std::path::PathBuf::from("/opt/aios-app/models/kws");
    let user_kws = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/home/aios"))
        .join(".aios/models/kws");
    let kws_models_dir = if system_kws.join("infrastructure/melspectrogram.onnx").exists() {
        system_kws
    } else {
        user_kws
    };

    // Create shared application state.
    let llm_arc = Arc::new(tokio::sync::Mutex::new(llm));
    let state = Rc::new(RefCell::new(AiosApp {
        config,
        llm: llm_arc,
        tools,
        conversation: Vec::new(),
        rt: rt.clone(),
        kws_engine: None,
        wake_training_in_progress: None,
        wake_enabled: None,
        kws_models_dir: kws_models_dir.clone(),
        queue: Some(queue),
    }));

    // Remote channel message loop (Web + Signal).
    voice_setup::start_remote_channel_loop(
        &rt, &runtime, &state, &ui.chat_view, web_server, signal_sender,
    );

    // Make UI interactive.
    main_window::set_settings_button_visible(&ui.window, true);
    ui.prompt_input.widget().set_visible(true);
    ui.prompt_input.widget().set_sensitive(true);
    ui.prompt_input.widget().set_hexpand(true);

    // Update provider dropdown.
    let configured = crate::providers::configured_display_names_excluding_ollama(
        &state.borrow().config,
    );
    let refs: Vec<&str> = configured.iter().map(|s| s.as_str()).collect();
    main_window::update_provider_dropdown(&ui.window, &refs);

    // Connect all GTK signals.
    first_boot_flow::connect_common_signals(&state, &ui.chat_view, &ui.prompt_input, &ui.window);

    // KWS + voice setup.
    voice_setup::setup_kws_and_voice(
        &state,
        &ui.chat_view,
        &ui.prompt_input,
        &ui.window,
        ui.vu_meter.clone(),
        &kws_models_dir,
    );
}
