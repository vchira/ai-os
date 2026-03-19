//! AiOS application orchestrator.
//!
//! [`AiosApp`] is a plain Rust struct (no GObject subclassing) that holds
//! all subsystem managers and wires them together via closures and
//! [`glib::MainContext::channel`] for async communication from Tokio back
//! to the GTK main thread.
//!
//! Most of the heavy lifting is delegated to extracted modules:
//! - [`crate::command_handler`] -- slash command processing
//! - [`crate::llm_handler`] -- LLM calls and response handling
//! - [`crate::first_boot_flow`] -- first-boot wizard and autoconfig
//! - [`crate::boot_status`] -- boot status text generation
//! - [`crate::voice_setup`] -- KWS engine, voice listener, VU meter
//! - [`crate::tts`] -- TTS functions
//! - [`crate::voice_listener`] -- voice / wake-word listener

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use tracing::{info, warn};

use aios_core::config::ConfigManager;
use aios_core::i18n::t;
use aios_core::secure::Vault;
use aios_llm::{ClaudeProvider, LlmManager, OpenAIProvider};
use aios_tools::ToolRegistry;
use aios_tools::builtin::ui_panel::UiPanelTool;

use crate::boot_status;
use crate::command_handler;
use crate::first_boot_flow;
use crate::llm_handler;
use crate::ui::channel_overlay::ChannelOverlay;
use crate::ui::chat_view::ChatView;
use crate::ui::main_window;
use crate::ui::panel_renderer::PanelRenderer;
use crate::ui::prompt_input::PromptInput;
use crate::voice_setup;

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// Shared message queue handle (thread-safe).
pub(crate) type SharedQueue = Arc<std::sync::Mutex<aios_core::queue::MessageQueue>>;

// ---------------------------------------------------------------------------
// AiosApp
// ---------------------------------------------------------------------------

/// Application-level state shared across signal handlers.
///
/// Wrapped in `Rc<RefCell<...>>` for the GTK main-thread parts, and
/// `Arc<tokio::sync::Mutex<...>>` for anything shared with the Tokio runtime.
pub struct AiosApp {
    pub(crate) config: ConfigManager,
    pub(crate) llm: Arc<tokio::sync::Mutex<LlmManager>>,
    #[allow(dead_code)]
    pub(crate) tools: ToolRegistry,
    pub(crate) conversation: Vec<aios_core::types::Message>,
    pub(crate) rt: tokio::runtime::Handle,
    /// Persistent message queue (SQLite-backed).
    pub(crate) queue: Option<SharedQueue>,
    /// KWS engine for wake word detection (None if infrastructure models missing).
    pub(crate) kws_engine: Option<Arc<std::sync::Mutex<Option<aios_voice::KwsEngine>>>>,
    /// Flag: wake word training is currently in progress.
    pub(crate) wake_training_in_progress: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Flag: wake word detection is enabled.
    pub(crate) wake_enabled: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Directory containing KWS models.
    pub(crate) kws_models_dir: std::path::PathBuf,
}

impl AiosApp {
    /// Create a new AiosApp with the given subsystems.
    pub(crate) fn new(
        config: ConfigManager,
        llm: LlmManager,
        tools: ToolRegistry,
        rt: tokio::runtime::Handle,
        queue: Option<SharedQueue>,
    ) -> Self {
        Self {
            config,
            llm: Arc::new(tokio::sync::Mutex::new(llm)),
            tools,
            conversation: Vec::new(),
            rt,
            queue,
            kws_engine: None,
            wake_training_in_progress: None,
            wake_enabled: None,
            kws_models_dir: std::path::PathBuf::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Entry point
    // -----------------------------------------------------------------------

    /// Called from `Application::connect_activate`. Builds the entire UI and
    /// wires up signals.
    pub fn activate(app: &adw::Application, rt: tokio::runtime::Handle) {
        // Apply dark theme IMMEDIATELY -- before any window is created.
        {
            let theme = ConfigManager::new()
                .map(|c| c.get_str("ui.theme", "dark"))
                .unwrap_or_else(|_| "dark".to_string());
            Self::apply_theme(&theme);
        }

        // Initialize the persistent message queue (SQLite).
        let queue_path = ConfigManager::default_config_dir().join("messages.db");
        let queue = aios_core::queue::MessageQueue::open(&queue_path).unwrap_or_else(|e| {
            warn!("Failed to open message queue: {e}, using in-memory fallback");
            aios_core::queue::MessageQueue::open_in_memory()
                .expect("Failed to create in-memory message queue")
        });
        let queue: SharedQueue = Arc::new(std::sync::Mutex::new(queue));
        info!("Message queue initialized at {:?}", queue_path);

        // Check if the vault exists. If not, run the first-boot setup.
        let vault_path = ConfigManager::default_config_dir().join("vault.enc");
        let vault = Vault::new(vault_path);

        if !vault.exists() {
            if let Some(auto) = aios_core::config::autoconfig::load_autoconfig() {
                info!("Autoconfig found -- applying unattended setup");
                first_boot_flow::apply_autoconfig(app, rt, auto, queue);
                return;
            }
            info!("No vault found -- launching first-boot setup conversation");
            first_boot_flow::run_first_boot_setup(app, rt, queue);
            return;
        }

        // Vault exists -- proceed with normal startup.
        Self::activate_main(app, rt, queue);
    }

    // -----------------------------------------------------------------------
    // Normal startup
    // -----------------------------------------------------------------------

    /// Normal application startup -- builds the UI and wires up all signals.
    fn activate_main(app: &adw::Application, rt: tokio::runtime::Handle, queue: SharedQueue) {
        let mut config = first_boot_flow::load_config();
        first_boot_flow::init_i18n(&config);

        // Migrate legacy "auto" summary provider to explicit values.
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
        Self::init_llm(&config, &mut llm);

        // Build the UI.
        let chat_view = ChatView::new();
        let prompt_input = PromptInput::new();
        let channel_overlay = ChannelOverlay::new();

        let available_providers = Self::available_providers(&config);
        let provider_refs: Vec<&str> = available_providers.iter().map(|s| s.as_str()).collect();

        let active_provider = crate::providers::find_by_id(
            &config.get_str("llm.provider", "claude")
        )
        .map(|p| p.display_name)
        .unwrap_or("Claude");
        let mw = main_window::build_main_window(
            app, &chat_view, &prompt_input, &channel_overlay, &provider_refs, active_provider,
        );
        let window = mw.window;
        let vu_meter_widget = mw.vu_meter;
        main_window::set_settings_button_visible(&window, true);

        // Wire UiPanelTool and tool executor into LLM.
        Self::wire_tool_executor(&mut llm, &window, queue.clone());

        // Read channel config.
        let signal_enabled = config.get_bool("channels.signal.enabled", false);
        let signal_phone = config.get_str("channels.signal.phone", "");

        // Boot status.
        let boot_status_text = boot_status::build_boot_status(&config);
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

        Self::apply_theme(&config.get_str("ui.theme", "dark"));
        first_boot_flow::show_hostname_conflict_card(&chat_view);
        chat_view.add_message("system", &t("setup.type_message"));

        let assistant_name = config.get_str("assistant.name", "Assistant");
        crate::ui::chat_view::set_assistant_display_name(&assistant_name);

        // --- Channel infrastructure ---
        let runtime = aios_core::channel::AppRuntime::new();
        runtime.switcher.register_channel(
            aios_core::channel::ChannelKind::Desktop,
            aios_core::channel::ChannelContext::desktop(),
        );

        let web_server =
            Self::start_web_server(&mut config, &runtime, Some(boot_status_text.clone()));

        let signal_sender = Self::start_signal_listener(
            &config, signal_enabled, &signal_phone, &runtime,
        );

        // Wire channel switcher to overlay.
        Self::wire_channel_overlay(&runtime, &channel_overlay);

        // Warn if no API key.
        Self::warn_if_no_api_key(&config, &chat_view);

        // Create shared application state.
        let llm_arc = Arc::new(tokio::sync::Mutex::new(llm));
        // KWS models can be in the system path (ISO) or user path.
        let system_kws = std::path::PathBuf::from("/opt/aios-app/models/kws");
        let user_kws = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/home/aios"))
            .join(".aios/models/kws");
        let kws_models_dir = if system_kws.join("infrastructure/melspectrogram.onnx").exists() {
            system_kws
        } else {
            user_kws
        };
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

        // Remote channel message loop.
        voice_setup::start_remote_channel_loop(
            &rt, &runtime, &state, &chat_view, web_server, signal_sender,
        );

        // Connect GTK signals (provider dropdown, settings, prompt submit).
        first_boot_flow::connect_common_signals(&state, &chat_view, &prompt_input, &window);

        // KWS + voice setup.
        voice_setup::setup_kws_and_voice(
            &state, &chat_view, &prompt_input, &window, vu_meter_widget, &kws_models_dir,
        );

        window.present();
    }

    // -----------------------------------------------------------------------
    // Command handling (delegates to crate::command_handler)
    // -----------------------------------------------------------------------

    /// Handle a slash command by bridging to the extracted command handler.
    pub(crate) fn handle_command(
        state: &Rc<RefCell<AiosApp>>,
        chat_view: &ChatView,
        input: &str,
    ) {
        let cmd_state = {
            let s = state.borrow();
            let config = first_boot_flow::load_config();
            command_handler::CommandHandlerState {
                config,
                llm: s.llm.clone(),
                conversation: s.conversation.clone(),
                rt: s.rt.clone(),
                kws_engine: s.kws_engine.clone(),
                wake_training_in_progress: s.wake_training_in_progress.clone(),
                wake_enabled: s.wake_enabled.clone(),
                kws_models_dir: s.kws_models_dir.clone(),
            }
        };

        let cmd_state = Rc::new(RefCell::new(cmd_state));
        command_handler::handle_command(&cmd_state, chat_view, input);

        // Sync changes back.
        {
            let cmd_s = cmd_state.borrow();
            let mut s = state.borrow_mut();
            s.conversation = cmd_s.conversation.clone();
            s.config = first_boot_flow::load_config();
        }
    }

    // -----------------------------------------------------------------------
    // Small helpers
    // -----------------------------------------------------------------------

    /// Initialize LLM providers from config.
    pub(crate) fn init_llm(config: &ConfigManager, llm: &mut LlmManager) {
        // Claude (Anthropic)
        let claude_key = config.get_str("llm.claude_api_key", "");
        let claude_model = config.get_str("llm.claude_model", "claude-sonnet-4-20250514");
        llm.register_provider(Box::new(ClaudeProvider::new(
            claude_key, Some(claude_model), None,
        )));

        // OpenAI (ChatGPT)
        let openai_key = config.get_str("llm.openai_api_key", "");
        let openai_model = config.get_str("llm.openai_model", "gpt-4o");
        llm.register_provider(Box::new(OpenAIProvider::new(
            openai_key, Some(openai_model), None,
        )));

        // DeepSeek — near-Claude quality at ~10x cheaper
        let deepseek_key = config.get_str("llm.deepseek_api_key", "");
        if !deepseek_key.is_empty() {
            let deepseek_model = config.get_str("llm.deepseek_model", "deepseek-chat");
            llm.register_provider(Box::new(OpenAIProvider::deepseek(
                deepseek_key, Some(deepseek_model),
            )));
        }

        // Mistral — very cheap, good quality
        let mistral_key = config.get_str("llm.mistral_api_key", "");
        if !mistral_key.is_empty() {
            let mistral_model = config.get_str("llm.mistral_model", "mistral-small-latest");
            llm.register_provider(Box::new(OpenAIProvider::mistral(
                mistral_key, Some(mistral_model),
            )));
        }

        // Groq — extremely fast inference
        let groq_key = config.get_str("llm.groq_api_key", "");
        if !groq_key.is_empty() {
            let groq_model = config.get_str("llm.groq_model", "llama-3.3-70b-versatile");
            llm.register_provider(Box::new(OpenAIProvider::groq(
                groq_key, Some(groq_model),
            )));
        }

        // Google Gemini
        let gemini_key = config.get_str("llm.gemini_api_key", "");
        if !gemini_key.is_empty() {
            let gemini_model = config.get_str("llm.gemini_model", "gemini-2.0-flash");
            llm.register_provider(Box::new(OpenAIProvider::gemini(
                gemini_key, Some(gemini_model),
            )));
        }

        // Ollama (local, free — no API key needed)
        let ollama_enabled = config.get_bool("llm.ollama_enabled", false);
        if ollama_enabled {
            let ollama_model = config.get_str("llm.ollama_model", "llama3.2");
            llm.register_provider(Box::new(OpenAIProvider::ollama(
                Some(ollama_model),
            )));
        }

        let active = config.get_str("llm.provider", "claude");
        if let Err(e) = llm.set_active(&active) {
            warn!("Failed to set active provider to '{active}': {e}");
        }
        info!("LLM providers initialized, active: {active}");
    }

    /// Start the web server if enabled in config.
    pub(crate) fn start_web_server(
        config: &mut ConfigManager,
        runtime: &Arc<aios_core::channel::AppRuntime>,
        welcome_message: Option<String>,
    ) -> Option<tokio::sync::broadcast::Sender<String>> {
        if !config.get_bool("channels.web.enabled", true) {
            return None;
        }

        let port: u16 = config.get_str("channels.web.port", "80").parse().unwrap_or(80);
        let web_tx = runtime.message_sender();

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
            port, web_tx, web_token, Some(runtime.switcher.clone()), welcome_message,
        );
        let response_tx = server.response_tx.clone();
        server.start();
        runtime.switcher.register_channel(
            aios_core::channel::ChannelKind::Web,
            aios_core::channel::ChannelContext::web(),
        );
        info!("Web channel started on port {port}");
        Some(response_tx)
    }

    /// Create a [`UiPanelTool`] with its callback wired to the GTK
    /// [`PanelRenderer`].
    pub(crate) fn create_ui_panel_tool(
        parent_window: &adw::ApplicationWindow,
    ) -> Arc<UiPanelTool> {
        use aios_tools::builtin::ui_panel::{PanelRequest, PanelResponse};

        type PanelMsg = (PanelRequest, std::sync::mpsc::Sender<Option<PanelResponse>>);

        let queue: Arc<std::sync::Mutex<Vec<PanelMsg>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        let queue_for_gtk = queue.clone();
        let window: adw::ApplicationWindow = parent_window.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            let msg = {
                let mut q = queue_for_gtk.lock().unwrap();
                if q.is_empty() { None } else { Some(q.remove(0)) }
            };
            if let Some((request, reply_tx)) = msg {
                let parent: gtk4::Window = window.clone().upcast();
                PanelRenderer::build_and_show_panel(request, &parent, reply_tx);
            }
            glib::ControlFlow::Continue
        });

        let queue_for_tool = queue.clone();
        let tool = Arc::new(UiPanelTool::new());
        tool.set_panel_callback(move |request, _channel| {
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            {
                let mut q = queue_for_tool.lock().unwrap();
                q.push((request, reply_tx));
            }
            reply_rx.recv().ok().flatten()
        });
        tool
    }

    /// Apply a theme setting via libadwaita's StyleManager.
    pub(crate) fn apply_theme(theme: &str) {
        command_handler::apply_theme(theme);
    }

    /// Get list of available providers (those with API keys configured, excluding Ollama).
    fn available_providers(config: &ConfigManager) -> Vec<String> {
        crate::providers::configured_display_names_excluding_ollama(config)
    }

    /// Wire UiPanelTool and tool executor into the LLM manager.
    fn wire_tool_executor(
        llm: &mut LlmManager,
        window: &adw::ApplicationWindow,
        queue: SharedQueue,
    ) {
        let ui_panel_tool = Self::create_ui_panel_tool(window);
        let tool_registry = Arc::new(std::sync::Mutex::new(ToolRegistry::new()));
        {
            let mut tr = tool_registry.lock().unwrap();
            tr.load_builtins();
            tr.register_queue_tools(queue);
            let _ = tr.unregister("ui_panel");
            let _ = tr.register(Box::new(UiPanelToolWrapper(ui_panel_tool)));
        }
        // Wire browse_url callback — fetch page content since AiOS has no browser.
        {
            let mut tr = tool_registry.lock().unwrap();
            let _ = tr.unregister("browse_url");
            let mut browse_tool = aios_tools::builtin::browse_url::BrowseUrlTool::new();
            browse_tool.set_browse_callback(std::sync::Arc::new(|url: &str| {
                // In AiOS kiosk mode, there's no browser. Fetch the page
                // content and let the AI summarize it for the user.
                tracing::info!("browse_url: fetching {url}");
            }));
            let _ = tr.register(Box::new(browse_tool));
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
    }

    /// Start the Signal listener if enabled.
    fn start_signal_listener(
        config: &ConfigManager,
        signal_enabled: bool,
        signal_phone: &str,
        runtime: &Arc<aios_core::channel::AppRuntime>,
    ) -> Option<Arc<aios_signal::SignalSender>> {
        if !signal_enabled || signal_phone.is_empty() {
            return None;
        }

        let contacts_str = config.get_str("channels.signal.allowed_contacts", "");
        let allowed: Vec<String> = contacts_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let listener =
            aios_signal::SignalListener::new(signal_phone.to_string(), allowed);
        let sig_tx = runtime.message_sender();
        listener.start(sig_tx);
        runtime.switcher.register_channel(
            aios_core::channel::ChannelKind::Signal,
            aios_core::channel::ChannelContext::signal(),
        );
        info!("Signal channel started for {signal_phone}");
        Some(Arc::new(aios_signal::SignalSender::new(
            signal_phone.to_string(),
        )))
    }

    /// Wire channel switcher to the overlay widget.
    fn wire_channel_overlay(
        runtime: &Arc<aios_core::channel::AppRuntime>,
        channel_overlay: &ChannelOverlay,
    ) {
        let (overlay_tx, overlay_rx) =
            std::sync::mpsc::channel::<aios_core::channel::ChannelKind>();

        runtime.switcher.on_switch(Arc::new(move |_old, new| {
            let _ = overlay_tx.send(new);
        }));

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

        let switcher_for_btn = runtime.switcher.clone();
        channel_overlay.on_switch_back(move || {
            let _ = switcher_for_btn.switch_to(aios_core::channel::ChannelKind::Desktop);
        });
    }

    /// Show a warning if no API key is configured for the active provider.
    fn warn_if_no_api_key(config: &ConfigManager, chat_view: &ChatView) {
        let provider_id = config.get_str("llm.provider", "claude");
        let has_key = crate::providers::find_by_id(&provider_id)
            .map(|p| crate::providers::is_configured(p, config))
            .unwrap_or(false);
        if !has_key {
            chat_view.add_level_message(
                aios_core::types::MessageLevel::Warning,
                &t("app.no_api_key_warning"),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// LlmState trait implementation for AiosApp
// ---------------------------------------------------------------------------

impl llm_handler::LlmState for AiosApp {
    fn llm(&self) -> Arc<tokio::sync::Mutex<LlmManager>> {
        self.llm.clone()
    }

    fn rt(&self) -> tokio::runtime::Handle {
        self.rt.clone()
    }

    fn conversation(&self) -> Vec<aios_core::types::Message> {
        self.conversation.clone()
    }

    fn push_conversation(&mut self, msg: aios_core::types::Message) {
        self.conversation.push(msg);
    }

    fn replace_conversation(&mut self, msgs: Vec<aios_core::types::Message>) {
        self.conversation = msgs;
    }

    fn queue(&self) -> Option<SharedQueue> {
        self.queue.clone()
    }

    fn tool_schemas(&self) -> Vec<aios_core::types::ToolSchema> {
        self.tools.get_schemas()
    }

    fn config_snapshot(&self) -> ConfigManager {
        first_boot_flow::load_config()
    }
}

// ---------------------------------------------------------------------------
// UiPanelToolWrapper
// ---------------------------------------------------------------------------

/// Wraps an `Arc<UiPanelTool>` as a `Tool` so it can be registered in the
/// [`ToolRegistry`].
pub(crate) struct UiPanelToolWrapper(pub(crate) Arc<UiPanelTool>);

impl aios_tools::Tool for UiPanelToolWrapper {
    fn name(&self) -> &str { self.0.name() }
    fn description(&self) -> &str { self.0.description() }
    fn parameters(&self) -> serde_json::Value { self.0.parameters() }
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
