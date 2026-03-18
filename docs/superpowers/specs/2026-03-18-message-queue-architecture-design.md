# Message Queue Architecture — Centralized Message Store

**Date:** 2026-03-18
**Status:** Approved
**Scope:** Replace the current in-memory, per-session, per-path message handling with a centralized persistent message queue (SQLite). Collapse 3 startup paths into 1. Formalize channels with a trait. Decompose the 3,350-line app.rs monolith.

## Problem

The current architecture has three critical issues:

1. **3 startup paths** (`run_first_boot_setup`, `apply_autoconfig`, `activate_main`) each build their own window, wire their own signals, and initialize subsystems independently. Features like the VU meter and voice listener are missing in 2 of 3 paths. ~40% code duplication.

2. **No persistent history.** `conversation: Vec<Message>` is in-memory only, lost on reboot. Messages have no timestamp, channel, or source/model field.

3. **Direct rendering.** 25+ call sites directly call `chat_view.add_message()`. No way to search, export, or scroll back beyond the current session. Each channel has its own ad-hoc message flow.

## Solution

One persistent message queue (SQLite) that all channels read from and write to. One startup path. Formal channel trait with lazy rendering.

## 1. Message Queue (`aios-core/src/queue/`)

### Storage

SQLite via `rusqlite`, file at `~/.aios/messages.db`.

### Schema

```sql
CREATE TABLE messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp    TEXT NOT NULL,       -- ISO 8601 UTC
    channel      TEXT NOT NULL,       -- "desktop", "web", "signal", "voice", "system"
    role         TEXT NOT NULL,       -- "user", "assistant", "system", "tool"
    source       TEXT,                -- "claude-sonnet-4", "gpt-4o", "user", "system", "whisper"
    content      TEXT,                -- message text (nullable for tool-call-only)
    level        TEXT,                -- NULL, "info", "success", "warning", "error"
    tool_calls   TEXT,                -- JSON array of tool calls (nullable)
    tool_call_id TEXT,                -- for tool results: correlates to a tool_call
    metadata     TEXT                 -- JSON blob for extensibility
);

CREATE INDEX idx_messages_timestamp ON messages(timestamp);
CREATE INDEX idx_messages_channel ON messages(channel);
CREATE INDEX idx_messages_role ON messages(role);

CREATE TABLE clear_markers (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    channel   TEXT NOT NULL          -- "desktop", "web", "signal", "voice", "all"
);

CREATE INDEX idx_clear_channel ON clear_markers(channel);

-- FTS5 virtual table for full-text search on message content
CREATE VIRTUAL TABLE messages_fts USING fts5(content, content=messages, content_rowid=id);

-- Schema version tracking for future migrations
CREATE TABLE schema_version (
    version INTEGER NOT NULL
);
INSERT INTO schema_version VALUES (1);
```

### Database Size Management

Messages are never deleted. For typical usage (~100 messages/day), the database grows ~10MB/year. SQLite handles databases up to 281 TB. No retention policy needed for v1. If needed later, a `/history prune --before <date>` command can be added.

### QueuedMessage Type

```rust
pub struct QueuedMessage {
    pub id: i64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub channel: ChannelKind,   // uses existing enum (Desktop, Web, Signal, Voice)
    pub role: Role,             // uses existing enum (User, Assistant, System, Tool)
    pub source: Option<String>, // "claude-sonnet-4", "gpt-4o", "user", "whisper", etc.
    pub content: Option<String>,
    pub level: Option<MessageLevel>, // uses existing enum (Info, Success, Warning, Error)
    pub tool_calls: Option<String>,  // JSON serialized Vec<ToolCall>
    pub tool_call_id: Option<String>,
    pub metadata: Option<String>,    // JSON blob (card_type for setup, etc.)
}

// Conversion to existing LLM Message type:
impl From<&QueuedMessage> for Message {
    fn from(qm: &QueuedMessage) -> Self {
        Message {
            role: qm.role.clone(),
            content: qm.content.clone(),
            tool_call_id: qm.tool_call_id.clone(),
            tool_calls: qm.tool_calls.as_ref()
                .and_then(|j| serde_json::from_str(j).ok())
                .unwrap_or_default(),
        }
    }
}
```

Note: `ChannelKind` and `Role` are stored in SQLite as their string representations (via serde). The `ChannelKind` enum is extended with a `System` variant for boot/setup messages that don't originate from a specific channel.

### Voice: Modality, Not a Separate Channel

Voice is NOT an independent channel — it's a modality applied on top of Desktop. When the user speaks via wake word, the transcribed text is pushed to the queue with `channel: Desktop` and `source: "whisper"`. The TTS output is triggered by the Desktop channel's `render_message()` when voice is enabled.

The `VoiceChannel` listed in Section 2 is removed. Voice input (STT) and output (TTS) are handled by the `DesktopChannel` with voice-specific logic.
```

### API

```rust
pub struct MessageQueue {
    db: rusqlite::Connection,
    subscribers: Vec<mpsc::Sender<QueueEvent>>,
}

impl MessageQueue {
    /// Open or create the message database.
    pub fn open(path: &Path) -> Result<Self>;

    /// Push a message to the queue. Returns the message ID.
    /// Notifies all subscribers with QueueEvent::NewMessage(id).
    pub fn push(&mut self, msg: QueuedMessage) -> i64;

    /// Get the latest N messages.
    pub fn get_latest(&self, n: usize) -> Vec<QueuedMessage>;

    /// Get N messages before the given ID (for scroll-back).
    pub fn get_before(&self, before_id: i64, n: usize) -> Vec<QueuedMessage>;

    /// Get messages after the latest clear marker for a channel.
    /// This is what a channel should display by default.
    pub fn get_after_clear_marker(&self, channel: &str, n: usize) -> Vec<QueuedMessage>;

    /// Get recent user/assistant/tool messages for LLM context.
    /// Filters out system messages, clear markers, etc.
    pub fn get_llm_context(&self, limit: usize) -> Vec<QueuedMessage>;

    /// Insert a clear marker for a channel.
    /// Does NOT delete messages — just marks a display boundary.
    pub fn clear(&mut self, channel: &str);

    /// Insert clear markers for ALL channels.
    pub fn clear_all(&mut self);

    /// Subscribe to queue events. Returns a receiver.
    pub fn subscribe(&mut self) -> mpsc::Receiver<QueueEvent>;

    /// Full-text search across all messages.
    pub fn search(&self, query: &str) -> Vec<QueuedMessage>;

    /// Export all messages as JSON.
    pub fn export_json(&self, writer: &mut dyn std::io::Write) -> Result<()>;
}
```

### Events

```rust
pub enum QueueEvent {
    /// A new message was added. Contains the message ID.
    NewMessage(i64),
    /// A clear marker was inserted for the given channel.
    ClearMarker(String),
}
```

### Thread Safety

`MessageQueue` is wrapped in `Arc<tokio::sync::RwLock<MessageQueue>>` for shared access. SQLite is opened in WAL mode for concurrent read support.

- **Writes** (`push`, `clear`): take a write lock, execute INSERT, then release lock BEFORE notifying subscribers (to avoid blocking under lock)
- **Reads** (`get_latest`, `get_before`, `search`, `get_llm_context`): take a read lock — multiple readers do not block each other
- **Subscribers**: use `tokio::sync::mpsc::UnboundedSender<QueueEvent>` — sending never blocks
- **push(&self)**: uses `&self` not `&mut self` — the `RwLock` provides interior mutability. SQLite WAL mode handles concurrent access safely.

## 2. Channel Trait (`aios-core/src/channel/`)

### Base and Trait

```rust
/// Shared state for all channel implementations.
pub struct ChannelBase {
    pub kind: ChannelKind,
    pub queue_rx: mpsc::Receiver<QueueEvent>,
    pub last_rendered_id: Option<i64>,
}

/// Trait that every channel implements.
///
/// No `Send` bound — DesktopChannel holds GTK widgets which are not Send.
/// Queue event dispatch is handled per-channel: Desktop events are marshaled
/// to the GTK main thread via `glib::idle_add_local`, Web/Signal events
/// run on the Tokio runtime directly.
pub trait Channel {
    /// Channel identity.
    fn kind(&self) -> ChannelKind;
    fn capabilities(&self) -> &ChannelCapabilities;

    /// Render a single message in this channel's format.
    fn render_message(&self, msg: &QueuedMessage);

    /// Render a batch of messages (initial load or scroll-back).
    fn render_batch(&self, msgs: &[QueuedMessage]);

    /// Handle incoming user input from this channel.
    /// Returns the text to push to the queue, or None to ignore.
    fn on_input(&self, raw: &str) -> Option<String>;

    /// Whether this channel is currently active/visible.
    fn is_active(&self) -> bool;
}

// GTK thread safety note: DesktopChannel methods MUST be called from the
// GTK main thread. The event dispatch layer polls queue events via
// glib::timeout_add_local and calls render_message on the main thread.
// Web and Signal channels can receive events on any thread.
```

### Implementations

| Channel | `render_message` | Input Source |
|---------|-----------------|-------------|
| `DesktopChannel` | Appends GTK widget to ChatView | Prompt input + Voice STT |
| `WebChannel` | Sends `ServerMessage` via WebSocket | WebSocket `ClientMessage` |
| `SignalChannel` | Calls `signal-cli send_text` | signal-cli listener daemon |
| `VoiceChannel` | Calls TTS `speak_if_enabled` | Wake word + Whisper STT |

### Channel Lifecycle

1. On boot: all channels are created, each calls `queue.subscribe()` to get a receiver
2. `QueueEvent::NewMessage(id)` received → channel fetches the message from the queue → calls `render_message()` if `is_active()`
3. Inactive channels note the event but skip rendering — they catch up when activated
4. Scroll-up (Desktop/Web): channel calls `queue.get_before(oldest_id, 20)` → `render_batch()`
5. `QueueEvent::ClearMarker(channel)` received → channel clears its display, re-renders from the marker

### Channel Activation

When a channel becomes active (e.g., user opens the web UI):
1. Find the latest clear marker for this channel
2. Call `queue.get_after_clear_marker(channel, 50)` → `render_batch()`
3. Start processing new `QueueEvent`s

## 3. Unified Startup (One Path)

### Current: 3 paths, ~840 lines

```
activate()
  ├── vault missing + no autoconfig → run_first_boot_setup() → transition_to_normal_mode()
  ├── vault missing + autoconfig    → apply_autoconfig()      → transition_to_normal_mode()
  └── vault exists                  → activate_main()
```

### New: 1 path, ~100 lines

```
activate()
  1. Apply theme
  2. Open message queue (SQLite)
  3. Load config (create defaults if needed)
  4. Initialize i18n
  5. Initialize subsystems: tools, LLM manager, voice listener, VU meter
  6. Create all channels (Desktop, Web, Signal, Voice)
     — each subscribes to queue
  7. Build main window (always the same — prompt, chat, VU meter, header)
  8. Desktop channel loads last 50 messages from queue → render
  9. Check vault:
     a. Vault exists       → push system message "Ready. How can I help?"
     b. Autoconfig found   → apply config, create vault, push setup messages to queue
     c. No vault           → push first-boot conversation messages to queue
  10. Start message loop
```

### Setup as Conversation

The first-boot setup wizard becomes a conversation in the queue:

```
queue.push(role: "system", source: "system", level: "info", content: "Welcome to AiOS!")
queue.push(role: "assistant", source: "system", content: "Which AI provider would you like to use? Type 'claude' or 'openai'.")
// User types "claude" → pushed to queue as role: "user"
queue.push(role: "assistant", source: "system", content: "Please enter your Claude API key:")
// User types key → pushed to queue, key stored in vault
queue.push(role: "system", source: "system", level: "success", content: "Setup complete!")
```

No special `SetupConversation` state machine. The same queue, the same rendering pipeline. The setup conversation is scrollable, searchable, and persisted.

**Interactive elements via metadata:** Setup steps that need rich UI (provider selection buttons, password fields, wake word dropdowns) use the `metadata` JSON field to describe the card:

```json
{
  "card_type": "provider_select",
  "options": ["Claude", "OpenAI"],
  "description": "Choose your AI provider"
}
```

The `DesktopChannel` renders these as interactive GTK cards (buttons, entries, dropdowns) — same as today's setup cards but driven by queue metadata. `WebChannel` renders them as HTML forms. `SignalChannel` renders as numbered text options. Once the user interacts, the response is pushed to the queue as a normal user message and the card is dismissed.

**Card types:** `provider_select`, `api_key_entry`, `password_entry`, `dropdown_select`, `wake_word_select`, `confirmation`. Each is a simple JSON schema in metadata — the channel renders it according to its capabilities.

**Setup state tracking:** A simple state enum in `setup.rs` tracks which step we're on (provider selection, API key, password, etc.). Each user message advances the state. The state machine pushes the next card to the queue.

### Autoconfig Path

Autoconfig also just pushes messages:

```
queue.push(system/info, "Autoconfig detected — applying unattended configuration...")
queue.push(system/info, "Provider: claude | Keyboard: de | Mode: live ISO")
// vault created, config applied
queue.push(system/success, "Autoconfig applied successfully!")
```

Same rendering, same queue, same path.

### What Gets Eliminated

| Removed | Lines | Reason |
|---------|-------|--------|
| `run_first_boot_setup()` | ~365 | Setup is queue messages |
| `apply_autoconfig()` | ~141 | Autoconfig pushes to queue |
| `transition_to_normal_mode()` | ~245 | No transition needed — one path |
| `SetupConversation` state machine | ~500 | Simple state enum in `setup.rs` |
| `first_boot.rs` | ~800 | Deprecated |
| Duplicated config/i18n/window code | ~200 | Done once |
| **Total removed** | **~2,250** | |

## 4. Lazy Rendering (Desktop Channel)

### Current

`ChatView` appends a GTK widget for every `add_message()` call. All widgets stay in the tree forever. Long sessions = thousands of widgets = slow.

### New

The `DesktopChannel` manages a **window** of ~50 message widgets:

- On load: render last 50 messages from queue (after clear marker)
- On new message: append widget at bottom. If >50 widgets, remove the oldest from top.
- On scroll-up near top: `queue.get_before(oldest_rendered_id, 20)` → prepend widgets
- On clear marker: remove all widgets, render fresh from marker

### ChatView Changes

`ChatView` no longer has `add_message()` as a public API called from 25+ places. Instead:

```rust
impl ChatView {
    /// Render a batch of QueuedMessages (replaces add_message).
    pub fn render_messages(&self, msgs: &[QueuedMessage]);

    /// Append a single new message (for real-time updates).
    pub fn append_message(&self, msg: &QueuedMessage);

    /// Prepend older messages (for scroll-back).
    pub fn prepend_messages(&self, msgs: &[QueuedMessage]);

    /// Clear all widgets (for clear marker).
    pub fn clear_display(&self);
}
```

The `DesktopChannel` calls these methods. Nothing else does.

### Thinking Placeholder

The "thinking dots" indicator is **local UI state**, not a queue message. When the LLM is processing:

1. `DesktopChannel` shows a thinking placeholder widget (animated dots)
2. When the LLM response arrives → pushed to queue → `QueueEvent::NewMessage` received
3. `DesktopChannel` removes the placeholder, renders the real message
4. TTS signal flag works the same — placeholder stays until voice starts

The thinking state is per-channel, transient, never persisted. Only real messages go to the queue.

## 5. Conversation History Tool

### Location

`aios-tools/src/builtin/conversation_history.rs`

### Tool Definition

```
Name: conversation_history
Category: memory
Description: Search and browse past conversations from the persistent message queue.
```

### Actions

| Action | Parameters | Returns |
|--------|-----------|---------|
| `search` | `query: string` | Messages matching the text, with timestamps |
| `recent` | `n: int` (default 20) | Last N messages |
| `on_date` | `date: string` (YYYY-MM-DD) | Messages from that date |
| `between` | `from: string, to: string` | Messages in date range |
| `from_channel` | `channel: string, n: int` | Last N from a specific channel |
| `stats` | (none) | Total count, date range, per-channel counts, per-source counts |

### Integration

**Injection:** The tool receives a read-only reference to `MessageQueue` via constructor injection: `ConversationHistoryTool::new(queue: Arc<RwLock<MessageQueue>>)`. Registered in `ToolRegistry` during startup. Read-only — never pushes or modifies.

**Search implementation:** Uses SQLite FTS5 for the `search()` action. An FTS5 virtual table indexes the `content` column for fast full-text search.

The LLM can call this tool like any other:
- User: "What did we discuss yesterday?"
- AI: calls `conversation_history.on_date("2026-03-17")`
- AI: summarizes the results and responds

## 6. LLM Context from Queue

### Current

`conversation: Vec<Message>` in memory. Rebuilt each session. Lost on reboot.

### New

Before each LLM call:

```rust
let context = queue.get_llm_context(50);
```

**Filtering rules for `get_llm_context`:**
- Include: `role` in (User, Assistant, Tool)
- Exclude: `role` = System (boot messages, info/warning/error)
- Respects `clear_all` markers — returns nothing before the latest `clear_all` marker
- Per-channel `/clear` does NOT affect LLM context (only `/clear -all` does)
- Includes messages from ALL channels (one AI brain, one conversation — a Signal message and a Desktop message are both part of the context)
- Includes tool_call and tool_result messages (needed for the LLM to understand tool interactions)
- Returns results converted via `From<&QueuedMessage> for Message`

Context pruning/summarization (already in `aios-llm/src/context.rs`) works on the query result, same as before.

### Cross-Session Context

Because the queue persists, the LLM can see messages from before the last reboot. The `get_llm_context()` method returns the most recent N messages regardless of session boundaries (but respects `clear_all` markers). This gives the AI natural continuity.

## 7. `/clear` Command

### Current

`/clear` deletes all widgets from ChatView and resets the conversation vector.

### New

- `/clear` → inserts a `ClearMarker` for the **current active channel** only
- `/clear -all` → inserts `ClearMarker` for every channel

No messages are ever deleted. The clear marker is a display boundary — the channel renders nothing before its latest marker.

Each channel independently tracks its own clear marker. Clearing Desktop doesn't affect Web or Signal.

## 8. app.rs Decomposition

### Current

`aios-gtk/src/app.rs` — 3,350 lines, monolith.

### New File Structure

```
aios-gtk/src/
├── app.rs               (~300 lines)  — AiosApp struct, activate(), subsystem wiring
├── setup.rs             (~200 lines)  — first-boot + autoconfig (pushes to queue)
├── voice_listener.rs    (~250 lines)  — start_voice_listener() state machine
├── tts.rs               (~150 lines)  — speak_if_enabled, do_tts, summarize_for_tts
├── command_handler.rs   (~200 lines)  — slash command dispatch, BackgroundTask handling
├── message_loop.rs      (~150 lines)  — queue events → LLM calls → responses to queue
├── ui/
│   ├── chat_view.rs     (~300 lines)  — lazy rendering from queue, scroll-back
│   ├── main_window.rs   (~500 lines)  — window layout (existing, mostly unchanged)
│   ├── desktop_channel.rs (~200 lines) — Channel trait impl for GTK
│   ├── settings_dialog.rs (existing)
│   ├── panel_renderer.rs  (existing)
│   └── ...
```

### app.rs Becomes a Thin Orchestrator

```rust
impl AiosApp {
    pub fn activate(app: &adw::Application, rt: Handle) {
        Self::apply_theme("dark");

        let queue = MessageQueue::open(&config_dir().join("messages.db"));
        let config = ConfigManager::new();

        aios_core::i18n::init();
        let mut tools = ToolRegistry::new();
        tools.load_builtins();
        let llm = LlmManager::new();

        let desktop = DesktopChannel::new(&queue);
        let web = WebChannel::new(&queue);

        let window = build_main_window(app, &desktop);
        start_voice_listener(&queue, ...);

        if !vault_exists() {
            setup::run(&queue, &config);
        }

        message_loop::start(&queue, &llm, &tools);
    }
}
```

## 9. New Dependencies

| Crate | Version | Used In | Purpose |
|-------|---------|---------|---------|
| `rusqlite` | `0.32` | `aios-core` | SQLite database for message queue |

## 10. Migration

On first boot after the update, there is no existing `messages.db` — the queue starts empty. This is fine since the current system doesn't persist history anyway.

The setup conversation (if vault doesn't exist) or the boot status message (if vault exists) will be the first messages in the new queue.

No data migration needed.

## 11. Deliverables Summary

### New Files

| File | Crate | Purpose |
|------|-------|---------|
| `aios-core/src/queue/mod.rs` | aios-core | MessageQueue, QueuedMessage, QueueEvent |
| `aios-core/src/queue/schema.rs` | aios-core | SQLite table creation + migrations |
| `aios-tools/src/builtin/conversation_history.rs` | aios-tools | History search tool for AI |
| `aios-gtk/src/setup.rs` | aios-gtk | First-boot + autoconfig (queue-based) |
| `aios-gtk/src/voice_listener.rs` | aios-gtk | Extracted voice listener state machine |
| `aios-gtk/src/tts.rs` | aios-gtk | Extracted TTS functions |
| `aios-gtk/src/command_handler.rs` | aios-gtk | Extracted command dispatch |
| `aios-gtk/src/message_loop.rs` | aios-gtk | Unified message loop |
| `aios-gtk/src/ui/desktop_channel.rs` | aios-gtk | Channel trait impl for GTK |

### Modified Files

| File | Change |
|------|--------|
| `aios-core/Cargo.toml` | Add `rusqlite` dependency |
| `aios-core/src/lib.rs` | Add `pub mod queue;` |
| `aios-core/src/channel/mod.rs` | Add `Channel` trait, `ChannelBase` |
| `aios-core/src/types/message.rs` | Keep existing `Message` for LLM API compat; add conversion from `QueuedMessage` |
| `aios-tools/src/builtin/mod.rs` | Register `conversation_history` tool |
| `aios-gtk/src/app.rs` | Gut from 3,350 to ~300 lines |
| `aios-gtk/src/ui/chat_view.rs` | Replace `add_message` with queue-based lazy rendering |
| `aios-web/src/server.rs` | Implement `Channel` trait for `WebChannel` |
| `aios-signal/src/listener.rs` | Implement `Channel` trait for `SignalChannel` |

### Deprecated/Removed Files

| File | Reason |
|------|--------|
| `aios-gtk/src/ui/first_boot.rs` | ~2,300 lines — setup is queue messages now |

## 12. Testing

- Unit tests for `MessageQueue`: push, get_latest, get_before, clear, clear_all, search, export
- Unit tests for `QueuedMessage` serialization/deserialization
- Unit tests for clear marker logic (per-channel independence)
- Unit tests for `get_llm_context` filtering
- Integration test: push messages → subscribe → verify events received
- Integration test: full startup path → messages appear in queue
- Integration test: `/clear` on Desktop doesn't affect Web channel's view
- `conversation_history` tool: search, recent, date queries
