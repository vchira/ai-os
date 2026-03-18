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

The voice listener always maintains a rolling 2-second circular buffer of raw audio alongside the KWS processing. When KWS triggers:

1. Wake word detected — the circular buffer contains the last 2 seconds of audio, which includes overlapping command audio if the user spoke without pausing (e.g., "hey jarvis what time is it")
2. Copy the circular buffer contents into a new command buffer, then trim: discard everything before the KWS trigger point, plus skip `post_wake_delay_ms` (300ms) after the trigger to drop the wake word tail
3. Continue accumulating new audio into the command buffer as it arrives
4. VAD monitors the command audio — when speech ends, send command buffer to Whisper
5. Send transcribed text to GTK thread via `stt_tx` channel
6. Return to Idle Mode

This means:
- If user pauses between wake word and command: the 300ms skip drops the wake word tail, then new speech is captured cleanly
- If user speaks without pausing ("hey jarvis what time is it"): the circular buffer has the overlapping audio, the 300ms skip is applied from the trigger point, and the remaining audio ("what time is it") is preserved

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
/// Result from processing an audio chunk through the KWS model.
pub struct KwsResult {
    /// Confidence score (0.0-1.0) for the active wake word.
    pub confidence: f32,
    /// Whether the confidence exceeded the threshold.
    pub triggered: bool,
}

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
    // openWakeWord expects 80ms chunks (1280 samples at 16kHz) — this is
    // specific to its architecture, not a generic KWS requirement.
    audio_buffer: Vec<f32>,
    embedding_buffer: Vec<Vec<f32>>,
}

impl KwsEngine {
    pub fn new(models_dir: &Path) -> Result<Self, VoiceError>;
    pub fn load_wake_model(&mut self, model_path: &Path, wake_word: &str) -> Result<(), VoiceError>;
    pub fn process_audio(&mut self, samples: &[f32]) -> KwsResult;
    pub fn reset(&mut self);
    pub fn wake_word(&self) -> &str;
    pub fn set_threshold(&mut self, threshold: f32);
}
```

Key details:
- `process_audio()` accepts raw f32 mono 16kHz samples of any length
- Internally buffers to 80ms chunks (openWakeWord-specific), runs melspectrogram → embedding → wake model
- Always returns `KwsResult` with confidence + triggered flag (useful for debugging/tuning)
- `reset()` clears internal buffers (called after wake detection or speech end)
- Thread-safe via `Arc<Mutex<KwsEngine>>` in the voice listener
- `ort::Session` is `Send` (ort v2.x), so the engine can be shared across threads via `Arc<Mutex>`
- Holding the mutex during inference (~1-5ms per 80ms chunk) is acceptable; the settings dialog or `/wake` command will briefly stall inference when hot-swapping models

### ONNX Runtime Initialization

- `ort::init()` is called once inside `KwsEngine::new()` with CPU execution provider
- The `ort` crate's `download-binaries` Cargo feature is used to bundle `libonnxruntime` at build time — no system package needed
- For GPU-capable machines, the CUDA execution provider can be added later as an optimization

### Error Handling

`KwsEngine` errors use the existing `VoiceError` enum with a new variant:

```rust
// In aios-voice/src/error.rs
pub enum VoiceError {
    // ... existing variants ...
    /// KWS model loading or inference failure.
    Kws(String),
}
```

Error scenarios:
- Infrastructure models missing/corrupted → `VoiceError::Kws("Failed to load embedding model: ...")` → voice listener falls back to Whisper + keyword match
- Wake model invalid → `VoiceError::Kws(...)` → logged, fallback used
- ONNX Runtime init failure → `VoiceError::Kws(...)` → logged, KWS disabled, pure Whisper mode

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

A new `wake_word` field is added to `AssistantConfig` in `autoconfig.rs` (the existing struct only has `name` and `effort`):

```rust
pub struct AssistantConfig {
    pub name: String,
    pub effort: String,
    pub wake_word: String,  // NEW — default: "hey assistant"
}
```

The autoconfig system checks:
- Is the `wake_word` value a pre-trained model name? → load `.onnx` directly, mic enabled
- Otherwise → trigger background training, mic disabled until done

## Config Keys

The default wake word changes from `"Assistant"` (current codebase) to `"hey assistant"` to match the pre-trained KWS model. The `WakeWordConfig::default()` in `detector.rs` (currently `"hey aios"`) is also updated for consistency.

New keys `wake_word_source` and `wake_threshold` are added to `voice` in `defaults.rs`:

```json
"voice": {
    "stt_enabled": true,
    "stt_model": "medium",
    "stt_language": "",
    "tts_enabled": true,
    "tts_voice": "en_US-amy-medium",
    "tts_gender": "female",
    "tts_rate": 1.0,
    "wake_word": "hey assistant",
    "wake_enabled": true,
    "wake_word_source": "pretrained",
    "wake_threshold": 0.5
}
```

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `wake_word` | string | "hey assistant" | Active wake phrase (changed from "Assistant") |
| `wake_enabled` | bool | true | Whether KWS filtering is active |
| `wake_word_source` | string | "pretrained" | "pretrained", "custom", or "training" |
| `wake_threshold` | f32 | 0.5 | KWS confidence threshold (0.0-1.0) |

## `/wake` Command

The current `cmd_wake()` in `commands.rs` handles: empty (show status), `on`, `off`, and catch-all (set phrase). It must be rewritten with explicit match arms in this priority order:

| Priority | Command | Behavior |
|----------|---------|----------|
| 1 | `/wake on` | Enable wake word detection |
| 2 | `/wake off` | Disable — all speech goes directly to Whisper |
| 3 | `/wake list` | Show all available pre-trained wake words |
| 4 | `/wake train <phrase>` | Force (re)train a custom model |
| 5 | `/wake threshold <0.0-1.0>` | Adjust KWS confidence threshold |
| 6 | `/wake` (no args) | Show current wake word, source, and status |
| 7 | `/wake <phrase>` (catch-all) | If pre-trained → load immediately. Otherwise → start background training |

The catch-all must be last to avoid capturing `list`, `train`, or `threshold` as wake phrases.

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
- `ort` crate with `download-binaries` feature — downloads and statically links `libonnxruntime` at build time (no system package required)
- No `libonnxruntime-dev` needed in Dockerfile — the `ort` crate handles everything

### Python (training only)
- `openwakeword`, `torch`, `piper-sample-generator`, `speechbrain`, `onnxruntime`
- Bundled at `/opt/aios-app/kws-trainer/` with virtualenv
- Not loaded at runtime — only invoked for custom model training
- **ISO size impact**: PyTorch adds ~1.5GB to the ISO. This is acceptable — AiOS ISOs are already multi-GB and disk space is not a constraint. The training capability is a core feature, not optional.

### ISO Build
- Download 3 infrastructure models from openWakeWord v0.5.1 release:
  - `https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/embedding_model.onnx`
  - `https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/melspectrogram.onnx`
  - `https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/silero_vad.onnx`
- Download 1 official model from openWakeWord v0.5.1:
  - `https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/hey_jarvis_v0.1.onnx`
- Download 16 community models from `fwartner/home-assistant-wakewords-collection` (main branch, `en/` directory)
- Train `hey_assistant.onnx` as part of the build process (or pre-train on dev machine)
- Bundle Python training virtualenv
- All model downloads should be pinned to specific commit SHAs for reproducible builds

## Existing Code Changes

| File | Change |
|------|--------|
| `aios-voice/Cargo.toml` | Add `ort` with `download-binaries` feature |
| `aios-voice/src/error.rs` | Add `Kws(String)` variant to `VoiceError` |
| `aios-voice/src/wake/mod.rs` | Add `pub mod kws; pub mod trainer;` exports |
| `aios-voice/src/wake/detector.rs` | Keep as fallback; update `WakeWordConfig::default()` wake phrase to "hey assistant" |
| `aios-voice/src/lib.rs` | Export new `KwsEngine`, `KwsTrainer`, `KwsResult` types |
| `aios-gtk/src/app.rs` | Rewrite `start_voice_listener()` to use KwsEngine state machine with circular buffer |
| `aios-gtk/src/app.rs` | Add `wake_training_in_progress` flag, mic button disable logic |
| `aios-gtk/src/ui/settings_dialog.rs` | Add Wake Word group to Voice page; needs `Arc<Mutex<KwsEngine>>` for hot-swap |
| `aios-core/src/config/defaults.rs` | Change `wake_word` default from "Assistant" to "hey assistant"; add `wake_word_source`, `wake_threshold` |
| `aios-core/src/config/autoconfig.rs` | Add `wake_word` field to `AssistantConfig` |
| `aios-core/src/config/commands.rs` | Extend `/wake` with `list`, `train`, `threshold` subcommands; `/wake train` returns a new `CommandResult::BackgroundTask` variant |
| `aios-core/src/types/` | Add `CommandResult::BackgroundTask` variant (see below) |
| `distro/_inner_build.sh` | Download KWS infrastructure + wake word models, rename to clean snake_case names, bundle Python training env |

### `CommandResult::BackgroundTask`

```rust
pub enum BackgroundTaskKind {
    WakeWordTraining { phrase: String, output_dir: PathBuf },
}

// Added to the existing CommandResult enum:
pub enum CommandResult {
    // ... existing variants ...
    BackgroundTask {
        description: String,
        task: BackgroundTaskKind,
    },
}
```

The GTK layer handles `BackgroundTask` by:
1. Showing the description as a `[SYSTEM]` message in chat
2. Spawning a background thread for the task
3. Setting `wake_training_in_progress` to `true`
4. On completion: sending a `[SYSTEM]` message via the existing `stt_tx` channel (with a special prefix like `\x00SYSTEM:` to distinguish from transcriptions), setting `wake_training_in_progress` to `false`, and hot-swapping the KWS model

### Updated `start_voice_listener()` Signature

```rust
fn start_voice_listener(
    stt_tx: std::sync::mpsc::Sender<String>,
    stt_enabled: Arc<AtomicBool>,
    wake_enabled: Arc<AtomicBool>,
    wake_training_in_progress: Arc<AtomicBool>,
    kws_engine: Arc<Mutex<KwsEngine>>,
) -> std::thread::JoinHandle<()>
```

The `wake_phrase` and `wake_word_source` parameters are no longer needed — the voice listener reads them from `KwsEngine` via the mutex. The old `WakeWordDetector` fallback is used when `kws_engine.lock().wake_session` is `None`.

### Model File Naming Convention

Models are stored with clean snake_case names in `pretrained/`. The ISO build script renames downloaded files:
- `hey_jarvis_v0.1.onnx` → `hey_jarvis.onnx`
- `computer_v1.onnx` or `computer_v2.onnx` → `computer.onnx` (pick the latest version)
- Community models: strip version suffixes, lowercase, snake_case

### `KwsEngine::process_audio()` with No Wake Model

When `wake_session` is `None` (no model loaded), `process_audio()` returns `KwsResult { confidence: 0.0, triggered: false }`. The voice listener checks this and falls back to the Whisper + keyword match path.

### Recovery on Reboot with `wake_word_source: "training"`

If the system was shut down mid-training (`wake_word_source` is `"training"` at boot):
1. Boot status shows: `⏳ KWS: restarting training — "phrase"`
2. Training is restarted automatically in a background thread
3. Mic button stays disabled until training completes
4. If training fails 3 times, `wake_word_source` is reset to `"pretrained"` with the default `"hey assistant"` model, and a `[SYSTEM]` error message is shown

### `hey_assistant.onnx` Production

The default `hey_assistant.onnx` model is pre-trained on the dev machine and checked into the repo at `distro/models/kws/hey_assistant.onnx`. This avoids requiring the Python training environment in the Docker ISO builder and ensures reproducible builds. The model is copied into the ISO alongside the downloaded pre-trained models.

### `ort` Crate Version

Pin to `ort = "2.0"` with `download-binaries` feature in `aios-voice/Cargo.toml`. The `download-binaries` feature downloads the correct `libonnxruntime` for the target platform at build time.

### Boot Status Integration

Add KWS status to the boot status message:
```
  ✅ KWS: active — hey_assistant (pretrained)
```
or during training:
```
  ⏳ KWS: training — "hey custom phrase" (mic disabled until ready)
```

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
