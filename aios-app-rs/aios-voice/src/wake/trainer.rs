//! On-device wake word model training via openWakeWord Python pipeline.
//!
//! Uses Piper TTS to generate synthetic training data, then trains a small
//! neural network and exports it as an ONNX model. Training takes ~5-10
//! minutes on CPU.

use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

use crate::error::VoiceError;

/// Path to the Python training script.
const TRAINER_SCRIPT: &str = "/opt/aios-app/kws-trainer/train.py";

/// Path to the Python virtualenv.
const TRAINER_VENV: &str = "/opt/aios-app/kws-trainer/venv";

/// On-device wake word model trainer.
pub struct KwsTrainer;

impl KwsTrainer {
    /// Check if the training environment is available.
    pub fn is_available() -> bool {
        Path::new(TRAINER_SCRIPT).exists() && Path::new(TRAINER_VENV).exists()
    }

    /// Train a custom wake word model.
    ///
    /// Spawns the Python training pipeline as a subprocess. Blocks until
    /// training completes (~5-10 minutes on CPU).
    ///
    /// Returns the path to the produced `.onnx` model file.
    pub fn train(phrase: &str, output_dir: &Path) -> Result<PathBuf, VoiceError> {
        if !Self::is_available() {
            return Err(VoiceError::Kws(
                "Training environment not available. Install the kws-trainer package.".to_string(),
            ));
        }

        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");

        let output_path = output_dir.join(format!("{sanitized}.onnx"));

        // Ensure output directory exists
        std::fs::create_dir_all(output_dir)
            .map_err(|e| VoiceError::Kws(format!("Failed to create output dir: {e}")))?;

        info!("KWS trainer: starting training for \"{phrase}\" → {}", output_path.display());

        let python = PathBuf::from(TRAINER_VENV).join("bin/python");

        let output = Command::new(&python)
            .arg(TRAINER_SCRIPT)
            .arg("--phrase")
            .arg(phrase)
            .arg("--output")
            .arg(&output_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| VoiceError::Kws(format!("Failed to spawn trainer: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("KWS trainer failed: {stderr}");
            return Err(VoiceError::Kws(format!("Training failed: {stderr}")));
        }

        if !output_path.exists() {
            return Err(VoiceError::Kws(
                "Training completed but model file not found".to_string(),
            ));
        }

        info!("KWS trainer: model saved to {}", output_path.display());
        Ok(output_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_phrase() {
        let phrase = "Hey My Assistant!";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        assert_eq!(sanitized, "hey_my_assistant");
    }

    #[test]
    fn trainer_not_available_without_files() {
        assert!(!KwsTrainer::is_available());
    }
}
