# Provider & Model UI Overhaul

**Date:** 2026-03-19
**Status:** Approved

## Problem

Multiple UI surfaces display incorrect or incomplete provider/model information:

1. Settings dialog provider dropdown shows wrong active provider and raw IDs instead of display names.
2. Settings API Keys section shows all providers (even unconfigured), eye icon reveals placeholder dots instead of real key, no add/delete capability.
3. Settings Models section shows per-provider model dropdowns rather than the two assignments that matter (Main AI + Summarizer).
4. Title bar dropdown shows wrong active provider.
5. Boot status message shows only main provider, not summarizer.
6. Autoconfig detected message doesn't show selected models.
7. Chat messages don't show which model generated the response.

## Design

### 1. Settings Dialog — AI Page Restructure

The AI page is reorganized into three sections (top to bottom):

**Section A: AI Model Assignment** (new, top of page)

Two rows, each with a provider `ComboRow` and a model `ComboRow`:

- **Main AI**: Provider dropdown (only providers with configured API keys, excluding Ollama) + model dropdown (models for selected provider). Subtitle: "Used for conversations and tool calls".
- **TTS Summarizer**: Same structure, also excluding Ollama. Subtitle: "Generates one-sentence TTS summaries of long answers".

Both must always have explicit values. No "auto", no empty selection. Provider dropdowns show display names (e.g., "DeepSeek"), not raw IDs. Model dropdowns show human names (e.g., "DeepSeek Reasoner"), stored as slugs internally.

When Main AI provider changes, the model dropdown updates to show that provider's models. Same for Summarizer.

Config keys:
- `llm.provider` + `llm.{provider}_model` for Main AI (existing). The Main AI model is stored per-provider because each provider remembers its last-used model independently.
- `llm.tts_summary_provider` + `llm.tts_summary_model` for Summarizer. The Summarizer model slug is stored directly in `llm.tts_summary_model` (not per-provider), because only one model is active for summarization. This is independent of per-provider model keys — if DeepSeek is Main AI using `deepseek-reasoner` and Summarizer also uses DeepSeek but with `deepseek-chat`, both values coexist without conflict.

**Filtering Ollama from dropdowns:** Add a new helper `configured_display_names_excluding_ollama(config) -> Vec<String>` in `providers.rs` that filters out the "ollama" provider. Both the Main AI and Summarizer provider dropdowns, as well as the title bar dropdown, use this helper. The existing `configured_display_names()` remains unchanged for any other callers.

**Section B: API Keys** (middle)

Shows only configured providers (those with a non-empty API key):

Each configured key is an `ActionRow` showing:
- Title: provider display name (e.g., "Claude")
- Subtitle: masked key showing last 4 characters (e.g., `sk-ant-...lwAA`)
- Suffix: red "Delete" button

**Delete protection:** If the provider is currently assigned to Main AI or Summarizer, the Delete button is insensitive (grayed out) with a tooltip: "In use by Main AI" or "In use by Summarizer". The user must reassign the role first.

Below the configured keys, an **inline add row** (always visible):
- Provider `ComboRow` showing only unconfigured providers that need API keys (excluding Ollama)
- `PasswordEntryRow` for the key value
- "Save" button
- When saved, the row resets and the new key appears in the configured list above. The "Add" provider dropdown refreshes to exclude the newly configured provider.

Ollama toggle is **removed from the UI** for now (not a practical option yet). The `ProviderDef` and config entries remain in code.

**Section C: Custom AI Instructions** (bottom)

Unchanged — `EntryRow` for `llm.extra_system_prompt`.

**Removed from the old design:**
- "LLM Provider" combo row (replaced by Main AI provider in Section A)
- Per-provider model dropdowns (replaced by two assignment dropdowns in Section A)
- Per-provider `PasswordEntryRow` list (replaced by configured-only display in Section B)
- Ollama toggle (hidden until bundled)

### 2. Title Bar — Provider Dropdown

The provider dropdown in the header bar must:
- Show the display name of the active Main AI provider (not raw ID)
- Reflect the actual `llm.provider` config value
- Only list providers that have API keys configured, excluding Ollama (use `configured_display_names_excluding_ollama()`)
- Select the correct index matching the active provider on startup

When changed, it updates `llm.provider` and the Main AI model to that provider's current model from config.

**Settings sync:** `show_settings()` takes an additional `on_close: impl Fn() + 'static` callback parameter. Inside `show_settings()`, connect to the dialog's `connect_close_request` signal (matching the existing codebase pattern used in `auth_dialog.rs` and `panel_renderer.rs`). The callback re-reads config and refreshes the title bar provider dropdown model and selected index. The caller in `first_boot_flow.rs` (settings button handler) passes a closure that calls `update_provider_dropdown()` on the window.

### 3. Boot Status Message

Replace the single "LLM Provider" line and "Backup" lines with two explicit lines:

```
✓ Main AI: available — DeepSeek (deepseek-reasoner)
✓ Summarizer: available — Groq (llama-3.1-8b-instant)
```

Each line checks its respective provider's API key. If not configured:
```
✗ Main AI: unavailable — no API key
```

New i18n keys required (in both `aios-core/i18n/en.json` and `aios-core/i18n/de.json`):
- `boot.status.main_ai` → EN: "Main AI", DE: "Haupt-KI"
- `boot.status.summarizer` → EN: "Summarizer", DE: "Zusammenfasser"

Remove old i18n keys: `boot.status.llm_provider`, `boot.status.backup`, `boot.status.backup_available`, `boot.status.no_api_key_hint`.

Remove all "Backup: {provider} available" lines — they're no longer meaningful since both roles have explicit assignments.

### 4. Autoconfig Detected Message

Replace the `**Provider:** {primary}` line with two lines at the same position (before API key lines):

```
**Main AI:** DeepSeek — DeepSeek Reasoner
**Summarizer:** Groq — Llama 3.1 8B
```

Both use human-readable model names (from `model_human_name()` helper), consistent with chat attribution. The format string in `first_boot_flow.rs` line 368-394 changes from:

```
"**Provider:** {}\n{api_key_lines}\n..."
```
to:
```
"**Main AI:** {} — {}\n**Summarizer:** {} — {}\n{api_key_lines}\n..."
```

Where the values are `(main_provider_display, main_model_human, summary_provider_display, summary_model_human)`.

Also update the **autoconfig success message** at `first_boot_flow.rs` line 469-481. Change from:
```
"Provider: **{}** | Keyboard: **{}** | Mode: **{}**"
```
to:
```
"Main AI: **{} — {}** | Summarizer: **{} — {}** | Keyboard: **{}** | Mode: **{}**"
```

### 5. Chat Message Model Attribution

The assistant role label changes from:
```
Assistant
```
to:
```
Assistant — {Model Human Name}
```

Where `{Model Human Name}` is the human-readable name from `PROVIDERS` (e.g., "DeepSeek Reasoner"), looked up from the current model slug.

When the summarizer is a different provider/model from the main AI, always append it:
```
Assistant — DeepSeek Reasoner (Llama 3.1 8B)
```

This is a **static label** determined at display time from config, not tracked per-message. If `llm.tts_summary_provider` != `llm.provider` or `llm.tts_summary_model` differs from the main model, the summarizer name is appended.

**Trade-off note:** The summarizer label appears on ALL assistant messages when the summarizer is configured differently from the main AI, even short responses where TTS summarization does not actually run (responses <= 300 chars). This is intentional: it communicates the system configuration rather than per-message behavior, avoiding the complexity of async per-message tracking. The label means "this is the model pair in use" not "summarization happened for this specific message."

**Implementation approach:** Add a new method `add_assistant_message(content: &str, model_label: &str)` to `ChatView`. This avoids changing the signature of `add_message()` which has 78 call sites across 9 files. The new method internally calls the existing rendering logic but prepends the model label to the role display: `"{ASSISTANT_NAME} — {model_label}"`.

Both `send_to_llm()` and `handle_remote_llm_message()` in `llm_handler.rs` must use `add_assistant_message()` instead of `add_message("assistant", ...)`. The label is built by:
1. Reading `llm.provider` and `llm.{provider}_model` from config snapshot
2. Looking up the human name via `model_human_name(slug)`
3. If summarizer differs from main, also looking up summarizer human name and appending in parentheses
4. Passing the composed label to `add_assistant_message()`

This requires:
- A new helper in `providers.rs`: `model_human_name(slug: &str) -> String` that searches all providers' model lists for the slug and returns the human name, falling back to the slug itself if not found.
- A new method `add_assistant_message()` in `chat_view.rs`.
- Both LLM handler paths compose the label from config state at call time.

### 6. TTS Summarizer Backend Fix

The current `tts.rs` has the summarizer hardcoded:
- Line 104: model `"claude-haiku-4-5-20251001"` hardcoded
- Line 122-128: API URL `https://api.anthropic.com/v1/messages` hardcoded
- Line 199: reads `llm.claude_api_key` hardcoded

This must be changed to use the configured summarizer:
1. `speak_if_enabled_with_signal()` reads `llm.tts_summary_provider` and `llm.tts_summary_model` from config
2. Looks up the provider in PROVIDERS to get the `api_key_config` key
3. Reads the correct API key from config
4. `summarize_for_tts()` signature changes to accept a struct:
   ```rust
   pub(crate) struct SummarizerConfig {
       pub api_key: String,
       pub model: String,
       pub provider_id: String,
   }
   ```
5. **Protocol determination:** Use `provider_id` to decide the HTTP request format:
   - If `provider_id == "claude"`: use Anthropic protocol (x-api-key header, anthropic-version header, `https://api.anthropic.com/v1/messages`, Anthropic message format)
   - Otherwise: use OpenAI-compatible protocol (Authorization: Bearer header, provider's base URL from a lookup, OpenAI chat completions format)

   Add a helper `provider_api_url(provider_id: &str) -> &str` in `providers.rs` that returns the **base** URL for each provider (same URLs used by `OpenAIProvider::with_base_url()` constructors). The caller appends `/v1/chat/completions` for OpenAI-compatible providers, or `/v1/messages` for Claude. Note that Gemini uses `https://generativelanguage.googleapis.com/v1beta/openai` as its base URL (an OpenAI-compatible shim). Both Anthropic and OpenAI-compatible protocol branches use `reqwest::blocking::Client` since `summarize_for_tts()` runs in a background thread, not an async context.

### 7. First-Boot / Autoconfig Changes

**Config migration (startup):** On app startup, before building boot status, check if `llm.tts_summary_provider == "auto"` or `llm.tts_summary_model == "auto"`. If so, replace with the current main provider and model:
```rust
let summary_prov = config.get_str("llm.tts_summary_provider", "");
if summary_prov == "auto" || summary_prov.is_empty() {
    let main = config.get_str("llm.provider", "claude");
    config.set("llm.tts_summary_provider", json!(main));
    let model = config.get_str(&format!("llm.{main}_model"), "");
    config.set("llm.tts_summary_model", json!(model));
}
```
This handles existing installations that have "auto" stored in their config.json.

**Autoconfig flow** (`apply_autoconfig()` in `first_boot_flow.rs`):
- Always set both `llm.provider` + model AND `llm.tts_summary_provider` + `llm.tts_summary_model` to explicit values
- Never write "auto" for the summarizer
- If `auto.ai.summary_provider` is empty, explicitly write the fallback values (same as main provider/model) instead of skipping the write:
  ```rust
  if auto.ai.summary_provider.is_empty() {
      config.set("llm.tts_summary_provider", json!(main_provider));
      config.set("llm.tts_summary_model", json!(main_model));
  } else {
      config.set("llm.tts_summary_provider", json!(auto.ai.summary_provider));
      config.set("llm.tts_summary_model", json!(auto.ai.summary_model));
  }
  ```

**First-boot interactive flow** (`first_boot.rs`):
- After the user selects their primary provider and enters API keys, add a setup step for the Summarizer provider+model
- If only one provider has a key, both Main AI and Summarizer default to that provider's cheapest model (first in the models list) and the step is auto-completed
- Add fields to `SetupResult`: `summary_provider: String` and `summary_model: String`
- The `first_boot_flow.rs` code that processes `SetupResult` writes these to config

**Defaults change** (`defaults.rs`):
- Change `"tts_summary_provider": "auto"` to `"tts_summary_provider": "claude"`
- Change `"tts_summary_model": "auto"` to `"tts_summary_model": "claude-sonnet-4-20250514"`
- Combined with the startup migration, this handles both fresh installs and upgrades

## Data Flow

### Config → UI (on open)

1. Read `llm.provider` → find in PROVIDERS → set Main AI provider dropdown index
2. Read `llm.{provider}_model` → find in provider's models → set Main AI model dropdown index
3. Read `llm.tts_summary_provider` → find in PROVIDERS → set Summarizer provider dropdown index
4. Read `llm.tts_summary_model` → find in that provider's models → set Summarizer model dropdown index
5. Read all `llm.{id}_api_key` for providers with `needs_api_key` → filter non-empty → build configured keys list
6. Remaining unconfigured providers with `needs_api_key` (excluding Ollama) → populate "Add" dropdown

### UI → Config (on change)

1. Main AI provider dropdown changes → write `llm.provider`, update model dropdown to show that provider's models, write `llm.{provider}_model` (use provider's default if no prior selection)
2. Main AI model dropdown changes → write `llm.{provider}_model`
3. Summarizer provider dropdown changes → write `llm.tts_summary_provider`, update model dropdown, write `llm.tts_summary_model` (use provider's cheapest/default model)
4. Summarizer model dropdown changes → write `llm.tts_summary_model`
5. Delete key button → clear `llm.{id}_api_key`, refresh configured keys list, refresh "Add" dropdown. Button is insensitive if provider is in use by Main AI or Summarizer.
6. Add key saved → write `llm.{id}_api_key`, refresh configured keys list, refresh "Add" dropdown, refresh Main AI and Summarizer provider dropdowns (new provider now available)

### Model Attribution Flow

1. LLM handler gets response from provider (both `send_to_llm` and `handle_remote_llm_message`)
2. Build the model label:
   a. Read `llm.provider` and `llm.{provider}_model` from config snapshot
   b. Call `model_human_name(slug)` to get human name (e.g., "DeepSeek Reasoner")
   c. Read `llm.tts_summary_provider` and `llm.tts_summary_model`
   d. If summarizer differs from main (different provider or different model), call `model_human_name()` for summarizer and compose: `"DeepSeek Reasoner (Llama 3.1 8B)"`
   e. If same, just: `"DeepSeek Reasoner"`
3. Call `add_assistant_message(&content, &model_label)` (new method, not `add_message`)

### Settings Close → Title Bar Refresh

1. Settings dialog close request fires (`connect_close_request` on `adw::PreferencesWindow`)
2. The `on_close` callback fires:
   a. Re-read `llm.provider` from config
   b. Rebuild provider dropdown model from `configured_display_names_excluding_ollama()`
   c. Set selected index to match new active provider

## Files to Modify

| File | Changes |
|------|---------|
| `aios-gtk/src/ui/settings_dialog.rs` | Rewrite `build_ai_page()`: new 3-section layout with model assignment, API key management, custom instructions |
| `aios-gtk/src/ui/main_window.rs` | Fix provider dropdown to use display names via `configured_display_names_excluding_ollama()`, select correct active provider; add refresh callback on settings close |
| `aios-gtk/src/boot_status.rs` | Replace LLM Provider + Backup lines with Main AI + Summarizer lines; use new i18n keys |
| `aios-gtk/src/first_boot_flow.rs` | Update autoconfig message format (Main AI + Summarizer instead of Provider); add fallback logic for empty summarizer; write both to config always |
| `aios-gtk/src/ui/first_boot.rs` | Add Summarizer selection step to interactive setup; add `summary_provider`/`summary_model` to `SetupResult` |
| `aios-gtk/src/tts.rs` | Replace hardcoded Claude Haiku with configurable summarizer: read provider/model/key from config, use correct protocol (Anthropic vs OpenAI-compatible) |
| `aios-gtk/src/llm_handler.rs` | Both `send_to_llm()` and `handle_remote_llm_message()`: build model label from config and call `add_assistant_message()` |
| `aios-gtk/src/ui/chat_view.rs` | Add `add_assistant_message(content, model_label)` method; render role as `"{ASSISTANT_NAME} — {model_label}"` |
| `aios-gtk/src/providers.rs` | Add `model_human_name()`, `mask_api_key()`, `configured_display_names_excluding_ollama()`, `provider_api_url()` helpers |
| `aios-core/src/config/defaults.rs` | Change `tts_summary_provider` from "auto" to "claude"; change `tts_summary_model` from "auto" to "claude-sonnet-4-20250514" |
| `aios-core/i18n/en.json` | Add keys: `boot.status.main_ai`, `boot.status.summarizer`; remove old LLM provider keys |
| `aios-core/i18n/de.json` | Same keys with German translations |
| `aios-gtk/src/app.rs` | Add startup migration for "auto" summary provider; change `available_providers()` to use `configured_display_names_excluding_ollama()` |

## Testing

- Unit test: `model_human_name("deepseek-reasoner")` returns "DeepSeek Reasoner"; unknown slug returns itself
- Unit test: `model_human_name("claude-sonnet-4-20250514")` returns "Claude Sonnet 4"
- Unit test: `mask_api_key("sk-ant-api03-tjiIBa...lwAA")` returns `"sk-ant-...lwAA"` (prefix + last 4)
- Unit test: `mask_api_key("gsk_qBE9...6xcR")` returns `"gsk_...6xcR"`
- Unit test: `mask_api_key("")` returns empty string; `mask_api_key("abc")` returns `"...abc"` (short keys)
- Unit test: boot status with deepseek as main, groq as summarizer → output contains "Main AI" line with deepseek and "Summarizer" line with groq
- Unit test: boot status with no summarizer API key → "Summarizer: unavailable" line
- Unit test: autoconfig without `summary_provider` → defaults summarizer to same as main provider
- Unit test: autoconfig with `summary_provider` → sets both correctly
- Unit test: `configured_display_names_excluding_ollama()` never returns "Ollama" even when enabled
- Unit test: title bar dropdown reflects `llm.provider` config value using display name
- Unit test: startup migration converts "auto" summary provider to main provider values
- Unit test: `provider_api_url("claude")` returns Anthropic URL; `provider_api_url("deepseek")` returns DeepSeek URL
- Integration: settings dialog opens without panic with various config states (no keys, one key, all keys)
- Integration: deleting API key in use → button is insensitive, no deletion occurs
