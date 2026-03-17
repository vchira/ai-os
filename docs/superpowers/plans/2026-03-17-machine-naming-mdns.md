# Machine Naming + Assistant Identity Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each AiOS installation a configurable name (default "Assistant") used as the assistant display name, wake word, and network hostname. Add collision detection at boot. Update chat labels to show the assistant name.

**Architecture:** Three config keys (`assistant.name`, `voice.wake_word`, `system.machine_name`) default to "Assistant" / "assistant". A systemd service checks for mDNS collisions at boot. The first-boot setup asks for the name with a "use same for all?" toggle. Chat view labels read from `assistant.name`.

**Tech Stack:** Rust, GTK4, JavaScript (web client), Avahi, systemd, bash

---

### Task 1: Update Config Defaults

**Files:**
- Modify: `aios-app-rs/aios-core/src/config/defaults.rs`

- [ ] **Step 1: Add `assistant.name` to defaults and update wake word**

In `DEFAULTS_JSON`, add an `assistant` section and change the wake word default:

```json
"assistant": {
    "name": "Assistant"
},
```

Change `"wake_word": "hey aios"` to `"wake_word": "Assistant"`.

- [ ] **Step 2: Add `system.machine_name` default**

In the `system` section of `DEFAULTS_JSON`, add:
```json
"machine_name": "assistant"
```

- [ ] **Step 3: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test -p aios-core`

Fix any tests that assert on the old `"hey aios"` wake word default (check `conversation_sim.rs` line 579).

- [ ] **Step 4: Commit**

---

### Task 2: Update Chat Labels — GTK

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/chat_view.rs`

- [ ] **Step 1: Make `role_display_name()` configurable**

The function at line 420 currently returns hardcoded `"AiOS"` for assistant. Change it to accept a config-provided name. Two approaches:

**Option A (simple):** Add a static/thread-local for the assistant name that's set once at startup:

```rust
use std::cell::RefCell;

thread_local! {
    static ASSISTANT_NAME: RefCell<String> = RefCell::new("Assistant".to_string());
}

/// Set the display name for the assistant role.
pub fn set_assistant_display_name(name: &str) {
    ASSISTANT_NAME.with(|n| *n.borrow_mut() = name.to_string());
}

fn role_display_name(role: &str) -> String {
    match role {
        "user" => "You".to_string(),
        "assistant" => ASSISTANT_NAME.with(|n| n.borrow().clone()),
        "system" => "System".to_string(),
        "tool" => "Tool".to_string(),
        other => other.to_string(),
    }
}
```

- [ ] **Step 2: Call `set_assistant_display_name()` at startup**

In `app.rs`, both `run_first_boot_setup()` and `activate_main()`, after loading config:

```rust
let assistant_name = config.get_str("assistant.name", "Assistant");
crate::ui::chat_view::set_assistant_display_name(&assistant_name);
```

- [ ] **Step 3: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

- [ ] **Step 4: Commit**

---

### Task 3: Update Chat Labels — Web Client

**Files:**
- Modify: `aios-app-rs/aios-web/src/static/index.html`

- [ ] **Step 1: Add role name mapping in JavaScript**

Currently line 440 sets `roleLabel.textContent = role;` (raw string). Change to use friendly names:

```javascript
let assistantName = 'Assistant'; // Updated from server via System message

function roleDisplayName(role) {
    switch(role) {
        case 'user': return 'You';
        case 'assistant': return assistantName;
        case 'tool': return 'Tool';
        default: return role;
    }
}
```

Update line 440 to: `roleLabel.textContent = roleDisplayName(role);`

- [ ] **Step 2: Accept assistant name from server**

When the web client receives a `System` message with type `config`, update the `assistantName` variable. Add handling in the message receiver:

```javascript
if (data.type === 'System' && data.content) {
    try {
        const cfg = JSON.parse(data.content);
        if (cfg.assistant_name) assistantName = cfg.assistant_name;
    } catch(e) {}
}
```

Alternatively, include the assistant name in the welcome message or as a new `ServerMessage` variant. The simplest approach: send it as part of the welcome/boot status.

- [ ] **Step 3: Verify build**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-web`

- [ ] **Step 4: Commit**

---

### Task 4: Build System — Default Hostname "assistant"

**Files:**
- Modify: `distro/_inner_build.sh`

- [ ] **Step 1: Change hostname from "aios" to "assistant"**

Line 201-203: Change:
```bash
echo "aios" > /etc/hostname
echo "127.0.0.1 aios" >> /etc/hosts
```
To:
```bash
echo "assistant" > /etc/hostname
echo "127.0.0.1 assistant" >> /etc/hosts
```

- [ ] **Step 2: Update Avahi service names**

Line 234: Change `<name>AiOS Web Interface</name>` to `<name>Assistant Web Interface</name>`
Line 285: Change `<name>AiOS SSH</name>` to `<name>Assistant SSH</name>`

- [ ] **Step 3: Update all references to "aios.local" hostname**

Search the build script for `aios.local` and update to use the configured hostname. Key locations:
- Avahi mDNS comment (line 227)
- Any hardcoded references

Note: The actual `.local` resolution comes from the hostname + Avahi, so changing `/etc/hostname` is sufficient.

- [ ] **Step 4: Verify syntax**

Run: `bash -n distro/_inner_build.sh && echo OK`

- [ ] **Step 5: Commit**

---

### Task 5: Boot Collision Check Service

**Files:**
- Modify: `distro/_inner_build.sh` — add systemd service

- [ ] **Step 1: Create collision check script**

Add to the chroot hook — a script at `/usr/bin/aios-hostname-check`:

```bash
#!/bin/bash
# Check if our hostname collides with another machine on the LAN.
HOSTNAME=$(cat /etc/hostname 2>/dev/null || echo "assistant")
# Try to resolve <hostname>.local — if it resolves to a different IP, there's a conflict.
OUR_IPS=$(hostname -I 2>/dev/null | tr ' ' '\n')
RESOLVED=$(avahi-resolve -n "${HOSTNAME}.local" -4 2>/dev/null | awk '{print $2}')

if [ -n "$RESOLVED" ]; then
    # Check if the resolved IP is one of ours
    for ip in $OUR_IPS; do
        [ "$ip" = "$RESOLVED" ] && exit 0  # It's us, no conflict
    done
    # Conflict detected — write flag file
    echo "$HOSTNAME" > /tmp/aios-name-conflict
    exit 1
fi
exit 0  # No conflict (name not found on network = we can use it)
```

- [ ] **Step 2: Create systemd service**

Add to the chroot hook:

```ini
[Unit]
Description=Check AiOS hostname for mDNS conflicts
After=avahi-daemon.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStartPre=/bin/sleep 3
ExecStart=/usr/bin/aios-hostname-check
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
```

Enable with `systemctl enable aios-hostname-check.service`.

- [ ] **Step 3: Verify syntax**

Run: `bash -n distro/_inner_build.sh && echo OK`

- [ ] **Step 4: Commit**

---

### Task 6: Rename Popup Card in App

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs`

- [ ] **Step 1: Check for collision flag at startup**

In both `run_first_boot_setup()` and `activate_main()`, after building the window, check for the conflict flag file:

```rust
// Check for hostname collision (set by aios-hostname-check.service)
if std::path::Path::new("/tmp/aios-name-conflict").exists() {
    if let Ok(conflicting) = std::fs::read_to_string("/tmp/aios-name-conflict") {
        let conflicting = conflicting.trim().to_string();
        let random_suffix = rand::random::<u16>() % 10000;
        let suggestion = format!("{conflicting}-{random_suffix}");
        // Show rename card (implementation in step 2)
        Self::show_rename_card(&chat_view, &conflicting, &suggestion);
        let _ = std::fs::remove_file("/tmp/aios-name-conflict");
    }
}
```

- [ ] **Step 2: Implement rename card**

Create a setup card with a text entry pre-filled with the suggested name:

```rust
fn show_rename_card(chat_view: &ChatView, conflicting: &str, suggestion: &str) {
    // Similar pattern to first_boot setup cards
    // Text entry with suggestion pre-filled
    // "Apply" button that updates:
    //   1. /etc/hostname (via sudo hostnamectl set-hostname)
    //   2. Avahi (sudo systemctl restart avahi-daemon)
    //   3. Config (system.machine_name)
}
```

Validate the name: lowercase alphanumeric + hyphens, 1-63 chars, no leading/trailing hyphens.

- [ ] **Step 3: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

- [ ] **Step 4: Commit**

---

### Task 7: First-Boot Setup — Name Step

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs`

- [ ] **Step 1: Add NameAssistant step to SetupStep enum**

```rust
enum SetupStep {
    Welcome,
    ScreenResolution,      // NEW (future)
    TestAudioOutput,
    TestAudioInput,
    NameAssistant,         // NEW
    ChooseProvider,
    EnterApiKey { provider: String },
    CreatePassword,
    ConfirmPassword,
    AddBackup,
    EnterBackupKey { provider: String },
    ProviderOrder,
    Complete,
}
```

- [ ] **Step 2: Add name fields to SetupState**

```rust
struct SetupState {
    // ... existing fields ...
    assistant_name: String,       // NEW
    use_same_name: bool,          // NEW
    machine_name: String,         // NEW
    wake_word: String,            // NEW
}
```

Default all to `"Assistant"` / `"assistant"`.

- [ ] **Step 3: Implement show_name_assistant()**

Show a card with:
- Text field for "Name your assistant" (pre-filled "Assistant")
- Toggle: "Use same name for wake word and network?" (default: Yes)
- If No → show two additional fields for wake word and machine name
- Explain each: "Chat label & personality", "What you say to wake it", "Network hostname (name.aios.local)"

```rust
fn show_name_assistant(&self) {
    let input_box = gtk::Box::new(Orientation::Vertical, 8);
    input_box.set_margin_top(8);

    let name_entry = gtk::Entry::builder()
        .placeholder_text("Assistant name")
        .text("Assistant")
        .hexpand(true)
        .build();
    input_box.append(&name_entry);

    // "Use same for all" toggle
    let same_toggle = gtk::CheckButton::with_label(
        "Use same name for wake word and network hostname"
    );
    same_toggle.set_active(true);
    input_box.append(&same_toggle);

    // Hidden extra fields (shown when toggle is off)
    let extra_box = gtk::Box::new(Orientation::Vertical, 6);
    extra_box.set_visible(false);
    // ... wake word entry, machine name entry ...
    input_box.append(&extra_box);

    // Toggle visibility
    same_toggle.connect_toggled(move |t| {
        extra_box.set_visible(!t.is_active());
    });

    let next_btn = gtk::Button::with_label("Next →");
    // ... validation, store values, advance ...

    self.chat_view.add_setup_card(
        "avatar-default-symbolic",
        "Name Your Assistant",
        "Choose a name for your AI. This is shown in chat, used as the wake word,\n\
         and as the network hostname (name.aios.local).",
        Some(input_box.upcast_ref()),
    );
}
```

- [ ] **Step 4: Wire the step into the flow**

After TestAudioInput (or skip), advance to NameAssistant.
After NameAssistant, advance to ChooseProvider.

Update `show_step()` match and `on_voice_input()` to handle the new step.

- [ ] **Step 5: Store names in config on completion**

In the `on_complete` callback in `app.rs`, save the names:

```rust
// Save assistant identity
if let Ok(mut config) = ConfigManager::new() {
    let _ = config.set("assistant.name", json!(result.assistant_name));
    let _ = config.set("voice.wake_word", json!(result.wake_word));
    let _ = config.set("system.machine_name", json!(result.machine_name));
}
// Update system hostname
let _ = std::process::Command::new("sudo")
    .args(["hostnamectl", "set-hostname", &result.machine_name])
    .status();
let _ = std::process::Command::new("sudo")
    .args(["systemctl", "restart", "avahi-daemon"])
    .status();
```

Also update `SetupResult` to include the new fields.

- [ ] **Step 6: Update summary INFO message**

In `show_complete()`, add the name to the summary:
```rust
status.add(StatusLine::new("Assistant name", true, &s.assistant_name));
status.add(StatusLine::new("Wake word", true, &s.wake_word));
status.add(StatusLine::new("Network name", true, format!("{}.aios.local", s.machine_name)));
```

- [ ] **Step 7: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

- [ ] **Step 8: Commit**

---

### Task 8: Update Selftest

**Files:**
- Modify: `distro/_inner_build.sh` — update hostname check in aios-test
- Modify: `aios-app-rs/aios-core/src/selftest/scenarios.rs`

- [ ] **Step 1: Update selftest hostname reference**

In the `aios-test` script, the keyboard layout check reads from config. Add a hostname check:

```bash
HOSTNAME=$(cat /etc/hostname 2>/dev/null || echo "unknown")
pass "Hostname: $HOSTNAME"
```

- [ ] **Step 2: Add unit tests for hostname validation**

```rust
register("hostname: valid names", false, |_ctx| {
    assert!(is_valid_hostname("assistant"));
    assert!(is_valid_hostname("my-pc"));
    assert!(is_valid_hostname("jarvis-2"));
    assert!(!is_valid_hostname("-bad"));
    assert!(!is_valid_hostname("bad-"));
    assert!(!is_valid_hostname(""));
    assert!(!is_valid_hostname("has spaces"));
    assert!(!is_valid_hostname("UPPERCASE")); // must be lowercase
    TestResult::pass("hostname: valid names", "Validation works")
});
```

- [ ] **Step 3: Run all tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test --workspace`

- [ ] **Step 4: Commit**

---

### Task 9: Final Verification

- [ ] **Step 1: Run full workspace tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test --workspace`
Expected: all pass

- [ ] **Step 2: Verify release build**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check --release`

- [ ] **Step 3: Verify shell scripts**

Run: `bash -n distro/_inner_build.sh && echo OK`

- [ ] **Step 4: Commit any fixups**
