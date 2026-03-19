//! Default configuration values for AiOS.
//!
//! Mirrors the Python `DEFAULTS` dict in `aios/config/manager.py`.

/// Default configuration as a JSON string.
///
/// This is parsed at runtime and deep-merged with the user's saved config
/// so that any newly added keys are automatically present.
pub const DEFAULTS_JSON: &str = r#"{
    "assistant": {
        "name": "Assistant",
        "language": "en"
    },
    "llm": {
        "provider": "claude",
        "claude_api_key": "",
        "claude_model": "claude-sonnet-4-20250514",
        "openai_api_key": "",
        "openai_model": "gpt-4o",
        "deepseek_api_key": "",
        "deepseek_model": "deepseek-chat",
        "mistral_api_key": "",
        "mistral_model": "mistral-small-latest",
        "groq_api_key": "",
        "groq_model": "llama-3.3-70b-versatile",
        "gemini_api_key": "",
        "gemini_model": "gemini-2.0-flash",
        "ollama_enabled": false,
        "ollama_model": "llama3.2",
        "tts_summary_provider": "claude",
        "tts_summary_model": "claude-sonnet-4-20250514",
        "extra_system_prompt": "",
        "max_tool_rounds": 10,
        "effort": "auto",
        "quality_mode": "balanced"
    },
    "voice": {
        "stt_enabled": true,
        "stt_model": "medium",
        "stt_language": "",
        "tts_enabled": true,
        "tts_voice": "en_US-amy-medium",
        "tts_gender": "female",
        "tts_rate": 1.0,
        "wake_word": "hey jarvis",
        "wake_enabled": true,
        "wake_word_source": "pretrained",
        "wake_threshold": 0.5
    },
    "ui": {
        "theme": "dark",
        "font_size": 14
    },
    "system": {
        "keyboard_layout": "us",
        "keyboard_variant": "",
        "locale": "en_US.UTF-8",
        "timezone": "",
        "machine_name": "assistant",
        "update_url": "https://api.github.com/repos/aios-dev/aios/releases/latest"
    },
    "tools": {
        "plugins_dir": "~/.aios/plugins",
        "store_url": "https://store.aios.dev/api/v1"
    }
}"#;

/// Parse the default configuration into a [`serde_json::Value`].
///
/// # Panics
///
/// Panics if `DEFAULTS_JSON` is not valid JSON (this is a compile-time
/// constant so this should never happen).
pub fn defaults() -> serde_json::Value {
    serde_json::from_str(DEFAULTS_JSON).expect("DEFAULTS_JSON must be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_successfully() {
        let v = defaults();
        assert_eq!(v["llm"]["provider"], "claude");
        assert_eq!(v["voice"]["tts_voice"], "en_US-amy-medium");
        assert_eq!(v["ui"]["theme"], "dark");
        assert_eq!(v["system"]["keyboard_layout"], "us");
        assert_eq!(v["system"]["update_url"], "https://api.github.com/repos/aios-dev/aios/releases/latest");
        assert_eq!(v["tools"]["store_url"], "https://store.aios.dev/api/v1");
    }

    #[test]
    fn wake_word_default_is_ok_computer() {
        let v = defaults();
        assert_eq!(v["voice"]["wake_word"], "hey jarvis");
    }

    #[test]
    fn wake_enabled_by_default() {
        let v = defaults();
        assert_eq!(v["voice"]["wake_enabled"], true);
    }

    #[test]
    fn stt_enabled_by_default() {
        let v = defaults();
        assert_eq!(v["voice"]["stt_enabled"], true);
    }

    #[test]
    fn tts_enabled_by_default() {
        let v = defaults();
        assert_eq!(v["voice"]["tts_enabled"], true);
    }

    #[test]
    fn wake_word_source_is_pretrained() {
        let v = defaults();
        assert_eq!(v["voice"]["wake_word_source"], "pretrained");
    }

    #[test]
    fn wake_threshold_default() {
        let v = defaults();
        assert_eq!(v["voice"]["wake_threshold"], 0.5);
    }

    #[test]
    fn assistant_name_default() {
        let v = defaults();
        assert_eq!(v["assistant"]["name"], "Assistant");
    }

    #[test]
    fn language_default_is_en() {
        let v = defaults();
        assert_eq!(v["assistant"]["language"], "en");
    }

    #[test]
    fn machine_name_default() {
        let v = defaults();
        assert_eq!(v["system"]["machine_name"], "assistant");
    }

    #[test]
    fn all_required_keys_present() {
        let v = defaults();
        // Voice
        assert!(v["voice"]["stt_enabled"].is_boolean());
        assert!(v["voice"]["tts_enabled"].is_boolean());
        assert!(v["voice"]["wake_enabled"].is_boolean());
        assert!(v["voice"]["wake_word"].is_string());
        assert!(v["voice"]["wake_word_source"].is_string());
        assert!(v["voice"]["wake_threshold"].is_number());
        // LLM
        assert!(v["llm"]["provider"].is_string());
        assert!(v["llm"]["effort"].is_string());
        assert!(v["llm"]["quality_mode"].is_string());
        // UI
        assert!(v["ui"]["theme"].is_string());
        // System
        assert!(v["system"]["keyboard_layout"].is_string());
        assert!(v["system"]["machine_name"].is_string());
    }
}
