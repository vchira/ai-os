# Web Server During First-Boot + Setup Improvements — Design Spec

## Problem

The web server only starts after first-boot setup completes (`activate_main()`). During first-boot (`run_first_boot_setup()`), the web channel is unavailable. The selftest reports "Web server not listening" as a FAIL.

## Requirements

1. Web server must ALWAYS start, including during first-boot setup
2. If someone switches to the web channel during setup, setup continues there
3. Audio test adapts per channel (Desktop tests hardware, Web tests browser audio)
4. API key fields pre-fill with values from config (baked from `.env` at build time)
5. If only one provider key is configured, skip provider choice and backup question
6. "OpenAI" → "ChatGPT" in all user-facing text
7. Summary INFO message at end of setup showing what was configured
8. Selftest reverted to FAIL for web server (it should always work now)

## Architecture

Extract web server + AppRuntime startup from `activate_main()` into a shared helper. Call it from both `run_first_boot_setup()` and `activate_main()`. The setup conversation sends messages through the channel system so web clients see setup progress.

## Setup Flow (revised)

```
activate()
  → vault exists?
    → YES: activate_main() (unchanged)
    → NO:
      1. Load config (already has API keys from .env)
      2. Create AppRuntime + ChannelSwitcher
      3. Start WebServer (port 80, fallback 8080)
      4. Register Desktop + Web channels
      5. Build main window + ChatView
      6. Show boot status
      7. Start SetupConversation (pre-fills keys from config)
      8. On complete → transition_to_normal_mode()
```

## Pre-filled API Keys

- `show_enter_api_key("claude")` reads `config.get_str("llm.claude_api_key", "")`
- If non-empty, sets it as the default value in the password entry field
- User clicks Next to accept, or clears and types a different key
- This is a general "default value" feature for input fields

## Provider Auto-Selection

- If only Claude key in config → auto-select Claude, skip ChooseProvider
- If only ChatGPT key → auto-select openai, skip ChooseProvider
- If both → show ChooseProvider as normal
- If neither → show ChooseProvider as normal

## Backup Provider

- After primary key entered, check if other provider has a key in config
- If yes → ask "Add ChatGPT as backup?" (or Claude) with key pre-filled
- If no → skip AddBackup entirely, go straight to Complete

## Branding

All user-facing text:
- "OpenAI" → "ChatGPT"
- Internal code stays `openai` (provider name, config keys, API)
- Display function: `"openai" => "ChatGPT"`

## Summary INFO Message

At setup completion, display `MessageLevel::Info`:

```
First-Boot Setup Complete

  ✅ Primary provider: Claude (API key stored)
  ✅ Master password: set
  ✅ Vault: created
  ✅ Audio (Desktop): TTS works, mic works
  ❌ Audio (Web): not tested — open http://aios.local to test
  ✅ Web Channel: available — http://aios.local
```

## Files to Modify

| File | Change |
|------|--------|
| `aios-gtk/src/app.rs` | Extract `start_web_server()` helper; call from `run_first_boot_setup()` |
| `aios-gtk/src/ui/first_boot.rs` | Pre-fill keys; auto-select provider; skip backup; ChatGPT branding; summary INFO |
| `distro/_inner_build.sh` | Revert selftest web check to FAIL |
