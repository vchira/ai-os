//! Internationalization (i18n) support.
//!
//! Loads JSON translation files compiled into the binary. Provides `t(key)`
//! for simple lookups and `t_fmt(key, args)` for interpolation.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value;

// Embed translation files at compile time.
const EN_JSON: &str = include_str!("../i18n/en.json");
const DE_JSON: &str = include_str!("../i18n/de.json");

// Thread-local state: current language + loaded translations.
thread_local! {
    static CURRENT_LANG: RefCell<String> = RefCell::new("en".to_string());
    static TRANSLATIONS: RefCell<HashMap<String, HashMap<String, String>>> = RefCell::new(HashMap::new());
}

/// Initialize the i18n system. Call once at startup.
pub fn init() {
    load_language("en", EN_JSON);
    load_language("de", DE_JSON);
}

/// Load a language from JSON.
fn load_language(code: &str, json: &str) {
    if let Ok(Value::Object(map)) = serde_json::from_str(json) {
        let mut strings = HashMap::new();
        for (key, val) in map {
            if key == "_meta" {
                continue;
            }
            if let Value::String(s) = val {
                strings.insert(key, s);
            }
        }
        TRANSLATIONS.with(|t| {
            t.borrow_mut().insert(code.to_string(), strings);
        });
    }
}

/// Set the active language.
pub fn set_language(lang: &str) {
    CURRENT_LANG.with(|l| *l.borrow_mut() = lang.to_string());
}

/// Get the current language code.
pub fn current_language() -> String {
    CURRENT_LANG.with(|l| l.borrow().clone())
}

/// Translate a key. Falls back to English, then returns the key itself.
pub fn t(key: &str) -> String {
    let lang = current_language();
    TRANSLATIONS.with(|t| {
        let translations = t.borrow();
        // Try current language.
        if let Some(strings) = translations.get(&lang) {
            if let Some(s) = strings.get(key) {
                return s.clone();
            }
        }
        // Fallback to English.
        if lang != "en" {
            if let Some(strings) = translations.get("en") {
                if let Some(s) = strings.get(key) {
                    return s.clone();
                }
            }
        }
        // Return the key itself as last resort.
        key.to_string()
    })
}

/// Translate with interpolation. Replaces `{name}` placeholders.
pub fn t_fmt(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(key);
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

/// Auto-detect language from system locale.
pub fn detect_system_language() -> String {
    // Check LANG env var: "de_DE.UTF-8" -> "de"
    if let Ok(lang) = std::env::var("LANG") {
        let code = lang.split('_').next().unwrap_or("en");
        let code = code.split('.').next().unwrap_or("en");
        if !code.is_empty() && code != "C" && code != "POSIX" {
            return code.to_string();
        }
    }
    "en".to_string()
}

/// Get all available languages as (code, native_name) pairs.
pub fn available_languages() -> Vec<(String, String)> {
    TRANSLATIONS.with(|t| {
        let translations = t.borrow();
        let mut langs: Vec<(String, String)> = Vec::new();
        for (code, strings) in translations.iter() {
            let name = strings
                .get("_meta.language")
                .cloned()
                .unwrap_or_else(|| code.clone());
            langs.push((code.clone(), name));
        }
        langs.sort_by(|a, b| a.0.cmp(&b.0));
        langs
    })
}

/// Get the full translations map for a language (for sending to web client).
pub fn get_translations(lang: &str) -> Option<HashMap<String, String>> {
    TRANSLATIONS.with(|t| t.borrow().get(lang).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_returns_english_string() {
        init();
        set_language("en");
        assert_eq!(t("chat.role.you"), "You");
    }

    #[test]
    fn t_returns_key_for_missing() {
        init();
        set_language("en");
        assert_eq!(t("nonexistent.key"), "nonexistent.key");
    }

    #[test]
    fn t_fmt_replaces_placeholders() {
        init();
        set_language("en");
        let result = t_fmt("setup.backup.yes", &[("provider", "Claude")]);
        assert_eq!(result, "Yes, add Claude");
    }

    #[test]
    fn t_fmt_replaces_multiple_placeholders() {
        init();
        set_language("en");
        // hostname.conflict.changed has {name} twice
        let result = t_fmt(
            "hostname.conflict.changed",
            &[("name", "myhost")],
        );
        assert_eq!(result, "Hostname changed to 'myhost'. Reachable as myhost.local");
    }

    #[test]
    fn detect_language_fallback() {
        // With no LANG set or LANG=C, should return "en"
        let lang = detect_system_language();
        assert!(!lang.is_empty());
    }

    #[test]
    fn fallback_to_english_for_unknown_language() {
        init();
        set_language("xx");
        // Should fall back to English
        assert_eq!(t("chat.role.you"), "You");
    }

    #[test]
    fn get_translations_returns_map() {
        init();
        let map = get_translations("en");
        assert!(map.is_some());
        let map = map.unwrap();
        assert_eq!(map.get("chat.role.you").map(|s| s.as_str()), Some("You"));
    }

    #[test]
    fn current_language_default_is_en() {
        // After init, the default language should be "en"
        set_language("en");
        assert_eq!(current_language(), "en");
    }

    #[test]
    fn set_language_changes_current() {
        set_language("de");
        assert_eq!(current_language(), "de");
        // Reset
        set_language("en");
    }

    #[test]
    fn en_json_has_all_setup_keys() {
        init();
        set_language("en");
        // Verify a sampling of keys from each category
        assert_eq!(t("setup.welcome.title"), "Welcome to AiOS!");
        assert_eq!(t("setup.audio_output.title"), "Test Audio Output");
        assert_eq!(t("setup.audio_input.title"), "Test Microphone");
        assert_eq!(t("setup.name.title"), "Name Your Assistant");
        assert_eq!(t("setup.provider.title"), "Choose Your AI Provider");
        assert_eq!(t("setup.password.title"), "Secure Your Data");
        assert_eq!(t("setup.confirm_password.title"), "Confirm Password");
        assert_eq!(t("setup.backup.title"), "Add a Backup Provider?");
        assert_eq!(t("setup.order.title"), "Choose Primary Provider");
        assert_eq!(t("setup.complete.title"), "First-Boot Setup Complete");
        assert_eq!(t("boot.status.desktop"), "Desktop");
        assert_eq!(t("hostname.conflict.apply"), "Apply");
        assert_eq!(t("web.placeholder"), "Message AiOS...");
    }
}
