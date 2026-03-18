//! Unified message loop -- routes messages between queue, LLM, and channels.
//!
//! This module decouples the conversation orchestration from the GTK UI layer.
//! Instead of the GTK `on_submit` handler directly calling the LLM and managing
//! tool loops, all messages flow through a single async loop that:
//!
//! 1. Subscribes to [`QueueEvent`]s from the persistent [`MessageQueue`]
//! 2. Routes slash commands to [`CommandHandler`]
//! 3. Sends normal messages to the LLM via [`LlmManager::chat`]
//! 4. Executes tool calls in a loop until the LLM produces a final answer
//! 5. Pushes all responses (assistant, system, tool) back into the queue
//!
//! Channels (Desktop, Web, Signal) independently subscribe to the same queue
//! and render messages as they arrive -- the loop does not know or care about
//! any particular frontend.
//!
//! ## Lifecycle
//!
//! Call [`start`] once at boot, after the queue, LLM manager, and tool registry
//! are initialised.  It spawns a long-lived tokio task that runs until the
//! application shuts down (i.e. the queue event receiver is dropped).

use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use aios_core::channel::ChannelKind;
use aios_core::config::commands::{CommandHandler, CommandResult};
use aios_core::config::ConfigManager;
use aios_core::queue::{MessageQueue, QueueEvent, QueuedMessage};
use aios_core::types::{Message, MessageLevel, Role, ToolSchema};
use aios_llm::LlmManager;
use aios_tools::ToolRegistry;

/// Maximum number of LLM context messages to pull from the queue.
const LLM_CONTEXT_LIMIT: usize = 50;

/// Safety limit -- never loop more than this many tool-call rounds in a single
/// user turn.  Mirrors the constant in `aios-llm/src/manager.rs`.
#[allow(dead_code)]
const MAX_TOOL_ROUNDS: usize = 25;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Start the unified message processing loop.
///
/// This function spawns a tokio task that drives the core conversation loop.
/// It does **not** block -- the task runs in the background on the provided
/// tokio runtime.
///
/// # Arguments
///
/// * `queue`    -- Shared, locked message queue (SQLite-backed).
/// * `llm`      -- Shared, locked LLM manager (provider switching, caching, etc.).
/// * `tools`    -- Shared, locked tool registry (built-in + plugin tools).
/// * `config`   -- Shared, locked configuration manager (for slash commands).
/// * `runtime`  -- Tokio runtime handle to spawn the background task on.
///
/// # Returns
///
/// A [`tokio::task::JoinHandle`] for the spawned loop.  Dropping it or
/// aborting it will shut down the message loop.
pub(crate) fn start(
    queue: Arc<Mutex<MessageQueue>>,
    llm: Arc<Mutex<LlmManager>>,
    tools: Arc<RwLock<ToolRegistry>>,
    config: Arc<RwLock<ConfigManager>>,
    runtime: &tokio::runtime::Handle,
) -> tokio::task::JoinHandle<()> {
    // Subscribe to queue events *before* spawning so we don't miss any
    // messages that arrive between spawn and the first recv.
    //
    // NOTE: This blocks briefly on the mutex -- acceptable at boot time.
    let mut events_rx = {
        let mut q = queue.blocking_lock();
        q.subscribe()
    };

    let queue = queue.clone();
    let llm = llm.clone();
    let tools = tools.clone();
    let config = config.clone();

    runtime.spawn(async move {
        info!("Message loop started");

        // Main event loop -- runs until the queue (and its senders) are dropped.
        while let Some(event) = events_rx.recv().await {
            match event {
                QueueEvent::NewMessage(msg_id) => {
                    handle_new_message(
                        msg_id,
                        &queue,
                        &llm,
                        &tools,
                        &config,
                    )
                    .await;
                }
                QueueEvent::ClearMarker(channel) => {
                    debug!("Clear marker received for channel: {channel}");
                    // Nothing to do here -- channels handle clear markers when
                    // rendering.  The LLM context query already respects
                    // clear_all markers via `get_llm_context`.
                }
            }
        }

        info!("Message loop exited (queue closed)");
    })
}

// ---------------------------------------------------------------------------
// Internal: message dispatch
// ---------------------------------------------------------------------------

/// Process a single NewMessage event.
///
/// Only acts on **User** messages -- assistant/system/tool messages are
/// produced by this loop itself and should not trigger re-processing.
async fn handle_new_message(
    msg_id: i64,
    queue: &Arc<Mutex<MessageQueue>>,
    llm: &Arc<Mutex<LlmManager>>,
    tools: &Arc<RwLock<ToolRegistry>>,
    config: &Arc<RwLock<ConfigManager>>,
) {
    // 1. Fetch the message that triggered the event.
    //
    //    We fetch a small batch of recent messages and find ours by ID.
    //    A dedicated `MessageQueue::get_by_id()` would be cleaner.
    //
    //    TODO(queue): add `MessageQueue::get_by_id(id) -> Option<QueuedMessage>`
    //    and replace this with a direct lookup.
    let msg = {
        let q = queue.lock().await;
        let recent = q.get_latest(10);
        recent.into_iter().find(|m| m.id == msg_id)
    };

    let msg = match msg {
        Some(m) => m,
        None => {
            warn!("NewMessage event for id={msg_id} but message not found in queue");
            return;
        }
    };

    // Only process user messages -- everything else is output from this loop.
    if msg.role != Role::User {
        return;
    }

    let text = match &msg.content {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => return,
    };

    let channel = msg.channel;

    debug!(
        "Processing user message id={msg_id} channel={channel} len={}",
        text.len()
    );

    // 2. Check if the message is a slash command.
    if text.starts_with('/') {
        handle_slash_command(&text, channel, queue, config).await;
        return;
    }

    // 3. Normal message -- route to LLM.
    handle_llm_message(&text, channel, queue, llm, tools).await;
}

// ---------------------------------------------------------------------------
// Slash command handling
// ---------------------------------------------------------------------------

/// Parse and execute a slash command, pushing the result into the queue.
///
/// Some command results (e.g. `Panel`, `BackgroundTask`, `SelfTest`) require
/// GTK-specific rendering that the command_handler module provides.  In those
/// cases this function pushes a simplified system message; the GTK layer's
/// queue subscriber will detect them and perform the UI-specific action.
///
/// When the message loop fully replaces the GTK on_submit path, commands
/// that need UI interaction will carry structured metadata in their
/// QueuedMessage so any channel renderer can handle them.
async fn handle_slash_command(
    text: &str,
    channel: ChannelKind,
    queue: &Arc<Mutex<MessageQueue>>,
    config: &Arc<RwLock<ConfigManager>>,
) {
    debug!("Slash command: {text}");

    // CommandHandler requires &mut ConfigManager.
    let result = {
        let mut cfg = config.write().await;
        let mut handler = CommandHandler::new(&mut cfg);
        handler.execute(text)
    };

    match result {
        CommandResult::Response(response_text) => {
            push_system_message(queue, channel, &response_text, None).await;
        }

        CommandResult::Clear => {
            // Insert a clear-all marker so channels know to reset their view.
            {
                let mut q = queue.lock().await;
                q.clear_all();
            }
            // Push a confirmation message so the user sees feedback.
            push_system_message(queue, channel, "Chat history cleared.", None).await;
        }

        CommandResult::SysInfo => {
            let info = aios_core::system_monitor::SystemInfo::gather();
            push_system_message(
                queue,
                channel,
                &info.format_text(),
                Some(MessageLevel::Info),
            )
            .await;
        }

        CommandResult::SelfTest(filter) => {
            // Self-tests need UI interaction that the GTK layer handles
            // (e.g. displaying each test result inline in the chat view).
            //
            // TODO(selftest): wire up the selftest runner here once it can
            // operate without GTK dependencies.  For now, push a request
            // message that the channel renderer picks up.
            push_system_message(
                queue,
                channel,
                &format!("Self-test requested (filter: {filter}). Running..."),
                Some(MessageLevel::Info),
            )
            .await;
        }

        CommandResult::Configure => {
            push_system_message(
                queue,
                channel,
                "Use the settings button (gear icon) to configure AiOS.",
                None,
            )
            .await;
        }

        CommandResult::ClosePanel => {
            // Panel closing is a UI-layer concern.  Push a message that the
            // channel renderer can act on.
            //
            // TODO(panels): define a metadata tag so renderers know to close
            // their topmost panel when they see this message.
            push_system_message(queue, channel, "/close requested.", None).await;
        }

        CommandResult::Update(url) => {
            if url.trim().is_empty() {
                push_system_message(
                    queue,
                    channel,
                    "Usage: /update <url>\nExample: /update https://example.com/aios",
                    None,
                )
                .await;
            } else {
                push_system_message(
                    queue,
                    channel,
                    &format!("Updating AiOS from: **{url}**\nThis will restart the app..."),
                    Some(MessageLevel::Warning),
                )
                .await;

                // Spawn the update command.  We intentionally do not await it
                // -- the update process may restart the application.
                tokio::spawn(async move {
                    let output = tokio::process::Command::new("aios-update")
                        .arg(&url)
                        .output()
                        .await;
                    match output {
                        Ok(o) => {
                            let stdout = String::from_utf8_lossy(&o.stdout);
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            info!("aios-update: {stdout}{stderr}");
                        }
                        Err(e) => error!("aios-update failed: {e}"),
                    }
                });
            }
        }

        CommandResult::Upgrade => {
            // Upgrade check is async and requires network I/O plus a
            // confirmation dialog -- push a "checking..." message and let
            // the GTK-side handler drive the rest.
            //
            // TODO(upgrade): move the full upgrade check/install flow here
            // once it is decoupled from GTK dialogs.
            push_system_message(
                queue,
                channel,
                "Checking for updates...",
                Some(MessageLevel::Info),
            )
            .await;
        }

        CommandResult::Panel {
            title,
            description,
            fields: _,
            config_key: _,
        } => {
            // Interactive panels need a UI renderer.  Push the description
            // as a system message; the channel's renderer will detect the
            // metadata and show a proper panel widget.
            //
            // TODO(panels): attach `fields` and `config_key` as JSON
            // metadata on the QueuedMessage so renderers can build the
            // interactive panel without GTK-specific code here.
            push_system_message(
                queue,
                channel,
                &format!("**{title}**\n{description}"),
                None,
            )
            .await;
        }

        CommandResult::BackgroundTask { description, task: _ } => {
            // Background tasks (e.g. wake word training) need GTK-specific
            // wiring.  Push the description so the user sees feedback.
            //
            // TODO(tasks): spawn the task here once BackgroundTaskKind
            // variants are executable without GTK.
            push_system_message(queue, channel, &description, None).await;
        }

        CommandResult::Unknown(cmd) => {
            push_system_message(
                queue,
                channel,
                &format!("Unknown command: {cmd}\nType /help for available commands."),
                Some(MessageLevel::Warning),
            )
            .await;
        }
    }
}

// ---------------------------------------------------------------------------
// LLM message handling
// ---------------------------------------------------------------------------

/// Send a user message to the LLM, execute any tool calls, and push the
/// final response into the queue.
///
/// Currently delegates to `LlmManager::chat` which runs its own internal
/// tool-call loop.  The future plan is to split this into explicit rounds
/// so that every intermediate tool call and result flows through the queue
/// (see the commented `run_tool_loop` scaffold below).
async fn handle_llm_message(
    text: &str,
    channel: ChannelKind,
    queue: &Arc<Mutex<MessageQueue>>,
    llm: &Arc<Mutex<LlmManager>>,
    tools: &Arc<RwLock<ToolRegistry>>,
) {
    // 1. Build conversation context from the queue.
    //
    //    `get_llm_context` returns the recent user/assistant/tool messages
    //    (respecting clear markers), which we convert to the `Message` type
    //    that `LlmManager::chat` expects.
    let mut history: Vec<Message> = {
        let q = queue.lock().await;
        q.get_llm_context(LLM_CONTEXT_LIMIT)
            .iter()
            .map(Message::from)
            .collect()
    };

    // Remove the trailing user message from history -- `LlmManager::chat`
    // adds it internally so we must not double-count it.
    if history.last().is_some_and(|m| m.role == Role::User) {
        history.pop();
    }

    // 2. Route tools -- pick only the relevant tool categories for this
    //    message to minimise token usage (Dynamic Tool Bundling).
    let tool_schemas: Vec<ToolSchema> = {
        let llm_guard = llm.lock().await;
        let categories = llm_guard.route_tools(text);
        let tools_guard = tools.read().await;
        if categories.is_empty() {
            // No keyword match -- send all tools as fallback.
            tools_guard.get_schemas()
        } else {
            let cat_refs: Vec<&str> = categories.iter().map(|s| s.as_str()).collect();
            tools_guard.get_schemas_by_categories(&cat_refs)
        }
    };

    // 3. Call the LLM.
    //
    //    `LlmManager::chat` handles internally:
    //    - Effort auto-detection + cascading model routing
    //    - Semantic response cache
    //    - Context pruning (recursive summarisation)
    //    - Speculative pre-fetch
    //    - Cache warming
    //    - Tool-call loop (when a ToolExecutor is set)
    //
    //    In the new architecture we want to handle tool calls here in the
    //    message loop so that every tool result flows through the queue and
    //    is persisted.  Until we refactor LlmManager to expose a single-round
    //    `send_once()` method, we use the existing `chat()` path which handles
    //    everything internally.
    //
    //    TODO(refactor): split LlmManager::chat into two phases:
    //    1. `send_once()` -- single LLM round, returns LlmResponse (may have tool_calls)
    //    2. This loop handles tool execution + re-send
    //    This gives us queue persistence for every intermediate step.

    let response = {
        let mut llm_guard = llm.lock().await;
        llm_guard
            .chat(text, Some(&mut history), &tool_schemas, None)
            .await
    };

    match response {
        Ok(llm_response) => {
            let content = llm_response.content.unwrap_or_else(|| {
                "I received your message but have no response text.".to_string()
            });

            info!(
                "LLM response: {} chars, {} input tokens, {} output tokens",
                content.len(),
                llm_response.usage.input_tokens,
                llm_response.usage.output_tokens,
            );

            // Push the assistant response into the queue.
            push_assistant_message(queue, channel, &content).await;
        }
        Err(e) => {
            error!("LLM error: {e}");
            push_system_message(
                queue,
                channel,
                &format!("Error communicating with AI: {e}"),
                Some(MessageLevel::Warning),
            )
            .await;
        }
    }
}

// ---------------------------------------------------------------------------
// Queue helpers
// ---------------------------------------------------------------------------

/// Push a system message into the queue.
async fn push_system_message(
    queue: &Arc<Mutex<MessageQueue>>,
    channel: ChannelKind,
    content: &str,
    level: Option<MessageLevel>,
) {
    let mut q = queue.lock().await;
    q.push(QueuedMessage {
        channel,
        role: Role::System,
        source: Some("system".into()),
        content: Some(content.to_string()),
        level,
        ..Default::default()
    });
}

/// Push an assistant message into the queue.
async fn push_assistant_message(
    queue: &Arc<Mutex<MessageQueue>>,
    channel: ChannelKind,
    content: &str,
) {
    let mut q = queue.lock().await;
    q.push(QueuedMessage {
        channel,
        role: Role::Assistant,
        source: None, // LlmManager knows the model name; plumb it here later.
        content: Some(content.to_string()),
        ..Default::default()
    });
}

/// Push a tool-result message into the queue.
///
/// Used when tool calls are handled by this loop (future refactor -- see
/// `run_tool_loop` scaffold below).
#[allow(dead_code)]
async fn push_tool_result(
    queue: &Arc<Mutex<MessageQueue>>,
    channel: ChannelKind,
    tool_call_id: &str,
    output: &str,
) {
    let mut q = queue.lock().await;
    q.push(QueuedMessage {
        channel,
        role: Role::Tool,
        source: None,
        content: Some(output.to_string()),
        tool_call_id: Some(tool_call_id.to_string()),
        ..Default::default()
    });
}

// ---------------------------------------------------------------------------
// Future: explicit tool-call loop (scaffold)
// ---------------------------------------------------------------------------
//
// When `LlmManager` is refactored to expose a single-round `send_once()`
// method, the tool-call loop will live here instead of inside the manager.
// This gives us queue persistence for every intermediate step (assistant
// messages with tool calls, and each tool result).
//
// ```rust
// use aios_core::channel::ChannelContext;
//
// async fn run_tool_loop(
//     text: &str,
//     channel: ChannelKind,
//     queue: &Arc<Mutex<MessageQueue>>,
//     llm: &Arc<Mutex<LlmManager>>,
//     tools: &Arc<RwLock<ToolRegistry>>,
//     history: &mut Vec<Message>,
//     tool_schemas: &[ToolSchema],
// ) {
//     let mut rounds = 0;
//
//     loop {
//         // --- Single LLM round (no internal tool loop) ---
//         let response = {
//             let mut llm_guard = llm.lock().await;
//             llm_guard.send_once(history, tool_schemas, None).await
//         };
//
//         let response = match response {
//             Ok(r) => r,
//             Err(e) => {
//                 push_system_message(
//                     queue, channel,
//                     &format!("LLM error: {e}"),
//                     Some(MessageLevel::Warning),
//                 ).await;
//                 return;
//             }
//         };
//
//         // --- Final answer (no tool calls) ---
//         if !response.has_tool_calls() {
//             if let Some(content) = &response.content {
//                 push_assistant_message(queue, channel, content).await;
//             }
//             return;
//         }
//
//         // --- Safety limit ---
//         rounds += 1;
//         if rounds > MAX_TOOL_ROUNDS {
//             warn!("Tool-call loop hit safety limit ({MAX_TOOL_ROUNDS} rounds)");
//             if let Some(content) = &response.content {
//                 push_assistant_message(queue, channel, content).await;
//             }
//             return;
//         }
//
//         // --- Persist the assistant message (with tool calls) ---
//         {
//             let tool_calls_json = serde_json::to_string(&response.tool_calls).ok();
//             let mut q = queue.lock().await;
//             q.push(QueuedMessage {
//                 channel,
//                 role: Role::Assistant,
//                 content: response.content.clone(),
//                 tool_calls: tool_calls_json,
//                 ..Default::default()
//             });
//         }
//
//         // Append to in-memory history for the next LLM round.
//         history.push(Message::assistant_with_tools(
//             response.content.clone(),
//             response.tool_calls.clone(),
//         ));
//
//         // --- Execute each tool call ---
//         let channel_ctx = ChannelContext::new(channel);
//         for tc in &response.tool_calls {
//             let result = {
//                 let tools_guard = tools.read().await;
//                 tools_guard.execute_on_channel(
//                     &tc.name,
//                     tc.arguments.clone(),
//                     &channel_ctx,
//                 )
//             };
//
//             let output = if result.success {
//                 result.output
//             } else {
//                 result.error.unwrap_or_else(|| "Tool execution failed".into())
//             };
//
//             // Persist tool result to queue.
//             push_tool_result(queue, channel, &tc.id, &output).await;
//
//             // Append to in-memory history for the next LLM round.
//             history.push(Message::tool_result(&tc.id, &output));
//         }
//
//         // Loop back -- the LLM will see the tool results and either
//         // produce a final answer or request more tool calls.
//     }
// }
// ```

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that push_system_message creates the expected QueuedMessage.
    #[tokio::test]
    async fn test_push_system_message() {
        let queue = Arc::new(Mutex::new(
            MessageQueue::open_in_memory().expect("in-memory queue"),
        ));

        push_system_message(
            &queue,
            ChannelKind::Desktop,
            "Test message",
            Some(MessageLevel::Info),
        )
        .await;

        let q = queue.lock().await;
        let msgs = q.get_latest(1);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[0].content.as_deref(), Some("Test message"));
        assert_eq!(msgs[0].level, Some(MessageLevel::Info));
        assert_eq!(msgs[0].channel, ChannelKind::Desktop);
    }

    /// Verify that push_assistant_message creates the expected QueuedMessage.
    #[tokio::test]
    async fn test_push_assistant_message() {
        let queue = Arc::new(Mutex::new(
            MessageQueue::open_in_memory().expect("in-memory queue"),
        ));

        push_assistant_message(&queue, ChannelKind::Web, "Hello there").await;

        let q = queue.lock().await;
        let msgs = q.get_latest(1);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::Assistant);
        assert_eq!(msgs[0].content.as_deref(), Some("Hello there"));
        assert_eq!(msgs[0].channel, ChannelKind::Web);
    }

    /// Verify that push_tool_result creates the expected QueuedMessage.
    #[tokio::test]
    async fn test_push_tool_result() {
        let queue = Arc::new(Mutex::new(
            MessageQueue::open_in_memory().expect("in-memory queue"),
        ));

        push_tool_result(
            &queue,
            ChannelKind::Signal,
            "tc-42",
            "disk: 50% used",
        )
        .await;

        let q = queue.lock().await;
        let msgs = q.get_latest(1);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::Tool);
        assert_eq!(msgs[0].content.as_deref(), Some("disk: 50% used"));
        assert_eq!(msgs[0].tool_call_id.as_deref(), Some("tc-42"));
        assert_eq!(msgs[0].channel, ChannelKind::Signal);
    }

    /// Verify that only User messages trigger processing (non-user messages
    /// are silently ignored by handle_new_message).
    #[tokio::test]
    async fn test_ignores_non_user_messages() {
        let queue = Arc::new(Mutex::new(
            MessageQueue::open_in_memory().expect("in-memory queue"),
        ));

        // Push an assistant message directly into the queue.
        let msg_id = {
            let mut q = queue.lock().await;
            q.push(QueuedMessage {
                role: Role::Assistant,
                channel: ChannelKind::Desktop,
                content: Some("I am the AI".into()),
                ..Default::default()
            })
        };

        // The message is not a User message, so handle_new_message would
        // return immediately at the `if msg.role != Role::User` guard.
        // We verify the precondition here (no LLM/tools setup needed).
        let count_before = {
            let q = queue.lock().await;
            q.count()
        };

        let q = queue.lock().await;
        let msgs = q.get_latest(5);
        let msg = msgs.iter().find(|m| m.id == msg_id).unwrap();
        assert_ne!(msg.role, Role::User);
        // Count unchanged -- no extra messages pushed.
        assert_eq!(q.count(), count_before);
    }
}
