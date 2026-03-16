//! eSpeak-ng TTS backend — lightweight formant synthesis.
//!
//! Shells out to the `espeak-ng` binary.  This is a fallback TTS engine that
//! works on virtually any hardware and supports an enormous range of
//! languages, including Romanian.
//!
//! The quality is lower than Piper but it requires no model files and is
//! always available on a Debian system with `espeak-ng` installed.

use std::process::Command;

use tracing::{debug, warn};

use super::{TtsBackend, TtsEngine, VoiceGender, VoiceInfo, VoiceQuality};
use crate::error::VoiceError;

/// Name of the CLI binary to invoke.
const ESPEAK_CLI: &str = "espeak-ng";

/// Default voice (English).
const DEFAULT_VOICE: &str = "en";

/// eSpeak-ng output sample rate.
const SAMPLE_RATE: u32 = 22050;

/// eSpeak-ng TTS engine — subprocess-based.
pub struct EspeakTts {
    /// Currently active voice identifier (language code like "en", "de", "ro").
    voice_id: String,
}

impl EspeakTts {
    /// Create a new eSpeak engine with the default English voice.
    pub fn new() -> Self {
        Self {
            voice_id: DEFAULT_VOICE.to_string(),
        }
    }

    /// Query `espeak-ng --voices` and parse the output into [`VoiceInfo`] entries.
    fn query_system_voices() -> Vec<VoiceInfo> {
        let output = match Command::new(ESPEAK_CLI).arg("--voices").output() {
            Ok(o) => o,
            Err(e) => {
                warn!("failed to query espeak-ng voices: {e}");
                return Vec::new();
            }
        };

        if !output.status.success() {
            return Vec::new();
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut voices = Vec::new();

        // The first line is a header; skip it.
        // Format: "Pty  Language  Age/Gender  VoiceName   File   Other Languages"
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }

            let language = parts[1].to_string();
            let gender_str = parts[2];
            let voice_name = parts[3].to_string();

            let gender = if gender_str.contains('F') {
                VoiceGender::Female
            } else if gender_str.contains('M') {
                VoiceGender::Male
            } else {
                VoiceGender::Neutral
            };

            voices.push(VoiceInfo {
                id: language.clone(),
                name: voice_name,
                language,
                gender,
                quality: VoiceQuality::Low,
                sample_rate: SAMPLE_RATE,
            });
        }

        voices
    }
}

impl Default for EspeakTts {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsEngine for EspeakTts {
    fn name(&self) -> &str {
        "espeak-ng"
    }

    fn backend(&self) -> TtsBackend {
        TtsBackend::Espeak
    }

    fn synthesize(&self, text: &str) -> Result<Vec<f32>, VoiceError> {
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let mut cmd = Command::new(ESPEAK_CLI);
        cmd.arg("-v")
            .arg(&self.voice_id)
            .arg("--stdout") // Output WAV to stdout.
            .arg(text)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        debug!(voice = %self.voice_id, "running espeak-ng");

        let output = cmd.output().map_err(|e| {
            VoiceError::TtsSynthesize(format!("failed to run {ESPEAK_CLI}: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VoiceError::TtsSynthesize(format!(
                "{ESPEAK_CLI} exited with {}: {stderr}",
                output.status
            )));
        }

        // espeak-ng --stdout produces a WAV file.  Parse it with hound.
        let cursor = std::io::Cursor::new(output.stdout);
        let mut reader = hound::WavReader::new(cursor)
            .map_err(|e| VoiceError::TtsSynthesize(format!("failed to parse WAV output: {e}")))?;

        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / max_val)
                    .collect()
            }
            hound::SampleFormat::Float => {
                reader
                    .samples::<f32>()
                    .filter_map(|s| s.ok())
                    .collect()
            }
        };

        // If stereo, mix down to mono.
        let mono = if spec.channels > 1 {
            let ch = spec.channels as usize;
            samples
                .chunks(ch)
                .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                .collect()
        } else {
            samples
        };

        debug!(samples = mono.len(), "espeak-ng synthesis complete");
        Ok(mono)
    }

    fn set_voice(&mut self, voice_id: &str) -> Result<(), VoiceError> {
        // Validate by attempting a dry run.  espeak-ng will fail if the voice
        // is not installed.
        let output = Command::new(ESPEAK_CLI)
            .arg("-v")
            .arg(voice_id)
            .arg("-q") // Quiet — don't produce audio.
            .arg("test")
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .output()
            .map_err(|e| {
                VoiceError::TtsModel(format!("failed to run {ESPEAK_CLI}: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VoiceError::TtsModel(format!(
                "espeak-ng voice '{voice_id}' not available: {stderr}"
            )));
        }

        self.voice_id = voice_id.to_string();
        debug!(voice = %self.voice_id, "espeak-ng voice set");
        Ok(())
    }

    fn current_voice(&self) -> &str {
        &self.voice_id
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn list_voices(&self) -> Vec<VoiceInfo> {
        Self::query_system_voices()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_espeak_has_default_voice() {
        let engine = EspeakTts::new();
        assert_eq!(engine.current_voice(), DEFAULT_VOICE);
        assert_eq!(engine.sample_rate(), SAMPLE_RATE);
        assert_eq!(engine.name(), "espeak-ng");
        assert_eq!(engine.backend(), TtsBackend::Espeak);
    }

    #[test]
    fn synthesize_empty_text_returns_empty() {
        let engine = EspeakTts::new();
        let result = engine.synthesize("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
