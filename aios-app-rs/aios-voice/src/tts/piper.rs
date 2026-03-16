//! Piper TTS backend — high-quality VITS neural synthesis.
//!
//! Shells out to the `piper` binary.  Text is piped via stdin, raw 16-bit
//! PCM audio is captured from stdout.
//!
//! # Voice models
//!
//! Each voice consists of an ONNX model and a JSON config file stored under:
//!
//! ```text
//! ~/.aios/models/piper/<voice_id>/
//!     <voice_id>.onnx
//!     <voice_id>.onnx.json
//! ```
//!
//! The default voice is `en_US-amy-medium` (female, English US).

use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

use tracing::{debug, warn};

use super::{TtsBackend, TtsEngine, VoiceInfo};
use crate::error::VoiceError;

/// Default Piper voice.
const DEFAULT_VOICE: &str = "en_US-amy-medium";

/// Typical Piper output sample rate.
const DEFAULT_SAMPLE_RATE: u32 = 22050;

/// Name of the CLI binary to invoke.
const PIPER_CLI: &str = "piper";

/// Piper TTS engine — subprocess-based.
pub struct PiperTts {
    /// Currently active voice identifier.
    voice_id: String,
    /// Output sample rate (voice-dependent).
    sample_rate: u32,
    /// Base directory for voice models.
    models_dir: PathBuf,
}

impl PiperTts {
    /// Create a new Piper engine with the default voice.
    pub fn new() -> Self {
        let models_dir = dirs_model_path();
        Self {
            voice_id: DEFAULT_VOICE.to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            models_dir,
        }
    }

    /// Resolve the ONNX model path for a voice.
    fn model_path(&self, voice_id: &str) -> PathBuf {
        self.models_dir
            .join(voice_id)
            .join(format!("{voice_id}.onnx"))
    }

    /// Check whether a voice model is present on disk.
    fn voice_exists(&self, voice_id: &str) -> bool {
        self.model_path(voice_id).exists()
    }

    /// Read the sample rate from the voice JSON config if available.
    fn read_sample_rate_from_config(&self, voice_id: &str) -> Option<u32> {
        let config_path = self
            .models_dir
            .join(voice_id)
            .join(format!("{voice_id}.onnx.json"));

        let contents = std::fs::read_to_string(config_path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
        value["audio"]["sample_rate"].as_u64().map(|r| r as u32)
    }
}

impl Default for PiperTts {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsEngine for PiperTts {
    fn name(&self) -> &str {
        "piper"
    }

    fn backend(&self) -> TtsBackend {
        TtsBackend::Piper
    }

    fn synthesize(&self, text: &str) -> Result<Vec<f32>, VoiceError> {
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let model_path = self.model_path(&self.voice_id);

        let mut cmd = Command::new(PIPER_CLI);
        cmd.arg("--model")
            .arg(&model_path)
            .arg("--output_raw")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        debug!(voice = %self.voice_id, model = %model_path.display(), "running piper");

        let mut child = cmd
            .spawn()
            .map_err(|e| VoiceError::TtsSynthesize(format!("failed to spawn {PIPER_CLI}: {e}")))?;

        // Write text to piper's stdin.
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| VoiceError::TtsSynthesize(format!("stdin write error: {e}")))?;
            // Dropping stdin closes it, signaling EOF to piper.
        }

        let output = child
            .wait_with_output()
            .map_err(|e| VoiceError::TtsSynthesize(format!("{PIPER_CLI} wait error: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VoiceError::TtsSynthesize(format!(
                "{PIPER_CLI} exited with {}: {stderr}",
                output.status
            )));
        }

        // Piper outputs raw 16-bit signed PCM (little-endian).
        let raw_bytes = &output.stdout;
        if raw_bytes.len() % 2 != 0 {
            warn!("piper output has odd byte count, truncating last byte");
        }

        let num_samples = raw_bytes.len() / 2;
        let mut samples = Vec::with_capacity(num_samples);
        for chunk in raw_bytes.chunks_exact(2) {
            let s16 = i16::from_le_bytes([chunk[0], chunk[1]]);
            samples.push(s16 as f32 / 32768.0);
        }

        debug!(samples = samples.len(), "piper synthesis complete");
        Ok(samples)
    }

    fn set_voice(&mut self, voice_id: &str) -> Result<(), VoiceError> {
        if !self.voice_exists(voice_id) {
            return Err(VoiceError::TtsModel(format!(
                "piper voice model not found: {} (expected at {})",
                voice_id,
                self.model_path(voice_id).display()
            )));
        }

        // Try to read the sample rate from the config.
        if let Some(sr) = self.read_sample_rate_from_config(voice_id) {
            self.sample_rate = sr;
        } else {
            self.sample_rate = DEFAULT_SAMPLE_RATE;
        }

        self.voice_id = voice_id.to_string();
        debug!(voice = %self.voice_id, sample_rate = self.sample_rate, "piper voice set");
        Ok(())
    }

    fn current_voice(&self) -> &str {
        &self.voice_id
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn list_voices(&self) -> Vec<VoiceInfo> {
        // Return voices from the catalog that have models installed locally.
        super::catalog::VOICES
            .iter()
            .filter(|v| self.voice_exists(&v.id))
            .cloned()
            .collect()
    }
}

/// Return the default Piper model directory: `~/.aios/models/piper/`.
fn dirs_model_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home)
        .join(".aios")
        .join("models")
        .join("piper")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_piper_has_default_voice() {
        let engine = PiperTts::new();
        assert_eq!(engine.current_voice(), DEFAULT_VOICE);
        assert_eq!(engine.sample_rate(), DEFAULT_SAMPLE_RATE);
        assert_eq!(engine.name(), "piper");
        assert_eq!(engine.backend(), TtsBackend::Piper);
    }

    #[test]
    fn synthesize_empty_text_returns_empty() {
        let engine = PiperTts::new();
        let result = engine.synthesize("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn set_voice_rejects_missing_model() {
        let mut engine = PiperTts::new();
        let result = engine.set_voice("nonexistent-voice-xyz");
        assert!(result.is_err());
    }
}
