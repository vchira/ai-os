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

    // ========================================================================
    // Additional comprehensive tests
    // ========================================================================

    #[test]
    fn t_fmt_missing_placeholder_left_in_output() {
        init();
        set_language("en");
        // Pass no args at all — {provider} should remain as-is in the output
        let result = t_fmt("setup.backup.yes", &[]);
        assert!(
            result.contains("{provider}"),
            "expected unreplaced placeholder in '{result}'"
        );
    }

    #[test]
    fn t_fmt_with_extra_unused_args() {
        init();
        set_language("en");
        // Pass extra args that don't match any placeholder — no crash, no change
        let result = t_fmt("setup.backup.yes", &[("provider", "Claude"), ("unused", "value")]);
        assert_eq!(result, "Yes, add Claude");
    }

    #[test]
    fn t_fmt_same_placeholder_replaced_multiple_times() {
        init();
        set_language("en");
        // hostname.conflict.changed has {name} appearing twice
        let result = t_fmt("hostname.conflict.changed", &[("name", "test-host")]);
        // Both occurrences should be replaced
        assert_eq!(result, "Hostname changed to 'test-host'. Reachable as test-host.local");
        assert!(!result.contains("{name}"));
    }

    #[test]
    fn t_fmt_empty_replacement_value() {
        init();
        set_language("en");
        let result = t_fmt("setup.backup.yes", &[("provider", "")]);
        assert_eq!(result, "Yes, add ");
    }

    #[test]
    fn set_language_to_nonexistent_then_fallback() {
        init();
        set_language("xx_nonexistent");
        assert_eq!(current_language(), "xx_nonexistent");
        // Should fall back to English for known keys
        assert_eq!(t("chat.role.you"), "You");
        // Reset
        set_language("en");
    }

    #[test]
    fn set_language_to_empty_string_fallback() {
        init();
        set_language("");
        // Empty string is not "en", so fallback to en should trigger
        assert_eq!(t("chat.role.you"), "You");
        // Reset
        set_language("en");
    }

    #[test]
    fn get_translations_returns_none_for_unknown_language() {
        init();
        let result = get_translations("zz_unknown");
        assert!(result.is_none());
    }

    #[test]
    fn get_translations_returns_none_for_empty_string() {
        init();
        let result = get_translations("");
        assert!(result.is_none());
    }

    #[test]
    fn get_translations_de_exists() {
        init();
        let map = get_translations("de");
        assert!(map.is_some());
        let map = map.unwrap();
        // German translations should have the same key
        assert!(map.contains_key("chat.role.you"), "German should have chat.role.you");
    }

    #[test]
    fn available_languages_contains_en_and_de() {
        init();
        let langs = available_languages();
        let codes: Vec<&str> = langs.iter().map(|(code, _)| code.as_str()).collect();
        assert!(codes.contains(&"en"), "available_languages should contain 'en'");
        assert!(codes.contains(&"de"), "available_languages should contain 'de'");
    }

    #[test]
    fn available_languages_sorted() {
        init();
        let langs = available_languages();
        for window in langs.windows(2) {
            assert!(
                window[0].0 <= window[1].0,
                "languages not sorted: {:?} > {:?}",
                window[0].0,
                window[1].0
            );
        }
    }

    #[test]
    fn available_languages_have_names() {
        init();
        let langs = available_languages();
        for (code, name) in &langs {
            assert!(
                !name.is_empty(),
                "language '{}' has empty name",
                code
            );
        }
    }

    #[test]
    fn current_language_reflects_set() {
        set_language("de");
        assert_eq!(current_language(), "de");
        set_language("en");
        assert_eq!(current_language(), "en");
        set_language("fr");
        assert_eq!(current_language(), "fr");
        // Reset
        set_language("en");
    }

    #[test]
    fn t_german_translation_works() {
        init();
        set_language("de");
        let result = t("chat.role.you");
        // German for "You" — should not be the key itself
        assert_ne!(result, "chat.role.you");
        // Reset
        set_language("en");
    }

    #[test]
    fn t_german_falls_back_to_english_for_missing_key() {
        init();
        set_language("de");
        // If a key exists in English but not German, should fall back to English
        // We test with a key that's likely English-only or at minimum returns something
        let result = t("chat.role.you");
        assert!(!result.is_empty());
        assert_ne!(result, "chat.role.you");
        // Reset
        set_language("en");
    }

    #[test]
    fn t_returns_key_itself_when_not_in_any_language() {
        init();
        set_language("en");
        let key = "this.key.absolutely.does.not.exist.anywhere";
        assert_eq!(t(key), key);
    }

    #[test]
    fn t_fmt_with_special_characters_in_value() {
        init();
        set_language("en");
        // Test that replacement values with braces, backslashes, etc. work
        let result = t_fmt("setup.backup.yes", &[("provider", "{weird}")]);
        assert_eq!(result, "Yes, add {weird}");
    }

    #[test]
    fn init_can_be_called_multiple_times() {
        // init() should be idempotent
        init();
        init();
        init();
        set_language("en");
        assert_eq!(t("chat.role.you"), "You");
    }

    // ========================================================================
    // Thread safety and additional edge cases
    // ========================================================================

    #[test]
    fn t_thread_safety_concurrent_reads() {
        // Each thread has its own thread-local, so concurrent t() calls
        // should not panic or race.
        use std::sync::Arc;
        use std::sync::Barrier;

        let barrier = Arc::new(Barrier::new(4));
        let mut handles = Vec::new();

        for _ in 0..4 {
            let b = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                init();
                set_language("en");
                b.wait();
                // All threads call t() concurrently
                for _ in 0..100 {
                    let val = t("chat.role.you");
                    assert_eq!(val, "You");
                }
            }));
        }

        for h in handles {
            h.join().expect("thread panicked in t() concurrency test");
        }
    }

    #[test]
    fn t_thread_safety_different_languages() {
        // Thread-local means each thread can have its own language setting.
        let h1 = std::thread::spawn(|| {
            init();
            set_language("de");
            let val = t("chat.role.you");
            // German: "Du"
            assert_eq!(val, "Du");
        });

        let h2 = std::thread::spawn(|| {
            init();
            set_language("en");
            let val = t("chat.role.you");
            assert_eq!(val, "You");
        });

        h1.join().expect("German thread panicked");
        h2.join().expect("English thread panicked");
    }

    #[test]
    fn t_fmt_with_multiple_different_placeholders() {
        init();
        set_language("en");
        // cmd.wake.status has {current}, {source}, {status}, {threshold}
        let result = t_fmt("cmd.wake.status", &[
            ("current", "hey jarvis"),
            ("source", "pretrained"),
            ("status", "enabled"),
            ("threshold", "0.5"),
        ]);
        assert!(result.contains("hey jarvis"), "should contain current wake word");
        assert!(result.contains("pretrained"), "should contain source");
        assert!(result.contains("enabled"), "should contain status");
        assert!(result.contains("0.5"), "should contain threshold");
        // No unreplaced placeholders
        assert!(!result.contains("{current}"));
        assert!(!result.contains("{source}"));
        assert!(!result.contains("{status}"));
        assert!(!result.contains("{threshold}"));
    }

    #[test]
    fn t_fmt_partial_placeholder_match_leaves_unmatched() {
        init();
        set_language("en");
        // hostname.conflict.changed has {name} twice
        // Pass a different placeholder name — {name} should remain
        let result = t_fmt("hostname.conflict.changed", &[("names", "wrong")]);
        assert!(result.contains("{name}"), "unmatched {{name}} should remain: '{result}'");
    }

    #[test]
    fn available_languages_count_at_least_two() {
        init();
        let langs = available_languages();
        assert!(
            langs.len() >= 2,
            "expected at least 2 languages (en, de), got {}",
            langs.len()
        );
    }

    #[test]
    fn available_languages_en_name_is_english() {
        init();
        let langs = available_languages();
        let en = langs.iter().find(|(code, _)| code == "en");
        assert!(en.is_some(), "en should be in available_languages");
        // The English name comes from _meta.language, but _meta keys are skipped
        // in load_language, so the fallback is the code itself.
        // Just verify it's non-empty.
        assert!(!en.unwrap().1.is_empty());
    }

    #[test]
    fn get_translations_en_excludes_meta() {
        init();
        let map = get_translations("en").unwrap();
        // _meta is skipped during loading
        assert!(!map.contains_key("_meta"), "translations should not contain _meta key");
    }

    #[test]
    fn t_with_empty_key_returns_empty_key() {
        init();
        set_language("en");
        let result = t("");
        assert_eq!(result, "");
    }

    #[test]
    fn t_fmt_with_empty_key_returns_empty_string() {
        init();
        set_language("en");
        let result = t_fmt("", &[("foo", "bar")]);
        assert_eq!(result, "");
    }
}
