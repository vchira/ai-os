//! Voice Activity Detection (VAD) — simple energy-based approach.
//!
//! Uses RMS (root mean square) energy to determine whether a frame of audio
//! contains speech or silence.  This is intentionally lightweight — no neural
//! network required.

/// Standard frame size: 30 ms at 16 kHz = 480 samples.
pub const DEFAULT_FRAME_SIZE: usize = 480;

/// Default energy threshold for speech detection.
///
/// Typical speech produces RMS values of 0.02–0.10; silence is < 0.005.
/// A threshold of 0.01 provides a reasonable balance between sensitivity
/// and false-positive rejection.
pub const DEFAULT_THRESHOLD: f32 = 0.01;

/// Configuration for voice activity detection.
#[derive(Debug, Clone)]
pub struct VadConfig {
    /// RMS energy threshold — frames above this are considered speech.
    pub threshold: f32,
    /// Number of consecutive speech frames required to trigger detection.
    pub min_speech_frames: usize,
    /// Number of consecutive silence frames before speech is considered ended.
    pub min_silence_frames: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_THRESHOLD,
            min_speech_frames: 3,
            min_silence_frames: 10,
        }
    }
}

/// Compute the RMS (root mean square) energy of an audio frame.
///
/// Returns 0.0 for empty slices.
///
/// # Examples
///
/// ```
/// use aios_voice::audio::vad::rms_energy;
///
/// let silence = vec![0.0f32; 480];
/// assert_eq!(rms_energy(&silence), 0.0);
///
/// let tone = vec![0.5f32; 480];
/// assert!((rms_energy(&tone) - 0.5).abs() < 1e-6);
/// ```
pub fn rms_energy(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = frame.iter().map(|&s| s * s).sum();
    (sum_sq / frame.len() as f32).sqrt()
}

/// Determine whether a single frame contains speech.
///
/// Returns `true` if the RMS energy of `frame` exceeds `threshold`.
///
/// # Examples
///
/// ```
/// use aios_voice::audio::vad::is_speech;
///
/// let loud = vec![0.1f32; 480];
/// assert!(is_speech(&loud, 0.01));
///
/// let quiet = vec![0.001f32; 480];
/// assert!(!is_speech(&quiet, 0.01));
/// ```
pub fn is_speech(frame: &[f32], threshold: f32) -> bool {
    rms_energy(frame) > threshold
}

/// Stateful voice activity detector.
///
/// Tracks consecutive speech/silence frames to provide stable detection
/// with hysteresis, avoiding rapid on/off toggling.
#[derive(Debug)]
pub struct VoiceActivityDetector {
    config: VadConfig,
    /// Number of consecutive speech frames seen.
    speech_count: usize,
    /// Number of consecutive silence frames seen.
    silence_count: usize,
    /// Whether we are currently in the "speech active" state.
    active: bool,
}

impl VoiceActivityDetector {
    /// Create a new VAD with the given configuration.
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            speech_count: 0,
            silence_count: 0,
            active: false,
        }
    }

    /// Create a new VAD with default settings.
    pub fn with_defaults() -> Self {
        Self::new(VadConfig::default())
    }

    /// Process a single frame and return whether speech is currently active.
    ///
    /// The detector applies hysteresis: it requires `min_speech_frames`
    /// consecutive speech frames to transition to the active state, and
    /// `min_silence_frames` consecutive silence frames to leave it.
    pub fn process_frame(&mut self, frame: &[f32]) -> bool {
        if is_speech(frame, self.config.threshold) {
            self.speech_count += 1;
            self.silence_count = 0;

            if !self.active && self.speech_count >= self.config.min_speech_frames {
                self.active = true;
            }
        } else {
            self.silence_count += 1;
            self.speech_count = 0;

            if self.active && self.silence_count >= self.config.min_silence_frames {
                self.active = false;
            }
        }

        self.active
    }

    /// Whether the detector is currently in the "speech active" state.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Reset the detector to its initial state.
    pub fn reset(&mut self) {
        self.speech_count = 0;
        self.silence_count = 0;
        self.active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_energy_of_silence_is_zero() {
        let frame = vec![0.0f32; DEFAULT_FRAME_SIZE];
        assert_eq!(rms_energy(&frame), 0.0);
    }

    #[test]
    fn rms_energy_of_empty_is_zero() {
        assert_eq!(rms_energy(&[]), 0.0);
    }

    #[test]
    fn rms_energy_of_constant_signal() {
        let frame = vec![0.5f32; DEFAULT_FRAME_SIZE];
        let energy = rms_energy(&frame);
        assert!((energy - 0.5).abs() < 1e-6);
    }

    #[test]
    fn is_speech_detects_loud_frame() {
        let frame = vec![0.1f32; DEFAULT_FRAME_SIZE];
        assert!(is_speech(&frame, DEFAULT_THRESHOLD));
    }

    #[test]
    fn is_speech_rejects_quiet_frame() {
        let frame = vec![0.001f32; DEFAULT_FRAME_SIZE];
        assert!(!is_speech(&frame, DEFAULT_THRESHOLD));
    }

    #[test]
    fn vad_requires_consecutive_frames() {
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.01,
            min_speech_frames: 3,
            min_silence_frames: 2,
        });

        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];

        // First two speech frames — not yet active.
        assert!(!vad.process_frame(&speech));
        assert!(!vad.process_frame(&speech));

        // Third speech frame — now active.
        assert!(vad.process_frame(&speech));

        // One silence frame — still active (need 2).
        assert!(vad.process_frame(&silence));

        // Second silence frame — now inactive.
        assert!(!vad.process_frame(&silence));
    }

    #[test]
    fn vad_reset_clears_state() {
        let mut vad = VoiceActivityDetector::with_defaults();
        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];

        for _ in 0..5 {
            vad.process_frame(&speech);
        }
        assert!(vad.is_active());

        vad.reset();
        assert!(!vad.is_active());
    }
}
