# i18n Framework Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add full multilanguage support to AiOS — all ~200 UI strings translatable, auto-detected language, 25+ languages generated via LLM, compiled into the binary.

**Architecture:** JSON translation files per language compiled via `include_str!`. A core `i18n` module provides `t(key)` and `t_fmt(key, args)` functions. Strings extracted from 5 files (first_boot.rs, app.rs, commands.rs, chat_view.rs, index.html). A dev script generates translations via LLM from the English source.

**Tech Stack:** Rust, serde_json, GTK4, JavaScript, bash (translation generator)

---

### Task 1: Core i18n Module + English Source File

**Files:**
- Create: `aios-app-rs/aios-core/src/i18n.rs`
- Create: `aios-app-rs/aios-core/i18n/en.json`
- Modify: `aios-app-rs/aios-core/src/lib.rs` — add `pub mod i18n;`
- Modify: `aios-app-rs/aios-core/src/config/defaults.rs` — add `assistant.language` default

- [ ] **Step 1: Create the English source JSON**

Create `aios-app-rs/aios-core/i18n/en.json` with ALL extractable strings organized by category. Start with the framework keys:

```json
{
  "_meta": {
    "language": "English",
    "code": "en",
    "direction": "ltr"
  },
  "chat.role.you": "You",
  "chat.role.system": "System",
  "chat.role.tool": "Tool",
  "chat.role.assistant_default": "Assistant",

  "boot.status.desktop": "Desktop",
  "boot.status.web_channel": "Web Channel",
  "boot.status.signal": "Signal",
  "boot.status.llm_provider": "LLM Provider",
  "boot.status.backup": "Backup",
  "boot.status.audio_output": "Audio Output (TTS)",
  "boot.status.audio_input": "Audio Input (STT)",
  "boot.status.keyboard": "Keyboard",
  "boot.status.timezone": "Timezone",
  "boot.status.boot_time": "Boot time",
  "boot.status.mode": "Mode",
  "boot.status.first_boot": "First-boot setup",
  "boot.status.available": "available",
  "boot.status.unavailable": "unavailable",
  "boot.status.disabled": "disabled",
  "boot.status.not_installed": "not installed",
  "boot.status.no_api_key": "no API key",

  "setup.welcome.title": "Welcome to AiOS!",
  "setup.welcome.description": "I'm your AI assistant. Let's set up your system together.\nFirst, let's check your audio.",
  "setup.welcome.button": "Get Started →",
  "setup.welcome.tts": "Welcome to AiOS! I'm your AI assistant. Let's set up your system together.",

  "setup.audio_output.title": "Test Audio Output",
  "setup.audio_output.description": "Let's check if you can hear me.\nI'll play a test message. Click Replay if you need to hear it again.",
  "setup.audio_output.replay": "🔊 Replay Audio",
  "setup.audio_output.yes": "✅ Yes, I can hear",
  "setup.audio_output.no": "❌ No audio / Skip",
  "setup.audio_output.tts": "Can you hear me? This is AiOS speaking.",
  "setup.audio_output.skipped": "Audio output skipped. You can configure it later in Settings.",

  "setup.audio_input.title": "Test Microphone",
  "setup.audio_input.description": "Speak into your microphone.\nThe meter below shows your audio level in real time.",
  "setup.audio_input.speak_now": "🎤 Speak now — the meter should move:",
  "setup.audio_input.no_audio": "No audio detected",
  "setup.audio_input.waiting": "Waiting for audio...",
  "setup.audio_input.detected": "✅ Audio detected! Your microphone works.",
  "setup.audio_input.works": "✅ Mic works!",
  "setup.audio_input.skip": "No mic / Skip →",
  "setup.audio_input.skipped": "Mic test skipped. You can configure voice input later in Settings.",
  "setup.audio_input.tts": "Now let's test your microphone. Please say something.",

  "setup.name.title": "Name Your Assistant",
  "setup.name.description": "Choose a name for your AI assistant.\nThis is shown in chat, used as the wake word, and as the network hostname.",
  "setup.name.label": "Assistant name (shown in chat):",
  "setup.name.same_toggle": "Use same name for wake word and network hostname",
  "setup.name.wake_label": "Wake word (what you say to activate):",
  "setup.name.host_label": "Network hostname (reachable as <name>.local):",
  "setup.name.error_empty": "Please enter a name",
  "setup.name.error_hostname": "Invalid hostname: use lowercase letters, numbers, hyphens",
  "setup.name.tts": "What would you like to call me? The default is Assistant.",

  "setup.provider.title": "Choose Your AI Provider",
  "setup.provider.description": "Which AI would you like to use as your primary assistant?",
  "setup.provider.claude_name": "Claude (Anthropic)",
  "setup.provider.claude_desc": "Advanced reasoning and analysis, strong at coding tasks",
  "setup.provider.chatgpt_name": "ChatGPT (OpenAI)",
  "setup.provider.chatgpt_desc": "GPT-4o with broad general knowledge and tool use",
  "setup.provider.auto_claude": "Claude API key found in system config — using Claude.",
  "setup.provider.auto_chatgpt": "ChatGPT API key found in system config — using ChatGPT.",
  "setup.provider.tts": "Which AI provider would you like to use? You can say Claude or ChatGPT.",

  "setup.api_key.title_claude": "Enter Your Claude API Key",
  "setup.api_key.title_chatgpt": "Enter Your ChatGPT API Key",
  "setup.api_key.tutorial_claude": "How to get your key:\n1. Go to console.anthropic.com\n2. Sign in or create an account\n3. Go to Settings → API Keys\n4. Click \"Create Key\" and copy it\n\nThe key starts with sk-ant-...",
  "setup.api_key.tutorial_chatgpt": "How to get your key:\n1. Go to platform.openai.com\n2. Sign in or create an account\n3. Go to API Keys in the sidebar\n4. Click \"Create new secret key\" and copy it\n\nThe key starts with sk-...",
  "setup.api_key.placeholder": "Paste your API key here",
  "setup.api_key.error_empty": "Please enter an API key",
  "setup.api_key.next": "Next →",
  "setup.api_key.tts": "Please type or paste your API key.",

  "setup.password.title": "Secure Your Data",
  "setup.password.description": "Create a master password to protect your API keys and personal data.\nMinimum 8 characters.",
  "setup.password.placeholder": "Master password (min. 8 characters)",
  "setup.password.strength": "Strength:",
  "setup.password.error_short": "Password must be at least 8 characters",
  "setup.password.tts": "Now let's secure your data. Please type a master password.",

  "setup.confirm_password.title": "Confirm Password",
  "setup.confirm_password.description": "Type your password again to confirm.",
  "setup.confirm_password.placeholder": "Confirm your password",
  "setup.confirm_password.error_mismatch": "Passwords do not match",
  "setup.confirm_password.tts": "Please type your password again to confirm.",

  "setup.backup.title": "Add a Backup Provider?",
  "setup.backup.description": "If your primary AI is unavailable, a backup can take over automatically.",
  "setup.backup.yes": "Yes, add {provider}",
  "setup.backup.no": "No, I'm good",
  "setup.backup.tts": "Would you like to add a backup AI provider?",

  "setup.order.title": "Choose Primary Provider",
  "setup.order.description": "Which provider should be your primary AI? The other will be used as fallback.",
  "setup.order.primary": "{provider} as primary",
  "setup.order.tts": "Which provider should be your primary AI? Say Claude or ChatGPT.",

  "setup.complete.title": "First-Boot Setup Complete",
  "setup.complete.primary_provider": "Primary provider",
  "setup.complete.backup_provider": "Backup provider",
  "setup.complete.api_key_stored": "{provider} (API key stored)",
  "setup.complete.master_password": "Master password",
  "setup.complete.master_password_set": "set",
  "setup.complete.vault": "Vault",
  "setup.complete.vault_created": "created",
  "setup.complete.assistant_name": "Assistant name",
  "setup.complete.wake_word": "Wake word",
  "setup.complete.network": "Network",
  "setup.complete.button": "Start Chatting →",
  "setup.complete.tts": "You're all set! Your AI assistant is ready.",

  "setup.transition": "Setup complete. How can I help?",
  "setup.type_message": "Type a message or use /help to see available commands.",

  "hostname.conflict.warning": "Hostname conflict: another machine on this network is already using '{name}.local'",
  "hostname.conflict.title": "Rename Your Machine",
  "hostname.conflict.description": "Another machine is using '{name}' on this network.\nChoose a different name:",
  "hostname.conflict.hint": "Lowercase letters, numbers, and hyphens only. Will be reachable as <name>.local",
  "hostname.conflict.apply": "Apply",
  "hostname.conflict.changed": "Hostname changed to '{name}'. Reachable as {name}.local",

  "web.placeholder": "Message AiOS...",
  "web.send": "Send",
  "web.connected": "Connected",
  "web.disconnected": "Disconnected",
  "web.connecting": "Connecting...",
  "web.panel.cancel": "Cancel",
  "web.panel.submit": "Submit"
}
```

Then add ALL command strings (help text, descriptions, error messages) from `commands.rs`. This will be ~80 more keys under `cmd.*` prefix.

- [ ] **Step 2: Create the i18n Rust module**

Create `aios-app-rs/aios-core/src/i18n.rs`:

```rust
//! Internationalization (i18n) support.
//!
//! Loads JSON translation files compiled into the binary. Provides `t(key)`
//! for simple lookups and `t_fmt(key, args)` for interpolation.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value;

// Embed translation files at compile time.
const EN_JSON: &str = include_str!("../i18n/en.json");

// Thread-local state: current language + loaded translations.
thread_local! {
    static CURRENT_LANG: RefCell<String> = RefCell::new("en".to_string());
    static TRANSLATIONS: RefCell<HashMap<String, HashMap<String, String>>> = RefCell::new(HashMap::new());
}

/// Initialize the i18n system. Call once at startup.
pub fn init() {
    load_language("en", EN_JSON);
    // Load other languages here as they're added.
}

/// Load a language from JSON.
fn load_language(code: &str, json: &str) {
    if let Ok(Value::Object(map)) = serde_json::from_str(json) {
        let mut strings = HashMap::new();
        for (key, val) in map {
            if key == "_meta" { continue; }
            if let Value::String(s) = val {
                strings.insert(key, s);
            }
        }
        TRANSLATIONS.with(|t| {
            t.borrow_mut().insert(code.to_string(), strings);
        });
    }
}

/// Set the active language.
pub fn set_language(lang: &str) {
    CURRENT_LANG.with(|l| *l.borrow_mut() = lang.to_string());
}

/// Get the current language code.
pub fn current_language() -> String {
    CURRENT_LANG.with(|l| l.borrow().clone())
}

/// Translate a key. Falls back to English, then returns the key itself.
pub fn t(key: &str) -> String {
    let lang = current_language();
    TRANSLATIONS.with(|t| {
        let translations = t.borrow();
        // Try current language.
        if let Some(strings) = translations.get(&lang) {
            if let Some(s) = strings.get(key) {
                return s.clone();
            }
        }
        // Fallback to English.
        if lang != "en" {
            if let Some(strings) = translations.get("en") {
                if let Some(s) = strings.get(key) {
                    return s.clone();
                }
            }
        }
        // Return the key itself as last resort.
        key.to_string()
    })
}

/// Translate with interpolation. Replaces `{name}` placeholders.
pub fn t_fmt(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(key);
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

/// Auto-detect language from system locale.
pub fn detect_system_language() -> String {
    // Check LANG env var: "de_DE.UTF-8" -> "de"
    if let Ok(lang) = std::env::var("LANG") {
        let code = lang.split('_').next().unwrap_or("en");
        let code = code.split('.').next().unwrap_or("en");
        if !code.is_empty() && code != "C" && code != "POSIX" {
            return code.to_string();
        }
    }
    "en".to_string()
}

/// Get all available languages as (code, native_name) pairs.
pub fn available_languages() -> Vec<(String, String)> {
    TRANSLATIONS.with(|t| {
        let translations = t.borrow();
        let mut langs: Vec<(String, String)> = Vec::new();
        for (code, strings) in translations.iter() {
            let name = strings.get("_meta.language")
                .cloned()
                .unwrap_or_else(|| code.clone());
            langs.push((code.clone(), name));
        }
        langs.sort_by(|a, b| a.0.cmp(&b.0));
        langs
    })
}

/// Get the full translations map for a language (for sending to web client).
pub fn get_translations(lang: &str) -> Option<HashMap<String, String>> {
    TRANSLATIONS.with(|t| {
        t.borrow().get(lang).cloned()
    })
}
```

- [ ] **Step 3: Register the module**

Add to `aios-app-rs/aios-core/src/lib.rs`:
```rust
pub mod i18n;
```

Add `"en"` as default to `assistant.language` in `defaults.rs`.

- [ ] **Step 4: Add tests**

Add tests to `i18n.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_returns_english_string() {
        init();
        set_language("en");
        assert_eq!(t("chat.role.you"), "You");
    }

    #[test]
    fn t_returns_key_for_missing() {
        init();
        set_language("en");
        assert_eq!(t("nonexistent.key"), "nonexistent.key");
    }

    #[test]
    fn t_fmt_replaces_placeholders() {
        init();
        set_language("en");
        let result = t_fmt("setup.backup.yes", &[("provider", "Claude")]);
        assert_eq!(result, "Yes, add Claude");
    }

    #[test]
    fn detect_language_fallback() {
        // With no LANG set or LANG=C, should return "en"
        let lang = detect_system_language();
        assert!(!lang.is_empty());
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test -p aios-core i18n`

- [ ] **Step 6: Commit**

---

### Task 2: Extract Strings from first_boot.rs

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs`

- [ ] **Step 1: Add i18n import and replace all hardcoded strings**

Add `use aios_core::i18n::{t, t_fmt};` at the top.

Replace every user-visible string with `t()` or `t_fmt()` calls. Examples:

```rust
// Before:
"Welcome to AiOS!"
// After:
&t("setup.welcome.title")

// Before:
"I'm your AI assistant. Let's set up your system together.\nFirst, let's check your audio."
// After:
&t("setup.welcome.description")

// Before (with interpolation):
&format!("Yes, add {}", if backup == "claude" { "Claude" } else { "ChatGPT (OpenAI)" })
// After:
&t_fmt("setup.backup.yes", &[("provider", display)])
```

Go through EVERY `add_setup_card()`, `add_message()`, `add_level_message()`, `Button::with_label()`, `Label::new()`, `speak()`, and `placeholder_text()` call. Replace all literal strings with `t()` calls using the keys defined in `en.json`.

- [ ] **Step 2: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

- [ ] **Step 3: Commit**

---

### Task 3: Extract Strings from app.rs

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs`

- [ ] **Step 1: Initialize i18n at startup**

In both `run_first_boot_setup()` and `activate_main()`, early (before boot status):

```rust
aios_core::i18n::init();
let lang = config.get_str("assistant.language", "");
if lang.is_empty() {
    let detected = aios_core::i18n::detect_system_language();
    aios_core::i18n::set_language(&detected);
} else {
    aios_core::i18n::set_language(&lang);
}
```

- [ ] **Step 2: Replace boot status strings**

Replace all `StatusLine::new("Desktop", ...)` with `StatusLine::new(&t("boot.status.desktop"), ...)`. Do this for every boot status line in both `run_first_boot_setup()` and `activate_main()`.

- [ ] **Step 3: Replace system messages and transition text**

Replace `"Setup complete. How can I help?"` with `t("setup.transition")`, etc.

- [ ] **Step 4: Replace hostname conflict card strings**

Replace all strings in the hostname conflict card with `t()` calls.

- [ ] **Step 5: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

- [ ] **Step 6: Commit**

---

### Task 4: Extract Strings from commands.rs

**Files:**
- Modify: `aios-app-rs/aios-core/src/config/commands.rs`
- Modify: `aios-app-rs/aios-core/i18n/en.json` — add `cmd.*` keys

- [ ] **Step 1: Add command strings to en.json**

Add ~80 keys under the `cmd.*` prefix for all command descriptions, help text, and error messages. Example:

```json
{
  "cmd.help.title": "Available Commands",
  "cmd.help.description": "Show available commands",
  "cmd.key.description": "Set API key for a provider",
  "cmd.key.usage": "Usage: /key <provider> <api-key>",
  "cmd.key.error_empty": "API key cannot be empty.",
  "cmd.provider.description": "Switch LLM provider",
  "cmd.provider.switched": "Switched to {provider}",
  "cmd.provider.not_found": "Unknown provider: {name}",
  ...
}
```

- [ ] **Step 2: Replace strings in commands.rs**

Add `use crate::i18n::{t, t_fmt};` and replace all user-visible strings.

- [ ] **Step 3: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test -p aios-core`

Some command tests may assert on specific English strings — update those to use `t()` or match the new output.

- [ ] **Step 4: Commit**

---

### Task 5: Extract Strings from Web Client

**Files:**
- Modify: `aios-app-rs/aios-web/src/static/index.html`
- Modify: `aios-app-rs/aios-web/src/server.rs` — send translations on WebSocket connect

- [ ] **Step 1: Add i18n system to JavaScript**

Near the top of the script section in index.html:

```javascript
let translations = {};
let currentLang = 'en';

function t(key) {
    return translations[currentLang]?.[key] || translations['en']?.[key] || key;
}

function tFmt(key, args) {
    let s = t(key);
    for (const [k, v] of Object.entries(args)) {
        s = s.replace('{' + k + '}', v);
    }
    return s;
}
```

- [ ] **Step 2: Replace all hardcoded strings in JavaScript**

Replace placeholder text, button labels, status messages, command descriptions, role names with `t()` calls.

- [ ] **Step 3: Send translations from server on WebSocket connect**

In `server.rs`, after sending the welcome message on connect, send the translations for the current language:

```rust
// Send translations for the current language.
if let Some(trans) = aios_core::i18n::get_translations(&aios_core::i18n::current_language()) {
    let msg = ServerMessage::System {
        content: serde_json::to_string(&serde_json::json!({
            "type": "translations",
            "lang": aios_core::i18n::current_language(),
            "strings": trans,
        })).unwrap_or_default(),
    };
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = ws_tx.send(Message::Text(json.into())).await;
    }
}
```

- [ ] **Step 4: Handle translations message in JavaScript**

In the web client message handler, when receiving a System message with type "translations":

```javascript
if (data.type === 'System') {
    try {
        const cfg = JSON.parse(data.content);
        if (cfg.type === 'translations') {
            translations[cfg.lang] = cfg.strings;
            currentLang = cfg.lang;
            // Re-render any static UI elements
            document.getElementById('input').placeholder = t('web.placeholder');
        }
    } catch(e) {}
}
```

- [ ] **Step 5: Verify build**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-web -p aios-gtk`

- [ ] **Step 6: Commit**

---

### Task 6: Welcome Card Language Selector

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs` — add language dropdown to welcome card

- [ ] **Step 1: Add language dropdown to welcome card**

In `show_welcome()`, add a dropdown before the "Get Started" button:

```rust
// Language selector
let lang_label = gtk::Label::new(Some(&t("setup.welcome.language")));
lang_label.set_halign(Align::Start);
input_box.append(&lang_label);

let lang_dropdown = gtk::DropDown::from_strings(&[
    "English", "Deutsch", "Français", "Español", "Italiano",
    "Português", "Română", "日本語", "中文", "한국어", "Русский",
    // ... more languages
]);
// Set default based on detected language
input_box.append(&lang_dropdown);
```

When the user selects a language, update `i18n::set_language()` and re-render the welcome card text.

- [ ] **Step 2: Add same selector to web client welcome**

Add a language dropdown in the web client that sends the selection to the server.

- [ ] **Step 3: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

- [ ] **Step 4: Commit**

---

### Task 7: Translation Generator Script

**Files:**
- Create: `scripts/generate-translations.sh`

- [ ] **Step 1: Create the script**

```bash
#!/bin/bash
# Generate translation files from English source using the Claude API.
# Usage: ./scripts/generate-translations.sh [language_code]
# If no language specified, generates all supported languages.

set -euo pipefail

SOURCE="aios-app-rs/aios-core/i18n/en.json"
OUTPUT_DIR="aios-app-rs/aios-core/i18n"

# All supported languages
LANGUAGES=(
    de fr es it pt ro nl pl cs hu sv no da fi
    el tr ar hi ja zh ko ru uk id
)

# Read API key
if [ -z "${CLAUDE_API_KEY:-}" ]; then
    CLAUDE_API_KEY=$(grep -oP 'CLAUDE_API_KEY\s*=\s*\K\S+' .env 2>/dev/null || true)
fi
if [ -z "$CLAUDE_API_KEY" ]; then
    echo "Error: Set CLAUDE_API_KEY in .env or environment"
    exit 1
fi

generate_language() {
    local lang=$1
    local lang_name=$2
    echo "Generating $lang_name ($lang)..."

    local prompt="Translate all string values in this JSON to $lang_name. Keep the keys exactly as they are. Only translate the values. Keep {placeholder} variables unchanged. Keep emoji/unicode symbols unchanged. Return valid JSON only, no explanation."

    local response=$(curl -s https://api.anthropic.com/v1/messages \
        -H "x-api-key: $CLAUDE_API_KEY" \
        -H "anthropic-version: 2023-06-01" \
        -H "content-type: application/json" \
        -d "$(jq -n --arg prompt "$prompt" --rawfile source "$SOURCE" '{
            model: "claude-sonnet-4-20250514",
            max_tokens: 8192,
            messages: [{role: "user", content: ($prompt + "\n\n" + $source)}]
        }')")

    echo "$response" | jq -r '.content[0].text' > "${OUTPUT_DIR}/${lang}.json"
    echo "  → ${OUTPUT_DIR}/${lang}.json"
}

# Language names for the prompt
declare -A LANG_NAMES=(
    [de]="German" [fr]="French" [es]="Spanish" [it]="Italian"
    [pt]="Portuguese" [ro]="Romanian" [nl]="Dutch" [pl]="Polish"
    [cs]="Czech" [hu]="Hungarian" [sv]="Swedish" [no]="Norwegian"
    [da]="Danish" [fi]="Finnish" [el]="Greek" [tr]="Turkish"
    [ar]="Arabic" [hi]="Hindi" [ja]="Japanese" [zh]="Chinese Simplified"
    [ko]="Korean" [ru]="Russian" [uk]="Ukrainian" [id]="Indonesian"
)

if [ -n "${1:-}" ]; then
    generate_language "$1" "${LANG_NAMES[$1]}"
else
    for lang in "${LANGUAGES[@]}"; do
        generate_language "$lang" "${LANG_NAMES[$lang]}"
    done
fi

echo "Done! Generated translations for ${#LANGUAGES[@]} languages."
```

- [ ] **Step 2: Make executable and test with one language**

```bash
chmod +x scripts/generate-translations.sh
./scripts/generate-translations.sh de  # Test with German only
```

- [ ] **Step 3: Generate all languages**

```bash
./scripts/generate-translations.sh
```

- [ ] **Step 4: Register all language files in i18n.rs**

Add `include_str!` and `load_language()` calls for each generated file.

- [ ] **Step 5: Commit all translation files**

---

### Task 8: Tests + Final Verification

**Files:**
- Modify: `aios-app-rs/aios-core/src/selftest/scenarios.rs`

- [ ] **Step 1: Add i18n selftest scenarios**

```rust
register("i18n: english translations load", false, |_ctx| {
    aios_core::i18n::init();
    aios_core::i18n::set_language("en");
    let welcome = aios_core::i18n::t("setup.welcome.title");
    assert_eq!(welcome, "Welcome to AiOS!");
    TestResult::pass("i18n: english translations load", "English strings load correctly")
});

register("i18n: interpolation works", false, |_ctx| {
    aios_core::i18n::init();
    aios_core::i18n::set_language("en");
    let result = aios_core::i18n::t_fmt("setup.backup.yes", &[("provider", "Claude")]);
    assert_eq!(result, "Yes, add Claude");
    TestResult::pass("i18n: interpolation works", "Placeholder interpolation works")
});

register("i18n: missing key returns key", false, |_ctx| {
    aios_core::i18n::init();
    let result = aios_core::i18n::t("nonexistent.key.here");
    assert_eq!(result, "nonexistent.key.here");
    TestResult::pass("i18n: missing key returns key", "Missing keys return the key string")
});
```

- [ ] **Step 2: Run full workspace tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test --workspace`
Expected: all pass

- [ ] **Step 3: Verify release build**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check --release`

- [ ] **Step 4: Verify shell scripts**

Run: `bash -n distro/_inner_build.sh && echo OK`

- [ ] **Step 5: Commit any fixups**
