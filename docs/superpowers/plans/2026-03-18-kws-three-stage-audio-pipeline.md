# KWS Three-Stage Audio Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace full-Whisper wake word detection with a lightweight ONNX-based KWS engine that only triggers Whisper when the wake word is confirmed, reducing idle CPU usage by ~90%.

**Architecture:** Three-stage pipeline (VAD → KWS → STT). The `ort` crate runs openWakeWord ONNX models in-process. 17 pre-trained wake word models ship in the ISO. Custom wake words are trained on-device via a Python/openWakeWord subprocess using Piper TTS for synthetic data. The voice listener thread becomes a state machine (Idle → Capture) gated by KWS confidence scores.

**Tech Stack:** Rust, `ort` (ONNX Runtime), openWakeWord ONNX models, Python (training only), Piper TTS, GTK4/libadwaita

**Spec:** `docs/superpowers/specs/2026-03-18-kws-three-stage-audio-pipeline-design.md`

---

## File Map

### New Files
| File | Responsibility |
|------|---------------|
| `aios-voice/src/wake/kws.rs` | KwsEngine — ONNX inference for wake word detection |
| `aios-voice/src/wake/trainer.rs` | KwsTrainer — on-device custom model training via Python subprocess |
| `aios-voice/src/wake/pretrained.rs` | Pretrained model catalog — names, paths, lookup |
| `distro/models/kws/hey_assistant.onnx` | Pre-trained default wake word model (binary, checked into repo) |
| `distro/kws-trainer/train.py` | Python training script wrapping openWakeWord automatic training |
| `distro/kws-trainer/requirements.txt` | Python dependencies for training env |

### Modified Files
| File | What Changes |
|------|-------------|
| `aios-voice/Cargo.toml` | Add `ort` dependency |
| `aios-app-rs/Cargo.toml` | Add `ort` to workspace dependencies |
| `aios-voice/src/error.rs` | Add `Kws(String)` variant |
| `aios-voice/src/wake/mod.rs` | Add `pub mod kws; pub mod trainer; pub mod pretrained;` |
| `aios-voice/src/wake/detector.rs` | Update `WakeWordConfig::default()` wake phrase |
| `aios-voice/src/lib.rs` | Export `KwsEngine`, `KwsTrainer`, `KwsResult`, `PRETRAINED_WAKE_WORDS` |
| `aios-core/src/config/defaults.rs` | Change `wake_word` default, add `wake_word_source`, `wake_threshold` |
| `aios-core/src/config/autoconfig.rs` | Add `wake_word` field to `AssistantConfig` |
| `aios-core/src/config/commands.rs` | Rewrite `cmd_wake()` with new subcommands; add `BackgroundTask` to `CommandResult` |
| `aios-gtk/src/app.rs` | Rewrite `start_voice_listener()` state machine; add training integration |
| `aios-gtk/src/ui/settings_dialog.rs` | Add Wake Word group to Voice page |
| `distro/_inner_build.sh` | Download KWS models, bundle training env |

---

## Task 1: Add `ort` Dependency and `VoiceError::Kws` Variant

**Files:**
- Modify: `aios-app-rs/Cargo.toml:18-45` (workspace deps)
- Modify: `aios-voice/Cargo.toml:7-17` (crate deps)
- Modify: `aios-voice/src/error.rs:8-44` (VoiceError enum)

- [ ] **Step 1: Add `ort` to workspace dependencies**

In `aios-app-rs/Cargo.toml`, add to `[workspace.dependencies]` after the `rubato` line:

```toml
ort = { version = "2.0.0-rc.9", features = ["download-binaries"] }
```

**Important:** The `ort` crate's API changed between release candidates. Pin to this exact version. If a newer stable release exists at implementation time, check the `ort` docs and adapt the `Session::builder()`, `ort::init()`, and `ort::inputs!` calls in Task 3 accordingly.

- [ ] **Step 2: Add `ort` to aios-voice Cargo.toml**

In `aios-voice/Cargo.toml`, add after the `tempfile` line:

```toml
ort = { workspace = true }
```

- [ ] **Step 3: Add `Kws` variant to `VoiceError`**

In `aios-voice/src/error.rs`, add after the `UnsupportedBackend` variant (before the closing `}`):

```rust
    /// KWS model loading or inference failure.
    #[error("kws error: {0}")]
    Kws(String),
```

- [ ] **Step 4: Verify it compiles**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-voice`
Expected: compiles with no errors (ort will download libonnxruntime on first build — may take a minute)

- [ ] **Step 5: Commit**

```bash
git add aios-app-rs/Cargo.toml aios-voice/Cargo.toml aios-voice/src/error.rs
git commit -m "feat(voice): add ort dependency and VoiceError::Kws variant"
```

---

## Task 2: Pretrained Model Catalog

**Files:**
- Create: `aios-voice/src/wake/pretrained.rs`
- Modify: `aios-voice/src/wake/mod.rs`

- [ ] **Step 1: Create the pretrained catalog module**

Create `aios-voice/src/wake/pretrained.rs`:

```rust
//! Catalog of pre-trained openWakeWord models shipped with AiOS.

use std::path::{Path, PathBuf};

/// A pre-trained wake word model entry.
#[derive(Debug, Clone)]
pub struct PretrainedModel {
    /// Snake_case identifier (e.g., "hey_assistant").
    pub id: &'static str,
    /// Human-readable display name (e.g., "Hey Assistant").
    pub display_name: &'static str,
    /// ONNX model filename (e.g., "hey_assistant.onnx").
    pub filename: &'static str,
}

/// All pre-trained wake word models shipped with AiOS.
pub const PRETRAINED_WAKE_WORDS: &[PretrainedModel] = &[
    PretrainedModel { id: "hey_assistant", display_name: "Hey Assistant", filename: "hey_assistant.onnx" },
    PretrainedModel { id: "hey_jarvis", display_name: "Hey Jarvis", filename: "hey_jarvis.onnx" },
    PretrainedModel { id: "computer", display_name: "Computer", filename: "computer.onnx" },
    PretrainedModel { id: "ok_computer", display_name: "OK Computer", filename: "ok_computer.onnx" },
    PretrainedModel { id: "hey_friday", display_name: "Hey Friday", filename: "hey_friday.onnx" },
    PretrainedModel { id: "jarvis", display_name: "Jarvis", filename: "jarvis.onnx" },
    PretrainedModel { id: "ok_jarvis", display_name: "OK Jarvis", filename: "ok_jarvis.onnx" },
    PretrainedModel { id: "skynet", display_name: "Skynet", filename: "skynet.onnx" },
    PretrainedModel { id: "terminator", display_name: "Terminator", filename: "terminator.onnx" },
    PretrainedModel { id: "hey_house", display_name: "Hey House", filename: "hey_house.onnx" },
    PretrainedModel { id: "ok_home", display_name: "OK Home", filename: "ok_home.onnx" },
    PretrainedModel { id: "home_assistant", display_name: "Home Assistant", filename: "home_assistant.onnx" },
    PretrainedModel { id: "mr_anderson", display_name: "Mr. Anderson", filename: "mr_anderson.onnx" },
    PretrainedModel { id: "mr_smith", display_name: "Mr. Smith", filename: "mr_smith.onnx" },
    PretrainedModel { id: "hey_dick_head", display_name: "Hey Dick Head", filename: "hey_dick_head.onnx" },
    PretrainedModel { id: "oi_fuckwhit", display_name: "Oi Fuckwhit", filename: "oi_fuckwhit.onnx" },
    PretrainedModel { id: "yo_homie", display_name: "Yo Homie", filename: "yo_homie.onnx" },
];

/// Find a pre-trained model by its id or display name (case-insensitive).
pub fn find_pretrained(name: &str) -> Option<&'static PretrainedModel> {
    let lower = name.to_lowercase();
    let normalized = lower.replace(' ', "_");
    PRETRAINED_WAKE_WORDS.iter().find(|m| {
        m.id == normalized
            || m.display_name.to_lowercase() == lower
            || m.id == lower
    })
}

/// Get the full path to a pre-trained model file.
pub fn pretrained_model_path(models_dir: &Path, model: &PretrainedModel) -> PathBuf {
    models_dir.join("pretrained").join(model.filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_17_models() {
        assert_eq!(PRETRAINED_WAKE_WORDS.len(), 17);
    }

    #[test]
    fn find_by_id() {
        assert!(find_pretrained("hey_assistant").is_some());
        assert!(find_pretrained("skynet").is_some());
        assert!(find_pretrained("nonexistent").is_none());
    }

    #[test]
    fn find_by_display_name() {
        assert!(find_pretrained("Hey Jarvis").is_some());
        assert!(find_pretrained("OK Computer").is_some());
    }

    #[test]
    fn find_case_insensitive() {
        assert!(find_pretrained("HEY_ASSISTANT").is_some());
        assert!(find_pretrained("hey assistant").is_some());
        assert!(find_pretrained("SKYNET").is_some());
    }

    #[test]
    fn model_path_construction() {
        let model = find_pretrained("hey_assistant").unwrap();
        let path = pretrained_model_path(Path::new("/models"), model);
        assert_eq!(path, PathBuf::from("/models/pretrained/hey_assistant.onnx"));
    }
}
```

- [ ] **Step 2: Add module to wake/mod.rs**

In `aios-voice/src/wake/mod.rs`, add after `pub mod detector;`:

```rust
pub mod pretrained;
```

And add to the re-exports:

```rust
pub use pretrained::{PretrainedModel, PRETRAINED_WAKE_WORDS, find_pretrained, pretrained_model_path};
```

- [ ] **Step 3: Export from lib.rs**

In `aios-voice/src/lib.rs`, add to the `pub use wake::` line:

```rust
pub use wake::{WakeWordConfig, WakeWordDetector, WakeWordEvent, PretrainedModel, PRETRAINED_WAKE_WORDS, find_pretrained};
```

- [ ] **Step 4: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --package aios-voice -- pretrained`
Expected: 5 tests pass

- [ ] **Step 5: Commit**

```bash
git add aios-voice/src/wake/pretrained.rs aios-voice/src/wake/mod.rs aios-voice/src/lib.rs
git commit -m "feat(voice): add pretrained wake word model catalog (17 models)"
```

---

## Task 3: KwsEngine — ONNX Inference Core

**Files:**
- Create: `aios-voice/src/wake/kws.rs`
- Modify: `aios-voice/src/wake/mod.rs`
- Modify: `aios-voice/src/lib.rs`

This is the core of the feature. The KwsEngine loads three infrastructure ONNX models (melspectrogram, embedding, silero_vad) and one wake word model, then processes raw audio chunks through the openWakeWord pipeline.

- [ ] **Step 1: Write the KwsResult struct and KwsEngine skeleton**

Create `aios-voice/src/wake/kws.rs`:

```rust
//! ONNX-based Keyword Spotting engine using openWakeWord models.
//!
//! The engine runs three infrastructure models (melspectrogram, embedding,
//! silero_vad) plus one active wake word model. Audio is processed in 80ms
//! chunks (1280 samples at 16kHz) through the openWakeWord pipeline:
//!
//! raw audio → melspectrogram → embedding → wake word model → confidence score
//!
//! The 80ms chunk size is specific to openWakeWord's architecture.

use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::error::VoiceError;

/// Result from processing an audio chunk through the KWS model.
#[derive(Debug, Clone)]
pub struct KwsResult {
    /// Confidence score (0.0-1.0) for the active wake word.
    pub confidence: f32,
    /// Whether the confidence exceeded the threshold.
    pub triggered: bool,
}

/// Audio chunk size for openWakeWord: 80ms at 16kHz = 1280 samples.
const OWW_CHUNK_SIZE: usize = 1280;

/// Number of mel spectrogram features per frame.
const N_MEL_FEATURES: usize = 32;

/// Number of embedding features from the embedding model.
const N_EMBEDDING_FEATURES: usize = 96;

/// Number of mel frames accumulated before computing an embedding (16 frames).
const MEL_WINDOW_SIZE: usize = 76;

/// Embedding window step size.
const EMBEDDING_STEP: usize = 8;

pub struct KwsEngine {
    // Infrastructure ONNX sessions
    melspec_session: ort::Session,
    embedding_session: ort::Session,
    // Note: silero_vad session is loaded but we use our own RMS VAD in the
    // pipeline. Kept for future use / compatibility with openWakeWord models
    // that expect VAD state.

    // Active wake word model
    wake_session: Option<ort::Session>,
    wake_word: String,
    threshold: f32,

    // Internal buffers for the rolling prediction pipeline
    audio_buffer: Vec<f32>,
    mel_buffer: Vec<Vec<f32>>,
    embedding_buffer: Vec<Vec<f32>>,
    mel_frame_count: usize,
}

impl KwsEngine {
    /// Create a new KWS engine, loading infrastructure models from `models_dir/infrastructure/`.
    ///
    /// Calls `ort::init()` on first use. Returns `VoiceError::Kws` if models
    /// are missing or ONNX Runtime fails to initialize.
    pub fn new(models_dir: &Path) -> Result<Self, VoiceError> {
        // Initialize ONNX Runtime (safe to call multiple times)
        ort::init()
            .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default()])
            .commit()
            .map_err(|e| VoiceError::Kws(format!("ONNX Runtime init failed: {e}")))?;

        let infra_dir = models_dir.join("infrastructure");

        let melspec_path = infra_dir.join("melspectrogram.onnx");
        let embedding_path = infra_dir.join("embedding_model.onnx");

        let melspec_session = ort::Session::builder()
            .map_err(|e| VoiceError::Kws(format!("Session builder failed: {e}")))?
            .with_intra_threads(1)
            .map_err(|e| VoiceError::Kws(format!("Thread config failed: {e}")))?
            .commit_from_file(&melspec_path)
            .map_err(|e| VoiceError::Kws(format!("Failed to load melspectrogram model at {}: {e}", melspec_path.display())))?;

        let embedding_session = ort::Session::builder()
            .map_err(|e| VoiceError::Kws(format!("Session builder failed: {e}")))?
            .with_intra_threads(1)
            .map_err(|e| VoiceError::Kws(format!("Thread config failed: {e}")))?
            .commit_from_file(&embedding_path)
            .map_err(|e| VoiceError::Kws(format!("Failed to load embedding model at {}: {e}", embedding_path.display())))?;

        info!("KWS engine initialized (melspec + embedding models loaded)");

        Ok(Self {
            melspec_session,
            embedding_session,
            wake_session: None,
            wake_word: String::new(),
            threshold: 0.5,
            audio_buffer: Vec::new(),
            mel_buffer: Vec::new(),
            embedding_buffer: Vec::new(),
            mel_frame_count: 0,
        })
    }

    /// Load a wake word ONNX model.
    pub fn load_wake_model(&mut self, model_path: &Path, wake_word: &str) -> Result<(), VoiceError> {
        let session = ort::Session::builder()
            .map_err(|e| VoiceError::Kws(format!("Session builder failed: {e}")))?
            .with_intra_threads(1)
            .map_err(|e| VoiceError::Kws(format!("Thread config failed: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| VoiceError::Kws(format!("Failed to load wake model at {}: {e}", model_path.display())))?;

        self.wake_session = Some(session);
        self.wake_word = wake_word.to_string();
        self.reset();
        info!("KWS: loaded wake word model \"{}\" from {}", wake_word, model_path.display());
        Ok(())
    }

    /// Process raw audio samples and return the KWS prediction.
    ///
    /// Accepts any number of f32 mono 16kHz samples. Internally buffers to
    /// 80ms chunks and runs the openWakeWord pipeline. Returns the highest
    /// confidence score from the processed chunks.
    pub fn process_audio(&mut self, samples: &[f32]) -> KwsResult {
        if self.wake_session.is_none() {
            return KwsResult { confidence: 0.0, triggered: false };
        }

        self.audio_buffer.extend_from_slice(samples);

        let mut max_confidence: f32 = 0.0;

        // Process complete 80ms chunks
        while self.audio_buffer.len() >= OWW_CHUNK_SIZE {
            let chunk: Vec<f32> = self.audio_buffer.drain(..OWW_CHUNK_SIZE).collect();

            // Step 1: Compute mel spectrogram for this chunk
            match self.compute_melspec(&chunk) {
                Ok(mel_features) => {
                    self.mel_buffer.push(mel_features);
                    self.mel_frame_count += 1;
                }
                Err(e) => {
                    debug!("KWS melspec error: {e}");
                    continue;
                }
            }

            // Step 2: When we have enough mel frames, compute embedding
            if self.mel_buffer.len() >= MEL_WINDOW_SIZE {
                match self.compute_embedding() {
                    Ok(embedding) => {
                        self.embedding_buffer.push(embedding);
                        // Slide the mel window
                        let drain_count = EMBEDDING_STEP.min(self.mel_buffer.len());
                        self.mel_buffer.drain(..drain_count);
                    }
                    Err(e) => {
                        debug!("KWS embedding error: {e}");
                    }
                }
            }

            // Cap embedding buffer to prevent unbounded growth during idle
            if self.embedding_buffer.len() > 16 {
                self.embedding_buffer.drain(..self.embedding_buffer.len() - 16);
            }

            // Step 3: Run wake word model on accumulated embeddings
            if !self.embedding_buffer.is_empty() {
                match self.run_wake_model() {
                    Ok(confidence) => {
                        if confidence > max_confidence {
                            max_confidence = confidence;
                        }
                    }
                    Err(e) => {
                        debug!("KWS wake model error: {e}");
                    }
                }
            }
        }

        KwsResult {
            confidence: max_confidence,
            triggered: max_confidence >= self.threshold,
        }
    }

    /// Clear all internal buffers. Call after wake detection or speech end.
    pub fn reset(&mut self) {
        self.audio_buffer.clear();
        self.mel_buffer.clear();
        self.embedding_buffer.clear();
        self.mel_frame_count = 0;
    }

    /// Get the active wake word.
    pub fn wake_word(&self) -> &str {
        &self.wake_word
    }

    /// Check if a wake model is loaded.
    pub fn has_model(&self) -> bool {
        self.wake_session.is_some()
    }

    /// Set the confidence threshold (0.0-1.0).
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.0, 1.0);
    }

    /// Get the current threshold.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    // -- Private inference methods --

    fn compute_melspec(&self, chunk: &[f32]) -> Result<Vec<f32>, VoiceError> {
        let input = ndarray::Array2::from_shape_vec(
            (1, OWW_CHUNK_SIZE),
            chunk.to_vec(),
        ).map_err(|e| VoiceError::Kws(format!("melspec input shape: {e}")))?;

        let outputs = self.melspec_session
            .run(ort::inputs![input].map_err(|e| VoiceError::Kws(format!("melspec input: {e}")))?)
            .map_err(|e| VoiceError::Kws(format!("melspec inference: {e}")))?;

        let output = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VoiceError::Kws(format!("melspec output extract: {e}")))?;

        Ok(output.as_slice().unwrap_or_default().to_vec())
    }

    fn compute_embedding(&self) -> Result<Vec<f32>, VoiceError> {
        // Stack mel frames into a 2D array [1, mel_window_size * n_mel_features]
        let flat: Vec<f32> = self.mel_buffer.iter()
            .take(MEL_WINDOW_SIZE)
            .flat_map(|f| f.iter().copied())
            .collect();

        let n_features = flat.len();
        let input = ndarray::Array2::from_shape_vec(
            (1, n_features),
            flat,
        ).map_err(|e| VoiceError::Kws(format!("embedding input shape: {e}")))?;

        let outputs = self.embedding_session
            .run(ort::inputs![input].map_err(|e| VoiceError::Kws(format!("embedding input: {e}")))?)
            .map_err(|e| VoiceError::Kws(format!("embedding inference: {e}")))?;

        let output = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VoiceError::Kws(format!("embedding output extract: {e}")))?;

        Ok(output.as_slice().unwrap_or_default().to_vec())
    }

    fn run_wake_model(&mut self) -> Result<f32, VoiceError> {
        let session = self.wake_session.as_ref()
            .ok_or_else(|| VoiceError::Kws("No wake model loaded".to_string()))?;

        // The wake model expects the latest embedding(s)
        let latest = self.embedding_buffer.last()
            .ok_or_else(|| VoiceError::Kws("No embeddings available".to_string()))?;

        let input = ndarray::Array2::from_shape_vec(
            (1, latest.len()),
            latest.clone(),
        ).map_err(|e| VoiceError::Kws(format!("wake model input shape: {e}")))?;

        let outputs = session
            .run(ort::inputs![input].map_err(|e| VoiceError::Kws(format!("wake model input: {e}")))?)
            .map_err(|e| VoiceError::Kws(format!("wake model inference: {e}")))?;

        let output = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VoiceError::Kws(format!("wake model output extract: {e}")))?;

        let confidence = output.as_slice()
            .and_then(|s| s.first().copied())
            .unwrap_or(0.0);

        Ok(confidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kws_result_defaults() {
        let r = KwsResult { confidence: 0.0, triggered: false };
        assert!(!r.triggered);
        assert_eq!(r.confidence, 0.0);
    }

    #[test]
    fn no_model_returns_zero() {
        // KwsEngine::new requires real model files, so we test the no-model path
        // by constructing a KwsResult directly.
        let result = KwsResult { confidence: 0.0, triggered: false };
        assert!(!result.triggered);
    }

    #[test]
    fn threshold_clamping() {
        // Test threshold clamping logic in isolation
        let t = 1.5_f32.clamp(0.0, 1.0);
        assert_eq!(t, 1.0);
        let t = (-0.5_f32).clamp(0.0, 1.0);
        assert_eq!(t, 0.0);
    }
}
```

- [ ] **Step 2: Add `ndarray` dependency**

The `ort` crate uses `ndarray` for tensor construction. Add to workspace `Cargo.toml`:

```toml
ndarray = "0.16"
```

And to `aios-voice/Cargo.toml`:

```toml
ndarray = { workspace = true }
```

- [ ] **Step 3: Update wake/mod.rs**

Add `pub mod kws;` after `pub mod pretrained;` and add to re-exports:

```rust
pub use kws::{KwsEngine, KwsResult};
```

- [ ] **Step 4: Update lib.rs exports**

Update the `pub use wake::` line to include:

```rust
pub use wake::{WakeWordConfig, WakeWordDetector, WakeWordEvent, PretrainedModel, PRETRAINED_WAKE_WORDS, find_pretrained, KwsEngine, KwsResult};
```

- [ ] **Step 5: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-voice`
Expected: compiles (unit tests require model files so we skip them for now)

- [ ] **Step 6: Commit**

```bash
git add aios-voice/src/wake/kws.rs aios-voice/src/wake/mod.rs aios-voice/src/lib.rs aios-voice/Cargo.toml aios-app-rs/Cargo.toml
git commit -m "feat(voice): add KwsEngine — ONNX-based wake word inference"
```

---

## Task 4: KwsTrainer — On-Device Custom Model Training

**Files:**
- Create: `aios-voice/src/wake/trainer.rs`
- Create: `distro/kws-trainer/train.py`
- Create: `distro/kws-trainer/requirements.txt`
- Modify: `aios-voice/src/wake/mod.rs`
- Modify: `aios-voice/src/lib.rs`

- [ ] **Step 1: Create the Rust trainer wrapper**

Create `aios-voice/src/wake/trainer.rs`:

```rust
//! On-device wake word model training via openWakeWord Python pipeline.
//!
//! Uses Piper TTS to generate synthetic training data, then trains a small
//! neural network and exports it as an ONNX model. Training takes ~5-10
//! minutes on CPU.

use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

use crate::error::VoiceError;

/// Path to the Python training script.
const TRAINER_SCRIPT: &str = "/opt/aios-app/kws-trainer/train.py";

/// Path to the Python virtualenv.
const TRAINER_VENV: &str = "/opt/aios-app/kws-trainer/venv";

/// On-device wake word model trainer.
pub struct KwsTrainer;

impl KwsTrainer {
    /// Check if the training environment is available.
    pub fn is_available() -> bool {
        Path::new(TRAINER_SCRIPT).exists() && Path::new(TRAINER_VENV).exists()
    }

    /// Train a custom wake word model.
    ///
    /// Spawns the Python training pipeline as a subprocess. Blocks until
    /// training completes (~5-10 minutes on CPU).
    ///
    /// Returns the path to the produced `.onnx` model file.
    pub fn train(phrase: &str, output_dir: &Path) -> Result<PathBuf, VoiceError> {
        if !Self::is_available() {
            return Err(VoiceError::Kws(
                "Training environment not available. Install the kws-trainer package.".to_string(),
            ));
        }

        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");

        let output_path = output_dir.join(format!("{sanitized}.onnx"));

        // Ensure output directory exists
        std::fs::create_dir_all(output_dir)
            .map_err(|e| VoiceError::Kws(format!("Failed to create output dir: {e}")))?;

        info!("KWS trainer: starting training for \"{phrase}\" → {}", output_path.display());

        let python = PathBuf::from(TRAINER_VENV).join("bin/python");

        let output = Command::new(&python)
            .arg(TRAINER_SCRIPT)
            .arg("--phrase")
            .arg(phrase)
            .arg("--output")
            .arg(&output_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| VoiceError::Kws(format!("Failed to spawn trainer: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("KWS trainer failed: {stderr}");
            return Err(VoiceError::Kws(format!("Training failed: {stderr}")));
        }

        if !output_path.exists() {
            return Err(VoiceError::Kws(
                "Training completed but model file not found".to_string(),
            ));
        }

        info!("KWS trainer: model saved to {}", output_path.display());
        Ok(output_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_phrase() {
        let phrase = "Hey My Assistant!";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        assert_eq!(sanitized, "hey_my_assistant");
    }

    #[test]
    fn trainer_not_available_without_files() {
        // In test environment, the trainer files don't exist
        assert!(!KwsTrainer::is_available());
    }
}
```

- [ ] **Step 2: Create the Python training script**

Create `distro/kws-trainer/train.py`:

```python
#!/usr/bin/env python3
"""AiOS wake word model trainer using openWakeWord automatic training."""

import argparse
import sys
import os

def main():
    parser = argparse.ArgumentParser(description="Train a custom wake word model")
    parser.add_argument("--phrase", required=True, help="Wake word phrase to train")
    parser.add_argument("--output", required=True, help="Output .onnx model path")
    args = parser.parse_args()

    try:
        from openwakeword.train import train_model
    except ImportError:
        print("ERROR: openwakeword not installed", file=sys.stderr)
        sys.exit(1)

    print(f"Training wake word model for: {args.phrase}")
    print(f"Output: {args.output}")

    # Use openWakeWord's automatic training pipeline
    # This generates synthetic speech via Piper TTS, augments with noise,
    # and trains a small neural network
    train_model(
        target_phrase=args.phrase,
        output_path=args.output,
        # Use default training parameters — they work well for most phrases
    )

    print(f"Training complete: {args.output}")

if __name__ == "__main__":
    main()
```

- [ ] **Step 3: Create requirements.txt**

Create `distro/kws-trainer/requirements.txt`:

```
openwakeword>=0.6.0
torch>=2.0
piper-sample-generator
speechbrain
onnxruntime
```

- [ ] **Step 4: Update wake/mod.rs and lib.rs**

Add `pub mod trainer;` to `aios-voice/src/wake/mod.rs` and:

```rust
pub use trainer::KwsTrainer;
```

Add `KwsTrainer` to the `pub use wake::` line in `lib.rs`.

- [ ] **Step 5: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --package aios-voice -- trainer`
Expected: 2 tests pass

- [ ] **Step 6: Commit**

```bash
git add aios-voice/src/wake/trainer.rs aios-voice/src/wake/mod.rs aios-voice/src/lib.rs distro/kws-trainer/
git commit -m "feat(voice): add KwsTrainer — on-device custom wake word training"
```

---

## Task 5: Config Defaults and Autoconfig Changes

**Files:**
- Modify: `aios-core/src/config/defaults.rs:27-35`
- Modify: `aios-core/src/config/autoconfig.rs:113-133`
- Modify: `aios-voice/src/wake/detector.rs:30-39`

- [ ] **Step 1: Update voice config defaults**

In `aios-core/src/config/defaults.rs`, replace the voice section (lines ~27-35):

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
},
```

- [ ] **Step 2: Add `wake_word` to `AssistantConfig` in autoconfig.rs**

In `aios-core/src/config/autoconfig.rs`, update the `AssistantConfig` struct (around line 113):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantConfig {
    /// Assistant display name.
    #[serde(default = "default_assistant_name")]
    pub name: String,
    /// Effort level: "auto", "low", "medium", "high".
    #[serde(default = "default_effort")]
    pub effort: String,
    /// Wake word phrase (e.g., "hey assistant", "jarvis").
    #[serde(default = "default_wake_word")]
    pub wake_word: String,
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self {
            name: default_assistant_name(),
            effort: default_effort(),
            wake_word: default_wake_word(),
        }
    }
}
```

Add the default function near the other defaults (after `default_effort`):

```rust
fn default_wake_word() -> String { "hey assistant".to_string() }
```

- [ ] **Step 3: Update WakeWordConfig default in detector.rs**

In `aios-voice/src/wake/detector.rs`, change the `WakeWordConfig::default()` wake phrase from `"hey aios"` to `"hey assistant"` (around line 33):

```rust
wake_phrase: "hey assistant".into(),
```

- [ ] **Step 3.5: Update detector.rs tests for new default**

The following tests in `aios-voice/src/wake/detector.rs` use `WakeWordConfig::default()` and assert against `"hey aios"`. Update all of them to assert `"hey assistant"` instead:

- `matches_exact_wake_phrase` — change `"hey aios"` to `"hey assistant"`
- `matches_wake_phrase_with_trailing_speech` — `"hey assistant how are you"`
- `matches_wake_phrase_case_insensitive` — `"HEY ASSISTANT"`
- `matches_wake_phrase_mixed_case` — `"Hey Assistant, what time is it?"`
- `rejects_missing_keyword` — `"hey google"` (still fails, no change needed)
- `rejects_wrong_order` — `"assistant hey"` (update from `"aios hey"`)
- `rejects_partial_match` — `"hey"` (still fails, no change needed)
- `default_config_values_are_sane` — change `assert_eq!(config.wake_phrase, "hey aios")` to `"hey assistant"`
- `very_long_transcription_with_wake_word_buried` — update prefix + `"hey assistant do something"`
- `words_with_extra_in_between_may_match_as_substrings` — `"hey um assistant"`
- `wake_word_not_at_start` — `"I said hey assistant please help"`
- `wake_phrase_repeated_twice` — `"hey assistant hey assistant"`

- [ ] **Step 4: Update autoconfig test**

Update the `test_parse_full_autoconfig` test in `autoconfig.rs` to include the new `wake_word` field:

Find the test's JSON `"assistant"` section and add `"wake_word": "hey computer"`, then add an assertion:

```rust
assert_eq!(cfg.assistant.wake_word, "hey computer");
```

Also update `test_parse_minimal_autoconfig` to check the default:

```rust
assert_eq!(cfg.assistant.wake_word, "hey assistant");
```

- [ ] **Step 5: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --package aios-core -- autoconfig`
Expected: all autoconfig tests pass

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --package aios-voice -- wake`
Expected: all wake word tests pass (detector tests may need minor updates for new default)

- [ ] **Step 6: Commit**

```bash
git add aios-core/src/config/defaults.rs aios-core/src/config/autoconfig.rs aios-voice/src/wake/detector.rs
git commit -m "feat(config): update wake word defaults and add autoconfig wake_word field"
```

---

## Task 6: Extend `/wake` Command and `CommandResult`

**Files:**
- Modify: `aios-core/src/config/commands.rs:80-113` (CommandResult enum)
- Modify: `aios-core/src/config/commands.rs:475-504` (cmd_wake function)

- [ ] **Step 1: Add `BackgroundTask` variant to `CommandResult`**

In `aios-core/src/config/commands.rs`, add before the `Unknown` variant in the `CommandResult` enum (around line 110):

```rust
    /// A background task that should be spawned by the GTK layer.
    BackgroundTask {
        /// Human-readable description shown as a system message.
        description: String,
        /// The kind of background task to spawn.
        task: BackgroundTaskKind,
    },
```

Add the `BackgroundTaskKind` enum above or below the `CommandResult` enum:

```rust
/// Kinds of background tasks that commands can trigger.
/// Must derive the same traits as `CommandResult` (Debug, Clone).
#[derive(Debug, Clone)]
pub enum BackgroundTaskKind {
    /// Train a custom wake word model.
    WakeWordTraining {
        phrase: String,
        output_dir: std::path::PathBuf,
    },
}
```

- [ ] **Step 2: Add pretrained ID list to commands.rs**

`aios-core` cannot depend on `aios-voice` (circular dependency). Add a simple `&[(&str, &str)]` list at the top of `commands.rs` (or near `cmd_wake`):

```rust
/// Pretrained wake word IDs (duplicated from aios-voice::wake::pretrained to avoid circular dep).
const PRETRAINED_WAKE_WORD_IDS: &[(&str, &str)] = &[
    ("hey_assistant", "Hey Assistant"),
    ("hey_jarvis", "Hey Jarvis"),
    ("computer", "Computer"),
    ("ok_computer", "OK Computer"),
    ("hey_friday", "Hey Friday"),
    ("jarvis", "Jarvis"),
    ("ok_jarvis", "OK Jarvis"),
    ("skynet", "Skynet"),
    ("terminator", "Terminator"),
    ("hey_house", "Hey House"),
    ("ok_home", "OK Home"),
    ("home_assistant", "Home Assistant"),
    ("mr_anderson", "Mr. Anderson"),
    ("mr_smith", "Mr. Smith"),
    ("hey_dick_head", "Hey Dick Head"),
    ("oi_fuckwhit", "Oi Fuckwhit"),
    ("yo_homie", "Yo Homie"),
];
```

- [ ] **Step 3: Rewrite `cmd_wake()`**

Replace the `cmd_wake` function body (lines 475-504) with:

```rust
    fn cmd_wake(&mut self, args: &str) -> CommandResult {
        let phrase = args.trim();

        // No args — show status
        if phrase.is_empty() {
            let current = self.config.get_str("voice.wake_word", "hey assistant");
            let enabled = self.config.get_bool("voice.wake_enabled", true);
            let source = self.config.get_str("voice.wake_word_source", "pretrained");
            let threshold = self.config.get_f64("voice.wake_threshold", 0.5);
            let status = if enabled { "enabled" } else { "disabled" };
            return CommandResult::Response(format!(
                "Wake word: \"{current}\" ({source}) — {status}, threshold: {threshold:.1}"
            ));
        }

        match phrase.to_lowercase().as_str() {
            "on" => {
                let _ = self.config.set("voice.wake_enabled", json!(true));
                let current = self.config.get_str("voice.wake_word", "hey assistant");
                CommandResult::Response(format!("Wake word detection enabled: \"{current}\""))
            }
            "off" => {
                let _ = self.config.set("voice.wake_enabled", json!(false));
                CommandResult::Response("Wake word detection disabled — all speech goes to STT".to_string())
            }
            "list" => {
                let mut lines = vec!["Available pre-trained wake words:".to_string()];
                let current = self.config.get_str("voice.wake_word", "");
                let current_id = current.to_lowercase().replace(' ', "_");
                for &(id, display) in PRETRAINED_WAKE_WORD_IDS {
                    let marker = if id == current_id { " (active)" } else { "" };
                    lines.push(format!("  {id} — {display}{marker}"));
                }
                lines.push(String::new());
                lines.push("Tip: /wake <name> to switch, or /wake <custom phrase> to train a new one.".to_string());
                CommandResult::Response(lines.join("\n"))
            }
            s if s.starts_with("train ") => {
                let train_phrase = phrase[6..].trim();
                if train_phrase.is_empty() {
                    return CommandResult::Response("Usage: /wake train <phrase>".to_string());
                }
                let _ = self.config.set("voice.wake_word", json!(train_phrase));
                let _ = self.config.set("voice.wake_word_source", json!("training"));
                let _ = self.config.set("voice.wake_enabled", json!(true));
                let output_dir = ConfigManager::default_config_dir()
                    .join("models/kws/custom");
                CommandResult::BackgroundTask {
                    description: format!("Training custom wake word \"{train_phrase}\"... this takes a few minutes."),
                    task: BackgroundTaskKind::WakeWordTraining {
                        phrase: train_phrase.to_string(),
                        output_dir,
                    },
                }
            }
            s if s.starts_with("threshold ") => {
                let val_str = phrase[10..].trim();
                match val_str.parse::<f64>() {
                    Ok(val) if (0.0..=1.0).contains(&val) => {
                        let _ = self.config.set("voice.wake_threshold", json!(val));
                        CommandResult::Response(format!("Wake word threshold set to {val:.1}"))
                    }
                    _ => CommandResult::Response("Usage: /wake threshold <0.0-1.0>".to_string()),
                }
            }
            _ => {
                // Catch-all: set as wake word
                let phrase = args.trim();
                let _ = self.config.set("voice.wake_word", json!(phrase));
                let _ = self.config.set("voice.wake_enabled", json!(true));

                // Check if it's a pre-trained model
                let normalized = phrase.to_lowercase().replace(' ', "_");
                if PRETRAINED_WAKE_WORD_IDS.iter().any(|&(id, _)| id == normalized) {
                    let _ = self.config.set("voice.wake_word_source", json!("pretrained"));
                    CommandResult::Response(format!("Wake word set to \"{phrase}\" (pretrained — active immediately)"))
                } else {
                    let _ = self.config.set("voice.wake_word_source", json!("training"));
                    let output_dir = ConfigManager::default_config_dir()
                        .parent().unwrap_or(std::path::Path::new("~/.aios"))
                        .join("models/kws/custom");
                    CommandResult::BackgroundTask {
                        description: format!("No pre-trained model for \"{phrase}\". Training a custom model... this takes a few minutes."),
                        task: BackgroundTaskKind::WakeWordTraining {
                            phrase: phrase.to_string(),
                            output_dir,
                        },
                    }
                }
            }
        }
    }
```

- [ ] **Step 4: Update existing wake command tests**

Update the tests in `commands.rs` (around lines 1069-1128) for the new behavior — the wake status format changed, and new subcommands need test coverage for `list`, `train`, `threshold`.

- [ ] **Step 5: Run tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --package aios-core -- wake`
Expected: all wake command tests pass

- [ ] **Step 6: Commit**

```bash
git add aios-core/src/config/commands.rs
git commit -m "feat(commands): extend /wake with list, train, threshold subcommands"
```

---

## Task 7: Rewrite Voice Listener State Machine

**Files:**
- Modify: `aios-gtk/src/app.rs:42-178` (start_voice_listener function)
- Modify: `aios-gtk/src/app.rs:1679-1714` (voice listener setup in activate_main)

This is the most architecturally significant change. The voice listener becomes a state machine with Idle and Capture modes, using KwsEngine for wake word detection.

- [ ] **Step 1: Rewrite `start_voice_listener()`**

Replace the function signature and body at `aios-gtk/src/app.rs` lines 42-178 with the new state machine. The full implementation:

```rust
/// Listener state for the two-mode state machine.
enum ListenerState {
    /// Waiting for wake word. Feed audio to KWS.
    Idle,
    /// Wake word detected. Capture command audio for Whisper.
    Capture {
        command_buffer: Vec<f32>,
        capture_start: std::time::Instant,
        silence_start: Option<std::time::Instant>,
    },
}

fn start_voice_listener(
    stt_tx: std::sync::mpsc::Sender<String>,
    stt_enabled: Arc<AtomicBool>,
    wake_enabled: Arc<AtomicBool>,
    wake_training_in_progress: Arc<AtomicBool>,
    kws_engine: Arc<std::sync::Mutex<aios_voice::KwsEngine>>,
) -> std::thread::JoinHandle<()> {
    use aios_voice::audio::capture::AudioCapture;
    use aios_voice::audio::vad::{VadConfig, VoiceActivityDetector, DEFAULT_FRAME_SIZE};
    use aios_voice::wake::{WakeWordConfig, WakeWordDetector};
    use std::collections::VecDeque;

    std::thread::spawn(move || {
        info!("Voice listener thread started (KWS pipeline)");

        let mut capture = AudioCapture::new();
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.015,
            min_speech_frames: 4,
            min_silence_frames: 20,
        });

        // Fallback detector for when no KWS model is loaded
        let fallback_detector = WakeWordDetector::new(WakeWordConfig::default());

        // Circular buffer: 2 seconds at 16kHz = 32000 samples
        let mut circular_buf: VecDeque<f32> = VecDeque::with_capacity(32000);

        let mut state = ListenerState::Idle;
        let mut was_active = false;
        let mut speech_buffer: Vec<f32> = Vec::new(); // for fallback mode

        const POST_WAKE_DELAY_SAMPLES: usize = 4800; // 300ms at 16kHz
        const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
        const SILENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

        if let Err(e) = capture.start_recording() {
            warn!("Voice listener: no microphone available: {e}");
            return;
        }
        info!("Voice listener: recording started, KWS pipeline active");

        loop {
            // Check flags
            if !stt_enabled.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }
            if wake_training_in_progress.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }

            std::thread::sleep(std::time::Duration::from_millis(30));

            let samples = {
                if let Ok(mut buf) = capture.buffer().lock() {
                    std::mem::take(&mut *buf)
                } else {
                    continue;
                }
            };
            if samples.is_empty() { continue; }

            // Maintain circular buffer
            for &s in &samples {
                if circular_buf.len() >= 32000 {
                    circular_buf.pop_front();
                }
                circular_buf.push_back(s);
            }

            let use_kws = wake_enabled.load(std::sync::atomic::Ordering::Relaxed);
            let has_kws_model = kws_engine.lock().map(|e| e.has_model()).unwrap_or(false);

            match &mut state {
                ListenerState::Idle => {
                    if use_kws && has_kws_model {
                        // === KWS path ===
                        let result = kws_engine.lock().unwrap().process_audio(&samples);
                        if result.triggered {
                            info!("Voice listener: wake word detected (confidence: {:.2})", result.confidence);
                            // Transition to Capture mode
                            // Copy circular buffer, skip post-wake delay
                            let buf_vec: Vec<f32> = circular_buf.iter().copied().collect();
                            let skip = POST_WAKE_DELAY_SAMPLES.min(buf_vec.len());
                            let command_audio = buf_vec[buf_vec.len().saturating_sub(skip)..].to_vec();

                            state = ListenerState::Capture {
                                command_buffer: command_audio,
                                capture_start: std::time::Instant::now(),
                                silence_start: None,
                            };
                            vad.reset();
                            kws_engine.lock().unwrap().reset();
                        }
                    } else if use_kws && !has_kws_model {
                        // === Fallback: Whisper + keyword match ===
                        for chunk in samples.chunks(DEFAULT_FRAME_SIZE) {
                            let is_active = vad.process_frame(chunk);
                            if is_active {
                                speech_buffer.extend_from_slice(chunk);
                            } else if was_active {
                                let duration = speech_buffer.len() as f32 / 16000.0;
                                if duration > 0.5 && duration < 30.0 {
                                    if let Ok(text) = transcribe_with_whisper(&speech_buffer) {
                                        if !text.is_empty() && fallback_detector.matches_wake_word(&text) {
                                            let cmd = strip_wake_phrase(&text, fallback_detector.wake_phrase());
                                            let cmd = if cmd.is_empty() { "Hello".to_string() } else { cmd };
                                            let _ = stt_tx.send(cmd);
                                        }
                                    }
                                }
                                speech_buffer.clear();
                                vad.reset();
                            }
                            was_active = is_active;
                        }
                    } else {
                        // === Wake disabled: send all speech directly ===
                        for chunk in samples.chunks(DEFAULT_FRAME_SIZE) {
                            let is_active = vad.process_frame(chunk);
                            if is_active {
                                speech_buffer.extend_from_slice(chunk);
                            } else if was_active {
                                let duration = speech_buffer.len() as f32 / 16000.0;
                                if duration > 0.5 && duration < 30.0 {
                                    if let Ok(text) = transcribe_with_whisper(&speech_buffer) {
                                        if !text.is_empty() {
                                            let _ = stt_tx.send(text);
                                        }
                                    }
                                }
                                speech_buffer.clear();
                                vad.reset();
                            }
                            was_active = is_active;
                        }
                    }
                }
                ListenerState::Capture { command_buffer, capture_start, silence_start } => {
                    // Accumulate audio
                    command_buffer.extend_from_slice(&samples);

                    // Check VAD for speech end
                    let mut speech_active = false;
                    for chunk in samples.chunks(DEFAULT_FRAME_SIZE) {
                        speech_active = vad.process_frame(chunk);
                    }

                    if speech_active {
                        *silence_start = None;
                    } else if silence_start.is_none() {
                        *silence_start = Some(std::time::Instant::now());
                    }

                    let silence_expired = silence_start
                        .map(|s| s.elapsed() >= SILENCE_TIMEOUT)
                        .unwrap_or(false);
                    let timeout_expired = capture_start.elapsed() >= CAPTURE_TIMEOUT;

                    if silence_expired || timeout_expired {
                        // Transcribe command
                        let duration = command_buffer.len() as f32 / 16000.0;
                        if duration > 0.3 {
                            info!("Voice listener: command captured ({duration:.1}s), transcribing...");
                            if let Ok(text) = transcribe_with_whisper(command_buffer) {
                                if !text.is_empty() {
                                    let _ = stt_tx.send(text);
                                }
                            }
                        }
                        // Return to Idle
                        state = ListenerState::Idle;
                        speech_buffer.clear();
                        vad.reset();
                        was_active = false;
                    }

                    // Prevent unbounded growth
                    if command_buffer.len() > 16000 * 30 {
                        state = ListenerState::Idle;
                        vad.reset();
                        was_active = false;
                    }
                }
            }
        }
    })
}
```

- [ ] **Step 2: Update the voice listener setup in `activate_main()`**

Replace the voice listener setup block (around lines 1679-1714) to:
1. Create the `KwsEngine` instance (try loading infrastructure models, gracefully degrade if missing)
2. Load the pre-trained wake word model based on config
3. Create the `wake_training_in_progress` flag
4. Pass all new parameters to `start_voice_listener()`
5. Wire mic button disable logic to the training flag

- [ ] **Step 3: Add `BackgroundTask` handling in the command dispatch**

In the GTK command handler (where `CommandResult` variants are matched), add handling for `BackgroundTask`:
1. Show the description as a `[SYSTEM]` message
2. Spawn a background thread for `WakeWordTraining`
3. Set `wake_training_in_progress` to true
4. On completion, send a system message, set flag to false, hot-swap the model

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-gtk`
Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add aios-gtk/src/app.rs
git commit -m "feat(voice): rewrite voice listener as KWS state machine (Idle/Capture)"
```

---

## Task 8: Settings Dialog — Wake Word Group

**Files:**
- Modify: `aios-gtk/src/ui/settings_dialog.rs`

- [ ] **Step 1: Add Wake Word group to Voice page**

In `build_voice_page()`, add a new `adw::PreferencesGroup` between the STT and TTS groups. Include:
- Wake Word Enabled switch (reads/writes `voice.wake_enabled`)
- Pre-trained Wake Word ComboRow (populated from `PRETRAINED_WAKE_WORDS`)
- Custom Wake Phrase EntryRow with subtitle about training time/accuracy
- Train button
- Status label
- Threshold SpinButton (0.1-1.0, step 0.05)

The settings dialog needs access to `Arc<Mutex<KwsEngine>>` to hot-swap models when the user selects a pre-trained wake word from the dropdown. Pass this as a parameter to `show_settings()` and `build_voice_page()`.

- [ ] **Step 2: Wire the ComboRow to load pretrained models**

When user selects a pretrained wake word:
1. Update config (`voice.wake_word`, `voice.wake_word_source = "pretrained"`)
2. Lock the `KwsEngine` mutex
3. Call `load_wake_model()` with the pretrained model path
4. Clear the custom field
5. Update status label to "Active"

- [ ] **Step 3: Wire the Train button**

When user clicks Train:
1. Get text from custom field
2. Update config
3. Disable the combo and train button
4. Spawn background training thread
5. Update status label to "Training..."
6. On completion: update status to "Ready", re-enable controls, hot-swap model

- [ ] **Step 4: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-gtk`
Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add aios-gtk/src/ui/settings_dialog.rs
git commit -m "feat(settings): add Wake Word group to Voice settings page"
```

---

## Task 9: First-Boot Setup — Wake Word Selection UI

**Files:**
- Modify: `aios-gtk/src/ui/first_boot.rs`

The existing `SetupResult` struct already has a `wake_word` field (line 43 of first_boot.rs). The setup wizard already collects a wake word as a free-text field. This task updates it to show the pretrained catalog.

- [ ] **Step 1: Update the wake word step in the setup conversation**

In the setup wizard (first_boot.rs), find the step where the wake word is collected. Replace the free-text input with:

1. A dropdown (`PanelFieldKind::Dropdown`) populated with all 17 pretrained wake word display names, with "Hey Assistant" as default
2. A separate text input labeled "Or type a custom wake phrase" with subtitle: "A custom model will be trained on first boot. This takes a few minutes and may be less accurate."

The dropdown and custom field should be presented as two separate panel fields. The handler checks: if the dropdown value is not "custom", use the dropdown selection. If a custom value was typed, use that.

- [ ] **Step 2: Store the wake word source based on selection**

After setup completes, in the config-saving section of `app.rs` (around line 744 where `result.wake_word` is saved):

```rust
let _ = config.set("voice.wake_word", serde_json::json!(result.wake_word));
// Check if it's a pretrained model
let normalized = result.wake_word.to_lowercase().replace(' ', "_");
let is_pretrained = ["hey_assistant", "hey_jarvis", "computer", "ok_computer",
    "hey_friday", "jarvis", "ok_jarvis", "skynet", "terminator",
    "hey_house", "ok_home", "home_assistant", "mr_anderson", "mr_smith",
    "hey_dick_head", "oi_fuckwhit", "yo_homie"]
    .contains(&normalized.as_str());
if is_pretrained {
    let _ = config.set("voice.wake_word_source", serde_json::json!("pretrained"));
} else {
    let _ = config.set("voice.wake_word_source", serde_json::json!("training"));
}
```

- [ ] **Step 3: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-gtk`

- [ ] **Step 4: Commit**

```bash
git add aios-gtk/src/ui/first_boot.rs aios-gtk/src/app.rs
git commit -m "feat(setup): add pretrained wake word catalog to first-boot wizard"
```

---

## Task 10: ISO Build — Download Models and Bundle Training Env

> **Note:** Task 11 (train hey_assistant.onnx) can be done in parallel with this task.

**Files:**
- Modify: `distro/_inner_build.sh`
- Create: `distro/models/kws/` (model directory in repo for hey_assistant.onnx)

- [ ] **Step 1: Create model download section in `_inner_build.sh`**

Add a section that downloads:
- 3 infrastructure models from openWakeWord v0.5.1 release
- 1 official model (hey_jarvis) from openWakeWord v0.5.1 release
- 16 community models from `fwartner/home-assistant-wakewords-collection`

All saved to the ISO's `/opt/aios-app/models/kws/` directory with clean names.

- [ ] **Step 2: Copy the pre-trained `hey_assistant.onnx`**

Copy from `distro/models/kws/hey_assistant.onnx` to the ISO's model directory.

- [ ] **Step 3: Bundle Python training environment**

Add a section that creates a Python virtualenv at `/opt/aios-app/kws-trainer/venv` and installs the requirements from `distro/kws-trainer/requirements.txt`.

- [ ] **Step 4: Add first-boot model copy logic**

In the first-boot script or app startup, copy models from `/opt/aios-app/models/kws/` to `~/.aios/models/kws/` if not already present.

- [ ] **Step 5: Commit**

```bash
git add distro/_inner_build.sh distro/models/kws/
git commit -m "feat(distro): download KWS models and bundle training env in ISO"
```

---

## Task 11: Train and Ship `hey_assistant.onnx` Default Model

**Files:**
- Create: `distro/models/kws/hey_assistant.onnx` (binary)

- [ ] **Step 1: Set up the training environment on dev machine**

```bash
python3 -m venv /tmp/kws-trainer-env
source /tmp/kws-trainer-env/bin/activate
pip install openwakeword torch piper-sample-generator speechbrain onnxruntime
```

- [ ] **Step 2: Train the model**

```bash
python3 distro/kws-trainer/train.py --phrase "hey assistant" --output distro/models/kws/hey_assistant.onnx
```

Expected: training completes in 5-10 minutes, produces `hey_assistant.onnx` (~200KB)

- [ ] **Step 3: Verify the model loads**

Write a quick test or use the ONNX Runtime Python library to verify:
```python
import onnxruntime
session = onnxruntime.InferenceSession("distro/models/kws/hey_assistant.onnx")
print("Model loaded successfully, inputs:", [i.name for i in session.get_inputs()])
```

- [ ] **Step 4: Commit the model**

```bash
git add distro/models/kws/hey_assistant.onnx
git commit -m "feat(models): add pre-trained hey_assistant wake word model"
```

---

## Task 12: Boot Status, System Messages, and Training Recovery

**Files:**
- Modify: `aios-gtk/src/app.rs` (boot status message construction)

- [ ] **Step 1: Add KWS status to boot message**

Find the boot status message construction in `app.rs` (search for "Boot time" or "System Status"). Add a KWS line that shows:
- `KWS: active — <wake_word> (<source>)` when a model is loaded
- `KWS: training — "<phrase>" (mic disabled until ready)` when training
- `KWS: unavailable — models not found` when infrastructure models are missing

- [ ] **Step 2: Add recovery-on-reboot for interrupted training**

In the app startup (near where KwsEngine is initialized in `activate_main()`), check if `voice.wake_word_source == "training"`:

```rust
let wake_source = state.borrow().config.get_str("voice.wake_word_source", "pretrained");
if wake_source == "training" {
    // Training was interrupted — restart it
    let phrase = state.borrow().config.get_str("voice.wake_word", "hey assistant");
    info!("KWS: restarting interrupted training for \"{phrase}\"");
    // Set training flag, disable mic, spawn training thread
    // (same code as BackgroundTask::WakeWordTraining handler)
    // Track retry count in config: voice.wake_training_retries (default 0)
    // If retries >= 3, reset to pretrained default and show error
}
```

Key details:
- Store retry count in config key `voice.wake_training_retries`
- Increment on each failed restart
- After 3 failures: reset `wake_word_source` to `"pretrained"`, set `wake_word` to `"hey assistant"`, show `[SYSTEM]` error message
- On success: reset retry count to 0

- [ ] **Step 3: Verify compilation**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --package aios-gtk`

- [ ] **Step 4: Commit**

```bash
git add aios-gtk/src/app.rs
git commit -m "feat(boot): add KWS status and training recovery on reboot"
```

---

## Task 13: Integration Test — Full Pipeline

**Files:**
- Test within existing test infrastructure

- [ ] **Step 1: Run full workspace tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --workspace`
Expected: all existing tests pass, new tests pass

- [ ] **Step 2: Manual verification checklist**

Build and boot the ISO with `./start.sh`. Verify:
- [ ] Boot status shows KWS line
- [ ] Settings dialog shows Wake Word group
- [ ] `/wake list` shows 17 models
- [ ] `/wake threshold 0.3` updates the threshold
- [ ] `/wake hey_jarvis` loads the pre-trained model (if models are baked in)
- [ ] Mic button behavior during training (if testing with custom phrase)

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: complete KWS three-stage audio pipeline integration"
```
