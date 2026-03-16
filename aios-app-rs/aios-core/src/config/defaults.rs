//! Default configuration values for AiOS.
//!
//! Mirrors the Python `DEFAULTS` dict in `aios/config/manager.py`.

/// Default configuration as a JSON string.
///
/// This is parsed at runtime and deep-merged with the user's saved config
/// so that any newly added keys are automatically present.
pub const DEFAULTS_JSON: &str = r#"{
    "llm": {
        "provider": "claude",
        "claude_api_key": "",
        "claude_model": "claude-sonnet-4-20250514",
        "openai_api_key": "",
        "openai_model": "gpt-4o",
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
        "tts_rate": 1.0
    },
    "ui": {
        "theme": "dark",
        "font_size": 14
    },
    "system": {
        "keyboard_layout": "us",
        "keyboard_variant": "",
        "locale": "en_US.UTF-8",
        "timezone": ""
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
        assert_eq!(v["tools"]["store_url"], "https://store.aios.dev/api/v1");
    }
}
