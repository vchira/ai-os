#![allow(dead_code)]
//! Queue-based first-boot setup and autoconfig.
//!
//! Replaces the monolithic `SetupConversation` state machine. Setup steps
//! are pushed as messages to the queue, and user responses advance the state.

use aios_core::channel::ChannelKind;
use aios_core::config::ConfigManager;
use aios_core::queue::{MessageQueue, QueuedMessage};
use aios_core::types::{MessageLevel, Role};

/// Setup steps in the first-boot wizard.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SetupStep {
    Welcome,
    AudioOutput,
    AudioInput,
    NameAssistant,
    ProviderSelect,
    ApiKeyEntry { provider: String },
    MasterPassword,
    ConfirmPassword,
    BackupProvider,
    ProviderOrder,
    Country,
    InstallDecision,
    DriveSelection,
    PartitionPlan,
    InstallConfirm,
    InstallProgress,
    Complete,
}

/// State for the first-boot setup wizard.
pub(crate) struct SetupState {
    pub step: SetupStep,
    pub primary_provider: Option<String>,
    pub backup_provider: Option<String>,
    pub assistant_name: String,
    pub wake_word: String,
    pub machine_name: String,
    pub master_password: Option<String>,
}

impl Default for SetupState {
    fn default() -> Self {
        Self {
            step: SetupStep::Welcome,
            primary_provider: None,
            backup_provider: None,
            assistant_name: "Assistant".to_string(),
            wake_word: "hey jarvis".to_string(),
            machine_name: "assistant".to_string(),
            master_password: None,
        }
    }
}

/// Push the welcome message to the queue to start the first-boot setup.
pub(crate) fn start_first_boot(queue: &mut MessageQueue) -> SetupState {
    // Push welcome system message.
    queue.push(QueuedMessage {
        role: Role::System,
        channel: ChannelKind::System,
        source: Some("system".into()),
        content: Some("Welcome to AiOS!".into()),
        level: Some(MessageLevel::Info),
        ..Default::default()
    });

    // Push the first interactive message.
    queue.push(QueuedMessage {
        role: Role::Assistant,
        channel: ChannelKind::System,
        source: Some("system".into()),
        content: Some(
            "I'm your AI assistant. Let's set up your system together.\n\
             Which AI provider would you like to use? Type 'claude' or 'chatgpt'."
                .into(),
        ),
        metadata: Some(
            serde_json::json!({
                "card_type": "provider_select",
                "options": ["Claude", "ChatGPT"]
            })
            .to_string(),
        ),
        ..Default::default()
    });

    SetupState::default()
}

/// Process user input during setup. Advances the state and pushes
/// the next step's messages to the queue.
///
/// Returns `true` when setup is complete.
pub(crate) fn handle_setup_input(
    state: &mut SetupState,
    queue: &mut MessageQueue,
    config: &mut ConfigManager,
    input: &str,
) -> bool {
    let input = input.trim();

    match &state.step {
        SetupStep::Welcome | SetupStep::ProviderSelect => {
            let lower = input.to_lowercase();
            if lower.contains("claude") {
                state.primary_provider = Some("claude".to_string());
                state.step = SetupStep::ApiKeyEntry {
                    provider: "claude".to_string(),
                };
                queue.push(QueuedMessage {
                    role: Role::Assistant,
                    channel: ChannelKind::System,
                    source: Some("system".into()),
                    content: Some("Please enter your Claude API key:".into()),
                    metadata: Some(
                        serde_json::json!({
                            "card_type": "api_key_entry",
                            "provider": "claude"
                        })
                        .to_string(),
                    ),
                    ..Default::default()
                });
            } else if lower.contains("chatgpt") || lower.contains("openai") || lower.contains("gpt")
            {
                state.primary_provider = Some("openai".to_string());
                state.step = SetupStep::ApiKeyEntry {
                    provider: "openai".to_string(),
                };
                queue.push(QueuedMessage {
                    role: Role::Assistant,
                    channel: ChannelKind::System,
                    source: Some("system".into()),
                    content: Some("Please enter your ChatGPT API key:".into()),
                    metadata: Some(
                        serde_json::json!({
                            "card_type": "api_key_entry",
                            "provider": "openai"
                        })
                        .to_string(),
                    ),
                    ..Default::default()
                });
            } else {
                queue.push(QueuedMessage {
                    role: Role::Assistant,
                    channel: ChannelKind::System,
                    source: Some("system".into()),
                    content: Some(
                        "Please choose 'claude' or 'chatgpt' as your AI provider.".into(),
                    ),
                    ..Default::default()
                });
            }
        }

        SetupStep::ApiKeyEntry { provider } => {
            let key = input.to_string();
            if key.is_empty() {
                queue.push(QueuedMessage {
                    role: Role::Assistant,
                    channel: ChannelKind::System,
                    source: Some("system".into()),
                    content: Some("API key cannot be empty. Please enter your key:".into()),
                    ..Default::default()
                });
                return false;
            }

            // Store the key in config.
            let config_key = match provider.as_str() {
                "claude" => "llm.claude_api_key",
                "openai" => "llm.openai_api_key",
                _ => "llm.api_key",
            };
            let _ = config.set(config_key, serde_json::json!(key));

            // Move to password step.
            state.step = SetupStep::MasterPassword;
            queue.push(QueuedMessage {
                role: Role::Assistant,
                channel: ChannelKind::System,
                source: Some("system".into()),
                content: Some(
                    "Now let's secure your data. Create a master password (minimum 8 characters):"
                        .into(),
                ),
                metadata: Some(
                    serde_json::json!({
                        "card_type": "password_entry"
                    })
                    .to_string(),
                ),
                ..Default::default()
            });
        }

        SetupStep::MasterPassword => {
            if input.len() < 8 {
                queue.push(QueuedMessage {
                    role: Role::Assistant,
                    channel: ChannelKind::System,
                    source: Some("system".into()),
                    content: Some("Password must be at least 8 characters. Try again:".into()),
                    ..Default::default()
                });
                return false;
            }

            state.master_password = Some(input.to_string());
            state.step = SetupStep::ConfirmPassword;
            queue.push(QueuedMessage {
                role: Role::Assistant,
                channel: ChannelKind::System,
                source: Some("system".into()),
                content: Some("Confirm your password:".into()),
                metadata: Some(
                    serde_json::json!({
                        "card_type": "password_entry"
                    })
                    .to_string(),
                ),
                ..Default::default()
            });
        }

        SetupStep::ConfirmPassword => {
            if Some(input.to_string()) != state.master_password {
                queue.push(QueuedMessage {
                    role: Role::Assistant,
                    channel: ChannelKind::System,
                    source: Some("system".into()),
                    content: Some("Passwords don't match. Enter your master password again:".into()),
                    ..Default::default()
                });
                state.step = SetupStep::MasterPassword;
                return false;
            }

            // Setup complete!
            state.step = SetupStep::Complete;
            queue.push(QueuedMessage {
                role: Role::System,
                channel: ChannelKind::System,
                source: Some("system".into()),
                content: Some("First-Boot Setup Complete".into()),
                level: Some(MessageLevel::Success),
                ..Default::default()
            });

            queue.push(QueuedMessage {
                role: Role::Assistant,
                channel: ChannelKind::System,
                source: Some("system".into()),
                content: Some("You're all set! How can I help?".into()),
                ..Default::default()
            });

            return true;
        }

        SetupStep::Complete => return true,

        // Other steps handled by the GTK first_boot.rs for now.
        _ => {}
    }

    false
}

/// Apply autoconfig and push status messages to the queue.
pub(crate) fn apply_autoconfig(
    queue: &mut MessageQueue,
    config: &mut ConfigManager,
    autoconfig: &serde_json::Value,
) {
    queue.push(QueuedMessage {
        role: Role::System,
        channel: ChannelKind::System,
        source: Some("system".into()),
        content: Some("Autoconfig detected — applying unattended configuration...".into()),
        level: Some(MessageLevel::Info),
        ..Default::default()
    });

    // Apply provider settings.
    if let Some(provider) = autoconfig.get("provider").and_then(|v| v.as_str()) {
        let _ = config.set("llm.provider", serde_json::json!(provider));
    }
    if let Some(key) = autoconfig.get("claude_api_key").and_then(|v| v.as_str()) {
        let _ = config.set("llm.claude_api_key", serde_json::json!(key));
    }
    if let Some(key) = autoconfig.get("openai_api_key").and_then(|v| v.as_str()) {
        let _ = config.set("llm.openai_api_key", serde_json::json!(key));
    }

    queue.push(QueuedMessage {
        role: Role::System,
        channel: ChannelKind::System,
        source: Some("system".into()),
        content: Some("Autoconfig applied successfully!".into()),
        level: Some(MessageLevel::Success),
        ..Default::default()
    });
}
