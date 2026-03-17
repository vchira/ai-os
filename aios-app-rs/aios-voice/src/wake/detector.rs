//! Wake word detector — energy + keyword matching approach.
//!
//! This module provides a simple but effective wake word detector that does
//! not require a neural network. It works by:
//!
//! 1. Continuously monitoring audio energy levels (RMS).
//! 2. When energy exceeds a threshold (someone is speaking), buffering audio.
//! 3. After the speech segment ends (energy drops), running a simple keyword
//!    match against the configured wake phrase.
//! 4. If match confidence exceeds the threshold, emitting a
//!    [`WakeWordEvent::Detected`].
//! 5. Then capturing the NEXT speech segment and emitting
//!    [`WakeWordEvent::SpeechCaptured`] with the audio data.

/// Configuration for wake word detection.
#[derive(Debug, Clone)]
pub struct WakeWordConfig {
    /// The wake word phrase (default: "hey aios").
    pub wake_phrase: String,
    /// Energy threshold for speech detection (0.0 - 1.0, default: 0.02).
    pub energy_threshold: f32,
    /// How long to wait after wake word before capturing speech (ms).
    pub post_wake_delay_ms: u64,
    /// Maximum speech capture duration after wake word (ms).
    pub max_capture_ms: u64,
    /// Silence duration to end speech capture (ms).
    pub silence_timeout_ms: u64,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            wake_phrase: "hey aios".into(),
            energy_threshold: 0.02,
            post_wake_delay_ms: 300,
            max_capture_ms: 15000, // 15 seconds max
            silence_timeout_ms: 1500,
        }
    }
}

/// Events emitted by the wake word detector.
#[derive(Debug, Clone)]
pub enum WakeWordEvent {
    /// Wake word detected, starting to capture speech.
    Detected,
    /// Speech captured after wake word. Contains audio samples (f32, mono).
    SpeechCaptured(Vec<f32>),
    /// Speech capture timed out (silence too long or max duration reached).
    Timeout,
}

/// Wake word detector that uses keyword matching against STT transcriptions.
///
/// The detector splits the configured wake phrase into keywords and checks
/// whether all keywords appear in order in a given transcription. This is
/// intentionally simple — it relies on the STT engine for the heavy lifting
/// and only performs a lightweight keyword-order check.
///
/// # Example
///
/// ```
/// use aios_voice::wake::{WakeWordDetector, WakeWordConfig};
///
/// let detector = WakeWordDetector::new(WakeWordConfig::default());
/// assert!(detector.matches_wake_word("hey aios how are you"));
/// assert!(!detector.matches_wake_word("hey google"));
/// ```
pub struct WakeWordDetector {
    config: WakeWordConfig,
    /// Simple keyword fragments for matching (lowercase, split by space).
    keywords: Vec<String>,
}

impl WakeWordDetector {
    /// Create a new wake word detector with the given configuration.
    pub fn new(config: WakeWordConfig) -> Self {
        let keywords = config
            .wake_phrase
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        Self { config, keywords }
    }

    /// Check if an audio segment (as text from STT) contains the wake word.
    ///
    /// Uses simple substring/keyword matching. All keywords from the wake
    /// phrase must appear in order in the transcription.
    ///
    /// # Examples
    ///
    /// ```
    /// use aios_voice::wake::{WakeWordDetector, WakeWordConfig};
    ///
    /// let detector = WakeWordDetector::new(WakeWordConfig::default());
    ///
    /// // All keywords present in order.
    /// assert!(detector.matches_wake_word("hey aios"));
    /// assert!(detector.matches_wake_word("HEY AIOS"));
    /// assert!(detector.matches_wake_word("hey aios how are you"));
    ///
    /// // Missing keyword or wrong order.
    /// assert!(!detector.matches_wake_word("hey google"));
    /// assert!(!detector.matches_wake_word("aios hey"));
    /// ```
    pub fn matches_wake_word(&self, transcription: &str) -> bool {
        let lower = transcription.to_lowercase();
        // Strip punctuation from each word, then match whole words in order.
        let words: Vec<String> = lower
            .split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect())
            .collect();
        let mut word_idx = 0;
        for kw in &self.keywords {
            let mut found = false;
            while word_idx < words.len() {
                if words[word_idx] == *kw {
                    word_idx += 1;
                    found = true;
                    break;
                }
                word_idx += 1;
            }
            if !found {
                return false;
            }
        }
        true
    }

    /// Get the configured wake phrase.
    pub fn wake_phrase(&self) -> &str {
        &self.config.wake_phrase
    }

    /// Get the energy threshold for speech detection.
    pub fn energy_threshold(&self) -> f32 {
        self.config.energy_threshold
    }

    /// Get the silence timeout in milliseconds.
    pub fn silence_timeout_ms(&self) -> u64 {
        self.config.silence_timeout_ms
    }

    /// Get the maximum capture duration in milliseconds.
    pub fn max_capture_ms(&self) -> u64 {
        self.config.max_capture_ms
    }

    /// Get the post-wake delay in milliseconds.
    pub fn post_wake_delay_ms(&self) -> u64 {
        self.config.post_wake_delay_ms
    }

    /// Get a reference to the full configuration.
    pub fn config(&self) -> &WakeWordConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_wake_phrase() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(detector.matches_wake_word("hey aios"));
    }

    #[test]
    fn matches_wake_phrase_with_trailing_speech() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(detector.matches_wake_word("hey aios how are you"));
    }

    #[test]
    fn matches_wake_phrase_case_insensitive() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(detector.matches_wake_word("HEY AIOS"));
    }

    #[test]
    fn matches_wake_phrase_mixed_case() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(detector.matches_wake_word("Hey AiOS, what time is it?"));
    }

    #[test]
    fn rejects_missing_keyword() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(!detector.matches_wake_word("hey google"));
    }

    #[test]
    fn rejects_wrong_order() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(!detector.matches_wake_word("aios hey"));
    }

    #[test]
    fn rejects_partial_match() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(!detector.matches_wake_word("hey"));
    }

    #[test]
    fn rejects_empty_transcription() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(!detector.matches_wake_word(""));
    }

    #[test]
    fn custom_wake_phrase() {
        let config = WakeWordConfig {
            wake_phrase: "ok computer".into(),
            ..Default::default()
        };
        let detector = WakeWordDetector::new(config);
        assert!(detector.matches_wake_word("ok computer do something"));
        assert!(!detector.matches_wake_word("hey aios"));
        assert!(!detector.matches_wake_word("computer ok"));
    }

    #[test]
    fn default_config_values_are_sane() {
        let config = WakeWordConfig::default();
        assert_eq!(config.wake_phrase, "hey aios");
        assert!(config.energy_threshold > 0.0);
        assert!(config.energy_threshold < 1.0);
        assert!(config.post_wake_delay_ms > 0);
        assert!(config.max_capture_ms > config.silence_timeout_ms);
        assert!(config.silence_timeout_ms > 0);
    }

    #[test]
    fn accessor_methods_return_config_values() {
        let config = WakeWordConfig {
            wake_phrase: "hello world".into(),
            energy_threshold: 0.05,
            post_wake_delay_ms: 500,
            max_capture_ms: 20000,
            silence_timeout_ms: 2000,
        };
        let detector = WakeWordDetector::new(config);
        assert_eq!(detector.wake_phrase(), "hello world");
        assert!((detector.energy_threshold() - 0.05).abs() < f32::EPSILON);
        assert_eq!(detector.silence_timeout_ms(), 2000);
        assert_eq!(detector.max_capture_ms(), 20000);
        assert_eq!(detector.post_wake_delay_ms(), 500);
    }

    #[test]
    fn wake_word_event_variants_exist() {
        // Ensure all variants can be constructed.
        let _detected = WakeWordEvent::Detected;
        let _captured = WakeWordEvent::SpeechCaptured(vec![0.1, 0.2, 0.3]);
        let _timeout = WakeWordEvent::Timeout;
    }

    #[test]
    fn single_word_wake_phrase() {
        let config = WakeWordConfig {
            wake_phrase: "jarvis".into(),
            ..Default::default()
        };
        let detector = WakeWordDetector::new(config);
        assert!(detector.matches_wake_word("jarvis please help"));
        assert!(detector.matches_wake_word("JARVIS"));
        assert!(!detector.matches_wake_word("alexa do something"));
    }

    #[test]
    fn three_word_wake_phrase() {
        let config = WakeWordConfig {
            wake_phrase: "hey there aios".into(),
            ..Default::default()
        };
        let detector = WakeWordDetector::new(config);
        assert!(detector.matches_wake_word("hey there aios what's up"));
        assert!(!detector.matches_wake_word("hey aios"));
        assert!(!detector.matches_wake_word("there hey aios"));
    }

    // -- Additional edge-case tests --

    #[test]
    fn matches_with_punctuation() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        // Punctuation doesn't break the substring match.
        assert!(detector.matches_wake_word("hey, aios!"));
        assert!(detector.matches_wake_word("hey... aios?"));
        assert!(detector.matches_wake_word("\"hey aios\""));
    }

    #[test]
    fn very_long_transcription_with_wake_word_buried() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        let prefix = "a ".repeat(500);
        let transcription = format!("{prefix}hey aios do something");
        assert!(detector.matches_wake_word(&transcription));
    }

    #[test]
    fn empty_wake_phrase_matches_everything() {
        let config = WakeWordConfig {
            wake_phrase: "".into(),
            ..Default::default()
        };
        let detector = WakeWordDetector::new(config);
        // Empty keyword list means all keywords are trivially found.
        assert!(detector.matches_wake_word("anything at all"));
        assert!(detector.matches_wake_word(""));
    }

    #[test]
    fn config_with_zero_energy_threshold() {
        let config = WakeWordConfig {
            energy_threshold: 0.0,
            ..Default::default()
        };
        let detector = WakeWordDetector::new(config);
        assert!((detector.energy_threshold() - 0.0).abs() < f32::EPSILON);
        // Should still match wake words.
        assert!(detector.matches_wake_word("hey aios"));
    }

    #[test]
    fn config_with_very_high_max_capture_ms() {
        let config = WakeWordConfig {
            max_capture_ms: 600_000, // 10 minutes
            ..Default::default()
        };
        let detector = WakeWordDetector::new(config);
        assert_eq!(detector.max_capture_ms(), 600_000);
    }

    #[test]
    fn words_with_extra_in_between_may_match_as_substrings() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        // "hey um aios" — "hey" is found, then scanning from position after "hey"
        // finds "aios" in "um aios". The keywords appear in order so it matches.
        // This tests the actual behavior of the substring-based matching.
        assert!(detector.matches_wake_word("hey um aios"));
    }

    #[test]
    fn wake_word_not_at_start() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(detector.matches_wake_word("I said hey aios please help"));
    }

    #[test]
    fn wake_phrase_repeated_twice() {
        let detector = WakeWordDetector::new(WakeWordConfig::default());
        assert!(detector.matches_wake_word("hey aios hey aios"));
    }

    #[test]
    fn config_is_accessible_via_config_method() {
        let config = WakeWordConfig {
            wake_phrase: "test phrase".into(),
            energy_threshold: 0.1,
            post_wake_delay_ms: 200,
            max_capture_ms: 10000,
            silence_timeout_ms: 1000,
        };
        let detector = WakeWordDetector::new(config);
        let cfg = detector.config();
        assert_eq!(cfg.wake_phrase, "test phrase");
        assert!((cfg.energy_threshold - 0.1).abs() < f32::EPSILON);
        assert_eq!(cfg.post_wake_delay_ms, 200);
        assert_eq!(cfg.max_capture_ms, 10000);
        assert_eq!(cfg.silence_timeout_ms, 1000);
    }

    #[test]
    fn wake_word_with_unicode_phrase() {
        let config = WakeWordConfig {
            wake_phrase: "h\u{00e9} aios".into(), // "hé aios"
            ..Default::default()
        };
        let detector = WakeWordDetector::new(config);
        assert!(detector.matches_wake_word("h\u{00e9} aios do something"));
        assert!(!detector.matches_wake_word("hey aios")); // 'e' != 'é'
    }
}
