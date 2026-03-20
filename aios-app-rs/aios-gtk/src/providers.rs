//! Centralized provider definitions.
//!
//! Every UI surface that needs to enumerate or display LLM providers should
//! use the constants and helpers from this module.  This is the **single
//! source of truth** for provider metadata — display names, config keys,
//! default models, and model lists.

use aios_core::config::ConfigManager;

// ---------------------------------------------------------------------------
// Provider descriptor
// ---------------------------------------------------------------------------

/// Static metadata for a single LLM provider.
pub struct ProviderDef {
    /// Internal id (used in config, e.g. "claude", "openai").
    pub id: &'static str,
    /// Human-readable display name (e.g. "Claude", "ChatGPT").
    pub display_name: &'static str,
    /// Config key that holds the API key (e.g. "llm.claude_api_key").
    /// Empty string for providers that don't need a key (Ollama).
    pub api_key_config: &'static str,
    /// Config key that holds the selected model.
    pub model_config: &'static str,
    /// Default model slug.
    pub default_model: &'static str,
    /// Known models: (human name, slug).
    pub models: &'static [(&'static str, &'static str)],
    /// Whether this provider needs an API key (false for Ollama).
    pub needs_api_key: bool,
    /// Config key for the enabled flag (only Ollama uses this).
    /// Empty string means "enabled if API key is present".
    pub enabled_config: &'static str,
}

// ---------------------------------------------------------------------------
// The canonical list — ALL providers defined here, nowhere else.
// ---------------------------------------------------------------------------

pub const PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "claude",
        display_name: "Claude",
        api_key_config: "llm.claude_api_key",
        model_config: "llm.claude_model",
        default_model: "claude-sonnet-4-20250514",
        models: &[
            ("Claude Sonnet 4", "claude-sonnet-4-20250514"),
            ("Claude Opus 4", "claude-opus-4-20250514"),
            ("Claude Haiku 3.5", "claude-haiku-4-5-20251001"),
        ],
        needs_api_key: true,
        enabled_config: "",
    },
    ProviderDef {
        id: "openai",
        display_name: "ChatGPT",
        api_key_config: "llm.openai_api_key",
        model_config: "llm.openai_model",
        default_model: "gpt-4o",
        models: &[
            ("GPT-4o", "gpt-4o"),
            ("GPT-4o Mini", "gpt-4o-mini"),
            ("GPT-4 Turbo", "gpt-4-turbo"),
        ],
        needs_api_key: true,
        enabled_config: "",
    },
    ProviderDef {
        id: "deepseek",
        display_name: "DeepSeek",
        api_key_config: "llm.deepseek_api_key",
        model_config: "llm.deepseek_model",
        default_model: "deepseek-chat",
        models: &[
            ("DeepSeek Chat", "deepseek-chat"),
            ("DeepSeek Reasoner", "deepseek-reasoner"),
        ],
        needs_api_key: true,
        enabled_config: "",
    },
    ProviderDef {
        id: "mistral",
        display_name: "Mistral",
        api_key_config: "llm.mistral_api_key",
        model_config: "llm.mistral_model",
        default_model: "mistral-small-latest",
        models: &[
            ("Mistral Small", "mistral-small-latest"),
            ("Mistral Medium", "mistral-medium-latest"),
            ("Mistral Large", "mistral-large-latest"),
        ],
        needs_api_key: true,
        enabled_config: "",
    },
    ProviderDef {
        id: "groq",
        display_name: "Groq",
        api_key_config: "llm.groq_api_key",
        model_config: "llm.groq_model",
        default_model: "llama-3.3-70b-versatile",
        models: &[
            ("Llama 3.3 70B", "llama-3.3-70b-versatile"),
            ("Llama 3.1 8B", "llama-3.1-8b-instant"),
            ("Mixtral 8x7B", "mixtral-8x7b-32768"),
        ],
        needs_api_key: true,
        enabled_config: "",
    },
    ProviderDef {
        id: "gemini",
        display_name: "Gemini",
        api_key_config: "llm.gemini_api_key",
        model_config: "llm.gemini_model",
        default_model: "gemini-2.0-flash",
        models: &[
            ("Gemini 2.0 Flash", "gemini-2.0-flash"),
            ("Gemini 2.0 Pro", "gemini-2.0-pro"),
            ("Gemini 1.5 Pro", "gemini-1.5-pro"),
        ],
        needs_api_key: true,
        enabled_config: "",
    },
    ProviderDef {
        id: "ollama",
        display_name: "Local",
        api_key_config: "",
        model_config: "llm.ollama_model",
        default_model: "llama3.1:8b",
        models: &[
            ("Llama 3.2 1B", "llama3.2:1b"),
            ("Llama 3.2 3B", "llama3.2:3b"),
            ("Llama 3.1 8B", "llama3.1:8b"),
            ("Llama 3.1 70B", "llama3.1:70b"),
            ("Mistral 7B", "mistral:7b"),
            ("Phi-3 3.8B", "phi3:3.8b"),
        ],
        needs_api_key: false,
        enabled_config: "llm.ollama_enabled",
    },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Look up a provider definition by internal id (e.g. "claude").
pub fn find_by_id(id: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Look up a provider definition by display name (e.g. "ChatGPT").
pub fn find_by_display_name(name: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.display_name == name)
}

/// Map a display name (from the dropdown) to the internal id.
/// Falls back to lowercasing the input.
pub fn display_name_to_id(name: &str) -> String {
    find_by_display_name(name)
        .map(|p| p.id.to_string())
        .unwrap_or_else(|| name.to_lowercase())
}

/// Check whether a provider is configured (has a non-empty API key, or
/// is enabled via its toggle for key-less providers like Ollama).
pub fn is_configured(provider: &ProviderDef, config: &ConfigManager) -> bool {
    if provider.needs_api_key {
        let key = config.get_str(provider.api_key_config, "");
        !key.is_empty() && key != "your-api-key-here"
    } else {
        // Key-less provider (Ollama) — configured if explicitly enabled
        // OR if it's the active provider (e.g. set via autoconfig).
        if !provider.enabled_config.is_empty() && config.get_bool(provider.enabled_config, false) {
            return true;
        }
        config.get_str("llm.provider", "") == provider.id
    }
}

/// Return display names for all configured providers (including Ollama).
#[allow(dead_code)]
pub fn configured_display_names(config: &ConfigManager) -> Vec<String> {
    PROVIDERS
        .iter()
        .filter(|p| is_configured(p, config))
        .map(|p| p.display_name.to_string())
        .collect()
}

/// Get the current model string for a provider from config.
pub fn current_model(provider: &ProviderDef, config: &ConfigManager) -> String {
    config.get_str(provider.model_config, provider.default_model)
}

/// Look up the human-readable name for a model slug.
///
/// Searches all providers' model lists. Returns the slug itself if not found.
pub fn model_human_name(slug: &str) -> String {
    for prov in PROVIDERS {
        for (human, model_slug) in prov.models {
            if *model_slug == slug {
                return human.to_string();
            }
        }
    }
    slug.to_string()
}

/// Mask an API key for display: show a recognizable prefix + last 4 chars.
///
/// Examples: `"sk-ant-...lwAA"`, `"gsk_...6xcR"`, `""` → `""`.
pub fn mask_api_key(key: &str) -> String {
    let len = key.len();
    if len == 0 {
        return String::new();
    }
    if len <= 8 {
        return format!("...{key}");
    }
    // Find the last separator within the first 8 chars for a natural prefix break.
    let prefix_end = key[..8]
        .rfind(|c: char| c == '-' || c == '_')
        .map(|i| i + 1) // include the separator
        .unwrap_or(4);
    let last4 = &key[len - 4..];
    format!("{}...{}", &key[..prefix_end], last4)
}


/// Return the base API URL for a provider.
///
/// For Claude, returns the Anthropic API base. For OpenAI-compatible providers,
/// returns the base URL that `OpenAIProvider::with_base_url()` uses. The caller
/// appends `/v1/messages` for Claude or `/v1/chat/completions` for others.
pub fn provider_api_url(provider_id: &str) -> &'static str {
    match provider_id {
        "claude" => "https://api.anthropic.com",
        "openai" => "https://api.openai.com",
        "deepseek" => "https://api.deepseek.com",
        "mistral" => "https://api.mistral.ai",
        "groq" => "https://api.groq.com/openai",
        "gemini" => "https://generativelanguage.googleapis.com/v1beta/openai",
        "ollama" => "http://localhost:11434",
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_human_name_known() {
        assert_eq!(model_human_name("deepseek-reasoner"), "DeepSeek Reasoner");
        assert_eq!(model_human_name("claude-sonnet-4-20250514"), "Claude Sonnet 4");
        assert_eq!(model_human_name("gpt-4o"), "GPT-4o");
        assert_eq!(model_human_name("llama-3.1-8b-instant"), "Llama 3.1 8B"); // Groq
        assert_eq!(model_human_name("llama3.1:8b"), "Llama 3.1 8B"); // Local
    }

    #[test]
    fn model_human_name_unknown() {
        assert_eq!(model_human_name("some-unknown-model"), "some-unknown-model");
    }

    #[test]
    fn mask_api_key_normal() {
        assert_eq!(mask_api_key("sk-ant-FAKE-testkey1234abcdXXYY"), "sk-ant-...XXYY");
        assert_eq!(mask_api_key("gsk_FAKE_TESTKEYabcdefghijklmnopqrstuvwxyz0123456789ABCD"), "gsk_...ABCD");
    }

    #[test]
    fn mask_api_key_edge_cases() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("abc"), "...abc");
        assert_eq!(mask_api_key("abcdefgh"), "...abcdefgh"); // <= 8 chars
    }

    #[test]
    fn local_provider_configured_when_active() {
        let path = std::env::temp_dir().join("aios-test-local-active.json");
        let mut config = ConfigManager::with_path(path).unwrap();
        let _ = config.set("llm.provider", serde_json::json!("ollama"));
        let ollama = find_by_id("ollama").unwrap();
        assert!(is_configured(ollama, &config));
        let names = configured_display_names(&config);
        assert!(names.contains(&"Local".to_string()));
    }

    #[test]
    fn provider_api_url_known() {
        assert_eq!(provider_api_url("claude"), "https://api.anthropic.com");
        assert_eq!(provider_api_url("deepseek"), "https://api.deepseek.com");
        assert_eq!(provider_api_url("groq"), "https://api.groq.com/openai");
        assert!(provider_api_url("gemini").contains("googleapis.com"));
    }

    #[test]
    fn provider_api_url_unknown() {
        assert_eq!(provider_api_url("nonexistent"), "");
    }

    #[test]
    fn find_by_id_all_providers() {
        for prov in PROVIDERS {
            assert!(find_by_id(prov.id).is_some(), "find_by_id({}) should succeed", prov.id);
        }
        assert!(find_by_id("nonexistent").is_none());
    }

    #[test]
    fn find_by_display_name_all_providers() {
        for prov in PROVIDERS {
            assert!(
                find_by_display_name(prov.display_name).is_some(),
                "find_by_display_name({}) should succeed",
                prov.display_name
            );
        }
        assert!(find_by_display_name("Nonexistent").is_none());
    }

    #[test]
    fn display_name_to_id_roundtrip() {
        for prov in PROVIDERS {
            assert_eq!(
                display_name_to_id(prov.display_name),
                prov.id,
                "display_name_to_id({}) should return {}",
                prov.display_name,
                prov.id
            );
        }
    }

    #[test]
    fn model_human_name_all_providers() {
        // Every model slug in PROVIDERS should resolve to a human name
        for prov in PROVIDERS {
            for (human, slug) in prov.models {
                let result = model_human_name(slug);
                assert_eq!(
                    result, *human,
                    "model_human_name({slug}) should be {human}"
                );
            }
        }
    }

    #[test]
    fn provider_api_url_all_providers() {
        // Every provider should have a non-empty URL
        for prov in PROVIDERS {
            let url = provider_api_url(prov.id);
            assert!(
                !url.is_empty(),
                "provider_api_url({}) should not be empty",
                prov.id
            );
        }
    }

    #[test]
    fn mask_api_key_preserves_recognizable_prefix() {
        // Key should be recognizable from its masked form
        let masked = mask_api_key("sk-ant-FAKE-01234567890abcdef");
        assert!(masked.starts_with("sk-ant-"), "should preserve sk-ant- prefix");
        assert!(masked.ends_with("cdef"), "should preserve last 4 chars");
        assert!(masked.contains("..."), "should contain ...");
    }

    #[test]
    fn is_configured_with_key() {
        let path = std::env::temp_dir().join("aios-test-configured.json");
        let mut config = ConfigManager::with_path(path).unwrap();
        let _ = config.set("llm.claude_api_key", serde_json::json!("sk-test-key"));
        let claude = find_by_id("claude").unwrap();
        assert!(is_configured(claude, &config));
    }

    #[test]
    fn is_configured_without_key() {
        let path = std::env::temp_dir().join("aios-test-not-configured.json");
        let config = ConfigManager::with_path(path).unwrap();
        let deepseek = find_by_id("deepseek").unwrap();
        assert!(!is_configured(deepseek, &config));
    }

    #[test]
    fn configured_names_only_includes_keyed_providers() {
        let path = std::env::temp_dir().join("aios-test-configured-names.json");
        let mut config = ConfigManager::with_path(path).unwrap();
        let _ = config.set("llm.claude_api_key", serde_json::json!("sk-test"));
        let _ = config.set("llm.deepseek_api_key", serde_json::json!("sk-test-ds"));
        let names = configured_display_names(&config);
        assert!(names.contains(&"Claude".to_string()));
        assert!(names.contains(&"DeepSeek".to_string()));
        assert!(!names.contains(&"ChatGPT".to_string()));
    }
}
