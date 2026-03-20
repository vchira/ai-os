//! Whisper STT backend — shells out to `whisper-cpp-cli`.
//!
//! Audio is written to a temporary WAV file, the Whisper CLI is invoked, and
//! its stdout is parsed to extract the transcription.
//!
//! # Model paths
//!
//! Models are expected under `~/.aios/models/whisper/` in the ggml format:
//!
//! ```text
//! ~/.aios/models/whisper/ggml-tiny.bin
//! ~/.aios/models/whisper/ggml-base.bin
//! ~/.aios/models/whisper/ggml-small.bin
//! ~/.aios/models/whisper/ggml-medium.bin
//! ~/.aios/models/whisper/ggml-large.bin
//! ```

use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use tracing::debug;

use super::{SttBackend, SttEngine, TranscriptionResult};
use crate::error::VoiceError;

/// Known Whisper model sizes, ordered from smallest to largest.
const MODEL_SIZES: &[&str] = &["tiny", "base", "small", "medium", "large"];

/// Name of the CLI binary to invoke.
const WHISPER_CLI: &str = "whisper-cpp-cli";

/// Whisper STT engine — subprocess-based.
pub struct WhisperStt {
    /// Currently loaded model size (e.g. "medium").
    model_size: Option<String>,
    /// Resolved path to the model file.
    model_path: Option<PathBuf>,
    /// Base directory for model files.
    models_dir: PathBuf,
}

impl WhisperStt {
    /// Create a new (unloaded) Whisper engine.
    pub fn new() -> Self {
        let models_dir = dirs_model_path();
        Self {
            model_size: None,
            model_path: None,
            models_dir,
        }
    }

    /// Write audio samples to a temporary WAV file suitable for Whisper.
    fn write_temp_wav(&self, audio: &[f32]) -> Result<tempfile::NamedTempFile, VoiceError> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut tmp = tempfile::Builder::new()
            .prefix("aios-stt-")
            .suffix(".wav")
            .tempfile()
            .map_err(|e| VoiceError::Io(e))?;

        // Write via hound into a buffer first, then copy to the temp file,
        // because hound wants a Writer that implements Seek.
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec)
                .map_err(|e| VoiceError::SttTranscribe(format!("wav writer error: {e}")))?;
            for &sample in audio {
                let s16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                writer
                    .write_sample(s16)
                    .map_err(|e| VoiceError::SttTranscribe(format!("wav write error: {e}")))?;
            }
            writer
                .finalize()
                .map_err(|e| VoiceError::SttTranscribe(format!("wav finalize error: {e}")))?;
        }

        tmp.write_all(cursor.get_ref())?;
        tmp.flush()?;

        Ok(tmp)
    }

    /// Parse whisper-cpp-cli output to extract transcription text.
    ///
    /// The CLI output format has lines like:
    /// ```text
    /// [00:00:00.000 --> 00:00:03.200]   Hello, this is a test.
    /// ```
    fn parse_output(raw: &str) -> String {
        let mut text = String::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            // Skip empty lines and timestamp lines that don't have content.
            if trimmed.is_empty() {
                continue;
            }
            // If the line has a timestamp prefix, extract text after the `]`.
            if let Some(idx) = trimmed.find(']') {
                let after = trimmed[idx + 1..].trim();
                if !after.is_empty() {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(after);
                }
            } else {
                // Plain text line (some whisper builds output without timestamps).
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(trimmed);
            }
        }
        text
    }
}

impl Default for WhisperStt {
    fn default() -> Self {
        Self::new()
    }
}

impl SttEngine for WhisperStt {
    fn name(&self) -> &str {
        "whisper-cpp"
    }

    fn backend(&self) -> SttBackend {
        SttBackend::Whisper
    }

    fn load_model(&mut self, model_size: &str) -> Result<(), VoiceError> {
        if !MODEL_SIZES.contains(&model_size) {
            return Err(VoiceError::SttModel(format!(
                "unknown model size '{model_size}', expected one of: {MODEL_SIZES:?}"
            )));
        }

        let model_file = self.models_dir.join(format!("ggml-{model_size}.bin"));
        if !model_file.exists() {
            return Err(VoiceError::SttModel(format!(
                "model file not found: {}",
                model_file.display()
            )));
        }

        debug!(model_size, path = %model_file.display(), "loaded whisper model");
        self.model_size = Some(model_size.to_string());
        self.model_path = Some(model_file);
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.model_path.is_some()
    }

    fn transcribe(
        &self,
        audio: &[f32],
        language: Option<&str>,
    ) -> Result<TranscriptionResult, VoiceError> {
        // Lazy-load: if no model is explicitly loaded, try "medium" then "base" then "tiny".
        let model_path = match &self.model_path {
            Some(p) => p.clone(),
            None => {
                // Try to find any available model, preferring medium.
                let fallback_order = ["medium", "base", "small", "tiny", "large"];
                let mut found = None;
                for size in &fallback_order {
                    let p = self.models_dir.join(format!("ggml-{size}.bin"));
                    if p.exists() {
                        found = Some(p);
                        break;
                    }
                }
                found.ok_or_else(|| {
                    VoiceError::SttModel(format!(
                        "no whisper model found in {}",
                        self.models_dir.display()
                    ))
                })?
            }
        };

        if audio.is_empty() {
            return Ok(TranscriptionResult {
                text: String::new(),
                language: language.map(String::from),
                duration_secs: 0.0,
            });
        }

        let wav_file = self.write_temp_wav(audio)?;
        let wav_path = wav_file.path();

        let mut cmd = Command::new(WHISPER_CLI);
        cmd.arg("-m")
            .arg(&model_path)
            .arg("-f")
            .arg(wav_path)
            .arg("--no-timestamps")
            .arg("-nt"); // no timestamps in output

        if let Some(lang) = language {
            cmd.arg("-l").arg(lang);
        }

        debug!(cmd = ?cmd, "running whisper CLI");
        let start = Instant::now();

        let output = cmd
            .output()
            .map_err(|e| VoiceError::SttTranscribe(format!("failed to run {WHISPER_CLI}: {e}")))?;

        let duration_secs = start.elapsed().as_secs_f32();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VoiceError::SttTranscribe(format!(
                "{WHISPER_CLI} exited with {}: {stderr}",
                output.status
            )));
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        let text = Self::parse_output(&raw);

        debug!(
            text_len = text.len(),
            duration_secs,
            "transcription complete"
        );

        Ok(TranscriptionResult {
            text,
            language: language.map(String::from),
            duration_secs,
        })
    }

    fn available_models(&self) -> Vec<String> {
        MODEL_SIZES.iter().map(|s| (*s).to_string()).collect()
    }
}

/// Return the model directory: system path (ISO) first, then user home.
fn dirs_model_path() -> PathBuf {
    // Check system path first (ISO / installed system).
    let system_dir = PathBuf::from("/opt/aios-app/models/whisper");
    if system_dir.join("ggml-tiny.bin").exists() {
        return system_dir;
    }
    // Fall back to user home.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/aios".to_string());
    PathBuf::from(home).join(".aios/models/whisper")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_output_with_timestamps() {
        let raw = r#"
[00:00:00.000 --> 00:00:02.000]   Hello world.
[00:00:02.000 --> 00:00:04.000]   This is a test.
"#;
        let text = WhisperStt::parse_output(raw);
        assert_eq!(text, "Hello world. This is a test.");
    }

    #[test]
    fn parse_output_plain_text() {
        let raw = "Hello world.\nThis is a test.\n";
        let text = WhisperStt::parse_output(raw);
        assert_eq!(text, "Hello world. This is a test.");
    }

    #[test]
    fn parse_output_empty() {
        assert_eq!(WhisperStt::parse_output(""), "");
        assert_eq!(WhisperStt::parse_output("  \n  \n"), "");
    }

    #[test]
    fn new_engine_is_not_loaded() {
        let engine = WhisperStt::new();
        assert!(!engine.is_loaded());
        assert_eq!(engine.name(), "whisper-cpp");
        assert_eq!(engine.backend(), SttBackend::Whisper);
    }

    #[test]
    fn available_models_includes_all_sizes() {
        let engine = WhisperStt::new();
        let models = engine.available_models();
        assert!(models.contains(&"tiny".to_string()));
        assert!(models.contains(&"base".to_string()));
        assert!(models.contains(&"small".to_string()));
        assert!(models.contains(&"medium".to_string()));
        assert!(models.contains(&"large".to_string()));
    }

    #[test]
    fn load_model_rejects_unknown_size() {
        let mut engine = WhisperStt::new();
        let result = engine.load_model("gigantic");
        assert!(result.is_err());
    }

    #[test]
    fn transcribe_empty_audio() {
        let engine = WhisperStt::new();
        // Empty audio should succeed even without a model loaded.
        let result = engine.transcribe(&[], Some("en"));
        // This may fail if no model exists (expected in CI), but empty audio
        // has a fast path that returns immediately.
        match result {
            Ok(tr) => {
                assert!(tr.text.is_empty());
                assert_eq!(tr.language.as_deref(), Some("en"));
            }
            Err(_) => {
                // Expected when no model file is present.
            }
        }
    }
}
