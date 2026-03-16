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
use aios_llm::{ClaudeProvider, LlmManager, OpenAIProvider};
use aios_tools::ToolRegistry;

use crate::ui::chat_view::ChatView;
use crate::ui::main_window;
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
        // Load configuration.
        let config = match ConfigManager::new() {
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

        // Wire tool executor into LLM manager.
        let tool_registry = Arc::new(std::sync::Mutex::new(ToolRegistry::new()));
        {
            let mut tr = tool_registry.lock().unwrap();
            tr.load_builtins();
        }
        let tr_for_executor = tool_registry.clone();
        llm.set_tool_executor(Arc::new(move |name, args| {
            let registry = tr_for_executor.lock().unwrap();
            let result = registry.execute(&name, args);
            if result.success {
                result.output
            } else {
                format!("Tool error: {}", result.output)
            }
        }));

        // Build the UI.
        let chat_view = ChatView::new();
        let prompt_input = PromptInput::new();

        let window = main_window::build_main_window(
            app,
            &chat_view,
            &prompt_input,
        );

        // Show the welcome message.
        chat_view.add_message(
            "system",
            "Welcome to AiOS \u{2014} your AI-native operating system.\n\n\
             Type a message or use /help to see available commands.",
        );

        // Create shared application state.
        let state = Rc::new(RefCell::new(AiosApp {
            config,
            llm: Arc::new(tokio::sync::Mutex::new(llm)),
            tools,
            conversation: Vec::new(),
            rt,
        }));

        // --- Connect signals ---

        // Provider dropdown changed.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        main_window::connect_provider_dropdown(&window, move |provider_name| {
            let mut s = state_ref.borrow_mut();
            let name = provider_name.to_lowercase();
            let _ = s.config.set("llm.provider", serde_json::json!(name));
            // Provider switching is done synchronously via try_lock.
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

        // Message submission (Enter key or send button).
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
            let llm_guard = llm.lock().await;
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
