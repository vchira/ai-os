//! Audio I/O — capture, playback, and voice activity detection.
//!
//! All audio is processed as mono `f32` samples at 16 kHz unless noted
//! otherwise (TTS output may use a different sample rate and is resampled
//! before playback when necessary).

pub mod capture;
pub mod playback;
pub mod vad;

pub use capture::AudioCapture;
pub use playback::AudioPlayback;
pub use vad::{is_speech, rms_energy, VadConfig};
