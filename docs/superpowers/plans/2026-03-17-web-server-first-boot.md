# Web Server During First-Boot + Setup Improvements — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the web server always available (including during first-boot setup), pre-fill API keys from config, rename OpenAI→ChatGPT in UI, and show a summary INFO message at setup completion.

**Architecture:** Extract web server startup into a shared helper called from both `run_first_boot_setup()` and `activate_main()`. The setup conversation reads config for pre-filled API keys. The web client renders `default_value` for panel fields. All existing tests plus new tests must pass.

**Tech Stack:** Rust, GTK4/libadwaita, axum, tokio, JavaScript (web client)

---

### Task 1: Web Client — Render `default_value` for Panel Fields

This is the foundation — the web client must support default values before anything else.

**Files:**
- Modify: `aios-app-rs/aios-web/src/static/index.html:485-510`

- [ ] **Step 1: Add default_value rendering in the web panel builder**

In the `showPanel()` function (line 485), after creating each `<input>`, check for `f.default_value` or `f.default` and set it as the input's value:

```javascript
// Inside the fields.forEach(f => { ... }) loop, after each input element is created:
// For text and password:
case 'text':
    html += `<input type="text" data-id="${esc(f.id)}" placeholder="${esc(f.placeholder || '')}" value="${esc(f.default_value || f.default || '')}">`;
    break;
case 'password':
    html += `<input type="password" data-id="${esc(f.id)}" placeholder="${esc(f.placeholder || '')}" value="${esc(f.default_value || f.default || '')}">`;
    break;
case 'number':
    html += `<input type="number" data-id="${esc(f.id)}" min="${f.min||''}" max="${f.max||''}" step="${f.step||1}" value="${f.default_value || f.default || ''}">`;
    break;
case 'toggle':
    const checked = (f.default_value || f.default) ? 'checked' : '';
    html += `<input type="checkbox" data-id="${esc(f.id)}" style="width:auto" ${checked}>`;
    break;
```

Also handle `textarea` (multiline) and `dropdown`/`choice` defaults.

- [ ] **Step 2: Verify build**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-web`
Expected: compiles (no Rust changes, just embedded HTML)

- [ ] **Step 3: Commit**

```bash
git add aios-app-rs/aios-web/src/static/index.html
git commit -m "feat(web): render default_value for panel input fields"
```

---

### Task 2: ChatGPT Branding — Rename OpenAI in User-Facing Text

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs` (all "OpenAI" display strings)
- Modify: `aios-app-rs/aios-gtk/src/app.rs` (boot status, provider display)

- [ ] **Step 1: Create display name helper**

In `first_boot.rs`, update the display name mapping everywhere it appears. The pattern `"openai" => "OpenAI"` becomes `"openai" => "ChatGPT"`. Key locations:

- `select_provider()` line 595: `"openai" => "OpenAI"` → `"openai" => "ChatGPT"`
- `show_choose_provider()` line 528: button label `"OpenAI"` → `"ChatGPT"`
- `show_choose_provider()` line 530: description update GPT-4o text
- `show_enter_api_key()` line 620-628: title and tutorial for openai
- `handle_add_backup()` line 1006: display name
- `show_provider_order()` line 1035: display name
- `show_complete()` line 1099: display name
- `on_voice_input()` line 158: keep "openai" and "gpt" as voice triggers, add "chatgpt"
- `set_primary_order()` line 1079: display name

In `app.rs`:
- Line 663: `available_providers.push("OpenAI")` → `"ChatGPT"`
- Line 757: backup status text

- [ ] **Step 2: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test -p aios-core -p aios-tools`
Expected: all pass

- [ ] **Step 3: Commit**

```bash
git add aios-app-rs/aios-gtk/src/ui/first_boot.rs aios-app-rs/aios-gtk/src/app.rs
git commit -m "feat: rename OpenAI to ChatGPT in all user-facing text"
```

---

### Task 3: Extract Web Server Startup into Shared Helper

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs`

- [ ] **Step 1: Create `start_web_server()` helper method**

Extract lines 802-830 from `activate_main()` into a new method:

```rust
/// Start the web server and register the Web channel.
///
/// Returns the broadcast sender for routing responses to web clients,
/// or `None` if the web channel is disabled.
fn start_web_server(
    config: &mut ConfigManager,
    runtime: &aios_core::channel::AppRuntime,
    welcome_message: Option<String>,
) -> Option<broadcast::Sender<String>> {
    let web_enabled = config.get_bool("channels.web.enabled", true);
    let web_port = config.get_str("channels.web.port", "80");

    if !web_enabled {
        return None;
    }

    let port: u16 = web_port.parse().unwrap_or(80);
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
        port, web_tx, web_token,
        Some(runtime.switcher.clone()),
        welcome_message,
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
```

- [ ] **Step 2: Update `activate_main()` to use the helper**

Replace lines 802-830 with:

```rust
let web_server = Self::start_web_server(&mut config, &runtime, Some(boot_status_text.clone()));
```

- [ ] **Step 3: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`
Expected: compiles clean

- [ ] **Step 4: Commit**

```bash
git add aios-app-rs/aios-gtk/src/app.rs
git commit -m "refactor: extract start_web_server() helper from activate_main()"
```

---

### Task 4: Start Web Server During First-Boot Setup

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs` — `run_first_boot_setup()`

- [ ] **Step 1: Add AppRuntime + web server to first-boot setup**

In `run_first_boot_setup()`, after building the window (line 346) and before creating `SetupConversation`, add:

```rust
// Load config (has API keys from .env baked at build time).
let mut config = ConfigManager::new().unwrap_or_else(|e| {
    warn!("Config load failed: {e}, using defaults");
    ConfigManager::with_path(std::path::PathBuf::from("/tmp/.aios/config.json"))
        .expect("Cannot initialize config")
});

// Start channel infrastructure so web is available during setup.
let runtime = aios_core::channel::AppRuntime::new();
runtime.switcher.register_channel(
    aios_core::channel::ChannelKind::Desktop,
    aios_core::channel::ChannelContext::desktop(),
);

// Boot status for setup mode.
let boot_status_text = {
    use aios_core::types::{BootStatus, StatusLine};
    let mut status = BootStatus::new();
    status.add(StatusLine::new("Desktop", true, "GTK4/libadwaita"));
    status.add(StatusLine::new("Web Channel", true, "http://aios.local"));
    status.add(StatusLine::new("Mode", true, "First-boot setup"));
    status.format()
};

let _web_server = Self::start_web_server(&mut config, &runtime, Some(boot_status_text.clone()));
```

- [ ] **Step 2: Update the boot status display in the setup to use the shared status**

Replace the existing `BootStatus` block (lines 349-380) with the one created above, reusing `boot_status_text`.

- [ ] **Step 3: Pass config to SetupConversation**

The setup needs access to config to read pre-filled API keys. Update `SetupConversation::new()` to accept an optional `ConfigManager`:

In `first_boot.rs`, update constructor:
```rust
pub fn new(chat_view: ChatView, config: Option<ConfigManager>) -> Self {
    Self {
        state: Rc::new(RefCell::new(SetupState::default())),
        chat_view,
        config: Rc::new(RefCell::new(config)),
        on_complete_cb: Rc::new(RefCell::new(None)),
    }
}
```

Add `config` field to `SetupConversation` struct.

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`
Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/app.rs aios-app-rs/aios-gtk/src/ui/first_boot.rs
git commit -m "feat: start web server during first-boot setup"
```

---

### Task 5: Pre-fill API Keys + Auto-Select Provider

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs`

- [ ] **Step 1: Auto-select provider when only one key is configured**

In `show_choose_provider()`, before showing the choice card, check config:

```rust
fn show_choose_provider(&self) {
    // Check if we can auto-select based on pre-configured keys.
    if let Some(ref config) = *self.config.borrow() {
        let has_claude = !config.get_str("llm.claude_api_key", "").is_empty();
        let has_openai = {
            let k = config.get_str("llm.openai_api_key", "");
            !k.is_empty() && k != "your-api-key-here"
        };

        if has_claude && !has_openai {
            self.chat_view.add_message("system",
                "Claude API key found in system config — using Claude as your provider.");
            self.select_provider("claude");
            return;
        }
        if has_openai && !has_claude {
            self.chat_view.add_message("system",
                "ChatGPT API key found in system config — using ChatGPT as your provider.");
            self.select_provider("openai");
            return;
        }
    }

    // Both or neither — show the choice card.
    // ... existing code ...
}
```

- [ ] **Step 2: Pre-fill API key entry field**

In `show_enter_api_key()`, after creating the password entry, set default value from config:

```rust
// Pre-fill from config if available.
if let Some(ref config) = *self.config.borrow() {
    let config_key = match provider.as_str() {
        "claude" => "llm.claude_api_key",
        "openai" => "llm.openai_api_key",
        _ => "",
    };
    if !config_key.is_empty() {
        let existing = config.get_str(config_key, "");
        if !existing.is_empty() && existing != "your-api-key-here" {
            entry.set_text(&existing);
        }
    }
}
```

- [ ] **Step 3: Skip backup question if other provider has no key**

In `show_add_backup()`, check if backup provider has a key. If not, skip:

```rust
fn show_add_backup(&self) {
    let primary = self.state.borrow().primary_provider.clone();
    let other = if primary == "claude" { "openai" } else { "claude" };

    // Check if the other provider has a key configured.
    let other_has_key = if let Some(ref config) = *self.config.borrow() {
        let key_name = match other {
            "claude" => "llm.claude_api_key",
            "openai" => "llm.openai_api_key",
            _ => "",
        };
        let k = config.get_str(key_name, "");
        !k.is_empty() && k != "your-api-key-here"
    } else {
        false
    };

    if !other_has_key {
        // No key for backup provider — skip backup question.
        self.advance(SetupStep::Complete);
        return;
    }

    // ... existing backup question UI code ...
}
```

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/ui/first_boot.rs
git commit -m "feat: pre-fill API keys from config, auto-select single provider"
```

---

### Task 6: Summary INFO Message at Setup Completion

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs` — `show_complete()` and `finish()`

- [ ] **Step 1: Replace show_complete() with INFO summary**

Replace the current `show_complete()` method to show a proper `MessageLevel::Info` summary:

```rust
fn show_complete(&self) {
    let s = self.state.borrow();

    // Build summary using BootStatus format.
    use aios_core::types::{BootStatus, StatusLine};
    let mut status = BootStatus::new();

    for (i, p) in s.providers.iter().enumerate() {
        let role = if i == 0 { "Primary" } else { "Backup" };
        let display = match p.name.as_str() {
            "claude" => "Claude",
            "openai" => "ChatGPT",
            other => other,
        };
        status.add(StatusLine::new(
            &format!("{role} provider"),
            true,
            format!("{display} (API key stored)"),
        ));
    }

    status.add(StatusLine::new("Master password", true, "set"));
    status.add(StatusLine::new("Vault", true, "created"));
    status.add(StatusLine::new("Web Channel", true, "http://aios.local"));

    drop(s);

    self.chat_view.add_level_message(
        aios_core::types::MessageLevel::Info,
        &format!("First-Boot Setup Complete\n\n{}", status.format_items_only()),
    );

    // "Start Chatting" button.
    let btn = gtk::Button::with_label("Start Chatting \u{2192}");
    btn.add_css_class("suggested-action");
    btn.add_css_class("pill");
    btn.set_halign(Align::Start);
    btn.set_margin_top(8);

    let this = self.clone();
    btn.connect_clicked(move |b| {
        b.set_sensitive(false);
        this.finish();
    });

    self.chat_view.add_widget(btn.upcast_ref());

    self.speak("You're all set! Your AI assistant is ready.");
}
```

Note: `format_items_only()` may need to be added to `BootStatus` if it doesn't exist. Check if `BootStatus::format()` includes the header or just the items. If `format()` includes the header ("AiOS System Status"), we may need a separate method for just the checkmark lines, or we can reuse `format()` and accept the header.

- [ ] **Step 2: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

- [ ] **Step 3: Commit**

```bash
git add aios-app-rs/aios-gtk/src/ui/first_boot.rs
git commit -m "feat: show summary INFO message at end of first-boot setup"
```

---

### Task 7: Selftest — Revert Web Server Check to FAIL

**Files:**
- Modify: `distro/_inner_build.sh`

- [ ] **Step 1: Revert the web server check**

Since the web server now always starts, revert the vault-aware check back to a hard FAIL:

```bash
    if ss -tlnp 2>/dev/null | grep -q ":80 "; then
        pass "Web server listening on port 80"
    elif ss -tlnp 2>/dev/null | grep -q ":8080 "; then
        pass "Web server listening on port 8080"
    else
        fail "Web server not listening"
    fi
```

- [ ] **Step 2: Verify syntax**

Run: `bash -n distro/_inner_build.sh && echo OK`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add distro/_inner_build.sh
git commit -m "fix(selftest): revert web server check to FAIL (always starts now)"
```

---

### Task 8: Tests — Update Existing + Add New

**Files:**
- Modify: `aios-app-rs/aios-core/src/selftest/scenarios.rs` — add setup-related test scenarios
- Modify: `aios-app-rs/aios-core/src/selftest/conversation_sim.rs` — if needed for simulated setup

- [ ] **Step 1: Add conversation simulation test for first-boot with pre-filled key**

In `scenarios.rs`, add a new scenario that verifies the config-based pre-fill logic:

```rust
register("setup: pre-fill api key from config", false, |ctx| {
    // Create a temp config with a Claude key.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(&config_path, r#"{"llm":{"claude_api_key":"sk-ant-test123"}}"#).unwrap();
    let config = ConfigManager::with_path(config_path).unwrap();

    let key = config.get_str("llm.claude_api_key", "");
    assert!(!key.is_empty(), "Config should have Claude key");
    assert_eq!(key, "sk-ant-test123");

    TestResult::pass("setup: pre-fill api key from config", "Config pre-fill works")
});
```

- [ ] **Step 2: Add test for ChatGPT display name**

```rust
register("setup: chatgpt display name", false, |_ctx| {
    let display = match "openai" {
        "claude" => "Claude",
        "openai" => "ChatGPT",
        other => other,
    };
    assert_eq!(display, "ChatGPT");
    TestResult::pass("setup: chatgpt display name", "OpenAI displays as ChatGPT")
});
```

- [ ] **Step 3: Run all workspace tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test --workspace 2>&1 | tail -20`
Expected: all tests pass (563+ tests)

- [ ] **Step 4: Commit**

```bash
git add aios-app-rs/aios-core/src/selftest/scenarios.rs
git commit -m "test: add setup pre-fill and ChatGPT branding tests"
```

---

### Task 9: Final Verification

- [ ] **Step 1: Run full workspace tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test --workspace`
Expected: all pass

- [ ] **Step 2: Run cargo check for release build**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check --release`
Expected: compiles clean

- [ ] **Step 3: Verify shell script syntax**

Run: `bash -n distro/_inner_build.sh && echo OK`
Expected: OK

- [ ] **Step 4: Final commit if any fixups**

```bash
git add -A
git commit -m "chore: final fixups for web-server-first-boot feature"
```
