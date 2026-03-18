//! Voice subsystem error types.
//!
//! [`VoiceError`] covers every failure mode in audio capture/playback,
//! speech-to-text, and text-to-speech pipelines.

/// Errors produced by the AiOS voice subsystem.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    /// Failed to enumerate or open an audio device.
    #[error("audio device error: {0}")]
    AudioDevice(String),

    /// Error during audio capture (recording).
    #[error("capture error: {0}")]
    Capture(String),

    /// Error during audio playback.
    #[error("playback error: {0}")]
    Playback(String),

    /// Failed to load or initialize an STT model.
    #[error("stt model error: {0}")]
    SttModel(String),

    /// Transcription failed.
    #[error("stt transcription error: {0}")]
    SttTranscribe(String),

    /// Failed to load or initialize a TTS model/voice.
    #[error("tts model error: {0}")]
    TtsModel(String),

    /// Speech synthesis failed.
    #[error("tts synthesis error: {0}")]
    TtsSynthesize(String),

    /// Filesystem or process I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The requested backend is not available on this system.
    #[error("unsupported backend: {0}")]
    UnsupportedBackend(String),

    /// KWS model loading or inference failure.
    #[error("kws error: {0}")]
    Kws(String),
}

/// Convenience alias used throughout this crate.
pub type Result<T> = std::result::Result<T, VoiceError>;

impl From<VoiceError> for aios_core::AiosError {
    fn from(e: VoiceError) -> Self {
        aios_core::AiosError::Voice(e.to_string())
    }
}
