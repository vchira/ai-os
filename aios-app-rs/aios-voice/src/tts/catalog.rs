//! Voice catalog — static registry of known Piper voice models.
//!
//! This provides metadata for well-known Piper voices so the UI can display
//! them even before models are downloaded.

use std::sync::LazyLock;

use super::{VoiceGender, VoiceInfo, VoiceQuality};

/// Static list of known Piper voices with metadata.
///
/// This is not exhaustive — users can install additional voices.  The catalog
/// simply provides a curated list for common languages.
pub static VOICES: LazyLock<Vec<VoiceInfo>> = LazyLock::new(build_catalog);

/// Build the curated voice catalog.
fn build_catalog() -> Vec<VoiceInfo> {
    vec![
        // -------------------------------------------------------------------
        // English (US)
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "en_US-amy-medium".into(),
            name: "Amy".into(),
            language: "en_US".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        VoiceInfo {
            id: "en_US-amy-low".into(),
            name: "Amy (Low)".into(),
            language: "en_US".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Low,
            sample_rate: 22050,
        },
        VoiceInfo {
            id: "en_US-ryan-medium".into(),
            name: "Ryan".into(),
            language: "en_US".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        VoiceInfo {
            id: "en_US-ryan-high".into(),
            name: "Ryan (High)".into(),
            language: "en_US".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::High,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // English (UK)
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "en_GB-alan-medium".into(),
            name: "Alan".into(),
            language: "en_GB".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        VoiceInfo {
            id: "en_GB-cori-medium".into(),
            name: "Cori".into(),
            language: "en_GB".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // German
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "de_DE-thorsten-medium".into(),
            name: "Thorsten".into(),
            language: "de_DE".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        VoiceInfo {
            id: "de_DE-eva_k-medium".into(),
            name: "Eva".into(),
            language: "de_DE".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // French
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "fr_FR-siwis-medium".into(),
            name: "Siwis".into(),
            language: "fr_FR".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        VoiceInfo {
            id: "fr_FR-gilles-medium".into(),
            name: "Gilles".into(),
            language: "fr_FR".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // Spanish
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "es_ES-davefx-medium".into(),
            name: "Dave".into(),
            language: "es_ES".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        VoiceInfo {
            id: "es_MX-ald-medium".into(),
            name: "Ald".into(),
            language: "es_MX".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // Italian
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "it_IT-riccardo-medium".into(),
            name: "Riccardo".into(),
            language: "it_IT".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // Portuguese (Brazil)
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "pt_BR-faber-medium".into(),
            name: "Faber".into(),
            language: "pt_BR".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // Romanian
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "ro_RO-mihai-medium".into(),
            name: "Mihai".into(),
            language: "ro_RO".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // Japanese
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "ja_JP-kokoro-medium".into(),
            name: "Kokoro".into(),
            language: "ja_JP".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // Chinese (Mandarin)
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "zh_CN-huayan-medium".into(),
            name: "Huayan".into(),
            language: "zh_CN".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // Korean
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "ko_KR-kss-medium".into(),
            name: "KSS".into(),
            language: "ko_KR".into(),
            gender: VoiceGender::Female,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
        // -------------------------------------------------------------------
        // Hindi
        // -------------------------------------------------------------------
        VoiceInfo {
            id: "hi_IN-madhur-medium".into(),
            name: "Madhur".into(),
            language: "hi_IN".into(),
            gender: VoiceGender::Male,
            quality: VoiceQuality::Medium,
            sample_rate: 22050,
        },
    ]
}

/// Find the default voice for a given language code.
///
/// Matches on the full code first (e.g. "en_US"), then falls back to the
/// language prefix (e.g. "en").  Returns the first medium-quality female
/// voice, falling back to any available voice.
pub fn default_voice_for_language(lang: &str) -> Option<&VoiceInfo> {
    let voices = &*VOICES;

    // Exact language match, prefer female + medium.
    let exact_preferred = voices.iter().find(|v| {
        v.language == lang && v.gender == VoiceGender::Female && v.quality == VoiceQuality::Medium
    });
    if exact_preferred.is_some() {
        return exact_preferred;
    }

    // Exact match, any voice.
    let exact_any = voices.iter().find(|v| v.language == lang);
    if exact_any.is_some() {
        return exact_any;
    }

    // Prefix match (e.g. "en" matches "en_US" and "en_GB").
    let prefix = if lang.contains('_') {
        lang.split('_').next().unwrap_or(lang)
    } else {
        lang
    };

    let prefix_preferred = voices.iter().find(|v| {
        v.language.starts_with(prefix)
            && v.gender == VoiceGender::Female
            && v.quality == VoiceQuality::Medium
    });
    if prefix_preferred.is_some() {
        return prefix_preferred;
    }

    voices.iter().find(|v| v.language.starts_with(prefix))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_entries() {
        assert!(!VOICES.is_empty());
    }

    #[test]
    fn default_voice_en_us() {
        let voice = default_voice_for_language("en_US");
        assert!(voice.is_some());
        let v = voice.unwrap();
        assert_eq!(v.language, "en_US");
        assert_eq!(v.gender, VoiceGender::Female);
    }

    #[test]
    fn default_voice_prefix_match() {
        let voice = default_voice_for_language("en");
        assert!(voice.is_some());
        assert!(voice.unwrap().language.starts_with("en"));
    }

    #[test]
    fn default_voice_romanian() {
        let voice = default_voice_for_language("ro_RO");
        assert!(voice.is_some());
        assert_eq!(voice.unwrap().language, "ro_RO");
    }

    #[test]
    fn default_voice_german() {
        let voice = default_voice_for_language("de_DE");
        assert!(voice.is_some());
        let v = voice.unwrap();
        assert_eq!(v.language, "de_DE");
        // Prefer female.
        assert_eq!(v.gender, VoiceGender::Female);
    }

    #[test]
    fn default_voice_unknown_returns_none() {
        let voice = default_voice_for_language("xx_XX");
        assert!(voice.is_none());
    }

    #[test]
    fn all_voices_have_valid_sample_rate() {
        for v in VOICES.iter() {
            assert!(v.sample_rate > 0, "voice {} has zero sample rate", v.id);
        }
    }

    #[test]
    fn all_voices_have_nonempty_fields() {
        for v in VOICES.iter() {
            assert!(!v.id.is_empty(), "voice has empty id");
            assert!(!v.name.is_empty(), "voice {} has empty name", v.id);
            assert!(!v.language.is_empty(), "voice {} has empty language", v.id);
        }
    }
}
