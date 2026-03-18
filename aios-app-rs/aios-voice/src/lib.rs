//! AiOS voice subsystem — speech-to-text, text-to-speech, and audio I/O.
//!
//! This crate provides an abstraction layer over multiple STT and TTS backends
//! so they can be swapped at runtime based on hardware capabilities or user
//! preference.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌──────────────┐     ┌──────────────┐
//! │ AudioCapture │────▶│  SttEngine   │────▶│ Transcription│
//! └─────────────┘     │  (trait)      │     │   Result     │
//!                     ├──────────────┤     └──────────────┘
//!                     │ WhisperStt   │
//!                     └──────────────┘
//!
//! ┌─────────────┐     ┌──────────────┐
//! │  TtsEngine   │────▶│AudioPlayback │
//! │  (trait)      │     └──────────────┘
//! ├──────────────┤
//! │ PiperTts     │
//! │ EspeakTts    │
//! └──────────────┘
//! ```
//!
//! # Modules
//!
//! * [`audio`] — Audio capture, playback, and voice activity detection (VAD).
//! * [`stt`] — Speech-to-text engine trait and backends (Whisper).
//! * [`tts`] — Text-to-speech engine trait and backends (Piper, eSpeak-ng).
//! * [`wake`] — Wake word detection (always-listening trigger phrase).
//! * [`controller`] — High-level orchestrator that wires everything together.
//! * [`error`] — Voice-specific error types.

pub mod audio;
pub mod controller;
pub mod error;
pub mod stt;
pub mod tts;
pub mod wake;

// Re-export the most commonly used items at the crate root.
pub use controller::VoiceController;
pub use error::{Result, VoiceError};
pub use stt::{SttBackend, SttEngine, TranscriptionResult};
pub use tts::{TtsBackend, TtsEngine, VoiceInfo};
pub use wake::{WakeWordConfig, WakeWordDetector, WakeWordEvent, PretrainedModel, PRETRAINED_WAKE_WORDS, find_pretrained, pretrained_model_path, KwsEngine, KwsResult, KwsTrainer};
