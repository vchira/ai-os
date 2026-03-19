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
        display_name: "Ollama",
        api_key_config: "",
        model_config: "llm.ollama_model",
        default_model: "llama3.2",
        models: &[
            ("Llama 3.2", "llama3.2"),
            ("Llama 3.1", "llama3.1"),
            ("Mistral 7B", "mistral"),
            ("Phi-3", "phi3"),
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
        // Key-less provider (Ollama) — check enabled flag.
        if provider.enabled_config.is_empty() {
            false
        } else {
            config.get_bool(provider.enabled_config, false)
        }
    }
}

/// Return display names for all configured providers.
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
