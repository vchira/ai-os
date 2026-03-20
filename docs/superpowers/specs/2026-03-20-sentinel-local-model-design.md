# Sentinel — Local Model for Security & Utility

**Date:** 2026-03-20
**Status:** Approved

## Summary

Replace the "Summarizer" concept with **Sentinel** — a mandatory local-only AI model that runs on every AI response for security sanitization and handles TTS summarization. Sentinel runs via Ollama (bundled in the ISO). No AI response reaches the user without Sentinel approval. Security first.

## Problem

1. The current sanitization uses pattern matching only — a smart AI can rephrase around fixed patterns.
2. TTS summarization currently uses a cloud model — sends response text to external APIs.
3. The "Summarizer" naming doesn't reflect its security role.
4. No local model infrastructure exists.

## Design

### Naming

All references to "Summarizer" are renamed to **Sentinel** throughout the codebase:

| Old | New |
|-----|-----|
| `llm.tts_summary_provider` | `llm.sentinel_model` |
| `llm.tts_summary_model` | removed (Sentinel is always local/Ollama) |
| "TTS Summarizer" (UI) | "Sentinel (local)" |
| Boot status "Summarizer" line | "Sentinel" line |
| `tts.rs` summarizer config | Uses Sentinel model |
| Autoconfig `summary_provider`/`summary_model` | `sentinel_model` |
| i18n `boot.status.summarizer` | `boot.status.sentinel` |

### Ollama Integration

**Bundled in ISO:**
- `/usr/bin/ollama` binary (~40MB) included in the live image via `_inner_build.sh`
- `ollama.service` systemd unit — started on demand by AiOS when needed
- Models stored at `/home/aios/.ollama/models/` (Ollama's default)

**Rust Client (`aios-llm/src/local.rs`):**
- `OllamaClient` struct wrapping `reqwest` HTTP calls to `http://localhost:11434`
- Methods:
  - `pull_model(name, progress_callback)` — download with progress reporting
  - `list_models()` — installed models
  - `delete_model(name)` — remove a model
  - `chat(model, messages)` — inference (OpenAI-compatible endpoint)
  - `is_running()` — check if Ollama daemon is up
  - `start()` — start Ollama via systemd if not running

**Inference for existing code:**
- Sentinel sanitization calls `OllamaClient::chat()` directly (simple yes/no query)
- Sentinel summarization calls `http://localhost:11434/v1/chat/completions` via the existing `OpenAIProvider::with_base_url()` — zero code change for TTS summarization
- Main AI (if user picks a local model) also uses `OpenAIProvider::with_base_url("http://localhost:11434", "ollama")`

### Model Selection Table

Shown in first boot, settings, and info popup. Hardware detection auto-selects default.

| Model | Download | RAM | Speed | Description | Recommended |
|-------|----------|-----|-------|-------------|-------------|
| Qwen 2.5 0.5B | 400MB | 1GB | Ultra fast | Minimal sanitization | < 2GB RAM |
| Llama 3.2 1B | 700MB | 2GB | Very fast | Good sanitization + basic summaries | 2-4GB RAM |
| Llama 3.2 3B | 2GB | 4GB | Fast | Strong sanitization + good summaries | **4-8GB RAM** |
| Phi-3 Mini 3.8B | 2.3GB | 4GB | Fast | Strong reasoning | Alternative |
| Llama 3.1 8B | 4.7GB | 8GB | Medium | Can serve as Main AI too | 8-16GB RAM |
| Mistral 7B | 4.1GB | 8GB | Medium | Good multilingual support | Alternative |
| Llama 3.1 70B | 40GB | 48GB | Slow | Full local AI replacement | > 48GB RAM |

**Hardware detection:**
- Read total RAM from `/proc/meminfo`
- Check for NVIDIA GPU via `nvidia-smi` or `/proc/driver/nvidia`
- Check for AMD GPU via `/sys/class/drm/card*/device/vendor`
- Recommend the largest model that fits in 50% of available RAM

### Info Icon (ⓘ)

A yellow/orange circle with "?" appears next to every model selection field:
- **Main AI** provider/model dropdowns
- **Sentinel** model dropdown

Clicking the icon opens a modal popup containing:
- The model table above
- Current hardware stats (RAM, GPU)
- The recommended model highlighted
- "Install" button for each model row
- Installed models show a checkmark

The same icon and popup are used consistently everywhere models are chosen.

### First Boot Flow

After provider setup (API keys entered):

1. **"Install Sentinel Model"** card — explains that Sentinel is required for security
2. Model table shown with hardware-recommended default pre-selected
3. User clicks "Download" (or the pre-selected default)
4. **Blocking modal dialog** appears:
   - Title: "Downloading Sentinel Model"
   - Subtitle: "Llama 3.2 3B (2GB)"
   - Progress bar (updated from Ollama's pull API streaming response)
   - "Cancel" button — returns to model table
   - Cannot be dismissed — must complete or cancel
5. Download completes → "Sentinel installed" confirmation
6. Setup continues to next step

**This step CANNOT be skipped.** Sentinel is mandatory for security.

### Settings Dialog

The "TTS Summarizer" section in Settings > AI becomes:

**Sentinel (Local Security Model)**
- Description: "Local AI model that checks every response for security violations and generates TTS summaries. Runs entirely on this device."
- Model dropdown (only installed local models) + ℹ️ info icon
- "Manage Models" button → opens model management popup (install/remove)
- Status indicator: "Running" / "Stopped" / "Not installed"

**Main AI section** also gets the ℹ️ info icon next to provider/model dropdowns.

### Sanitization Pipeline (Mandatory)

Every AI response goes through this pipeline before reaching the user:

```
AI Response
    ↓
[1. Pattern matching] — instant, catches obvious asks
    ↓ (if clean)
[2. Value leak scan] — instant, catches stored secrets in text
    ↓ (if clean)
[3. Sentinel model check] — ~0.5-2s depending on model/hardware
    Prompt: "Does this text ask the user to provide passwords,
    credentials, personal information, or any sensitive data
    directly in a chat message? Answer only YES or NO."
    ↓
    YES → Block response, show security notice
    NO  → Show response to user
```

**If Sentinel is not running or not installed:**
- AI responses are BLOCKED
- Chat shows: "⚠️ Sentinel model required. Install a local model in Settings > AI to enable chat."
- This ensures no response ever reaches the user without security checking

**Performance:**
- Step 1+2: <1ms
- Step 3: 200ms-2s depending on model size and hardware
- Total: noticeable but acceptable — security is worth the latency

### TTS Summarization

`tts.rs` changes:
- Instead of calling a cloud API for summarization, calls `http://localhost:11434/v1/chat/completions` with the Sentinel model
- Same prompt as before ("summarize in one sentence for TTS")
- Falls back to sentence truncation if Sentinel is unavailable (graceful degradation for TTS only — sanitization never degrades)

### Config

```json
{
  "llm": {
    "provider": "claude",
    "claude_model": "claude-sonnet-4-20250514",
    "sentinel_model": "llama3.2:3b"
  },
  "local": {
    "ollama_enabled": true,
    "installed_models": ["llama3.2:3b"]
  }
}
```

Autoconfig:
```json
{
  "ai": {
    "main_provider": "deepseek",
    "main_model": "deepseek-reasoner",
    "sentinel_model": "llama3.2:3b"
  }
}
```

### Boot Status

```
✓ Main AI: available — DeepSeek (deepseek-reasoner)
✓ Sentinel: available — llama3.2:3b (local, Ollama)
```

Or if not installed:
```
✗ Sentinel: unavailable — no local model installed (required for security)
```

### Files to Modify

| File | Changes |
|------|---------|
| `distro/_inner_build.sh` | Bundle Ollama binary + systemd service |
| `aios-llm/src/local.rs` | **New:** OllamaClient (pull, list, delete, chat, is_running, start) |
| `aios-llm/src/lib.rs` | Add `pub mod local;` |
| `aios-core/src/config/defaults.rs` | `tts_summary_*` → `sentinel_model`, add `local.ollama_enabled` |
| `aios-core/src/config/autoconfig.rs` | `summary_provider`/`summary_model` → `sentinel_model` |
| `aios-core/i18n/en.json` | `boot.status.summarizer` → `boot.status.sentinel` |
| `aios-core/i18n/de.json` | Same, DE: "Wächter" |
| `aios-gtk/src/boot_status.rs` | Summarizer line → Sentinel line |
| `aios-gtk/src/boot_context.rs` | Update finalize_boot for Sentinel |
| `aios-gtk/src/ui/settings_dialog.rs` | Rewrite Summarizer section → Sentinel section with info icon |
| `aios-gtk/src/ui/first_boot/provider.rs` | Add Sentinel model selection step (mandatory) |
| `aios-gtk/src/ui/first_boot/mod.rs` | Add SetupStep::SentinelModel |
| `aios-gtk/src/ui/model_info_popup.rs` | **New:** Model table popup with install/remove |
| `aios-gtk/src/ui/download_dialog.rs` | **New:** Blocking download modal with progress |
| `aios-gtk/src/tts.rs` | Use Sentinel model for summarization via Ollama |
| `aios-gtk/src/llm_handler.rs` | Add Sentinel sanitization check before showing responses |
| `aios-gtk/src/first_boot_flow.rs` | Update autoconfig sentinel_model handling |
| `aios-gtk/src/providers.rs` | Add Ollama to provider system for Main AI selection |
| `aios-gtk/src/app.rs` | Start Ollama service on boot, migrate config |

### Testing

- Unit: OllamaClient methods (mock HTTP responses)
- Unit: Hardware detection returns correct tier
- Unit: Sentinel sanitization prompt format
- Unit: Config migration from `tts_summary_*` to `sentinel_model`
- Integration: Ollama pull → model appears in list
- Security: Response blocked when Sentinel is unavailable
- Security: Sentinel correctly identifies "enter your password" prompts
- Security: Sentinel passes clean responses
- Security: Pattern matching + Sentinel combined pipeline
