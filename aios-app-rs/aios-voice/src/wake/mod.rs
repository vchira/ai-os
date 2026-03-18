//! Wake word detection — always-listening trigger phrase detection.
//!
//! Instead of push-to-talk, the microphone stays on (when enabled) and
//! continuously listens for a configurable trigger phrase (e.g., "Hey AiOS").
//! After the wake word is detected, the next speech segment is captured and
//! forwarded to STT.
//!
//! The detection pipeline:
//!
//! 1. Monitor audio energy (RMS) to detect speech segments.
//! 2. When a speech segment ends, run a phonetic/keyword match against the
//!    configured wake phrase.
//! 3. If the wake word is detected, emit [`WakeWordEvent::Detected`].
//! 4. Capture the *next* speech segment and emit
//!    [`WakeWordEvent::SpeechCaptured`].

pub mod detector;
pub mod kws;
pub mod pretrained;
pub mod trainer;

pub use detector::{WakeWordConfig, WakeWordDetector, WakeWordEvent};
pub use kws::{KwsEngine, KwsResult};
pub use pretrained::{PretrainedModel, PRETRAINED_WAKE_WORDS, find_pretrained, pretrained_model_path};
pub use trainer::KwsTrainer;
