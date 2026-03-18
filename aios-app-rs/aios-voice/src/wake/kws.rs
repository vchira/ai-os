//! ONNX-based keyword spotting (KWS) engine using the openWakeWord pipeline.
//!
//! The inference pipeline processes audio through three ONNX models:
//!
//! 1. **melspectrogram.onnx** — converts 80 ms of raw audio (1280 samples at 16 kHz)
//!    into mel-frequency features.
//! 2. **embedding_model.onnx** — maps accumulated mel features into a 96-dimensional
//!    embedding vector.
//! 3. **wake word model** (e.g. `hey_assistant.onnx`) — classifies the embedding and
//!    produces a confidence score in [0.0, 1.0].
//!
//! Audio is buffered internally and fed in 80 ms chunks. When a wake word model is
//! loaded and the confidence exceeds the configurable threshold, the result is marked
//! as `triggered`.

use std::path::Path;

use ort::session::Session;
use ort::value::TensorRef;
use tracing;

use crate::error::VoiceError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// 16 kHz * 80 ms = 1280 samples per chunk.
const CHUNK_SAMPLES: usize = 1280;

/// Maximum number of embedding vectors kept in the ring buffer.
const MAX_EMBEDDINGS: usize = 16;

/// Default detection threshold.
const DEFAULT_THRESHOLD: f32 = 0.5;

// ---------------------------------------------------------------------------
// KwsResult
// ---------------------------------------------------------------------------

/// Result of processing an audio chunk through the KWS pipeline.
#[derive(Debug, Clone, Copy)]
pub struct KwsResult {
    /// Raw confidence score from the wake word model (0.0 -- 1.0).
    pub confidence: f32,
    /// Whether the confidence exceeded the detection threshold.
    pub triggered: bool,
}

// ---------------------------------------------------------------------------
// KwsEngine
// ---------------------------------------------------------------------------

/// ONNX-based wake word detection engine.
///
/// Create with [`KwsEngine::new`], then load a wake word model via
/// [`KwsEngine::load_wake_model`] before calling [`KwsEngine::process_audio`].
pub struct KwsEngine {
    /// Mel spectrogram session (infrastructure).
    mel_session: Session,
    /// Embedding model session (infrastructure).
    emb_session: Session,
    /// Optional wake word classification session.
    wake_session: Option<Session>,
    /// Human-readable name of the currently loaded wake word.
    wake_word: String,

    /// Audio sample accumulator (partial chunk).
    audio_buf: Vec<f32>,
    /// Accumulated mel feature frames (each frame is a 1-D vector).
    mel_buf: Vec<Vec<f32>>,
    /// Ring buffer of embedding vectors.
    emb_buf: Vec<Vec<f32>>,

    /// Detection threshold in [0.0, 1.0].
    threshold: f32,
}

impl KwsEngine {
    /// Create a new KWS engine.
    ///
    /// `models_dir` must contain an `infrastructure/` subdirectory with
    /// `melspectrogram.onnx` and `embedding_model.onnx`.
    ///
    /// Calls `ort::init_from()` to load the ONNX Runtime shared library
    /// dynamically, then configures the CPU execution provider.
    ///
    /// The library is searched in this order:
    /// 1. `ORT_DYLIB_PATH` environment variable
    /// 2. `/opt/aios-app/lib/libonnxruntime.so`
    /// 3. System library path (LD_LIBRARY_PATH)
    pub fn new(models_dir: &Path) -> Result<Self, VoiceError> {
        // Load the ONNX Runtime shared library dynamically.
        // With the `load-dynamic` feature, the library is NOT linked at
        // compile time — it must be present at runtime.
        let lib_path = std::env::var("ORT_DYLIB_PATH")
            .unwrap_or_else(|_| "/opt/aios-app/lib/libonnxruntime.so".to_string());

        // Check if the library file exists before trying to load it.
        // ort::init_from may panic on some platforms if the file is missing.
        if !std::path::Path::new(&lib_path).exists() {
            return Err(VoiceError::Kws(format!(
                "ONNX Runtime library not found at {lib_path}"
            )));
        }

        // Check infrastructure models exist before loading ORT
        let infra = models_dir.join("infrastructure");
        if !infra.join("melspectrogram.onnx").exists() || !infra.join("embedding_model.onnx").exists() {
            return Err(VoiceError::Kws(format!(
                "KWS infrastructure models not found in {}",
                infra.display()
            )));
        }

        ort::init_from(lib_path)
            .map_err(|e| VoiceError::Kws(format!("ONNX Runtime library load failed: {e}")))?
            .with_execution_providers([ort::ep::CPU::default().build()])
            .commit();

        let mel_path = infra.join("melspectrogram.onnx");
        let mel_session = Session::builder()
            .map_err(|e| VoiceError::Kws(format!("session builder (mel): {e}")))?
            .with_intra_threads(1)
            .map_err(|e| VoiceError::Kws(format!("intra threads (mel): {e}")))?
            .commit_from_file(&mel_path)
            .map_err(|e| VoiceError::Kws(format!("load melspectrogram model at {}: {e}", mel_path.display())))?;

        let emb_path = infra.join("embedding_model.onnx");
        let emb_session = Session::builder()
            .map_err(|e| VoiceError::Kws(format!("session builder (emb): {e}")))?
            .with_intra_threads(1)
            .map_err(|e| VoiceError::Kws(format!("intra threads (emb): {e}")))?
            .commit_from_file(&emb_path)
            .map_err(|e| VoiceError::Kws(format!("load embedding model at {}: {e}", emb_path.display())))?;

        tracing::info!("KWS infrastructure models loaded from {}", infra.display());

        Ok(Self {
            mel_session,
            emb_session,
            wake_session: None,
            wake_word: String::new(),
            audio_buf: Vec::with_capacity(CHUNK_SAMPLES),
            mel_buf: Vec::new(),
            emb_buf: Vec::new(),
            threshold: DEFAULT_THRESHOLD,
        })
    }

    /// Load a wake word ONNX model.
    ///
    /// `path` is the full path to the `.onnx` file.
    /// `wake_word` is the human-readable name (e.g. "Hey Jarvis").
    pub fn load_wake_model(&mut self, path: &Path, wake_word: &str) -> Result<(), VoiceError> {
        let session = Session::builder()
            .map_err(|e| VoiceError::Kws(format!("session builder (wake): {e}")))?
            .with_intra_threads(1)
            .map_err(|e| VoiceError::Kws(format!("intra threads (wake): {e}")))?
            .commit_from_file(path)
            .map_err(|e| VoiceError::Kws(format!("load wake model at {}: {e}", path.display())))?;

        self.wake_session = Some(session);
        self.wake_word = wake_word.to_string();
        self.reset();

        tracing::info!("KWS wake word model loaded: \"{}\" from {}", wake_word, path.display());
        Ok(())
    }

    /// Process a buffer of f32 audio samples (mono, 16 kHz).
    ///
    /// Returns a [`KwsResult`] with the latest confidence and trigger state.
    /// If no wake model is loaded, returns confidence 0.0 / triggered false.
    pub fn process_audio(&mut self, samples: &[f32]) -> KwsResult {
        let no_detection = KwsResult { confidence: 0.0, triggered: false };

        if self.wake_session.is_none() {
            return no_detection;
        }

        // Append incoming samples to the internal buffer.
        self.audio_buf.extend_from_slice(samples);

        let mut last_result = no_detection;

        // Process as many complete 80 ms chunks as available.
        while self.audio_buf.len() >= CHUNK_SAMPLES {
            let chunk: Vec<f32> = self.audio_buf.drain(..CHUNK_SAMPLES).collect();

            match self.process_chunk(&chunk) {
                Ok(result) => last_result = result,
                Err(e) => {
                    tracing::warn!("KWS chunk processing error: {e}");
                    // Continue — one bad frame shouldn't stop the pipeline.
                }
            }
        }

        last_result
    }

    /// Clear all internal buffers to start fresh.
    pub fn reset(&mut self) {
        self.audio_buf.clear();
        self.mel_buf.clear();
        self.emb_buf.clear();
    }

    /// Whether a wake word model is currently loaded.
    pub fn has_model(&self) -> bool {
        self.wake_session.is_some()
    }

    /// The name of the currently loaded wake word (empty if none).
    pub fn wake_word(&self) -> &str {
        &self.wake_word
    }

    /// Set the detection threshold, clamped to [0.0, 1.0].
    pub fn set_threshold(&mut self, t: f32) {
        self.threshold = t.clamp(0.0, 1.0);
    }

    /// Current detection threshold.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Run a single 80 ms chunk through the full pipeline.
    fn process_chunk(&mut self, chunk: &[f32]) -> Result<KwsResult, VoiceError> {
        // Step 1: mel spectrogram
        let mel_features = self.run_mel(chunk)?;
        self.mel_buf.push(mel_features);

        // Cap the mel buffer to prevent unbounded growth (keep last 128 frames).
        if self.mel_buf.len() > 128 {
            self.mel_buf.drain(..self.mel_buf.len() - 128);
        }

        // Step 2: embedding (needs accumulated mel frames)
        let embedding = self.run_embedding()?;
        self.emb_buf.push(embedding);

        // Cap the embedding buffer to prevent unbounded growth.
        if self.emb_buf.len() > MAX_EMBEDDINGS {
            let excess = self.emb_buf.len() - MAX_EMBEDDINGS;
            self.emb_buf.drain(..excess);
        }

        // Step 3: wake word classification on the latest embedding
        let confidence = self.run_wake()?;
        let triggered = confidence >= self.threshold;

        Ok(KwsResult { confidence, triggered })
    }

    /// Run the mel spectrogram model on a single 80 ms chunk.
    fn run_mel(&mut self, chunk: &[f32]) -> Result<Vec<f32>, VoiceError> {
        let input = TensorRef::from_array_view(([1_usize, CHUNK_SAMPLES], chunk))
            .map_err(|e| VoiceError::Kws(format!("mel input tensor: {e}")))?;

        let outputs = self.mel_session
            .run(ort::inputs![input])
            .map_err(|e| VoiceError::Kws(format!("mel inference: {e}")))?;

        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VoiceError::Kws(format!("mel output extract: {e}")))?;

        Ok(data.to_vec())
    }

    /// Run the embedding model on the accumulated mel features.
    fn run_embedding(&mut self) -> Result<Vec<f32>, VoiceError> {
        // Flatten all mel frames into a single contiguous buffer and present
        // as shape [1, total_features].
        let flat: Vec<f32> = self.mel_buf.iter().flat_map(|f| f.iter().copied()).collect();
        let total = flat.len();

        let input = TensorRef::from_array_view(([1_usize, total], &*flat))
            .map_err(|e| VoiceError::Kws(format!("emb input tensor: {e}")))?;

        let outputs = self.emb_session
            .run(ort::inputs![input])
            .map_err(|e| VoiceError::Kws(format!("emb inference: {e}")))?;

        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VoiceError::Kws(format!("emb output extract: {e}")))?;

        Ok(data.to_vec())
    }

    /// Run the wake word model on the latest embedding.
    fn run_wake(&mut self) -> Result<f32, VoiceError> {
        let session = self.wake_session.as_mut()
            .ok_or_else(|| VoiceError::Kws("no wake model loaded".into()))?;

        let embedding = self.emb_buf.last()
            .ok_or_else(|| VoiceError::Kws("no embeddings available".into()))?;

        let emb_len = embedding.len();
        let input = TensorRef::from_array_view(([1_usize, emb_len], &**embedding))
            .map_err(|e| VoiceError::Kws(format!("wake input tensor: {e}")))?;

        let outputs = session
            .run(ort::inputs![input])
            .map_err(|e| VoiceError::Kws(format!("wake inference: {e}")))?;

        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VoiceError::Kws(format!("wake output extract: {e}")))?;

        // The model outputs a single confidence value (or the last element for
        // multi-output models).
        let confidence = data.last().copied().unwrap_or(0.0);
        Ok(confidence)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kws_result_default_values() {
        let r = KwsResult { confidence: 0.0, triggered: false };
        assert!(!r.triggered);
        assert!((r.confidence - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn kws_result_triggered_when_high_confidence() {
        let r = KwsResult { confidence: 0.95, triggered: true };
        assert!(r.triggered);
        assert!(r.confidence > 0.9);
    }

    #[test]
    fn kws_result_is_copy() {
        let a = KwsResult { confidence: 0.5, triggered: false };
        let b = a; // Copy
        assert!((a.confidence - b.confidence).abs() < f32::EPSILON);
        assert_eq!(a.triggered, b.triggered);
    }

    #[test]
    fn kws_result_debug_format() {
        let r = KwsResult { confidence: 0.42, triggered: true };
        let dbg = format!("{:?}", r);
        assert!(dbg.contains("0.42"));
        assert!(dbg.contains("true"));
    }

    #[test]
    fn threshold_clamping_above() {
        // Simulate threshold clamping without creating a full engine (which
        // requires model files). We test the clamping logic directly.
        let val = 1.5_f32.clamp(0.0, 1.0);
        assert!((val - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn threshold_clamping_below() {
        let val = (-0.3_f32).clamp(0.0, 1.0);
        assert!((val - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn threshold_clamping_in_range() {
        let val = 0.7_f32.clamp(0.0, 1.0);
        assert!((val - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn chunk_samples_constant() {
        // 16000 Hz * 0.08 s = 1280
        assert_eq!(CHUNK_SAMPLES, 1280);
    }

    #[test]
    fn max_embeddings_constant() {
        assert_eq!(MAX_EMBEDDINGS, 16);
    }

    #[test]
    fn default_threshold_is_half() {
        assert!((DEFAULT_THRESHOLD - 0.5).abs() < f32::EPSILON);
    }
}
