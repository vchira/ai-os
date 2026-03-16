//! Text-to-speech engine abstraction and backends.
//!
//! The [`TtsEngine`] trait defines a uniform interface for speech synthesis.
//! Backends can be swapped at runtime via [`create_tts_engine`].
//!
//! # Backends
//!
//! * [`TtsBackend::Piper`] — high-quality VITS-based synthesis via the `piper` binary.
//! * [`TtsBackend::Espeak`] — lightweight fallback using `espeak-ng`.

pub mod catalog;
pub mod espeak;
pub mod piper;

use serde::{Deserialize, Serialize};

use crate::error::VoiceError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Available TTS backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TtsBackend {
    /// Piper TTS — high-quality VITS neural synthesis.
    Piper,
    /// eSpeak-ng — lightweight formant synthesis, wide language coverage.
    Espeak,
}

impl std::fmt::Display for TtsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Piper => f.write_str("piper"),
            Self::Espeak => f.write_str("espeak"),
        }
    }
}

/// Gender of a TTS voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceGender {
    Male,
    Female,
    Neutral,
}

impl std::fmt::Display for VoiceGender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Male => f.write_str("male"),
            Self::Female => f.write_str("female"),
            Self::Neutral => f.write_str("neutral"),
        }
    }
}

/// Quality tier of a voice model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceQuality {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for VoiceQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => f.write_str("low"),
            Self::Medium => f.write_str("medium"),
            Self::High => f.write_str("high"),
        }
    }
}

/// Metadata for a single TTS voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    /// Unique identifier (e.g. "en_US-amy-medium").
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// BCP-47 language code (e.g. "en_US", "de_DE", "ro_RO").
    pub language: String,
    /// Gender of the voice.
    pub gender: VoiceGender,
    /// Quality tier.
    pub quality: VoiceQuality,
    /// Typical output sample rate in Hz.
    pub sample_rate: u32,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstraction over text-to-speech engines.
///
/// Implementations must be `Send + Sync` so they can be held inside an
/// `Arc<Mutex<>>` or moved across threads.
pub trait TtsEngine: Send + Sync {
    /// Human-readable engine name (e.g. "piper", "espeak-ng").
    fn name(&self) -> &str;

    /// Which backend this engine represents.
    fn backend(&self) -> TtsBackend;

    /// Synthesize speech from text, returning raw mono f32 PCM samples.
    fn synthesize(&self, text: &str) -> Result<Vec<f32>, VoiceError>;

    /// Set the active voice by identifier.
    fn set_voice(&mut self, voice_id: &str) -> Result<(), VoiceError>;

    /// Get the identifier of the currently active voice.
    fn current_voice(&self) -> &str;

    /// Output sample rate for synthesized audio (Hz).
    fn sample_rate(&self) -> u32;

    /// List all voices available for this backend.
    fn list_voices(&self) -> Vec<VoiceInfo>;
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create a TTS engine for the requested backend.
///
/// The engine is returned ready to use with a default voice.
pub fn create_tts_engine(backend: TtsBackend) -> Box<dyn TtsEngine> {
    match backend {
        TtsBackend::Piper => Box::new(piper::PiperTts::new()),
        TtsBackend::Espeak => Box::new(espeak::EspeakTts::new()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tts_backend_display() {
        assert_eq!(TtsBackend::Piper.to_string(), "piper");
        assert_eq!(TtsBackend::Espeak.to_string(), "espeak");
    }

    #[test]
    fn tts_backend_roundtrips_json() {
        let json = serde_json::to_string(&TtsBackend::Piper).unwrap();
        assert_eq!(json, "\"piper\"");
        let back: TtsBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TtsBackend::Piper);
    }

    #[test]
    fn voice_gender_display() {
        assert_eq!(VoiceGender::Female.to_string(), "female");
        assert_eq!(VoiceGender::Male.to_string(), "male");
        assert_eq!(VoiceGender::Neutral.to_string(), "neutral");
    }

    #[test]
    fn factory_creates_piper() {
        let engine = create_tts_engine(TtsBackend::Piper);
        assert_eq!(engine.backend(), TtsBackend::Piper);
    }

    #[test]
    fn factory_creates_espeak() {
        let engine = create_tts_engine(TtsBackend::Espeak);
        assert_eq!(engine.backend(), TtsBackend::Espeak);
    }
}
