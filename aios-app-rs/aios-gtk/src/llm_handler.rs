//! LLM message handler — sends user messages to the AI and displays responses.
//!
//! Extracted from `app.rs`. This module handles the async LLM call from the
//! GTK main thread: it spawns the call on the Tokio runtime, polls for the
//! response via `glib::timeout_add_local`, and updates the chat view + TTS.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use tracing::info;

use crate::tts::{speak_if_enabled_with_signal, stop_tts};
use crate::ui::chat_view::ChatView;
use crate::ui::prompt_input::PromptInput;

/// Internal result type for async LLM communication.
pub(crate) enum LlmResult {
    Success {
        content: String,
        updated_history: Vec<aios_core::types::Message>,
    },
    Error(String),
}

/// Parse a raw LLM API error into a user-friendly message.
fn format_llm_error(raw: &str) -> String {
    // Try to parse as JSON to extract structured error info.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(msg) = json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
            let code = json.get("error").and_then(|e| e.get("code")).and_then(|c| c.as_u64()).unwrap_or(0);
            return match code {
                429 => format!("Rate limit exceeded. Your API quota is exhausted.\nWait a moment or switch providers with /provider.\n\nDetails: {}", &msg[..msg.len().min(200)]),
                401 => "Authentication failed. Your API key may be invalid or expired.\nUse /key to set a new key.".to_string(),
                403 => "Access denied. Your API key doesn't have permission for this model.\nCheck your subscription plan.".to_string(),
                404 => format!("Model not found. The configured model may not exist.\nUse /model to change it.\n\nDetails: {}", &msg[..msg.len().min(150)]),
                500..=599 => "The AI provider is experiencing issues. Try again in a moment.".to_string(),
                _ => format!("AI error (code {code}): {}", &msg[..msg.len().min(200)]),
            };
        }
    }

    // Check for common patterns in raw text.
    let lower = raw.to_lowercase();
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("quota") {
        return format!("Rate limit exceeded. Wait a moment or switch providers with /provider.\n\nDetails: {}", &raw[..raw.len().min(200)]);
    }
    if lower.contains("401") || lower.contains("unauthorized") || lower.contains("invalid.*key") {
        return "Authentication failed. Check your API key with /key.".to_string();
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return "Request timed out. The AI provider may be slow. Try again.".to_string();
    }
    if lower.contains("connection") || lower.contains("network") {
        return "Network error. Check your internet connection.".to_string();
    }

    // Fallback: truncate the raw error.
    if raw.len() > 300 {
        format!("AI error: {}...", &raw[..300])
    } else {
        format!("AI error: {raw}")
    }
}

/// Minimal interface that `send_to_llm` needs from the application state.
///
/// Using a trait avoids coupling this module to the concrete `AiosApp` struct.
pub(crate) trait LlmState {
    fn llm(&self) -> Arc<tokio::sync::Mutex<aios_llm::LlmManager>>;
    fn rt(&self) -> tokio::runtime::Handle;
    fn conversation(&self) -> Vec<aios_core::types::Message>;
    fn tool_schemas(&self) -> Vec<aios_core::types::ToolSchema>;
    fn push_conversation(&mut self, msg: aios_core::types::Message);
    fn replace_conversation(&mut self, msgs: Vec<aios_core::types::Message>);
    fn queue(&self) -> Option<Arc<std::sync::Mutex<aios_core::queue::MessageQueue>>>;
    fn config_snapshot(&self) -> aios_core::config::ConfigManager;
}

/// Send a user message to the LLM on the Tokio runtime.
///
/// This is the primary Desktop-channel LLM path. It:
/// 1. Stops any active TTS
/// 2. Shows a thinking placeholder
/// 3. Records the user message in conversation history + queue
/// 4. Spawns the LLM call on Tokio
/// 5. Polls for the response and updates the UI + TTS
pub(crate) fn send_to_llm<S: LlmState + 'static>(
    state: &Rc<RefCell<S>>,
    chat_view: &ChatView,
    _prompt: &PromptInput,
    text: String,
) {
    // Stop any active TTS when the user sends a new message.
    stop_tts();

    // Show thinking placeholder immediately.
    let thinking_handle = chat_view.add_thinking();

    let llm;
    let rt;
    let queue_for_llm;
    {
        let s = state.borrow();
        llm = s.llm();
        rt = s.rt();
        queue_for_llm = s.queue();
    }

    // Record user message in conversation history.
    state
        .borrow_mut()
        .push_conversation(aios_core::types::Message::user(&text));

    // Push user message to the persistent queue.
    if let Some(ref q) = queue_for_llm {
        q.lock().unwrap().push(aios_core::queue::QueuedMessage {
            role: aios_core::types::Role::User,
            channel: aios_core::channel::ChannelKind::Desktop,
            source: Some("user".into()),
            content: Some(text.clone()),
            ..Default::default()
        });
    }

    // Use mpsc channel: tokio task sends result, GTK polls via timeout_add_local.
    let (tx, rx) = std::sync::mpsc::channel::<LlmResult>();
    let history = state.borrow().conversation();
    let tool_schemas = state.borrow().tool_schemas();

    // Spawn LLM call on Tokio (no GTK types captured -- all Send-safe).
    rt.spawn(async move {
        let mut llm_guard = llm.lock().await;
        let mut history = history;

        if history
            .last()
            .is_some_and(|m| m.role == aios_core::types::Role::User)
        {
            history.pop();
        }

        // Route tools based on message content (Dynamic Tool Bundling).
        let routed = llm_guard.route_tools(&text);
        let schemas = if routed.is_empty() {
            // No keyword match — send all tools as fallback.
            tool_schemas.clone()
        } else {
            tool_schemas
                .iter()
                .filter(|s| {
                    // Match tool category to routed categories.
                    // Tools without a known category are always included.
                    routed.iter().any(|c| s.name.contains(c) || c == "general")
                        || routed.contains(&"ui".to_string())
                        || routed.contains(&"general".to_string())
                })
                .cloned()
                .collect()
        };

        let result = llm_guard
            .chat(&text, Some(&mut history), &schemas, None)
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

    // Poll for LLM response. Once it arrives, start TTS and wait for
    // voice to begin before removing the thinking placeholder.
    let chat_view_ref = chat_view.clone();
    let state_ref = state.clone();
    let tts_started = Arc::new(std::sync::atomic::AtomicBool::new(false));

    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        match rx.try_recv() {
            Ok(LlmResult::Success {
                content,
                updated_history,
            }) => {
                // Response arrived -- start TTS with signal.
                let tts_flag = tts_started.clone();
                {
                    let s = state_ref.borrow();
                    let cfg = s.config_snapshot();
                    speak_if_enabled_with_signal(&content, &cfg, Some(tts_flag.clone()));
                }

                // Update conversation history.
                {
                    let mut s = state_ref.borrow_mut();
                    s.replace_conversation(updated_history);
                    s.push_conversation(aios_core::types::Message::assistant(&content));

                    // Push assistant response to the persistent queue.
                    if let Some(ref q) = s.queue() {
                        q.lock().unwrap().push(aios_core::queue::QueuedMessage {
                            role: aios_core::types::Role::Assistant,
                            channel: aios_core::channel::ChannelKind::Desktop,
                            source: Some("assistant".into()),
                            content: Some(content.clone()),
                            ..Default::default()
                        });
                    }
                }

                // Phase 2: wait for TTS to start, then swap thinking -> real message.
                let cv = chat_view_ref.clone();
                let handle = thinking_handle.clone();
                let content_for_display = content.clone();
                glib::timeout_add_local(
                    std::time::Duration::from_millis(30),
                    move || {
                        if tts_flag.load(std::sync::atomic::Ordering::Relaxed) {
                            cv.remove_thinking(&handle);
                            cv.add_message("assistant", &content_for_display);
                            glib::ControlFlow::Break
                        } else {
                            glib::ControlFlow::Continue
                        }
                    },
                );

                glib::ControlFlow::Break
            }
            Ok(LlmResult::Error(err)) => {
                chat_view_ref.remove_thinking(&thinking_handle);
                let friendly = format_llm_error(&err);
                chat_view_ref.add_level_message(
                    aios_core::types::MessageLevel::Warning,
                    &friendly,
                );
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                chat_view_ref.remove_thinking(&thinking_handle);
                glib::ControlFlow::Break
            }
        }
    });
}

/// Handle an incoming message from a remote channel (Web, Signal).
///
/// This runs the LLM call and routes the response back to the originating
/// channel via the broadcast/sender infrastructure.
pub(crate) fn handle_remote_llm_message<S: LlmState + 'static>(
    state: &Rc<RefCell<S>>,
    chat_view: &ChatView,
    text: &str,
    sender_id: Option<String>,
    active_channel: aios_core::channel::ChannelKind,
    web_tx: Option<tokio::sync::broadcast::Sender<String>>,
    signal_sender: Option<Arc<aios_signal::SignalSender>>,
) {
    let llm;
    let rt;
    let history;
    {
        let s = state.borrow();
        llm = s.llm();
        rt = s.rt();
        history = s.conversation();
    }

    let text = text.to_string();
    let (resp_tx, resp_rx) = std::sync::mpsc::channel::<LlmResult>();

    rt.spawn(async move {
        let mut llm_guard = llm.lock().await;
        let mut history = history;
        if history
            .last()
            .is_some_and(|m| m.role == aios_core::types::Role::User)
        {
            history.pop();
        }
        let result = llm_guard.chat(&text, Some(&mut history), &[], None).await;
        drop(llm_guard);

        match result {
            Ok(response) => {
                let content = response.content.unwrap_or_else(|| {
                    "I received your message but have no response text.".to_string()
                });
                // Route to active channel.
                match active_channel {
                    aios_core::channel::ChannelKind::Web => {
                        if let Some(ref tx) = web_tx {
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
                        if let Some(ref sender) = signal_sender {
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

    // Poll for this response (displays on Desktop).
    let chat_for_resp = chat_view.clone();
    let state_for_resp = state.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        match resp_rx.try_recv() {
            Ok(LlmResult::Success {
                content,
                updated_history,
            }) => {
                chat_for_resp.add_message("assistant", &content);
                {
                    let s = state_for_resp.borrow();
                    let cfg = s.config_snapshot();
                    crate::tts::speak_if_enabled(&content, &cfg);
                }
                {
                    let mut s = state_for_resp.borrow_mut();
                    s.replace_conversation(updated_history);
                    s.push_conversation(aios_core::types::Message::assistant(&content));
                }
                glib::ControlFlow::Break
            }
            Ok(LlmResult::Error(err)) => {
                let friendly = format_llm_error(&err);
                chat_for_resp.add_level_message(
                    aios_core::types::MessageLevel::Warning,
                    &friendly,
                );
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });

    info!(
        "Remote LLM message dispatched for channel {:?}",
        active_channel
    );
}
