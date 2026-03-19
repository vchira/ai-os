# Provider & Model UI Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 7 UI surfaces that display incorrect/incomplete provider and model information.

**Architecture:** Add helpers to the centralized `providers.rs`, update boot status and autoconfig messages to show both Main AI + Summarizer, restructure the settings dialog AI page into 3 sections (model assignment, API keys, custom instructions), add model attribution to chat messages, and decouple the TTS summarizer from hardcoded Claude Haiku.

**Tech Stack:** Rust, GTK4/libadwaita, serde_json, reqwest (blocking)

**Spec:** `docs/superpowers/specs/2026-03-19-provider-model-ui-overhaul-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `aios-gtk/src/providers.rs` | Modify | Add 4 new helpers: `model_human_name`, `mask_api_key`, `configured_display_names_excluding_ollama`, `provider_api_url` |
| `aios-core/src/config/defaults.rs` | Modify | Change `tts_summary_provider`/`tts_summary_model` defaults from "auto" to "claude"/"claude-sonnet-4-20250514" |
| `aios-core/i18n/en.json` | Modify | Add `boot.status.main_ai`, `boot.status.summarizer`; remove old LLM provider keys |
| `aios-core/i18n/de.json` | Modify | Same with German translations |
| `aios-gtk/src/boot_status.rs` | Modify | Replace LLM Provider + Backup lines with Main AI + Summarizer lines |
| `aios-gtk/src/app.rs` | Modify | Add startup migration for "auto" summary; use `configured_display_names_excluding_ollama()` |
| `aios-gtk/src/ui/chat_view.rs` | Modify | Add `add_assistant_message()` method |
| `aios-gtk/src/llm_handler.rs` | Modify | Build model label and use `add_assistant_message()` in both handler paths |
| `aios-gtk/src/tts.rs` | Modify | Replace hardcoded Claude Haiku with configurable summarizer |
| `aios-gtk/src/first_boot_flow.rs` | Modify | Update autoconfig messages; add summarizer fallback logic |
| `aios-gtk/src/ui/main_window.rs` | Modify | Fix title bar dropdown; add settings-close refresh |
| `aios-gtk/src/ui/settings_dialog.rs` | Modify | Rewrite `build_ai_page()` with new 3-section layout |
| `aios-gtk/src/ui/first_boot.rs` | Modify | Add summarizer step to interactive setup |

---

### Task 1: Provider Helpers

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/providers.rs:139-192`
- Test: `aios-app-rs/aios-gtk/src/providers.rs` (inline `#[cfg(test)]` module)

- [ ] **Step 1: Write failing tests for the 4 new helpers**

Add a `#[cfg(test)]` module at the end of `providers.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_human_name_known_slug() {
        assert_eq!(model_human_name("deepseek-reasoner"), "DeepSeek Reasoner");
        assert_eq!(model_human_name("claude-sonnet-4-20250514"), "Claude Sonnet 4");
        assert_eq!(model_human_name("gpt-4o"), "GPT-4o");
        assert_eq!(model_human_name("llama-3.1-8b-instant"), "Llama 3.1 8B");
    }

    #[test]
    fn model_human_name_unknown_slug_returns_itself() {
        assert_eq!(model_human_name("some-unknown-model"), "some-unknown-model");
    }

    #[test]
    fn mask_api_key_normal() {
        assert_eq!(mask_api_key("sk-ant-api03-tjiIBa7Z5UjXlwAA"), "sk-ant-...lwAA");
        assert_eq!(mask_api_key("gsk_qBE9SEHq3QBv6xcR"), "gsk_...6xcR");
    }

    #[test]
    fn mask_api_key_edge_cases() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("abc"), "...abc");
        assert_eq!(mask_api_key("abcdefgh"), "abcd...fgh");
    }

    #[test]
    fn configured_excluding_ollama_never_returns_ollama() {
        let config = ConfigManager::default();
        // Even if ollama were enabled, it should not appear
        let names = configured_display_names_excluding_ollama(&config);
        assert!(!names.iter().any(|n| n == "Ollama"));
    }

    #[test]
    fn provider_api_url_known() {
        assert_eq!(provider_api_url("claude"), "https://api.anthropic.com");
        assert_eq!(provider_api_url("deepseek"), "https://api.deepseek.com");
        assert_eq!(provider_api_url("groq"), "https://api.groq.com/openai");
        assert_eq!(
            provider_api_url("gemini"),
            "https://generativelanguage.googleapis.com/v1beta/openai"
        );
    }

    #[test]
    fn provider_api_url_unknown_returns_empty() {
        assert_eq!(provider_api_url("nonexistent"), "");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd aios-app-rs && cargo test -p aios-gtk --lib providers::tests -- --nocapture`
Expected: FAIL — functions not defined

- [ ] **Step 3: Implement the 4 helpers**

Add after the existing `current_model` function at line 191:

```rust
/// Look up the human-readable name for a model slug.
///
/// Searches all providers' model lists. Returns the slug itself if not found.
pub fn model_human_name(slug: &str) -> String {
    for prov in PROVIDERS {
        for (human, model_slug) in prov.models {
            if *model_slug == slug {
                return human.to_string();
            }
        }
    }
    slug.to_string()
}

/// Mask an API key for display: show a short prefix + last 4 chars.
///
/// Examples: `"sk-ant-...lwAA"`, `"gsk_...6xcR"`, `""` → `""`.
pub fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let len = key.len();
    if len <= 8 {
        return format!("...{key}");
    }
    // Find a natural prefix break: first 4 chars or up to first '-' or '_' within first 8 chars.
    let prefix_end = key[..8]
        .find(|c: char| c == '-' || c == '_')
        .map(|i| i + 1) // include the separator
        .unwrap_or(4)
        .min(len.saturating_sub(4));
    let suffix_start = len - 4;
    format!("{}...{}", &key[..prefix_end], &key[suffix_start..])
}

/// Return display names for configured providers, excluding Ollama.
pub fn configured_display_names_excluding_ollama(config: &ConfigManager) -> Vec<String> {
    PROVIDERS
        .iter()
        .filter(|p| p.id != "ollama" && is_configured(p, config))
        .map(|p| p.display_name.to_string())
        .collect()
}

/// Return the base API URL for a provider.
///
/// For Claude, returns the Anthropic API base. For OpenAI-compatible providers,
/// returns the base URL that `OpenAIProvider::with_base_url()` uses. The caller
/// appends the appropriate path (`/v1/messages` for Claude, `/v1/chat/completions`
/// for others).
pub fn provider_api_url(provider_id: &str) -> &'static str {
    match provider_id {
        "claude" => "https://api.anthropic.com",
        "openai" => "https://api.openai.com",
        "deepseek" => "https://api.deepseek.com",
        "mistral" => "https://api.mistral.ai",
        "groq" => "https://api.groq.com/openai",
        "gemini" => "https://generativelanguage.googleapis.com/v1beta/openai",
        "ollama" => "http://localhost:11434",
        _ => "",
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd aios-app-rs && cargo test -p aios-gtk --lib providers::tests -- --nocapture`
Expected: All 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/providers.rs
git commit -m "feat(providers): add model_human_name, mask_api_key, exclude-ollama, provider_api_url helpers"
```

---

### Task 2: Config Defaults + i18n Keys

**Files:**
- Modify: `aios-app-rs/aios-core/src/config/defaults.rs:30-31`
- Modify: `aios-app-rs/aios-core/i18n/en.json:16-17,29,163-164`
- Modify: `aios-app-rs/aios-core/i18n/de.json:16-17,29,163-164`

- [ ] **Step 1: Update defaults.rs**

Change lines 30-31 from:
```rust
"tts_summary_provider": "auto",
"tts_summary_model": "auto",
```
to:
```rust
"tts_summary_provider": "claude",
"tts_summary_model": "claude-sonnet-4-20250514",
```

- [ ] **Step 2: Update en.json**

Replace lines 16-17:
```json
"boot.status.main_ai": "Main AI",
"boot.status.summarizer": "Summarizer",
```

Remove lines 29, 163-164 (the old keys: `boot.status.no_api_key`, `boot.status.no_api_key_hint`, `boot.status.backup_available`). Keep `boot.status.no_api_key` if it's used elsewhere — check with grep first.

- [ ] **Step 3: Update de.json**

Same structure:
```json
"boot.status.main_ai": "Haupt-KI",
"boot.status.summarizer": "Zusammenfasser",
```

Remove same old keys.

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-core/src/config/defaults.rs aios-app-rs/aios-core/i18n/en.json aios-app-rs/aios-core/i18n/de.json
git commit -m "feat(config): change summary defaults from auto to claude; update i18n keys for Main AI / Summarizer"
```

---

### Task 3: Boot Status — Main AI + Summarizer Lines

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/boot_status.rs:57-98`

- [ ] **Step 1: Write failing test**

Add at the bottom of `boot_status.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_config_with(provider: &str, model_key: &str, model: &str, summary_prov: &str, summary_model: &str) -> ConfigManager {
        let mut config = ConfigManager::default();
        let _ = config.set("llm.provider", serde_json::json!(provider));
        let _ = config.set(model_key, serde_json::json!(model));
        let _ = config.set(&format!("llm.{provider}_api_key"), serde_json::json!("sk-test-key-1234"));
        let _ = config.set("llm.tts_summary_provider", serde_json::json!(summary_prov));
        let _ = config.set("llm.tts_summary_model", serde_json::json!(summary_model));
        if summary_prov != provider {
            let sum_key = format!("llm.{summary_prov}_api_key");
            let _ = config.set(&sum_key, serde_json::json!("sk-test-sum-5678"));
        }
        config
    }

    #[test]
    fn boot_status_shows_main_ai_and_summarizer() {
        let config = test_config_with("deepseek", "llm.deepseek_model", "deepseek-reasoner", "groq", "llama-3.1-8b-instant");
        let status = build_boot_status(&config);
        assert!(status.contains("Main AI"), "should contain Main AI label: {status}");
        assert!(status.contains("deepseek-reasoner"), "should contain main model: {status}");
        assert!(status.contains("Summarizer"), "should contain Summarizer label: {status}");
        assert!(status.contains("llama-3.1-8b-instant"), "should contain summary model: {status}");
    }

    #[test]
    fn boot_status_no_backup_lines() {
        let config = test_config_with("claude", "llm.claude_model", "claude-sonnet-4-20250514", "claude", "claude-sonnet-4-20250514");
        let status = build_boot_status(&config);
        assert!(!status.contains("Backup"), "should not contain Backup: {status}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd aios-app-rs && cargo test -p aios-gtk --lib boot_status::tests -- --nocapture`
Expected: FAIL — status still contains old "LLM Provider" line

- [ ] **Step 3: Replace LLM Provider + Backup block with Main AI + Summarizer**

Replace lines 57-98 in `boot_status.rs` with:

```rust
    // -- Main AI --
    let main_provider_id = config.get_str("llm.provider", "claude");
    let main_def = crate::providers::find_by_id(&main_provider_id);
    let main_has_key = main_def
        .map(|p| crate::providers::is_configured(p, config))
        .unwrap_or(false);
    let main_model = main_def
        .map(|p| crate::providers::current_model(p, config))
        .unwrap_or_else(|| main_provider_id.clone());
    let main_display = main_def
        .map(|p| p.display_name)
        .unwrap_or_else(|| main_provider_id.as_str());
    status.add(StatusLine::new(
        &t("boot.status.main_ai"),
        main_has_key,
        if main_has_key {
            format!("{main_display} ({main_model})")
        } else {
            t("boot.status.no_api_key").to_string()
        },
    ));

    // -- Summarizer --
    let sum_provider_id = config.get_str("llm.tts_summary_provider", "claude");
    let sum_def = crate::providers::find_by_id(&sum_provider_id);
    let sum_has_key = sum_def
        .map(|p| crate::providers::is_configured(p, config))
        .unwrap_or(false);
    let sum_model = config.get_str("llm.tts_summary_model", "");
    let sum_display = sum_def
        .map(|p| p.display_name)
        .unwrap_or_else(|| sum_provider_id.as_str());
    status.add(StatusLine::new(
        &t("boot.status.summarizer"),
        sum_has_key,
        if sum_has_key {
            format!("{sum_display} ({sum_model})")
        } else {
            t("boot.status.no_api_key").to_string()
        },
    ));
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd aios-app-rs && cargo test -p aios-gtk --lib boot_status::tests -- --nocapture`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/boot_status.rs
git commit -m "feat(boot): show Main AI + Summarizer lines instead of LLM Provider + Backup"
```

---

### Task 4: Startup Migration + available_providers Fix

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs:155-170,458-460`

- [ ] **Step 1: Add startup migration in `activate_main`**

Before line 163 (`let chat_view = ChatView::new()`), add the "auto" migration:

```rust
        // Migrate legacy "auto" summary provider to explicit values.
        {
            let summary_prov = config.get_str("llm.tts_summary_provider", "");
            if summary_prov == "auto" || summary_prov.is_empty() {
                let main = config.get_str("llm.provider", "claude");
                let main_model_key = format!("llm.{main}_model");
                let main_model = config.get_str(&main_model_key, "");
                let _ = config.set("llm.tts_summary_provider", serde_json::json!(main));
                if !main_model.is_empty() {
                    let _ = config.set("llm.tts_summary_model", serde_json::json!(main_model));
                }
            }
        }
```

- [ ] **Step 2: Change `available_providers()` to exclude Ollama**

Replace lines 458-460:
```rust
    fn available_providers(config: &ConfigManager) -> Vec<String> {
        crate::providers::configured_display_names(config)
    }
```
with:
```rust
    fn available_providers(config: &ConfigManager) -> Vec<String> {
        crate::providers::configured_display_names_excluding_ollama(config)
    }
```

- [ ] **Step 3: Verify compilation**

Run: `cd aios-app-rs && cargo check`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add aios-app-rs/aios-gtk/src/app.rs
git commit -m "feat(app): migrate auto summary provider on startup; exclude Ollama from provider list"
```

---

### Task 5: Chat View — add_assistant_message Method

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/chat_view.rs:154-256`

- [ ] **Step 1: Add `add_assistant_message` method**

Add after `add_message()` (after line 256):

```rust
    /// Append an assistant message with model attribution in the role label.
    ///
    /// Renders as: "Assistant — DeepSeek Reasoner" or
    /// "Assistant — DeepSeek Reasoner (Llama 3.1 8B)" if summarizer differs.
    pub fn add_assistant_message(&self, content: &str, model_label: &str) {
        let role = "assistant";
        let row = gtk::Box::new(Orientation::Vertical, 2);
        row.add_css_class("message-row");
        row.add_css_class(&format!("message-{role}"));

        row.set_halign(Align::Start);
        row.set_margin_start(0);
        row.set_margin_end(60);
        row.set_hexpand(true);

        // Role label with model attribution.
        let role_row = gtk::Box::new(Orientation::Horizontal, 4);
        role_row.set_halign(Align::Start);

        let display_name = ASSISTANT_NAME.with(|n| n.borrow().clone());
        let full_label = format!("{display_name} \u{2014} {model_label}");
        let role_label = gtk::Label::new(Some(&full_label));
        role_label.add_css_class("message-role-label");
        role_row.append(&role_label);

        // Stop-reading button.
        let stop_btn = gtk::Button::from_icon_name("audio-volume-muted-symbolic");
        stop_btn.add_css_class("flat");
        stop_btn.add_css_class("circular");
        stop_btn.set_tooltip_text(Some("Stop reading"));
        stop_btn.connect_clicked(|_btn| {
            std::thread::spawn(|| {
                let _ = std::process::Command::new("pkill")
                    .args(["-f", "piper"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                let _ = std::process::Command::new("pkill")
                    .args(["-f", "espeak-ng"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                let _ = std::process::Command::new("pkill")
                    .args(["-f", "aplay.*raw"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            });
        });
        role_row.append(&stop_btn);
        row.append(&role_row);

        // Message bubble — reuse same code-block rendering as add_message.
        let bubble = gtk::Box::new(Orientation::Vertical, 4);
        bubble.add_css_class("message-bubble");

        let parts = split_code_blocks(content);
        for part in parts {
            match part {
                ContentPart::Text(text) => {
                    if !text.is_empty() {
                        let pango = aios_core::types::to_pango(&text);
                        let label = gtk::Label::new(None);
                        label.set_markup(&pango);
                        label.set_wrap(true);
                        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                        label.set_xalign(0.0);
                        label.set_selectable(true);
                        bubble.append(&label);
                    }
                }
                ContentPart::Code(code) => {
                    let frame = gtk::Frame::new(None);
                    frame.add_css_class("code-block");
                    let code_label = gtk::Label::new(Some(&code));
                    code_label.set_wrap(true);
                    code_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    code_label.set_xalign(0.0);
                    code_label.set_selectable(true);
                    frame.set_child(Some(&code_label));
                    bubble.append(&frame);
                }
            }
        }

        row.append(&bubble);
        self.container.append(&row);
        self.scroll_to_bottom();
    }
```

**Note:** This duplicates bubble rendering from `add_message`. After all tasks are done, refactor to extract a shared `build_message_bubble(content) -> gtk::Box` helper. Do NOT do this now — complete all tasks first, then refactor.

- [ ] **Step 2: Verify compilation**

Run: `cd aios-app-rs && cargo check`
Expected: No errors (method exists but not yet called)

- [ ] **Step 3: Commit**

```bash
git add aios-app-rs/aios-gtk/src/ui/chat_view.rs
git commit -m "feat(chat): add add_assistant_message with model attribution label"
```

---

### Task 6: LLM Handler — Model Attribution

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/llm_handler.rs:231,345`

- [ ] **Step 1: Add model label builder helper at top of file**

Add after the imports (after line 15):

```rust
/// Build the model attribution label from config.
///
/// Returns e.g. "DeepSeek Reasoner" or "DeepSeek Reasoner (Llama 3.1 8B)"
/// if the summarizer differs from the main model.
fn build_model_label(config: &aios_core::config::ConfigManager) -> String {
    let provider_id = config.get_str("llm.provider", "claude");
    let model_key = format!("llm.{provider_id}_model");
    let model_slug = config.get_str(&model_key, "");
    let main_name = crate::providers::model_human_name(&model_slug);

    let sum_provider = config.get_str("llm.tts_summary_provider", "");
    let sum_model = config.get_str("llm.tts_summary_model", "");

    if !sum_provider.is_empty() && (sum_provider != provider_id || sum_model != model_slug) {
        let sum_name = crate::providers::model_human_name(&sum_model);
        format!("{main_name} ({sum_name})")
    } else {
        main_name
    }
}
```

- [ ] **Step 2: Update `send_to_llm` — line 231**

Replace line 231:
```rust
cv.add_message("assistant", &content_for_display);
```
with:
```rust
let model_label = {
    let s = state_outer.borrow();
    let cfg = s.config_snapshot();
    build_model_label(&cfg)
};
cv.add_assistant_message(&content_for_display, &model_label);
```

This requires capturing a reference to `state` in the inner closure. The variable `state_ref` is already captured. Add a clone before the inner `glib::timeout_add_local` at line 226:

```rust
let state_outer = state_ref.clone();
```

- [ ] **Step 3: Update `handle_remote_llm_message` — line 345**

Replace line 345:
```rust
chat_for_resp.add_message("assistant", &content);
```
with:
```rust
let model_label = {
    let s = state_for_resp.borrow();
    let cfg = s.config_snapshot();
    build_model_label(&cfg)
};
chat_for_resp.add_assistant_message(&content, &model_label);
```

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/llm_handler.rs
git commit -m "feat(llm): show model name in assistant message role label"
```

---

### Task 7: TTS Summarizer Backend — Decouple from Claude Haiku

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/tts.rs:82-204`

- [ ] **Step 1: Add `SummarizerConfig` struct**

Replace the doc comment + signature at line 82-87:

```rust
/// Configuration for the TTS summarizer LLM call.
pub(crate) struct SummarizerConfig {
    pub api_key: String,
    pub model: String,
    pub provider_id: String,
}

/// Summarize a long response using the configured TTS summarizer.
/// Falls back to sentence truncation if API call fails.
pub(crate) fn summarize_for_tts(raw: &str, cfg: &SummarizerConfig) -> String {
```

- [ ] **Step 2: Replace hardcoded request body + endpoint**

Replace lines 99-141 (the `if !api_key.is_empty() { ... }` block) with:

```rust
    if !cfg.api_key.is_empty() {
        let prompt = format!(
            "Summarize the following AI assistant response in exactly ONE short spoken sentence (max 30 words). \
             No markdown, no special characters, no asterisks, no hashtags — just plain spoken English. \
             End with: The detailed answer is in the chat.\n\n---\n{}",
            &plain[..plain.len().min(2000)]
        );

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build();

        if let Ok(client) = client {
            let result = if cfg.provider_id == "claude" {
                // Anthropic protocol
                let body = serde_json::json!({
                    "model": cfg.model,
                    "max_tokens": 100,
                    "messages": [{"role": "user", "content": prompt}]
                });
                let base = crate::providers::provider_api_url(&cfg.provider_id);
                client
                    .post(format!("{base}/v1/messages"))
                    .header("x-api-key", &cfg.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("content-type", "application/json")
                    .body(body.to_string())
                    .send()
                    .ok()
                    .and_then(|r| r.json::<serde_json::Value>().ok())
                    .and_then(|j| j["content"][0]["text"].as_str().map(|s| s.trim().to_string()))
            } else {
                // OpenAI-compatible protocol
                let body = serde_json::json!({
                    "model": cfg.model,
                    "max_tokens": 100,
                    "messages": [{"role": "user", "content": prompt}]
                });
                let base = crate::providers::provider_api_url(&cfg.provider_id);
                client
                    .post(format!("{base}/v1/chat/completions"))
                    .header("Authorization", format!("Bearer {}", cfg.api_key))
                    .header("content-type", "application/json")
                    .body(body.to_string())
                    .send()
                    .ok()
                    .and_then(|r| r.json::<serde_json::Value>().ok())
                    .and_then(|j| j["choices"][0]["message"]["content"].as_str().map(|s| s.trim().to_string()))
            };

            if let Some(summary) = result {
                if !summary.is_empty() {
                    tracing::debug!("TTS summary from {}: {summary}", cfg.provider_id);
                    return format!("{summary}{code_mention}");
                }
            }
        }
    }
```

- [ ] **Step 3: Update `speak_if_enabled_with_signal` caller — line 197-203**

Replace:
```rust
    let raw = text.to_string();
    let api_key = config.get_str("llm.claude_api_key", "");
    std::thread::spawn(move || {
        let speak_text = summarize_for_tts(&raw, &api_key);
        do_tts_with_signal(&speak_text, tts_started);
    });
```
with:
```rust
    let raw = text.to_string();
    let sum_provider = config.get_str("llm.tts_summary_provider", "claude");
    let sum_model = config.get_str("llm.tts_summary_model", "claude-sonnet-4-20250514");
    let api_key = crate::providers::find_by_id(&sum_provider)
        .map(|p| config.get_str(p.api_key_config, ""))
        .unwrap_or_default();
    let summarizer_cfg = SummarizerConfig {
        api_key,
        model: sum_model,
        provider_id: sum_provider,
    };
    std::thread::spawn(move || {
        let speak_text = summarize_for_tts(&raw, &summarizer_cfg);
        do_tts_with_signal(&speak_text, tts_started);
    });
```

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/tts.rs
git commit -m "feat(tts): decouple summarizer from hardcoded Claude Haiku — use configured provider"
```

---

### Task 8: Autoconfig Messages + Summarizer Fallback

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/first_boot_flow.rs:368-394,449-456,467-481`

- [ ] **Step 1: Update autoconfig detection message (line 368-394)**

Replace the `autoconfig_msg` format string. Change `**Provider:** {}` to Main AI + Summarizer lines. The values come from `auto.provider.primary`, `auto.ai.main_model`, `auto.ai.summary_provider`, `auto.ai.summary_model`. Use `model_human_name()` for display names:

```rust
    let main_model_human = crate::providers::model_human_name(&auto.ai.main_model);
    let main_display = crate::providers::find_by_id(&auto.ai.main_provider)
        .map(|p| p.display_name)
        .unwrap_or(auto.ai.main_provider.as_str());
    let sum_prov_id = if auto.ai.summary_provider.is_empty() {
        &auto.ai.main_provider
    } else {
        &auto.ai.summary_provider
    };
    let sum_model_id = if auto.ai.summary_model.is_empty() {
        &auto.ai.main_model
    } else {
        &auto.ai.summary_model
    };
    let sum_model_human = crate::providers::model_human_name(sum_model_id);
    let sum_display = crate::providers::find_by_id(sum_prov_id)
        .map(|p| p.display_name)
        .unwrap_or(sum_prov_id);

    let autoconfig_msg = format!(
        "**Autoconfig detected** \u{2014} applying unattended configuration:\n\n\
         **Main AI:** {main_display} \u{2014} {main_model_human}\n\
         **Summarizer:** {sum_display} \u{2014} {sum_model_human}\n\
         {api_key_lines}\n\
         **Keyboard:** {}\n\
         **Language:** {}\n\
         **Timezone:** {}\n\
         **Hostname:** {}\n\
         **Password:** ****\n\
         **Install to disk:** {}\n\
         **Assistant name:** {}",
        auto.system.keyboard,
        // ... (rest of args unchanged)
    );
```

- [ ] **Step 2: Add summarizer fallback (lines 449-456)**

Replace:
```rust
    if !auto.ai.summary_provider.is_empty() {
        let _ = config.set("llm.tts_summary_provider", serde_json::json!(auto.ai.summary_provider));
    }
    if !auto.ai.summary_model.is_empty() {
        let _ = config.set("llm.tts_summary_model", serde_json::json!(auto.ai.summary_model));
    }
```
with:
```rust
    if auto.ai.summary_provider.is_empty() {
        let _ = config.set("llm.tts_summary_provider", serde_json::json!(main_provider));
        let _ = config.set("llm.tts_summary_model", serde_json::json!(main_model));
    } else {
        let _ = config.set("llm.tts_summary_provider", serde_json::json!(auto.ai.summary_provider));
        let _ = config.set("llm.tts_summary_model", serde_json::json!(auto.ai.summary_model));
    }
```

Where `main_provider` and `main_model` are the values already computed earlier in the function for the main AI.

- [ ] **Step 3: Update autoconfig success message (line 467-481)**

Replace the format string to include Main AI + Summarizer:

```rust
        &format!(
            "**Autoconfig applied successfully!**\n\n\
             Vault created, API keys stored, system configured.\n\
             Main AI: **{main_display} \u{2014} {main_model_human}** | \
             Summarizer: **{sum_display} \u{2014} {sum_model_human}** | \
             Keyboard: **{}** | Mode: **{}**",
            auto.system.keyboard,
            if auto.install.enabled { "hard drive install" } else { "live ISO" },
        ),
```

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/first_boot_flow.rs
git commit -m "feat(autoconfig): show Main AI + Summarizer in messages; add summarizer fallback"
```

---

### Task 9: Title Bar — Fix Provider Dropdown + Settings Close Refresh

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/main_window.rs:300-310`
- Modify: `aios-app-rs/aios-gtk/src/ui/settings_dialog.rs:97-109`

- [ ] **Step 1: Fix title bar dropdown to select correct active provider**

The dropdown at line 300-310 already uses `available_providers` (now returning display names excluding Ollama thanks to Task 4). But it doesn't set the correct initial selection. After line 309 (`header.pack_start(&provider_dropdown)`), add:

```rust
    // Select the active provider in the dropdown.
    if let Some(active_display) = crate::providers::find_by_id(
        &aios_core::config::ConfigManager::default().get_str("llm.provider", "claude")
    ).map(|p| p.display_name) {
        if let Some(idx) = available_providers.iter().position(|n| *n == active_display) {
            provider_dropdown.set_selected(idx as u32);
        }
    }
```

Actually — the `build_main_window` function doesn't have access to config. The active provider selection needs to be passed in or done by the caller. The simpler approach: pass the active provider display name as a parameter. Add `active_provider: &str` to `build_main_window` signature, and use it:

```rust
    if let Some(idx) = provider_names.iter().position(|n| *n == active_provider) {
        provider_dropdown.set_selected(idx as u32);
    }
```

Update the caller in `app.rs` to pass it:
```rust
let active_provider = crate::providers::find_by_id(&config.get_str("llm.provider", "claude"))
    .map(|p| p.display_name)
    .unwrap_or("Claude");
```

- [ ] **Step 2: Add `on_close` callback to `show_settings`**

In `settings_dialog.rs`, change the signature at line 97:
```rust
pub fn show_settings(parent: &adw::ApplicationWindow, config: &ConfigManager, on_close: impl Fn() + 'static) {
```

After `dialog.present()` (line 109), add:
```rust
    dialog.connect_close_request(move |_| {
        on_close();
        gtk::glib::Propagation::Proceed
    });
```

- [ ] **Step 3: Update callers of `show_settings`**

Find all callers and add the closure. The main caller is in `first_boot_flow.rs` (settings button handler). Pass a closure that refreshes the title bar dropdown.

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/ui/main_window.rs aios-app-rs/aios-gtk/src/ui/settings_dialog.rs aios-app-rs/aios-gtk/src/app.rs aios-app-rs/aios-gtk/src/first_boot_flow.rs
git commit -m "feat(titlebar): fix provider dropdown selection; refresh on settings close"
```

---

### Task 10: Settings Dialog — Rewrite AI Page

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/settings_dialog.rs:116-306`

This is the largest change. Replace the entire `build_ai_page()` function with the new 3-section layout.

- [ ] **Step 1: Rewrite Section A — AI Model Assignment**

Replace the provider group + models group (lines 124-259) with two provider+model combo pairs for Main AI and Summarizer. Each pair:
- Provider `ComboRow` populated from `configured_display_names_excluding_ollama(config)`
- Model `ComboRow` populated from the selected provider's `models` list
- Provider change handler updates the model dropdown and writes config
- Model change handler writes config

- [ ] **Step 2: Rewrite Section B — API Keys**

Replace the API keys group (lines 148-205) with:
- Iterate configured providers only → create `ActionRow` per key with `mask_api_key()` subtitle + Delete button
- Delete button: insensitive if in use by Main AI or Summarizer, otherwise clears the key from config
- Inline add row: `ComboRow` for unconfigured providers + `PasswordEntryRow` + Save button

- [ ] **Step 3: Keep Section C — Custom AI Instructions**

The `system_prompt_row` code at lines 249-258 stays unchanged. Remove the old "AI Model Assignment" read-only section (lines 261-303).

- [ ] **Step 4: Verify the dialog opens**

Run: `cd aios-app-rs && cargo check`
Expected: No errors

- [ ] **Step 5: Run all tests**

Run: `cd aios-app-rs && cargo test`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add aios-app-rs/aios-gtk/src/ui/settings_dialog.rs
git commit -m "feat(settings): rewrite AI page — model assignment, API key management, custom instructions"
```

---

### Task 11: First-Boot Interactive — Add Summarizer Step

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs:61-76`
- Modify: `aios-app-rs/aios-gtk/src/first_boot_flow.rs` (SetupResult processing)

- [ ] **Step 1: Add fields to `SetupResult`**

At line 76 (before the closing `}`), add:
```rust
    /// The selected TTS summarizer provider id.
    pub summary_provider: String,
    /// The selected TTS summarizer model slug.
    pub summary_model: String,
```

- [ ] **Step 2: Add a setup step for Summarizer after provider selection**

In the `SetupStep` enum, add a `Summarizer` variant. After the user selects their main provider, show a card asking which provider/model to use for TTS summarization. If only one provider has a key, auto-complete this step with the same provider.

- [ ] **Step 3: Update `first_boot_flow.rs` to write summarizer config from SetupResult**

Where `SetupResult` is processed, add:
```rust
let _ = config.set("llm.tts_summary_provider", serde_json::json!(result.summary_provider));
let _ = config.set("llm.tts_summary_model", serde_json::json!(result.summary_model));
```

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/aios-gtk/src/ui/first_boot.rs aios-app-rs/aios-gtk/src/first_boot_flow.rs
git commit -m "feat(setup): add Summarizer provider/model selection to first-boot flow"
```

---

### Task 12: Refactor — Extract Shared Message Bubble Builder

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/chat_view.rs`

- [ ] **Step 1: Extract `build_message_bubble` helper**

Create a private function that both `add_message` and `add_assistant_message` call:

```rust
fn build_message_bubble(content: &str) -> gtk::Box {
    let bubble = gtk::Box::new(Orientation::Vertical, 4);
    bubble.add_css_class("message-bubble");

    let parts = split_code_blocks(content);
    for part in parts {
        match part {
            ContentPart::Text(text) => {
                if !text.is_empty() {
                    let pango = aios_core::types::to_pango(&text);
                    let label = gtk::Label::new(None);
                    label.set_markup(&pango);
                    label.set_wrap(true);
                    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    label.set_xalign(0.0);
                    label.set_selectable(true);
                    bubble.append(&label);
                }
            }
            ContentPart::Code(code) => {
                let frame = gtk::Frame::new(None);
                frame.add_css_class("code-block");
                let code_label = gtk::Label::new(Some(&code));
                code_label.set_wrap(true);
                code_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                code_label.set_xalign(0.0);
                code_label.set_selectable(true);
                frame.set_child(Some(&code_label));
                bubble.append(&frame);
            }
        }
    }

    bubble
}
```

- [ ] **Step 2: Refactor both methods to use it**

In `add_message`, replace the bubble building block (lines 216-250) with:
```rust
let bubble = build_message_bubble(content);
```

In `add_assistant_message`, replace the same block with:
```rust
let bubble = build_message_bubble(content);
```

- [ ] **Step 3: Run all tests**

Run: `cd aios-app-rs && cargo test`
Expected: All pass — no behavior change

- [ ] **Step 4: Commit**

```bash
git add aios-app-rs/aios-gtk/src/ui/chat_view.rs
git commit -m "refactor(chat): extract build_message_bubble to deduplicate add_message and add_assistant_message"
```

---

### Task 13: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cd aios-app-rs && cargo test`
Expected: All 563+ tests pass

- [ ] **Step 2: Run full build**

Run: `cd aios-app-rs && cargo build --release`
Expected: Clean build, no warnings

- [ ] **Step 3: Verify no regressions with `make dev`**

Run: `make dev`
Expected: check + test both pass
