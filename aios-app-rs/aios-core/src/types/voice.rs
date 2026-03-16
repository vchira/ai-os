//! Voice-related types — STT / TTS backends, hardware profiling, transcription.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

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

/// Quality tier of a TTS voice model.
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

/// Available TTS backend engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TtsBackend {
    /// Piper — fast VITS-based local TTS (default).
    Piper,
    /// eSpeak — lightweight fallback, lower quality.
    Espeak,
    /// Coqui XTTS — high quality, requires GPU.
    CoquiXtts,
}

impl std::fmt::Display for TtsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Piper => f.write_str("piper"),
            Self::Espeak => f.write_str("espeak"),
            Self::CoquiXtts => f.write_str("coqui-xtts"),
        }
    }
}

/// Available STT backend engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SttBackend {
    /// faster-whisper — OpenAI Whisper via CTranslate2 (default).
    Whisper,
    /// Vosk — lightweight offline recognition.
    Vosk,
}

impl std::fmt::Display for SttBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Whisper => f.write_str("whisper"),
            Self::Vosk => f.write_str("vosk"),
        }
    }
}

// ---------------------------------------------------------------------------
// VoiceInfo
// ---------------------------------------------------------------------------

/// Metadata for a single TTS voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    /// Unique voice identifier (e.g. `en_US-amy-medium`).
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// BCP-47 language tag (e.g. `en_US`, `de_DE`).
    pub language: String,
    /// Voice gender.
    pub gender: VoiceGender,
    /// Model quality tier.
    pub quality: VoiceQuality,
}

// ---------------------------------------------------------------------------
// Transcription types
// ---------------------------------------------------------------------------

/// A single timed segment within a transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    /// Start time in seconds.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
    /// Transcribed text for this segment.
    pub text: String,
    /// Model confidence in `[0.0, 1.0]`.
    pub confidence: f64,
}

/// Complete result of a speech-to-text transcription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// Full transcribed text.
    pub text: String,
    /// Detected or specified language code.
    pub language: String,
    /// Overall confidence in `[0.0, 1.0]`.
    pub confidence: f64,
    /// Per-segment breakdown.
    pub segments: Vec<TranscriptionSegment>,
}

// ---------------------------------------------------------------------------
// HardwareProfile
// ---------------------------------------------------------------------------

/// Snapshot of the host's hardware capabilities, used to select
/// the best voice backend for the current machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    /// Whether a GPU (CUDA / ROCm) is available.
    pub has_gpu: bool,
    /// Total system RAM in megabytes.
    pub ram_mb: u64,
    /// Number of logical CPU cores.
    pub cpu_cores: u32,
}

impl HardwareProfile {
    /// Return the recommended TTS backend for this hardware.
    ///
    /// * GPU + >= 4 GB RAM → [`TtsBackend::CoquiXtts`] (best quality)
    /// * >= 2 GB RAM        → [`TtsBackend::Piper`]     (good quality, fast)
    /// * otherwise          → [`TtsBackend::Espeak`]     (minimal resources)
    pub fn recommended_tts(&self) -> TtsBackend {
        if self.has_gpu && self.ram_mb >= 4096 {
            TtsBackend::CoquiXtts
        } else if self.ram_mb >= 2048 {
            TtsBackend::Piper
        } else {
            TtsBackend::Espeak
        }
    }

    /// Return the recommended STT backend for this hardware.
    ///
    /// * >= 4 GB RAM → [`SttBackend::Whisper`] (higher accuracy)
    /// * otherwise   → [`SttBackend::Vosk`]    (lower memory footprint)
    pub fn recommended_stt(&self) -> SttBackend {
        if self.ram_mb >= 4096 {
            SttBackend::Whisper
        } else {
            SttBackend::Vosk
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gender_serialization() {
        assert_eq!(serde_json::to_string(&VoiceGender::Female).unwrap(), "\"female\"");
        let g: VoiceGender = serde_json::from_str("\"male\"").unwrap();
        assert_eq!(g, VoiceGender::Male);
    }

    #[test]
    fn hardware_recommends_coqui_with_gpu() {
        let hw = HardwareProfile {
            has_gpu: true,
            ram_mb: 8192,
            cpu_cores: 8,
        };
        assert_eq!(hw.recommended_tts(), TtsBackend::CoquiXtts);
        assert_eq!(hw.recommended_stt(), SttBackend::Whisper);
    }

    #[test]
    fn hardware_recommends_piper_without_gpu() {
        let hw = HardwareProfile {
            has_gpu: false,
            ram_mb: 4096,
            cpu_cores: 4,
        };
        assert_eq!(hw.recommended_tts(), TtsBackend::Piper);
        assert_eq!(hw.recommended_stt(), SttBackend::Whisper);
    }

    #[test]
    fn hardware_recommends_espeak_low_ram() {
        let hw = HardwareProfile {
            has_gpu: false,
            ram_mb: 1024,
            cpu_cores: 2,
        };
        assert_eq!(hw.recommended_tts(), TtsBackend::Espeak);
        assert_eq!(hw.recommended_stt(), SttBackend::Vosk);
    }

    #[test]
    fn voice_info_roundtrips() {
        let v = VoiceInfo {
            id: "en_US-amy-medium".into(),
            name: "Amy".into(),
            language: "en_US".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Medium,
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: VoiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "en_US-amy-medium");
        assert_eq!(back.gender, VoiceGender::Female);
    }

    #[test]
    fn transcription_result_roundtrips() {
        let tr = TranscriptionResult {
            text: "hello world".into(),
            language: "en".into(),
            confidence: 0.95,
            segments: vec![TranscriptionSegment {
                start: 0.0,
                end: 1.5,
                text: "hello world".into(),
                confidence: 0.95,
            }],
        };
        let json = serde_json::to_string(&tr).unwrap();
        let back: TranscriptionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.segments.len(), 1);
    }
}
