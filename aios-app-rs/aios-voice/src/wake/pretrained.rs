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

    #[test]
    fn all_models_have_unique_ids() {
        let mut ids: Vec<&str> = PRETRAINED_WAKE_WORDS.iter().map(|m| m.id).collect();
        let original_len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            original_len,
            "Duplicate model IDs found in PRETRAINED_WAKE_WORDS"
        );
    }

    #[test]
    fn all_models_have_unique_filenames() {
        let mut filenames: Vec<&str> = PRETRAINED_WAKE_WORDS.iter().map(|m| m.filename).collect();
        let original_len = filenames.len();
        filenames.sort();
        filenames.dedup();
        assert_eq!(
            filenames.len(),
            original_len,
            "Duplicate filenames found in PRETRAINED_WAKE_WORDS"
        );
    }

    #[test]
    fn find_by_space_separated_name() {
        // "hey assistant" (with space) should match "hey_assistant" (with underscore).
        let model = find_pretrained("hey assistant");
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "hey_assistant");

        // "ok computer" should match "ok_computer".
        let model = find_pretrained("ok computer");
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "ok_computer");

        // "mr anderson" should match "mr_anderson".
        let model = find_pretrained("mr anderson");
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "mr_anderson");
    }

    #[test]
    fn model_path_construction_various_models() {
        // Verify path construction works for different models and base dirs.
        let model = find_pretrained("skynet").unwrap();
        let path = pretrained_model_path(Path::new("/opt/aios/models"), model);
        assert_eq!(path, PathBuf::from("/opt/aios/models/pretrained/skynet.onnx"));

        let model = find_pretrained("jarvis").unwrap();
        let path = pretrained_model_path(Path::new("."), model);
        assert_eq!(path, PathBuf::from("./pretrained/jarvis.onnx"));

        // Verify the filename field is consistent with the id.
        for m in PRETRAINED_WAKE_WORDS {
            let expected_filename = format!("{}.onnx", m.id);
            assert_eq!(
                m.filename, expected_filename,
                "Model '{}' filename '{}' does not match expected '{}'",
                m.id, m.filename, expected_filename,
            );
        }
    }

    #[test]
    fn find_returns_none_for_empty_string() {
        assert!(find_pretrained("").is_none());
    }

    // -- Comprehensive tests for every model ID --

    #[test]
    fn find_pretrained_hey_assistant() {
        let m = find_pretrained("hey_assistant").unwrap();
        assert_eq!(m.id, "hey_assistant");
        assert_eq!(m.display_name, "Hey Assistant");
        assert_eq!(m.filename, "hey_assistant.onnx");
    }

    #[test]
    fn find_pretrained_hey_jarvis() {
        let m = find_pretrained("hey_jarvis").unwrap();
        assert_eq!(m.id, "hey_jarvis");
        assert_eq!(m.display_name, "Hey Jarvis");
    }

    #[test]
    fn find_pretrained_computer() {
        let m = find_pretrained("computer").unwrap();
        assert_eq!(m.id, "computer");
        assert_eq!(m.display_name, "Computer");
    }

    #[test]
    fn find_pretrained_ok_computer() {
        let m = find_pretrained("ok_computer").unwrap();
        assert_eq!(m.id, "ok_computer");
    }

    #[test]
    fn find_pretrained_hey_friday() {
        let m = find_pretrained("hey_friday").unwrap();
        assert_eq!(m.id, "hey_friday");
    }

    #[test]
    fn find_pretrained_jarvis() {
        let m = find_pretrained("jarvis").unwrap();
        assert_eq!(m.id, "jarvis");
    }

    #[test]
    fn find_pretrained_ok_jarvis() {
        let m = find_pretrained("ok_jarvis").unwrap();
        assert_eq!(m.id, "ok_jarvis");
    }

    #[test]
    fn find_pretrained_skynet() {
        let m = find_pretrained("skynet").unwrap();
        assert_eq!(m.id, "skynet");
    }

    #[test]
    fn find_pretrained_terminator() {
        let m = find_pretrained("terminator").unwrap();
        assert_eq!(m.id, "terminator");
    }

    #[test]
    fn find_pretrained_hey_house() {
        let m = find_pretrained("hey_house").unwrap();
        assert_eq!(m.id, "hey_house");
    }

    #[test]
    fn find_pretrained_ok_home() {
        let m = find_pretrained("ok_home").unwrap();
        assert_eq!(m.id, "ok_home");
    }

    #[test]
    fn find_pretrained_home_assistant() {
        let m = find_pretrained("home_assistant").unwrap();
        assert_eq!(m.id, "home_assistant");
    }

    #[test]
    fn find_pretrained_mr_anderson() {
        let m = find_pretrained("mr_anderson").unwrap();
        assert_eq!(m.id, "mr_anderson");
    }

    #[test]
    fn find_pretrained_mr_smith() {
        let m = find_pretrained("mr_smith").unwrap();
        assert_eq!(m.id, "mr_smith");
    }

    #[test]
    fn find_pretrained_hey_dick_head() {
        let m = find_pretrained("hey_dick_head").unwrap();
        assert_eq!(m.id, "hey_dick_head");
    }

    #[test]
    fn find_pretrained_oi_fuckwhit() {
        let m = find_pretrained("oi_fuckwhit").unwrap();
        assert_eq!(m.id, "oi_fuckwhit");
    }

    #[test]
    fn find_pretrained_yo_homie() {
        let m = find_pretrained("yo_homie").unwrap();
        assert_eq!(m.id, "yo_homie");
    }

    // -- Display name with spaces --

    #[test]
    fn find_by_display_name_with_spaces_all_models() {
        for m in PRETRAINED_WAKE_WORDS {
            let found = find_pretrained(m.display_name);
            assert!(
                found.is_some(),
                "find_pretrained(\"{}\") returned None",
                m.display_name
            );
            assert_eq!(found.unwrap().id, m.id);
        }
    }

    // -- Mixed case lookups --

    #[test]
    fn find_with_mixed_case_variations() {
        assert!(find_pretrained("Hey_Assistant").is_some());
        assert!(find_pretrained("HEY_JARVIS").is_some());
        assert!(find_pretrained("COMPUTER").is_some());
        assert!(find_pretrained("Ok Computer").is_some());
        assert!(find_pretrained("Hey FRIDAY").is_some());
        assert!(find_pretrained("JARVIS").is_some());
        assert!(find_pretrained("Skynet").is_some());
        assert!(find_pretrained("TERMINATOR").is_some());
        assert!(find_pretrained("Hey House").is_some());
        assert!(find_pretrained("OK Home").is_some());
        assert!(find_pretrained("Home Assistant").is_some());
        assert!(find_pretrained("Mr. Anderson").is_some());
        assert!(find_pretrained("Mr. Smith").is_some());
        assert!(find_pretrained("Yo Homie").is_some());
    }

    // -- All IDs are snake_case --

    #[test]
    fn all_ids_are_snake_case() {
        for m in PRETRAINED_WAKE_WORDS {
            // snake_case: only lowercase alphanumeric and underscores
            assert!(
                m.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "Model ID '{}' is not snake_case — contains invalid characters",
                m.id,
            );
            // Must not start or end with underscore
            assert!(
                !m.id.starts_with('_') && !m.id.ends_with('_'),
                "Model ID '{}' starts or ends with underscore",
                m.id,
            );
            // Must not have consecutive underscores
            assert!(
                !m.id.contains("__"),
                "Model ID '{}' contains consecutive underscores",
                m.id,
            );
        }
    }

    // -- Path construction for all models --

    #[test]
    fn pretrained_model_path_for_all_models() {
        let base = Path::new("/opt/aios/models");
        for m in PRETRAINED_WAKE_WORDS {
            let path = pretrained_model_path(base, m);
            let expected = base.join("pretrained").join(m.filename);
            assert_eq!(
                path, expected,
                "Path mismatch for model '{}': got {:?}, expected {:?}",
                m.id, path, expected,
            );
        }
    }

    #[test]
    fn pretrained_model_path_with_relative_dir() {
        let model = find_pretrained("computer").unwrap();
        let path = pretrained_model_path(Path::new("models"), model);
        assert_eq!(path, PathBuf::from("models/pretrained/computer.onnx"));
    }

    #[test]
    fn pretrained_model_path_with_trailing_slash() {
        let model = find_pretrained("jarvis").unwrap();
        let path = pretrained_model_path(Path::new("/models/"), model);
        assert_eq!(path, PathBuf::from("/models/pretrained/jarvis.onnx"));
    }

    // -- Nonexistent model lookups --

    #[test]
    fn find_returns_none_for_nonexistent_ids() {
        assert!(find_pretrained("alexa").is_none());
        assert!(find_pretrained("siri").is_none());
        assert!(find_pretrained("hey_google").is_none());
        assert!(find_pretrained("cortana").is_none());
        assert!(find_pretrained("   ").is_none());
        assert!(find_pretrained("hey_assistant.onnx").is_none()); // filename, not id
    }

    // -- All display names are non-empty --

    #[test]
    fn all_display_names_are_non_empty() {
        for m in PRETRAINED_WAKE_WORDS {
            assert!(
                !m.display_name.is_empty(),
                "Model '{}' has empty display_name",
                m.id,
            );
        }
    }

    // -- All filenames end with .onnx --

    #[test]
    fn all_filenames_end_with_onnx() {
        for m in PRETRAINED_WAKE_WORDS {
            assert!(
                m.filename.ends_with(".onnx"),
                "Model '{}' filename '{}' does not end with .onnx",
                m.id, m.filename,
            );
        }
    }

    // -- Unique display names --

    #[test]
    fn all_models_have_unique_display_names() {
        let mut names: Vec<&str> = PRETRAINED_WAKE_WORDS.iter().map(|m| m.display_name).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            original_len,
            "Duplicate display names found in PRETRAINED_WAKE_WORDS"
        );
    }
}
