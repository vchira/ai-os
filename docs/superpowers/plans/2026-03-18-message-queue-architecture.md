# Message Queue Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace in-memory message handling with a centralized SQLite message queue, collapse 3 startup paths into 1, formalize channels with a trait, decompose the 3,350-line app.rs monolith, and add conversation history search.

**Architecture:** SQLite message queue in `aios-core` stores all messages persistently. Channels subscribe to queue events and render independently. One startup path initializes everything once. Setup is just messages in the queue. ChatView does lazy rendering from the queue.

**Tech Stack:** Rust, `rusqlite` (SQLite), `tokio::sync` (RwLock, mpsc), GTK4/libadwaita, existing `aios-core` types

**Spec:** `docs/superpowers/specs/2026-03-18-message-queue-architecture-design.md`

---

## Implementation Phases

This is a big-bang refactor but implemented in a specific order so each task builds on the previous. The system may not fully compile between some tasks in the middle (Tasks 5-8), but each phase boundary produces a working system.

**Phase 1 (Tasks 1-4):** Foundation — MessageQueue, Channel trait, rusqlite dep. Independently testable.
**Phase 2 (Tasks 5-10):** Integration — Extract modules from app.rs, rewrite startup, wire queue. This is the core refactor.
**Phase 3 (Tasks 11-13):** Enhancement — Lazy rendering, conversation history tool, final cleanup.

---

## File Map

### New Files
| File | Responsibility |
|------|---------------|
| `aios-core/src/queue/mod.rs` | MessageQueue struct, QueuedMessage, QueueEvent, all query methods |
| `aios-core/src/queue/schema.rs` | SQLite CREATE TABLE statements, migrations |
| `aios-core/src/channel/traits.rs` | Channel trait, ChannelBase struct |
| `aios-tools/src/builtin/conversation_history.rs` | History search/browse tool for AI |
| `aios-gtk/src/setup.rs` | First-boot + autoconfig logic (queue-based) |
| `aios-gtk/src/voice_listener.rs` | Extracted voice listener state machine |
| `aios-gtk/src/tts.rs` | Extracted TTS functions |
| `aios-gtk/src/command_handler.rs` | Extracted slash command dispatch |
| `aios-gtk/src/message_loop.rs` | Unified message loop (queue → LLM → queue) |
| `aios-gtk/src/ui/desktop_channel.rs` | Channel trait implementation for GTK Desktop |

### Modified Files
| File | What Changes |
|------|-------------|
| `aios-app-rs/Cargo.toml` | Add `rusqlite` to workspace deps |
| `aios-core/Cargo.toml` | Add `rusqlite` dependency |
| `aios-core/src/lib.rs` | Add `pub mod queue;` |
| `aios-core/src/channel/mod.rs` | Add `pub mod traits;` and re-exports |
| `aios-core/src/channel/types.rs` | Add `System` variant to `ChannelKind` |
| `aios-core/src/types/message.rs` | Add `From<&QueuedMessage> for Message` |
| `aios-tools/src/registry.rs` | Register conversation_history tool in `load_builtins()` |
| `aios-gtk/src/app.rs` | Rewrite from 3,350 to ~300 lines |
| `aios-gtk/src/ui/chat_view.rs` | New API: render_messages, append_message, prepend_messages, clear_display |
| `aios-gtk/src/ui/mod.rs` | Add `pub mod desktop_channel;` |
| `aios-gtk/src/lib.rs` or `main.rs` | Add new module declarations |

### Removed/Deprecated
| File | Reason |
|------|--------|
| `aios-gtk/src/ui/first_boot.rs` | ~2,300 lines — setup is queue messages now |

---

## Phase 1: Foundation

### Task 1: Add rusqlite Dependency

**Files:**
- Modify: `aios-app-rs/Cargo.toml:19-47` (workspace deps)
- Modify: `aios-core/Cargo.toml:7-25` (crate deps)

- [ ] **Step 1: Add rusqlite to workspace**

In `aios-app-rs/Cargo.toml`, add to `[workspace.dependencies]`:
```toml
rusqlite = { version = "0.32", features = ["bundled", "vtab"] }
```

The `bundled` feature compiles SQLite from source (no system dependency needed). The `vtab` feature enables FTS5 virtual tables.

- [ ] **Step 2: Add rusqlite to aios-core**

In `aios-core/Cargo.toml`, add:
```toml
rusqlite = { workspace = true }
```

- [ ] **Step 3: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-core`

- [ ] **Step 4: Commit**

```bash
git add aios-app-rs/Cargo.toml aios-core/Cargo.toml
git commit -m "feat(core): add rusqlite dependency for message queue"
```

---

### Task 2: MessageQueue — Schema and Core Types

**Files:**
- Create: `aios-core/src/queue/schema.rs`
- Create: `aios-core/src/queue/mod.rs`
- Modify: `aios-core/src/lib.rs`
- Modify: `aios-core/src/channel/types.rs`

- [ ] **Step 1: Add `System` variant to `ChannelKind`**

Read `aios-core/src/channel/types.rs`. Find the `ChannelKind` enum. Add a `System` variant for boot/setup messages. Update any `Display`, `FromStr`, `Serialize`, `Deserialize` impls to handle it.

- [ ] **Step 2: Create `aios-core/src/queue/schema.rs`**

Contains the `SCHEMA_V1` constant with all CREATE TABLE statements (messages, clear_markers, messages_fts, schema_version) and a function `ensure_schema(conn: &Connection) -> Result<()>` that creates tables if they don't exist, checks schema version, and applies migrations.

SQLite must be opened in WAL mode: `conn.pragma_update(None, "journal_mode", "WAL")?;`

- [ ] **Step 3: Create `aios-core/src/queue/mod.rs`**

Define:
- `QueuedMessage` struct (id, timestamp, channel: ChannelKind, role: Role, source, content, level: Option<MessageLevel>, tool_calls, tool_call_id, metadata)
- `QueueEvent` enum (NewMessage(i64), ClearMarker(String))
- `MessageQueue` struct with `rusqlite::Connection` and `Vec<tokio::sync::mpsc::UnboundedSender<QueueEvent>>`
- Implement all methods from the spec: `open`, `push`, `get_latest`, `get_before`, `get_after_clear_marker`, `get_llm_context`, `clear`, `clear_all`, `subscribe`, `search`, `export_json`
- `push` must: insert into messages, insert into messages_fts, notify all subscribers AFTER releasing any locks
- `get_llm_context` must: filter role in (User, Assistant, Tool), exclude System, respect clear_all markers, include messages from all channels
- Helper: `fn row_to_message(row: &rusqlite::Row) -> QueuedMessage` for DRY row mapping

- [ ] **Step 4: Add `From<&QueuedMessage> for Message` conversion**

In `aios-core/src/types/message.rs`, add the conversion that the spec defines. Parse tool_calls JSON into `Vec<ToolCall>`, map role and content.

- [ ] **Step 5: Add module declarations**

In `aios-core/src/lib.rs`, add `pub mod queue;`

- [ ] **Step 6: Write unit tests**

In `aios-core/src/queue/mod.rs` (or a separate test file):

```rust
#[cfg(test)]
mod tests {
    // Test: open creates database and tables
    // Test: push returns incrementing IDs
    // Test: get_latest returns correct order (newest first? oldest first? — spec says get_latest(n) returns last N, presumably in chronological order for display)
    // Test: get_before returns messages before given ID
    // Test: clear inserts marker, get_after_clear_marker respects it
    // Test: clear_all marker, get_llm_context respects it
    // Test: per-channel clear doesn't affect other channels
    // Test: get_llm_context filters system messages
    // Test: get_llm_context includes tool messages
    // Test: search via FTS5
    // Test: subscribe receives events on push
    // Test: export_json writes valid JSON
    // Test: QueuedMessage to Message conversion
}
```

At minimum 12 tests covering the core behaviors.

- [ ] **Step 7: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --package aios-core -- queue`

- [ ] **Step 8: Commit**

```bash
git add aios-core/src/queue/ aios-core/src/lib.rs aios-core/src/channel/types.rs aios-core/src/types/message.rs
git commit -m "feat(core): add MessageQueue — SQLite persistent message store with FTS5"
```

---

### Task 3: Channel Trait

**Files:**
- Create: `aios-core/src/channel/traits.rs`
- Modify: `aios-core/src/channel/mod.rs`

- [ ] **Step 1: Create `aios-core/src/channel/traits.rs`**

Define:
- `ChannelBase` struct with `kind: ChannelKind`, `queue_rx: tokio::sync::mpsc::UnboundedReceiver<QueueEvent>`, `last_rendered_id: Option<i64>`
- `Channel` trait (no `Send` bound) with methods: `kind()`, `capabilities()`, `render_message()`, `render_batch()`, `on_input()`, `is_active()`
- Include the doc comments from the spec about GTK thread safety

- [ ] **Step 2: Update channel/mod.rs**

Add `pub mod traits;` and re-export the types.

- [ ] **Step 3: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-core`

- [ ] **Step 4: Commit**

```bash
git add aios-core/src/channel/traits.rs aios-core/src/channel/mod.rs
git commit -m "feat(core): add Channel trait and ChannelBase for unified channel architecture"
```

---

### Task 4: Conversation History Tool

**Files:**
- Create: `aios-tools/src/builtin/conversation_history.rs`
- Modify: `aios-tools/src/builtin/mod.rs`
- Modify: `aios-tools/src/registry.rs`

- [ ] **Step 1: Create the tool**

Implement `ConversationHistoryTool` that:
- Holds `Arc<tokio::sync::RwLock<MessageQueue>>`
- Implements the `Tool` trait
- Supports actions: `search`, `recent`, `on_date`, `between`, `from_channel`, `stats`
- Constructor: `new(queue: Arc<RwLock<MessageQueue>>)`

Read the existing tool implementations (e.g., `memory.rs`, `files.rs`) to follow the pattern for argument parsing, error handling, and return format.

- [ ] **Step 2: Register in builtin/mod.rs**

Add `pub mod conversation_history;` to the module declarations.

- [ ] **Step 3: Add registration in registry.rs**

In the `load_builtins()` function, the tool needs a `MessageQueue` reference. This is a design change — `load_builtins` currently takes no arguments. Two options:
1. Add a separate `register_queue_tools(&mut self, queue: Arc<RwLock<MessageQueue>>)` method
2. Pass queue as an optional parameter to `load_builtins`

Choose option 1 — it's additive and doesn't break the existing API. Call it from app.rs after the queue is created.

- [ ] **Step 4: Write tests**

Test each action with a test database.

- [ ] **Step 5: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --package aios-tools -- conversation_history`

- [ ] **Step 6: Commit**

```bash
git add aios-tools/src/builtin/conversation_history.rs aios-tools/src/builtin/mod.rs aios-tools/src/registry.rs
git commit -m "feat(tools): add conversation_history tool — search and browse persistent history"
```

---

## Phase 2: Integration (The Core Refactor)

### Task 5: Extract TTS Functions from app.rs

**Files:**
- Create: `aios-gtk/src/tts.rs`
- Modify: `aios-gtk/src/app.rs`

- [ ] **Step 1: Create `aios-gtk/src/tts.rs`**

Move these functions from app.rs into tts.rs:
- `speak_if_enabled()`
- `speak_if_enabled_with_signal()`
- `do_tts_with_signal()`
- `do_tts()`
- `stop_tts()`
- `prepare_tts_text_short()`
- `summarize_for_tts()`
- Any helper constants/functions they depend on

Make them `pub(crate)` so app.rs can still call them.

- [ ] **Step 2: Update app.rs imports**

Replace the moved functions with `use crate::tts::*;` or specific imports.

- [ ] **Step 3: Add module declaration**

In `aios-gtk/src/main.rs` (or lib.rs), add `mod tts;`

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-gtk`

- [ ] **Step 5: Commit**

```bash
git add aios-gtk/src/tts.rs aios-gtk/src/app.rs aios-gtk/src/main.rs
git commit -m "refactor(gtk): extract TTS functions from app.rs into tts.rs"
```

---

### Task 6: Extract Voice Listener from app.rs

**Files:**
- Create: `aios-gtk/src/voice_listener.rs`
- Modify: `aios-gtk/src/app.rs`

- [ ] **Step 1: Create `aios-gtk/src/voice_listener.rs`**

Move from app.rs:
- `ListenerState` enum
- `start_voice_listener()` function
- `transcribe_with_whisper()` function
- `strip_wake_phrase()` function
- All constants (RING_BUFFER_CAPACITY, POST_WAKE_DELAY_SAMPLES, etc.)

- [ ] **Step 2: Update app.rs**

Replace with `use crate::voice_listener::start_voice_listener;`

- [ ] **Step 3: Add module declaration and verify**

- [ ] **Step 4: Commit**

```bash
git add aios-gtk/src/voice_listener.rs aios-gtk/src/app.rs aios-gtk/src/main.rs
git commit -m "refactor(gtk): extract voice listener from app.rs into voice_listener.rs"
```

---

### Task 7: Extract Command Handler from app.rs

**Files:**
- Create: `aios-gtk/src/command_handler.rs`
- Modify: `aios-gtk/src/app.rs`

- [ ] **Step 1: Create `aios-gtk/src/command_handler.rs`**

Move from app.rs:
- `handle_command()` function (or the equivalent command dispatch logic)
- `BackgroundTask` handling
- Any command-specific helpers

The command handler needs access to: state (config, llm, tools), chat_view, and the message queue. Pass these as parameters.

- [ ] **Step 2: Update app.rs**

- [ ] **Step 3: Verify and commit**

```bash
git add aios-gtk/src/command_handler.rs aios-gtk/src/app.rs aios-gtk/src/main.rs
git commit -m "refactor(gtk): extract command handler from app.rs"
```

---

### Task 8: Create DesktopChannel

**Files:**
- Create: `aios-gtk/src/ui/desktop_channel.rs`
- Modify: `aios-gtk/src/ui/mod.rs`
- Modify: `aios-gtk/src/ui/chat_view.rs`

- [ ] **Step 1: Update ChatView API**

Read `aios-gtk/src/ui/chat_view.rs`. Replace the public API:
- Keep the internal rendering logic (bubble creation, code blocks, role labels)
- Remove `add_message(role, content)` as the primary external API
- Add new methods that accept `QueuedMessage`:
  - `render_messages(&self, msgs: &[QueuedMessage])` — render a batch
  - `append_message(&self, msg: &QueuedMessage)` — append one at the bottom
  - `prepend_messages(&self, msgs: &[QueuedMessage])` — prepend for scroll-back
  - `clear_display(&self)` — remove all widgets
- Internally, these call the same bubble/widget creation code but read from `QueuedMessage` fields
- The existing `add_message` can be kept as a private helper or deprecated

- [ ] **Step 2: Create `desktop_channel.rs`**

Implement the `Channel` trait for Desktop:
- Holds: `ChatView`, reference to `MessageQueue`, `ChannelBase`
- `render_message()`: calls `chat_view.append_message()`
- `render_batch()`: calls `chat_view.render_messages()`
- `is_active()`: always true for Desktop (it's always the primary display)
- `on_input()`: returns the input text as-is
- Handles the thinking placeholder as local state
- TTS trigger: calls `speak_if_enabled_with_signal` from `crate::tts` when rendering an assistant message

- [ ] **Step 3: Update ui/mod.rs**

Add `pub mod desktop_channel;`

- [ ] **Step 4: Verify compilation**

- [ ] **Step 5: Commit**

```bash
git add aios-gtk/src/ui/desktop_channel.rs aios-gtk/src/ui/chat_view.rs aios-gtk/src/ui/mod.rs
git commit -m "feat(gtk): add DesktopChannel — Channel trait impl for GTK desktop"
```

---

### Task 9: Create Setup Module (Queue-Based)

**Files:**
- Create: `aios-gtk/src/setup.rs`

- [ ] **Step 1: Create `aios-gtk/src/setup.rs`**

This replaces `first_boot.rs` and the autoconfig path. Contains:

**Setup state machine:**
```rust
enum SetupStep {
    Welcome,
    ProviderSelect,
    ApiKeyEntry { provider: String },
    MasterPassword,
    AssistantName,
    WakeWord,
    Complete,
}
```

**Functions:**
- `pub fn run_first_boot(queue: &MessageQueue, config: &mut ConfigManager)` — pushes setup messages to queue, returns a state machine that processes user responses
- `pub fn run_autoconfig(queue: &MessageQueue, config: &mut ConfigManager, auto: AutoConfig)` — applies autoconfig and pushes status messages to queue
- `pub fn handle_setup_input(step: &mut SetupStep, queue: &MessageQueue, config: &mut ConfigManager, vault: &mut Vault, input: &str) -> bool` — processes user input during setup, advances state, returns true when setup is complete

**Interactive cards via metadata:**
Each setup step pushes a message with metadata describing the card type:
```rust
queue.push(QueuedMessage {
    role: Role::Assistant,
    source: Some("system".into()),
    content: Some("Choose your AI provider:".into()),
    metadata: Some(serde_json::json!({
        "card_type": "provider_select",
        "options": ["Claude", "OpenAI"]
    }).to_string()),
    ..Default::default()
});
```

The DesktopChannel's `render_message` checks the metadata and renders the appropriate interactive widget.

- [ ] **Step 2: Add module declaration**

- [ ] **Step 3: Commit**

```bash
git add aios-gtk/src/setup.rs aios-gtk/src/main.rs
git commit -m "feat(gtk): add queue-based setup module — replaces first_boot.rs"
```

---

### Task 10: Rewrite app.rs — Unified Startup

**Files:**
- Modify: `aios-gtk/src/app.rs` (major rewrite)
- Create: `aios-gtk/src/message_loop.rs`

This is the big task. The current 3,350-line app.rs is gutted to ~300 lines.

- [ ] **Step 1: Create `aios-gtk/src/message_loop.rs`**

The unified message loop:
- Receives user messages from the DesktopChannel (prompt input, voice STT)
- Receives messages from WebChannel (WebSocket) and SignalChannel (signal-cli)
- For each user message:
  1. Push to queue
  2. Check if setup is in progress → route to `setup::handle_setup_input`
  3. Check if slash command → route to `command_handler`
  4. Otherwise → call LLM with context from `queue.get_llm_context(50)`
  5. Push LLM response to queue
  6. Handle tool calls (loop)
- DesktopChannel shows thinking placeholder during LLM call

- [ ] **Step 2: Rewrite app.rs**

The new `activate()` function — ONE PATH:

```rust
pub fn activate(app: &adw::Application, rt: Handle) {
    // 1. Theme
    Self::apply_theme(&config.get_str("ui.theme", "dark"));

    // 2. Open queue
    let queue = MessageQueue::open(&config_dir().join("messages.db")).unwrap();
    let queue = Arc::new(RwLock::new(queue));

    // 3. Config
    let config = ConfigManager::new().unwrap_or_default();

    // 4. i18n
    aios_core::i18n::init();

    // 5. Subsystems
    let mut tools = ToolRegistry::new();
    tools.load_builtins();
    tools.register_queue_tools(queue.clone());
    let llm = LlmManager::new();
    Self::init_llm(&config, &mut llm);

    // 6. UI
    let chat_view = ChatView::new();
    let prompt_input = PromptInput::new();
    let desktop = DesktopChannel::new(queue.clone(), chat_view.clone());
    let window = build_main_window(app, &chat_view, &prompt_input, ...);

    // 7. Load history
    let messages = queue.read().get_after_clear_marker("desktop", 50);
    desktop.render_batch(&messages);

    // 8. Voice
    start_voice_listener(queue.clone(), ...);

    // 9. Vault check
    if !vault_exists() {
        if let Some(auto) = load_autoconfig() {
            setup::run_autoconfig(&queue, &config, auto);
        } else {
            setup::run_first_boot(&queue, &config);
        }
    } else {
        queue.push(system_message("Ready. How can I help?"));
    }

    // 10. Start the unified message loop
    message_loop::start(queue, llm, tools, desktop, ...);
}
```

Delete:
- `run_first_boot_setup()` (~365 lines)
- `apply_autoconfig()` (~141 lines)
- `transition_to_normal_mode()` (~245 lines)
- `activate_main()` (~333 lines)
- All duplicated signal wiring, config loading, i18n init, boot status construction

- [ ] **Step 3: Update `/clear` command**

In `command_handler.rs`, change `/clear` to push a clear marker to the queue instead of clearing the chat view directly. Add `/clear -all` support.

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-gtk`

This is the hardest step — many things may break. Work through compilation errors one by one. The extracted modules (tts, voice_listener, command_handler) should mostly work. The main challenge is wiring the queue into all the places that previously called `chat_view.add_message()` directly.

- [ ] **Step 5: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --workspace`

- [ ] **Step 6: Commit**

```bash
git add aios-gtk/src/app.rs aios-gtk/src/message_loop.rs aios-gtk/src/command_handler.rs
git commit -m "feat(gtk): unified startup — one path, queue-driven, app.rs ~300 lines"
```

---

## Phase 3: Enhancement

### Task 11: Lazy Rendering + Scroll-Back

**Files:**
- Modify: `aios-gtk/src/ui/chat_view.rs`
- Modify: `aios-gtk/src/ui/desktop_channel.rs`

- [ ] **Step 1: Add widget window management to ChatView**

Track the rendered message IDs (`oldest_rendered_id`, `newest_rendered_id`, widget count). When `append_message` is called and widget count > 50, remove the oldest widget from the top of the container.

- [ ] **Step 2: Add scroll-back trigger**

Connect to the ScrolledWindow's vadjustment. When the user scrolls near the top (value < 100px from upper bound), trigger a load of older messages: `queue.get_before(oldest_rendered_id, 20)` → `chat_view.prepend_messages()`.

- [ ] **Step 3: Test scroll-back**

Manually: push 100+ messages to the queue, open the app, verify only ~50 are rendered, scroll up to trigger loading of older ones.

- [ ] **Step 4: Commit**

```bash
git add aios-gtk/src/ui/chat_view.rs aios-gtk/src/ui/desktop_channel.rs
git commit -m "feat(ui): lazy rendering — 50 message window with scroll-back"
```

---

### Task 12: Remove first_boot.rs

**Files:**
- Remove: `aios-gtk/src/ui/first_boot.rs`
- Modify: `aios-gtk/src/ui/mod.rs`

- [ ] **Step 1: Verify first_boot.rs is no longer imported anywhere**

```bash
grep -r "first_boot\|SetupConversation\|SetupResult" aios-gtk/src/ --include="*.rs"
```

Remove any remaining references.

- [ ] **Step 2: Remove the file and module declaration**

- [ ] **Step 3: Verify compilation and tests**

- [ ] **Step 4: Commit**

```bash
git rm aios-gtk/src/ui/first_boot.rs
git add aios-gtk/src/ui/mod.rs
git commit -m "refactor(gtk): remove first_boot.rs — setup is now queue messages"
```

---

### Task 13: Final Integration Test

- [ ] **Step 1: Run full workspace tests**

```bash
cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --workspace
```

All tests must pass.

- [ ] **Step 2: Manual verification checklist**

Build and boot: `./start.sh --clean`

Verify:
- [ ] Boot status appears in chat (from queue)
- [ ] First-boot setup works as a conversation (if no vault)
- [ ] Autoconfig works and shows messages (if autoconfig.json present)
- [ ] Normal startup shows "Ready" message and loads history
- [ ] `/clear` clears the current channel only
- [ ] `/clear -all` clears everything
- [ ] Messages persist across reboots (reboot VM, messages still there)
- [ ] VU meter visible and working
- [ ] Voice input works
- [ ] Web channel receives messages
- [ ] `/wake list` still works
- [ ] Settings dialog still works
- [ ] LLM responds correctly (context from queue)

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: message queue architecture — unified startup, persistent history, channel trait"
```
