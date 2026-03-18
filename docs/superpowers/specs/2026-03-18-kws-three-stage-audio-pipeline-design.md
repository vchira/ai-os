# KWS Integration: Three-Stage Audio Pipeline

**Date:** 2026-03-18
**Status:** Approved
**Scope:** Replace full-Whisper wake word detection with a lightweight ONNX-based Keyword Spotting (KWS) engine, adding on-device custom model training.

## Problem

The current voice pipeline runs Whisper STT on every detected speech segment, then checks the transcription text for the wake phrase. This means expensive full transcription fires on all ambient speech (TV, other people, background noise) even when it's not directed at AiOS. CPU and latency are wasted on utterances that are immediately discarded.

## Solution

A three-stage audio pipeline where a lightweight KWS model (~1-3% CPU) filters speech before the expensive Whisper STT stage. Whisper only runs when the wake word is confirmed, reducing CPU usage by ~90% during idle listening.

## Pipeline Architecture

```
Stage 1 (VAD)     Stage 2 (KWS)           Stage 3 (STT)
Audio ──> RMS ──> ONNX wake model ──> Whisper (burst) ──> LLM
          ~0.1%   ~1-3% CPU              only on wake match
```

### Stage 1: VAD (existing)
- RMS energy-based Voice Activity Detection
- 480-sample frames (30ms at 16kHz)
- Filters silence, passes speech segments to Stage 2
- No changes needed

### Stage 2: KWS (new)
- ONNX-based openWakeWord inference via `ort` crate
- Processes 80ms audio chunks (1280 samples at 16kHz)
- Three infrastructure models loaded once at startup:
  - `embedding_model.onnx` (1.3 MB) — audio feature extraction
  - `melspectrogram.onnx` (1.0 MB) — mel spectrogram computation
  - `silero_vad.onnx` (1.8 MB) — openWakeWord's internal VAD
- One active wake word model loaded (~200KB-1.2MB)
- Returns confidence score (0.0-1.0); threshold default: 0.5
- On match: transition to capture mode for Stage 3

### Stage 3: STT (existing)
- Whisper via whisper-cpp-cli
- Only invoked after KWS confirms wake word
- Transcribes the command that follows the wake word
- No changes to the Whisper integration itself

## Voice Listener State Machine

The `start_voice_listener()` thread operates as a two-state machine:

### Idle Mode (waiting for wake word)
1. VAD detects speech — accumulate audio into buffer
2. Feed 80ms chunks to `KwsEngine::process_audio()` in real-time
3. If confidence > threshold — transition to Capture Mode
4. If speech ends without wake match — discard buffer, stay idle

### Capture Mode (recording the command)
1. Wake word detected — start a new buffer for command audio
2. Skip `post_wake_delay_ms` (300ms) to avoid wake word tail
3. VAD monitors command speech — accumulate into command buffer
4. Circular buffer: keep rolling 2-second audio buffer so we don't cut off the first word if the user speaks without pausing after the wake word
5. When VAD detects speech end — send command buffer to Whisper
6. Send transcribed text to GTK thread via `stt_tx` channel
7. Return to Idle Mode

### Timeouts
- Capture mode max duration: 15 seconds — force-transcribe and return to idle
- Silence timeout in capture mode: 1500ms — end capture, transcribe

### Fallback
If wake word is enabled but no KWS model is loaded (custom training in progress), fall back to the existing Whisper + keyword match approach. The system always works — just less efficiently.

## KWS Engine (`aios-voice`)

### New Files

| File | Purpose |
|------|---------|
| `aios-voice/src/wake/kws.rs` | ONNX-based KWS engine |
| `aios-voice/src/wake/trainer.rs` | On-device custom model training |

### `KwsEngine` Struct

```rust
pub struct KwsEngine {
    // Infrastructure models (loaded once)
    embedding_session: ort::Session,
    melspec_session: ort::Session,
    vad_session: ort::Session,

    // Active wake word model
    wake_session: Option<ort::Session>,
    wake_word: String,
    threshold: f32,

    // Internal state for rolling prediction
    audio_buffer: Vec<f32>,
    embedding_buffer: Vec<Vec<f32>>,
}

impl KwsEngine {
    pub fn new(models_dir: &Path) -> Result<Self>;
    pub fn load_wake_model(&mut self, model_path: &Path, wake_word: &str) -> Result<()>;
    pub fn process_audio(&mut self, samples: &[f32]) -> Option<f32>;
    pub fn reset(&mut self);
    pub fn wake_word(&self) -> &str;
    pub fn set_threshold(&mut self, threshold: f32);
}
```

Key details:
- `process_audio()` accepts raw f32 mono 16kHz samples of any length
- Internally buffers to 80ms chunks, runs melspectrogram → embedding → wake model
- Returns `Some(confidence)` when confidence > threshold, `None` otherwise
- `reset()` clears internal buffers (called after wake detection or speech end)
- Thread-safe via `Arc<Mutex<KwsEngine>>` in the voice listener

### `KwsTrainer` Struct

```rust
pub struct KwsTrainer;

impl KwsTrainer {
    pub fn train(phrase: &str, output_dir: &Path) -> Result<PathBuf>;
    pub fn is_available() -> bool;
}
```

Key details:
- `train()` spawns a Python subprocess: `/opt/aios-app/kws-trainer/train.py --phrase "hey assistant" --output ~/.aios/models/kws/custom/`
- The Python script uses openWakeWord's automatic training pipeline with Piper TTS for synthetic data generation
- Returns the path to the produced `.onnx` model file
- `is_available()` checks if the Python training environment exists
- Training takes ~5-10 minutes on CPU, runs in a background thread

## Pre-trained Models

### Shipped in ISO (17 models)

| Wake Word | Source | Size |
|-----------|--------|------|
| hey_assistant | Custom-trained (default) | ~200 KB |
| hey_jarvis | Official openWakeWord | 1.2 MB |
| computer | Community (HA collection) | 200 KB |
| ok_computer | Community | 200 KB |
| hey_friday | Community | 200 KB |
| jarvis | Community | 200 KB |
| ok_jarvis | Community | 200 KB |
| skynet | Community | 200 KB |
| terminator | Community | 200 KB |
| hey_house | Community | 200 KB |
| ok_home | Community | 200 KB |
| home_assistant | Community | 200 KB |
| mr_anderson | Community | 200 KB |
| mr_smith | Community | 200 KB |
| hey_dick_head | Community | 200 KB |
| oi_fuckwhit | Community | 200 KB |
| yo_homie | Community | 200 KB |

### Infrastructure Models (3)

| Model | Size | Purpose |
|-------|------|---------|
| embedding_model.onnx | 1.3 MB | Audio feature extraction |
| melspectrogram.onnx | 1.0 MB | Mel spectrogram computation |
| silero_vad.onnx | 1.8 MB | openWakeWord internal VAD |

**Total ISO footprint: ~7 MB**

### Model Storage

```
~/.aios/models/kws/
  infrastructure/
    embedding_model.onnx
    melspectrogram.onnx
    silero_vad.onnx
  pretrained/
    hey_assistant.onnx
    hey_jarvis.onnx
    computer.onnx
    ... (17 total)
  custom/
    (user-trained models appear here)
```

Baked into ISO at `/opt/aios-app/models/kws/`. Copied to `~/.aios/models/kws/` on first boot.

## First-Boot Setup Integration

### Wake Word Selection UI

When the user reaches the "Name your assistant" step, the `ui_panel` shows:

1. **Dropdown: "Choose a wake word"** — lists all 17 pre-trained wake words. Label: "High accuracy, works immediately"
2. **Text field: "Or type a custom wake phrase"** — subtitle: "A custom model will be trained on first boot. This takes a few minutes and may be less accurate."

The dropdown and custom field are mutually exclusive — picking one clears the other.

### Custom Wake Word Training on First Boot

1. User types a custom phrase during setup (or it's set in `autoconfig.json`)
2. Setup completes, main chat loads
3. System message: `[SYSTEM] Training custom wake word "hey assistant"... this takes a few minutes.`
4. **Mic button is disabled** (grayed out) during training
5. If user clicks disabled mic button: tooltip says "Wake word is still training... please wait."
6. Training runs in background thread via `KwsTrainer::train()`
7. On completion: system message `[SYSTEM] Wake word "hey assistant" is ready!`
8. Mic button **automatically enables**
9. KWS model hot-swapped into voice listener via `Arc<Mutex<KwsEngine>>`

### Pre-trained Wake Word on First Boot

If user picks a pre-trained wake word — no training, mic enabled immediately.

### Autoconfig Integration

`autoconfig.json` field `system.wake_word` (existing). The system checks:
- Is the value a pre-trained model name? → load `.onnx` directly, mic enabled
- Otherwise → trigger background training, mic disabled until done

## Config Keys

```json
"voice": {
    "wake_word": "hey assistant",
    "wake_enabled": true,
    "wake_word_source": "pretrained",
    "wake_threshold": 0.5
}
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `wake_word` | string | "hey assistant" | Active wake phrase |
| `wake_enabled` | bool | true | Whether KWS filtering is active |
| `wake_word_source` | string | "pretrained" | "pretrained", "custom", or "training" |
| `wake_threshold` | f32 | 0.5 | KWS confidence threshold (0.0-1.0) |

## `/wake` Command

| Command | Behavior |
|---------|----------|
| `/wake` | Show current wake word, source, and status |
| `/wake <phrase>` | If pre-trained → load immediately. Otherwise → start background training |
| `/wake on` | Enable wake word detection |
| `/wake off` | Disable — all speech goes directly to Whisper |
| `/wake list` | Show all available pre-trained wake words |
| `/wake train <phrase>` | Force (re)train a custom model |
| `/wake threshold <0.0-1.0>` | Adjust KWS confidence threshold |

## Settings Dialog Changes

New **Wake Word** group in the Voice page (between STT and TTS groups):

| Row | Widget | Description |
|-----|--------|-------------|
| Wake Word Enabled | Switch | Toggle wake detection on/off |
| Pre-trained Wake Word | ComboRow | Dropdown with 17 pre-trained options. Label: "High accuracy" |
| Custom Wake Phrase | EntryRow | Subtitle: "Custom phrases require training (~5 min) and may be less accurate" |
| Train | Button | Starts training for the custom field value |
| Status | Label | "Active", "Training...", or "Ready" |
| Threshold | SpinButton (0.1-1.0) | KWS confidence threshold |

Selecting a pre-trained wake word clears the custom field and hot-swaps the model immediately. Training a custom phrase disables the dropdown until training completes.

## Mic Button Behavior During Training

- `wake_training_in_progress: Arc<AtomicBool>` — new shared flag
- When true: mic button is disabled (grayed out, not clickable)
- Clicking disabled button shows: "Wake word is still training... please wait."
- When training completes: flag set to false, mic button auto-enables, voice listener starts

## Dependencies

### Rust (aios-voice)
- `ort` crate — ONNX Runtime Rust bindings (new dependency)
- Links to `libonnxruntime` native library

### Python (training only)
- `openwakeword`, `torch`, `piper-sample-generator`, `speechbrain`, `onnxruntime`
- Bundled at `/opt/aios-app/kws-trainer/` with virtualenv
- Not loaded at runtime — only invoked for custom model training

### ISO Build
- `libonnxruntime` / `libonnxruntime-dev` for Rust compilation and runtime
- Download 3 infrastructure models from openWakeWord v0.5.1 release
- Download 16 community models from `fwartner/home-assistant-wakewords-collection`
- Download 1 official model (hey_jarvis) from openWakeWord v0.5.1 release
- Train `hey_assistant.onnx` as part of the build process
- Bundle Python training environment

## Existing Code Changes

| File | Change |
|------|--------|
| `aios-voice/Cargo.toml` | Add `ort` dependency |
| `aios-voice/src/wake/mod.rs` | Add `pub mod kws; pub mod trainer;` exports |
| `aios-voice/src/wake/detector.rs` | Keep as fallback for custom wake words during training |
| `aios-voice/src/lib.rs` | Export new `KwsEngine`, `KwsTrainer` types |
| `aios-gtk/src/app.rs` | Rewrite `start_voice_listener()` to use KwsEngine state machine |
| `aios-gtk/src/app.rs` | Add `wake_training_in_progress` flag, mic button disable logic |
| `aios-gtk/src/ui/settings_dialog.rs` | Add Wake Word group to Voice page |
| `aios-core/src/config/defaults.rs` | Add `wake_word_source`, `wake_threshold` defaults |
| `aios-core/src/config/commands.rs` | Extend `/wake` with `list`, `train`, `threshold` subcommands |
| `distro/_inner_build.sh` | Download KWS models, bundle training env |
| `distro/Dockerfile` | Add `libonnxruntime-dev` build dependency |

## Testing

- Unit tests for `KwsEngine`: load infrastructure models, load wake model, process synthetic audio, verify confidence output
- Unit tests for `KwsTrainer`: verify `is_available()`, mock training subprocess
- Integration test: full pipeline VAD → KWS → capture → Whisper on a test audio file
- Settings dialog: verify wake word group renders, dropdown/custom field mutual exclusion
- `/wake` command tests: list, train, threshold, on/off
- Fallback test: verify Whisper + keyword match works when no KWS model loaded

## System Messages

All training-related messages use `[SYSTEM]` label (not the AI assistant name) to clearly distinguish system notifications from AI responses. This applies to:
- "Training custom wake word..."
- "Wake word is ready!"
- "Wake word training failed: ..."
- Boot status messages
