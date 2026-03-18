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

    // -- Additional tests --

    #[test]
    fn silence_is_not_speech() {
        // Pure silence (all zeros) should never be detected as speech.
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];
        assert!(!is_speech(&silence, DEFAULT_THRESHOLD));
        assert!(!is_speech(&silence, 0.001)); // Even with very low threshold
        assert_eq!(rms_energy(&silence), 0.0);
    }

    #[test]
    fn loud_signal_is_speech() {
        // A loud signal should always be detected as speech.
        let loud = vec![0.5f32; DEFAULT_FRAME_SIZE];
        assert!(is_speech(&loud, DEFAULT_THRESHOLD));
        assert!(is_speech(&loud, 0.1));
        assert!(is_speech(&loud, 0.4));

        // Very loud signal.
        let very_loud = vec![1.0f32; DEFAULT_FRAME_SIZE];
        assert!(is_speech(&very_loud, DEFAULT_THRESHOLD));
        let energy = rms_energy(&very_loud);
        assert!((energy - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vad_reset_clears_all_counters() {
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.01,
            min_speech_frames: 2,
            min_silence_frames: 2,
        });
        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];

        // Build up some speech count.
        vad.process_frame(&speech);
        vad.process_frame(&speech);
        assert!(vad.is_active());

        // Now add some silence frames.
        vad.process_frame(&silence);

        // Reset — everything should go back to initial state.
        vad.reset();
        assert!(!vad.is_active());

        // After reset, it should require min_speech_frames again to activate.
        vad.process_frame(&speech);
        assert!(!vad.is_active()); // Only 1 speech frame, need 2.
        vad.process_frame(&speech);
        assert!(vad.is_active()); // Now 2 consecutive speech frames.
    }

    #[test]
    fn vad_config_defaults_are_sane() {
        let config = VadConfig::default();

        // Threshold should be positive and less than typical speech RMS.
        assert!(config.threshold > 0.0);
        assert!(config.threshold < 0.1, "Threshold too high for typical speech detection");

        // Should require at least 1 frame for both speech and silence.
        assert!(config.min_speech_frames >= 1);
        assert!(config.min_silence_frames >= 1);

        // Silence frames should be more than speech frames (speech starts
        // faster than it ends, to avoid cutting off early).
        assert!(
            config.min_silence_frames >= config.min_speech_frames,
            "Silence frames ({}) should be >= speech frames ({}) to avoid premature cutoff",
            config.min_silence_frames,
            config.min_speech_frames,
        );

        // Verify the specific default values.
        assert!((config.threshold - DEFAULT_THRESHOLD).abs() < f32::EPSILON);
        assert_eq!(config.min_speech_frames, 3);
        assert_eq!(config.min_silence_frames, 10);
    }

    #[test]
    fn hysteresis_prevents_jitter() {
        // Alternating speech/silence frames should NOT cause rapid toggling.
        // The VAD should remain stable due to the consecutive frame requirements.
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.01,
            min_speech_frames: 3,
            min_silence_frames: 3,
        });

        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];

        // Start inactive.
        assert!(!vad.is_active());

        // Alternate speech/silence — should never activate because we never
        // get 3 consecutive speech frames.
        for _ in 0..20 {
            vad.process_frame(&speech);
            vad.process_frame(&silence);
        }
        assert!(
            !vad.is_active(),
            "VAD should not activate with alternating speech/silence frames"
        );

        // Now supply 3 consecutive speech frames to activate.
        vad.process_frame(&speech);
        vad.process_frame(&speech);
        vad.process_frame(&speech);
        assert!(vad.is_active());

        // Alternate again — should remain active because we never get 3
        // consecutive silence frames.
        for _ in 0..20 {
            vad.process_frame(&silence);
            vad.process_frame(&speech);
        }
        assert!(
            vad.is_active(),
            "VAD should remain active with alternating silence/speech frames"
        );

        // Now supply 3 consecutive silence frames to deactivate.
        vad.process_frame(&silence);
        vad.process_frame(&silence);
        vad.process_frame(&silence);
        assert!(!vad.is_active());
    }

    // -- Comprehensive additional tests --

    #[test]
    fn vad_with_default_config_starts_inactive() {
        let vad = VoiceActivityDetector::with_defaults();
        assert!(!vad.is_active());
    }

    #[test]
    fn vad_new_with_custom_config_starts_inactive() {
        let config = VadConfig {
            threshold: 0.05,
            min_speech_frames: 5,
            min_silence_frames: 20,
        };
        let vad = VoiceActivityDetector::new(config);
        assert!(!vad.is_active());
    }

    #[test]
    fn rms_energy_of_max_signal() {
        // A frame of all 1.0 values has RMS = 1.0.
        let frame = vec![1.0f32; DEFAULT_FRAME_SIZE];
        let energy = rms_energy(&frame);
        assert!((energy - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rms_energy_of_negative_signal() {
        // RMS doesn't care about sign — squared values are positive.
        let frame = vec![-0.5f32; DEFAULT_FRAME_SIZE];
        let energy = rms_energy(&frame);
        assert!((energy - 0.5).abs() < 1e-6);
    }

    #[test]
    fn rms_energy_of_mixed_polarity_signal() {
        // +0.5 and -0.5 alternating — RMS should still be 0.5.
        let frame: Vec<f32> = (0..DEFAULT_FRAME_SIZE)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let energy = rms_energy(&frame);
        assert!((energy - 0.5).abs() < 1e-6);
    }

    #[test]
    fn rms_energy_of_single_sample() {
        assert!((rms_energy(&[0.3]) - 0.3).abs() < 1e-6);
        assert!((rms_energy(&[-0.7]) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn rms_energy_of_zeros_is_zero() {
        let frame = vec![0.0f32; 1000];
        assert_eq!(rms_energy(&frame), 0.0);
    }

    #[test]
    fn rms_energy_known_value() {
        // RMS of [3.0, 4.0] = sqrt((9 + 16) / 2) = sqrt(12.5)
        let frame = [3.0f32, 4.0];
        let energy = rms_energy(&frame);
        let expected = (12.5_f32).sqrt();
        assert!((energy - expected).abs() < 1e-5);
    }

    #[test]
    fn is_speech_at_exact_threshold() {
        // At exactly the threshold, should NOT detect (uses > not >=).
        let threshold = 0.5_f32;
        let frame = vec![threshold; DEFAULT_FRAME_SIZE];
        assert!(!is_speech(&frame, threshold));
    }

    #[test]
    fn is_speech_just_above_threshold() {
        let threshold = 0.5_f32;
        let frame = vec![threshold + 0.001; DEFAULT_FRAME_SIZE];
        assert!(is_speech(&frame, threshold));
    }

    #[test]
    fn is_speech_with_empty_frame() {
        // rms_energy of empty is 0.0, which is not > any positive threshold.
        assert!(!is_speech(&[], 0.01));
        assert!(!is_speech(&[], 0.0));
    }

    #[test]
    fn hysteresis_needs_min_speech_frames_consecutive_custom() {
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.01,
            min_speech_frames: 4,
            min_silence_frames: 2,
        });

        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];

        // 3 speech frames — not enough (need 4).
        assert!(!vad.process_frame(&speech));
        assert!(!vad.process_frame(&speech));
        assert!(!vad.process_frame(&speech));

        // 1 silence frame resets the speech count.
        assert!(!vad.process_frame(&silence));

        // Start again — need 4 consecutive speech frames.
        assert!(!vad.process_frame(&speech)); // 1
        assert!(!vad.process_frame(&speech)); // 2
        assert!(!vad.process_frame(&speech)); // 3
        assert!(vad.process_frame(&speech));  // 4 — now active
        assert!(vad.is_active());
    }

    #[test]
    fn hysteresis_needs_min_silence_frames_to_end_speech() {
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.01,
            min_speech_frames: 2,
            min_silence_frames: 5,
        });

        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];

        // Activate.
        vad.process_frame(&speech);
        vad.process_frame(&speech);
        assert!(vad.is_active());

        // 4 silence frames — still active (need 5).
        assert!(vad.process_frame(&silence)); // 1
        assert!(vad.process_frame(&silence)); // 2
        assert!(vad.process_frame(&silence)); // 3
        assert!(vad.process_frame(&silence)); // 4

        // 5th silence frame — now inactive.
        assert!(!vad.process_frame(&silence));
        assert!(!vad.is_active());
    }

    #[test]
    fn hysteresis_silence_interrupted_by_speech_resets_silence_count() {
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.01,
            min_speech_frames: 2,
            min_silence_frames: 3,
        });

        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];

        // Activate.
        vad.process_frame(&speech);
        vad.process_frame(&speech);
        assert!(vad.is_active());

        // 2 silence frames, then 1 speech frame — silence count resets.
        vad.process_frame(&silence);
        vad.process_frame(&silence);
        assert!(vad.is_active()); // Still active, not 3 silence frames yet.

        vad.process_frame(&speech); // Resets silence count.
        assert!(vad.is_active());

        // Need 3 more consecutive silence frames to deactivate.
        vad.process_frame(&silence); // 1
        vad.process_frame(&silence); // 2
        assert!(vad.is_active());
        vad.process_frame(&silence); // 3 — now inactive.
        assert!(!vad.is_active());
    }

    #[test]
    fn reset_then_reactivate() {
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.01,
            min_speech_frames: 2,
            min_silence_frames: 2,
        });

        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];

        // Activate.
        vad.process_frame(&speech);
        vad.process_frame(&speech);
        assert!(vad.is_active());

        // Reset.
        vad.reset();
        assert!(!vad.is_active());

        // Must re-satisfy min_speech_frames from scratch.
        assert!(!vad.process_frame(&speech)); // 1
        assert!(vad.process_frame(&speech));  // 2 — active again
        assert!(vad.is_active());
    }

    #[test]
    fn vad_with_min_speech_frames_one() {
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.01,
            min_speech_frames: 1,
            min_silence_frames: 1,
        });

        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];

        // Single speech frame activates immediately.
        assert!(vad.process_frame(&speech));
        assert!(vad.is_active());

        // Single silence frame deactivates immediately.
        assert!(!vad.process_frame(&silence));
        assert!(!vad.is_active());
    }

    #[test]
    fn vad_config_clone() {
        let config = VadConfig {
            threshold: 0.07,
            min_speech_frames: 5,
            min_silence_frames: 15,
        };
        let cloned = config.clone();
        assert!((cloned.threshold - 0.07).abs() < f32::EPSILON);
        assert_eq!(cloned.min_speech_frames, 5);
        assert_eq!(cloned.min_silence_frames, 15);
    }

    #[test]
    fn default_frame_size_is_30ms_at_16khz() {
        // 16000 Hz * 0.030 s = 480 samples.
        assert_eq!(DEFAULT_FRAME_SIZE, 480);
    }

    #[test]
    fn process_many_silence_frames_stays_inactive() {
        let mut vad = VoiceActivityDetector::with_defaults();
        let silence = vec![0.0f32; DEFAULT_FRAME_SIZE];

        for _ in 0..100 {
            assert!(!vad.process_frame(&silence));
        }
        assert!(!vad.is_active());
    }

    #[test]
    fn process_many_speech_frames_stays_active() {
        let mut vad = VoiceActivityDetector::with_defaults();
        let speech = vec![0.1f32; DEFAULT_FRAME_SIZE];

        for _ in 0..100 {
            vad.process_frame(&speech);
        }
        assert!(vad.is_active());
    }

    #[test]
    fn vad_debug_format() {
        let vad = VoiceActivityDetector::with_defaults();
        let dbg = format!("{:?}", vad);
        assert!(dbg.contains("VoiceActivityDetector"));
        assert!(dbg.contains("active"));
    }
}
