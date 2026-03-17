# i18n Framework — Design Spec

## Problem

All AiOS UI strings are hardcoded in English. The OS needs full multilanguage support so the installer and first-boot setup can run in the user's language. LLM responses already adapt to the user's language — only OS UI strings need translation.

## Requirements

1. All UI strings (setup cards, system messages, command outputs, labels, errors, web client) must be translatable
2. Support all languages that espeak-ng supports (~25+)
3. English is the source of truth; other translations generated via LLM at dev time
4. Translation files are JSON, compiled into the binary via `include_str!`
5. Language auto-detected from system locale (Desktop) or `Accept-Language` header (Web)
6. Changeable at runtime via the welcome card language dropdown and `/language` command
7. `assistant.language` config key overrides auto-detection

## Translation System

### File Format

Simple key-value JSON per language:

```
i18n/en.json
i18n/de.json
i18n/fr.json
...
```

Example `en.json`:
```json
{
  "welcome.title": "Welcome to AiOS!",
  "welcome.subtitle": "I'm your AI assistant. Let's set up your system together.",
  "setup.audio.title": "Test Audio Output",
  "setup.audio.description": "Let's check if you can hear me.",
  "setup.audio.yes": "Yes, I can hear",
  "setup.audio.no": "No audio / Skip",
  "setup.name.title": "Name Your Assistant",
  "setup.name.description": "Choose a name for your AI assistant.",
  "setup.provider.title": "Choose Your AI Provider",
  "setup.password.title": "Secure Your Data",
  "setup.complete.title": "First-Boot Setup Complete",
  "chat.role.you": "You",
  "chat.role.system": "System",
  "chat.role.tool": "Tool",
  "cmd.help.title": "Available Commands",
  "boot.status.desktop": "GTK4/libadwaita",
  "boot.status.web_channel": "Web Channel",
  "error.invalid_hostname": "Invalid hostname: use lowercase letters, numbers, hyphens",
  "install.select_drive": "Select a drive to install AiOS:",
  "install.complete": "Installation complete! Remove the USB drive and press Reboot.",
  "install.reboot": "Reboot"
}
```

### Nested Keys Convention

- `setup.*` — first-boot setup strings
- `install.*` — hard drive installer strings
- `cmd.*` — command output strings
- `chat.*` — chat UI strings
- `boot.*` — boot status strings
- `error.*` — error messages
- `web.*` — web client specific strings

## Rust API

```rust
// aios-core/src/i18n.rs

/// Get a translated string by key.
pub fn t(key: &str) -> String

/// Get a translated string with interpolation.
/// Replaces {name} placeholders with provided values.
pub fn t_fmt(key: &str, args: &[(&str, &str)]) -> String

/// Set the current language (e.g. "de", "fr", "en").
pub fn set_language(lang: &str)

/// Get the current language code.
pub fn current_language() -> String

/// Get list of available languages.
pub fn available_languages() -> Vec<(String, String)>  // (code, native name)

/// Auto-detect language from system locale.
pub fn detect_system_language() -> String
```

### Usage in Rust

```rust
use aios_core::i18n::t;

chat_view.add_message("system", &t("welcome.subtitle"));
chat_view.add_message("system", &t_fmt("install.space_question", &[("size", "250GB")]));
```

## JavaScript API (Web Client)

```javascript
let translations = {};
let currentLang = 'en';

function t(key) {
    return translations[currentLang]?.[key] || translations['en']?.[key] || key;
}

function tFmt(key, args) {
    let s = t(key);
    for (const [k, v] of Object.entries(args)) {
        s = s.replace(`{${k}}`, v);
    }
    return s;
}
```

Translations are sent to the web client as a `ServerMessage::System` with the full language JSON on connect, or as a new `ServerMessage::Config` variant.

## Language Detection

| Channel | Detection | Fallback |
|---------|-----------|----------|
| Desktop (GTK) | `$LANG` env / `/etc/default/locale` | English |
| Web | `Accept-Language` HTTP header | English |
| Config override | `assistant.language` in config | Auto-detect |

### Welcome Card Language Selector

The welcome card (first thing shown in setup) includes a language dropdown. Auto-detected language is pre-selected. User can switch before proceeding. Everything from that point renders in the chosen language.

## What Gets Translated

- First-boot setup (all cards, buttons, descriptions, TTS text)
- System messages
- Boot status labels
- Command outputs (`/help`, `/info`, `/selftest`, etc.)
- Error messages
- Web client UI (placeholder text, button labels, role labels)
- Installer conversation (when built)

## What Does NOT Get Translated

- LLM responses (handles its own language)
- Log messages (always English for debugging)
- Config keys and internal identifiers
- Developer-facing error messages

## Translation Generation

A dev script generates translations from the English source:

```bash
scripts/generate-translations.sh
```

- Reads `i18n/en.json` as source
- For each target language, sends the JSON to the LLM with instructions to translate values only
- Writes output to `i18n/<lang>.json`
- Generated files are committed to the repo and reviewed
- Re-run when new strings are added

## Supported Languages

All languages supported by espeak-ng (~25+):
English, German, French, Spanish, Italian, Portuguese, Romanian, Dutch, Polish, Czech, Hungarian, Swedish, Norwegian, Danish, Finnish, Greek, Turkish, Arabic, Hindi, Japanese, Chinese (Simplified), Korean, Russian, Ukrainian, Indonesian

## Files to Create/Modify

| File | Purpose |
|------|---------|
| `aios-core/src/i18n.rs` | New: translation loading, `t()`, `t_fmt()`, language detection |
| `aios-core/src/i18n/en.json` | English source translations |
| `aios-core/src/i18n/de.json` | German translations (manually reviewed) |
| `aios-core/src/i18n/*.json` | All other language files (LLM-generated) |
| `aios-core/src/lib.rs` | Add `pub mod i18n;` |
| `aios-gtk/src/ui/first_boot.rs` | Replace hardcoded strings with `t()` calls |
| `aios-gtk/src/ui/chat_view.rs` | Role labels via `t()` |
| `aios-gtk/src/app.rs` | Language detection at startup, boot status via `t()` |
| `aios-web/src/static/index.html` | JS `t()` function, translations loading, language detection |
| `aios-web/src/server.rs` | Send translations to web client on connect |
| `aios-core/src/config/commands.rs` | Command outputs via `t()` |
| `aios-core/src/config/defaults.rs` | Add `assistant.language` default |
| `scripts/generate-translations.sh` | New: LLM-powered translation generator |
