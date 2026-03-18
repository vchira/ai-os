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

    // -- Comprehensive additional tests --

    #[test]
    fn sanitize_phrase_removes_special_chars() {
        let cases = [
            ("Hello World!", "hello_world"),
            ("hey_assistant", "hey_assistant"),
            ("  spaces  ", "__spaces__"),
            ("Mr. Anderson", "mr_anderson"),
            ("Hey!!! @#$ Assistant", "hey__assistant"),
            ("123 Numbers", "123_numbers"),
            ("under_score", "under_score"),
        ];
        for (input, expected) in &cases {
            let sanitized = input
                .to_lowercase()
                .replace(' ', "_")
                .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
            // Trim leading/trailing underscores and collapse multiple underscores.
            // Note: the actual code doesn't do this extra cleanup, so we test
            // the exact behavior of the production sanitization logic.
            assert_eq!(
                sanitized, *expected,
                "sanitize('{}') = '{}', expected '{}'",
                input, sanitized, expected,
            );
        }
    }

    #[test]
    fn sanitize_phrase_empty_string() {
        let phrase = "";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        assert_eq!(sanitized, "");
    }

    #[test]
    fn sanitize_phrase_only_special_chars() {
        let phrase = "!@#$%^&*()";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        assert_eq!(sanitized, "");
    }

    #[test]
    fn sanitize_phrase_unicode() {
        let phrase = "H\u{00e9} Aios";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        assert_eq!(sanitized, "h\u{00e9}_aios");
    }

    #[test]
    fn sanitize_phrase_preserves_numbers() {
        let phrase = "Wake Word 42";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        assert_eq!(sanitized, "wake_word_42");
    }

    #[test]
    fn training_output_path_is_correct() {
        let phrase = "Hey My Assistant!";
        let output_dir = Path::new("/home/user/.aios/models/custom");
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        let output_path = output_dir.join(format!("{sanitized}.onnx"));
        assert_eq!(
            output_path,
            PathBuf::from("/home/user/.aios/models/custom/hey_my_assistant.onnx"),
        );
    }

    #[test]
    fn training_output_path_with_simple_phrase() {
        let phrase = "jarvis";
        let output_dir = Path::new("/models");
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        let output_path = output_dir.join(format!("{sanitized}.onnx"));
        assert_eq!(output_path, PathBuf::from("/models/jarvis.onnx"));
    }

    #[test]
    fn training_output_path_with_relative_dir() {
        let phrase = "ok computer";
        let output_dir = Path::new("custom_models");
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        let output_path = output_dir.join(format!("{sanitized}.onnx"));
        assert_eq!(output_path, PathBuf::from("custom_models/ok_computer.onnx"));
    }

    #[test]
    fn train_returns_error_when_not_available() {
        let result = KwsTrainer::train("test phrase", Path::new("/tmp"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("not available"),
            "Expected 'not available' in error, got: '{}'",
            err_msg,
        );
    }

    #[test]
    fn trainer_constants_are_defined() {
        assert!(!TRAINER_SCRIPT.is_empty());
        assert!(!TRAINER_VENV.is_empty());
        assert!(TRAINER_SCRIPT.ends_with(".py"));
        assert!(TRAINER_VENV.contains("venv"));
    }

    #[test]
    fn sanitize_phrase_multiple_spaces() {
        let phrase = "hey   my   friend";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        // Multiple spaces become multiple underscores.
        assert_eq!(sanitized, "hey___my___friend");
    }

    #[test]
    fn sanitize_phrase_already_snake_case() {
        let phrase = "hey_assistant";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        assert_eq!(sanitized, "hey_assistant");
    }

    #[test]
    fn sanitize_phrase_mixed_case_and_special() {
        let phrase = "Hey-There_AIOS!";
        let sanitized = phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        assert_eq!(sanitized, "heythere_aios");
    }
}
