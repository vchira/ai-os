//! Setup conversation driven by the `ui_panel` tool.
//!
//! [`SetupConversation`] is a scripted first-boot flow that uses the same
//! `ui_panel` tool that the AI would use.  Each step adds a message to the
//! chat view, then calls the tool to show a panel dialog and collect input.
//!
//! This demonstrates that the panel system is generic: the same tool the AI
//! calls to ask for user input is also used by the system itself.

use std::sync::Arc;

use gtk4::glib;
use tracing::info;

use aios_tools::builtin::ui_panel::UiPanelTool;
use aios_tools::tool::Tool;

use super::chat_view::ChatView;

// ---------------------------------------------------------------------------
// Public result types (same as first_boot.rs — used by app.rs)
// ---------------------------------------------------------------------------

/// Configuration for a single LLM provider collected during setup.
#[derive(Debug, Clone)]
pub struct ProviderSetup {
    /// Provider identifier: `"claude"` or `"openai"`.
    pub name: String,
    /// The API key entered by the user.
    pub api_key: String,
}

/// The complete result of a successful first-boot setup.
#[derive(Debug, Clone)]
pub struct SetupResult {
    /// Configured providers, ordered by preference (first = primary).
    pub providers: Vec<ProviderSetup>,
    /// The master password chosen by the user.
    pub master_password: String,
}

// ---------------------------------------------------------------------------
// Chat message sender (Send-safe wrapper for cross-thread chat messages)
// ---------------------------------------------------------------------------

/// Send-safe handle that can enqueue messages to the ChatView on the GTK
/// main thread.
///
/// Internally uses `std::sync::mpsc` + `glib::idle_add_local_once` so it
/// can be called from any thread.
struct ChatSender {
    /// Shared queue of `(role, text)` messages.
    queue: Arc<std::sync::Mutex<Vec<(String, String)>>>,
}

impl ChatSender {
    /// Create a new `ChatSender` that will deliver messages to the given
    /// `ChatView`.  A periodic GTK timer drains the queue.
    fn new(chat_view: ChatView) -> Self {
        let queue: Arc<std::sync::Mutex<Vec<(String, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        // Poll the queue from the GTK main thread.
        let queue_ref = queue.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            let msgs: Vec<(String, String)> = {
                let mut q = queue_ref.lock().unwrap();
                std::mem::take(&mut *q)
            };
            for (role, text) in msgs {
                chat_view.add_message(&role, &text);
            }
            glib::ControlFlow::Continue
        });

        Self { queue }
    }

    /// Enqueue an assistant message and speak it via TTS.
    fn say(&self, text: &str) {
        let mut q = self.queue.lock().unwrap();
        q.push(("assistant".to_string(), text.to_string()));
        drop(q);
        // Speak using espeak-ng (available in the ISO, no model download needed)
        Self::speak(text);
    }

    /// Speak text using espeak-ng (fire-and-forget).
    fn speak(text: &str) {
        let text = text.to_string();
        std::thread::spawn(move || {
            let _ = std::process::Command::new("espeak-ng")
                .args(["-v", "en", "-s", "160", "-p", "50", &text])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    }

    /// Enqueue a user message.
    fn user_says(&self, text: &str) {
        let mut q = self.queue.lock().unwrap();
        q.push(("user".to_string(), text.to_string()));
    }
}

// ChatSender is Send because it only holds an Arc<Mutex<...>>.
// (The ChatView is NOT captured — it's only used on the main thread
// inside the glib::timeout_add_local closure.)
unsafe impl Send for ChatSender {}
unsafe impl Sync for ChatSender {}

// ---------------------------------------------------------------------------
// SetupConversation
// ---------------------------------------------------------------------------

/// Drives the first-boot setup as a conversation using the `ui_panel` tool.
///
/// Each step:
/// 1. Sends a chat message to the main thread.
/// 2. Calls the `ui_panel` tool (which shows a panel dialog).
/// 3. Processes the result.
/// 4. Advances to the next step.
///
/// The conversation runs on a background thread so that the tool's blocking
/// `execute()` call does not freeze the GTK main thread.
pub struct SetupConversation {
    chat_view: ChatView,
    panel_tool: Arc<UiPanelTool>,
}

impl SetupConversation {
    /// Create a new setup conversation.
    ///
    /// # Arguments
    ///
    /// * `chat_view` — The chat view to add messages to.
    /// * `panel_tool` — The `UiPanelTool` instance (with callback already set).
    pub fn new(chat_view: ChatView, panel_tool: Arc<UiPanelTool>) -> Self {
        Self {
            chat_view,
            panel_tool,
        }
    }

    /// Begin the setup flow and call `on_done` on the GTK main thread when
    /// finished.
    ///
    /// `on_done` receives `Some(SetupResult)` on success or `None` if the
    /// user cancelled.
    pub fn start(self, on_done: impl FnOnce(Option<SetupResult>) + 'static) {
        let panel_tool = self.panel_tool.clone();

        // Create a Send-safe chat sender (captures ChatView on GTK thread only).
        let chat_sender = Arc::new(ChatSender::new(self.chat_view.clone()));

        // Result channel: worker sends, GTK main thread polls.
        let result_slot: Arc<std::sync::Mutex<Option<Option<SetupResult>>>> =
            Arc::new(std::sync::Mutex::new(None));

        // Poll for result on the GTK main thread.
        // Wrap FnOnce in Option so we can take() it from the FnMut closure.
        let mut on_done = Some(on_done);
        let result_slot_gtk = result_slot.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let val = result_slot_gtk.lock().unwrap().take();
            if let Some(result) = val {
                if let Some(cb) = on_done.take() {
                    cb(result);
                }
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });

        // Run the conversation on a background thread.
        let result_slot_worker = result_slot.clone();
        std::thread::spawn(move || {
            let result = Self::run_conversation(&panel_tool, &chat_sender);
            *result_slot_worker.lock().unwrap() = Some(result);
        });
    }

    /// Run the full conversation flow. Returns `None` if the user cancels
    /// at any step.
    fn run_conversation(
        panel_tool: &UiPanelTool,
        chat: &ChatSender,
    ) -> Option<SetupResult> {
        // Step 1: Welcome message.
        chat.say("Welcome to AiOS! I'm your AI assistant. Let's get you set up.");
        std::thread::sleep(std::time::Duration::from_millis(800));

        // Step 2: Choose provider.
        chat.say("First, which AI provider would you like to use?");
        std::thread::sleep(std::time::Duration::from_millis(200));

        let provider_response = panel_tool.execute(serde_json::json!({
            "title": "Choose Your AI Provider",
            "icon": "network-server-symbolic",
            "description": "Which AI would you like to use as your primary assistant?",
            "fields": [{
                "id": "provider",
                "type": "choice",
                "label": "AI Provider",
                "required": true,
                "options": [
                    {
                        "value": "claude",
                        "label": "Claude (Anthropic)",
                        "description": "Advanced reasoning and analysis, strong at coding tasks"
                    },
                    {
                        "value": "openai",
                        "label": "OpenAI (GPT)",
                        "description": "GPT-4o with broad general knowledge and tool use"
                    }
                ]
            }]
        }));

        if !provider_response.success {
            return None;
        }

        let provider = provider_response
            .data
            .as_ref()
            .and_then(|d| d.get("provider"))
            .and_then(|v| v.as_str())
            .unwrap_or("claude")
            .to_string();

        let provider_display = match provider.as_str() {
            "claude" => "Claude (Anthropic)",
            "openai" => "OpenAI (GPT)",
            other => other,
        };

        chat.user_says(provider_display);
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Step 3: Enter API key.
        let hint_url = match provider.as_str() {
            "claude" => "console.anthropic.com",
            "openai" => "platform.openai.com",
            _ => "the provider's website",
        };

        chat.say(&format!(
            "Great choice! Now I need your {} API key. \
             You can get one at {}.",
            provider_display, hint_url
        ));
        std::thread::sleep(std::time::Duration::from_millis(200));

        let key_response = panel_tool.execute(serde_json::json!({
            "title": format!("Enter Your {} API Key", provider_display),
            "icon": "dialog-password-symbolic",
            "description": format!("Paste your API key below. Get one at {}", hint_url),
            "fields": [{
                "id": "api_key",
                "type": "password",
                "label": "API Key",
                "required": true,
                "placeholder": "sk-..."
            }]
        }));

        if !key_response.success {
            return None;
        }

        let api_key = key_response
            .data
            .as_ref()
            .and_then(|d| d.get("api_key"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Show masked key.
        let masked = if api_key.len() > 8 {
            format!("{}...{}", &api_key[..4], &api_key[api_key.len() - 4..])
        } else {
            "****".to_string()
        };
        chat.user_says(&format!("API Key: {masked}"));
        std::thread::sleep(std::time::Duration::from_millis(200));

        let mut providers = vec![ProviderSetup {
            name: provider.clone(),
            api_key,
        }];

        // Step 4: Create master password.
        chat.say(
            "Now let's secure your data. Create a master password to \
             protect your API keys and personal data.",
        );
        std::thread::sleep(std::time::Duration::from_millis(200));

        let password = loop {
            let pw_response = panel_tool.execute(serde_json::json!({
                "title": "Secure Your Data",
                "icon": "channel-secure-symbolic",
                "description": "Create a master password to protect your API keys.\nMinimum 8 characters.",
                "fields": [
                    {
                        "id": "password",
                        "type": "password",
                        "label": "Master Password",
                        "required": true,
                        "placeholder": "Minimum 8 characters"
                    },
                    {
                        "id": "confirm",
                        "type": "password",
                        "label": "Confirm Password",
                        "required": true,
                        "placeholder": "Type it again"
                    }
                ]
            }));

            if !pw_response.success {
                return None;
            }

            let pw = pw_response
                .data
                .as_ref()
                .and_then(|d| d.get("password"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let confirm = pw_response
                .data
                .as_ref()
                .and_then(|d| d.get("confirm"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if pw.len() < 8 {
                chat.say("Password must be at least 8 characters. Please try again.");
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }

            if pw != confirm {
                chat.say("Passwords don't match. Please try again.");
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }

            break pw;
        };

        chat.user_says("\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Step 5: Add backup provider?
        let other_provider = if provider == "claude" { "openai" } else { "claude" };
        let other_display = if other_provider == "claude" {
            "Claude (Anthropic)"
        } else {
            "OpenAI (GPT)"
        };

        chat.say(&format!(
            "Would you like to add {} as a backup provider? \
             If your primary AI is unavailable, the backup can take over.",
            other_display
        ));
        std::thread::sleep(std::time::Duration::from_millis(200));

        let backup_response = panel_tool.execute(serde_json::json!({
            "title": "Add a Backup Provider?",
            "icon": "list-add-symbolic",
            "description": format!(
                "Add {} as backup. It will be used automatically if {} is unavailable.",
                other_display, provider_display
            ),
            "fields": [{
                "id": "add_backup",
                "type": "choice",
                "label": "Add backup?",
                "required": true,
                "options": [
                    {
                        "value": "yes",
                        "label": format!("Yes, add {}", other_display),
                        "description": "Configure a second provider as fallback"
                    },
                    {
                        "value": "no",
                        "label": "No, I'm good",
                        "description": "Skip backup configuration"
                    }
                ]
            }]
        }));

        let add_backup = backup_response
            .data
            .as_ref()
            .and_then(|d| d.get("add_backup"))
            .and_then(|v| v.as_str())
            == Some("yes");

        if add_backup {
            chat.user_says(&format!("Yes, add {other_display}"));
            std::thread::sleep(std::time::Duration::from_millis(200));

            let backup_hint = match other_provider {
                "claude" => "console.anthropic.com",
                "openai" => "platform.openai.com",
                _ => "the provider's website",
            };

            chat.say(&format!("Enter your {} API key.", other_display));
            std::thread::sleep(std::time::Duration::from_millis(200));

            let backup_key_response = panel_tool.execute(serde_json::json!({
                "title": format!("Enter Your {} API Key", other_display),
                "icon": "dialog-password-symbolic",
                "description": format!("Paste your API key below. Get one at {}", backup_hint),
                "fields": [{
                    "id": "api_key",
                    "type": "password",
                    "label": "API Key",
                    "required": true,
                    "placeholder": "sk-..."
                }]
            }));

            if backup_key_response.success {
                let backup_key = backup_key_response
                    .data
                    .as_ref()
                    .and_then(|d| d.get("api_key"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if !backup_key.is_empty() {
                    let masked = if backup_key.len() > 8 {
                        format!(
                            "{}...{}",
                            &backup_key[..4],
                            &backup_key[backup_key.len() - 4..]
                        )
                    } else {
                        "****".to_string()
                    };
                    chat.user_says(&format!("API Key: {masked}"));
                    std::thread::sleep(std::time::Duration::from_millis(100));

                    providers.push(ProviderSetup {
                        name: other_provider.to_string(),
                        api_key: backup_key,
                    });
                }
            }
        } else {
            chat.user_says("No, I'm good");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Step 6: Complete.
        let mut summary_lines = Vec::new();
        for (i, p) in providers.iter().enumerate() {
            let role = if i == 0 { "primary" } else { "backup" };
            let display = match p.name.as_str() {
                "claude" => "Claude (Anthropic)",
                "openai" => "OpenAI (GPT)",
                other => other,
            };
            summary_lines.push(format!("\u{2713} {} as {}", display, role));
        }
        summary_lines.push("\u{2713} Secure vault created".to_string());

        chat.say(&format!(
            "You're all set! Here's your configuration:\n\n{}\n\n\
             Your AI assistant is ready.",
            summary_lines.join("\n")
        ));

        info!(
            "Setup conversation complete: {} provider(s)",
            providers.len()
        );

        Some(SetupResult {
            providers,
            master_password: password,
        })
    }
}
