//! Catalog of pre-trained openWakeWord models shipped with AiOS.

use std::path::{Path, PathBuf};

/// A pre-trained wake word model entry.
#[derive(Debug, Clone)]
pub struct PretrainedModel {
    /// Snake_case identifier (e.g., "hey_assistant").
    pub id: &'static str,
    /// Human-readable display name (e.g., "Hey Assistant").
    pub display_name: &'static str,
    /// ONNX model filename (e.g., "hey_assistant.onnx").
    pub filename: &'static str,
}

/// All pre-trained wake word models shipped with AiOS.
pub const PRETRAINED_WAKE_WORDS: &[PretrainedModel] = &[
    PretrainedModel { id: "hey_assistant", display_name: "Hey Assistant", filename: "hey_assistant.onnx" },
    PretrainedModel { id: "hey_jarvis", display_name: "Hey Jarvis", filename: "hey_jarvis.onnx" },
    PretrainedModel { id: "computer", display_name: "Computer", filename: "computer.onnx" },
    PretrainedModel { id: "ok_computer", display_name: "OK Computer", filename: "ok_computer.onnx" },
    PretrainedModel { id: "hey_friday", display_name: "Hey Friday", filename: "hey_friday.onnx" },
    PretrainedModel { id: "jarvis", display_name: "Jarvis", filename: "jarvis.onnx" },
    PretrainedModel { id: "ok_jarvis", display_name: "OK Jarvis", filename: "ok_jarvis.onnx" },
    PretrainedModel { id: "skynet", display_name: "Skynet", filename: "skynet.onnx" },
    PretrainedModel { id: "terminator", display_name: "Terminator", filename: "terminator.onnx" },
    PretrainedModel { id: "hey_house", display_name: "Hey House", filename: "hey_house.onnx" },
    PretrainedModel { id: "ok_home", display_name: "OK Home", filename: "ok_home.onnx" },
    PretrainedModel { id: "home_assistant", display_name: "Home Assistant", filename: "home_assistant.onnx" },
    PretrainedModel { id: "mr_anderson", display_name: "Mr. Anderson", filename: "mr_anderson.onnx" },
    PretrainedModel { id: "mr_smith", display_name: "Mr. Smith", filename: "mr_smith.onnx" },
    PretrainedModel { id: "hey_dick_head", display_name: "Hey Dick Head", filename: "hey_dick_head.onnx" },
    PretrainedModel { id: "oi_fuckwhit", display_name: "Oi Fuckwhit", filename: "oi_fuckwhit.onnx" },
    PretrainedModel { id: "yo_homie", display_name: "Yo Homie", filename: "yo_homie.onnx" },
];

/// Find a pre-trained model by its id or display name (case-insensitive).
pub fn find_pretrained(name: &str) -> Option<&'static PretrainedModel> {
    let lower = name.to_lowercase();
    let normalized = lower.replace(' ', "_");
    PRETRAINED_WAKE_WORDS.iter().find(|m| {
        m.id == normalized
            || m.display_name.to_lowercase() == lower
            || m.id == lower
    })
}

/// Get the full path to a pre-trained model file.
pub fn pretrained_model_path(models_dir: &Path, model: &PretrainedModel) -> PathBuf {
    models_dir.join("pretrained").join(model.filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_17_models() {
        assert_eq!(PRETRAINED_WAKE_WORDS.len(), 17);
    }

    #[test]
    fn find_by_id() {
        assert!(find_pretrained("hey_assistant").is_some());
        assert!(find_pretrained("skynet").is_some());
        assert!(find_pretrained("nonexistent").is_none());
    }

    #[test]
    fn find_by_display_name() {
        assert!(find_pretrained("Hey Jarvis").is_some());
        assert!(find_pretrained("OK Computer").is_some());
    }

    #[test]
    fn find_case_insensitive() {
        assert!(find_pretrained("HEY_ASSISTANT").is_some());
        assert!(find_pretrained("hey assistant").is_some());
        assert!(find_pretrained("SKYNET").is_some());
    }

    #[test]
    fn model_path_construction() {
        let model = find_pretrained("hey_assistant").unwrap();
        let path = pretrained_model_path(Path::new("/models"), model);
        assert_eq!(path, PathBuf::from("/models/pretrained/hey_assistant.onnx"));
    }
}
