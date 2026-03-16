//! Speech-to-text engine abstraction and backends.
//!
//! The [`SttEngine`] trait defines a uniform interface for transcription.
//! Backends can be swapped at runtime via [`create_stt_engine`].
//!
//! # Backends
//!
//! * [`SttBackend::Whisper`] — shells out to `whisper-cpp-cli` for local
//!   Whisper inference (multilingual, accent-aware).

pub mod whisper;

use serde::{Deserialize, Serialize};

use crate::error::VoiceError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Available STT backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SttBackend {
    /// OpenAI Whisper (via whisper.cpp CLI).
    Whisper,
}

impl std::fmt::Display for SttBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Whisper => f.write_str("whisper"),
        }
    }
}

/// Result of a transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// Transcribed text.
    pub text: String,
    /// Detected or forced language code (e.g. "en", "de", "ro").
    pub language: Option<String>,
    /// Transcription duration in seconds (wall-clock time, not audio length).
    pub duration_secs: f32,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstraction over speech-to-text engines.
///
/// Implementations must be `Send + Sync` so they can be held inside an
/// `Arc<Mutex<>>` or moved across threads.
pub trait SttEngine: Send + Sync {
    /// Human-readable engine name (e.g. "whisper-cpp").
    fn name(&self) -> &str;

    /// Which backend this engine represents.
    fn backend(&self) -> SttBackend;

    /// Load (or switch) the model.
    ///
    /// `model_size` is backend-specific — for Whisper it is one of
    /// `tiny`, `base`, `small`, `medium`, `large`.
    fn load_model(&mut self, model_size: &str) -> Result<(), VoiceError>;

    /// Whether a model has been loaded and is ready for transcription.
    fn is_loaded(&self) -> bool;

    /// Transcribe audio samples (mono f32 at 16 kHz).
    ///
    /// `language` is an optional BCP-47 code to force the recognition
    /// language.  When `None`, the engine performs automatic detection.
    fn transcribe(
        &self,
        audio: &[f32],
        language: Option<&str>,
    ) -> Result<TranscriptionResult, VoiceError>;

    /// List model sizes available for this backend.
    fn available_models(&self) -> Vec<String>;
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create an STT engine for the requested backend.
///
/// The engine is returned in an unloaded state — call
/// [`SttEngine::load_model`] before transcribing.
pub fn create_stt_engine(backend: SttBackend) -> Box<dyn SttEngine> {
    match backend {
        SttBackend::Whisper => Box::new(whisper::WhisperStt::new()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stt_backend_display() {
        assert_eq!(SttBackend::Whisper.to_string(), "whisper");
    }

    #[test]
    fn stt_backend_roundtrips_json() {
        let json = serde_json::to_string(&SttBackend::Whisper).unwrap();
        assert_eq!(json, "\"whisper\"");
        let back: SttBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SttBackend::Whisper);
    }

    #[test]
    fn factory_creates_whisper() {
        let engine = create_stt_engine(SttBackend::Whisper);
        assert_eq!(engine.backend(), SttBackend::Whisper);
        assert!(!engine.is_loaded());
    }
}
