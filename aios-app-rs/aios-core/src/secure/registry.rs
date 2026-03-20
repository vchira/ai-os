//! Secure value registry — AI-visible metadata, AI-blind values.
//!
//! The [`SecureRegistry`] stores metadata about sensitive values (name,
//! description, kind) that the AI can read. The actual values live in the
//! encrypted vault and are NEVER returned to the AI model.
//!
//! # Security Model
//!
//! - AI can: list entries, check if a key exists, read descriptions
//! - AI cannot: read values, ask for values in prompts
//! - Values entered: only via UI panel (user types directly into secure input)
//! - Values used: only by tools, only after user permission popup
//! - Values NEVER appear in: AI responses, conversation history, logs

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Metadata about a secure value — visible to the AI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureEntry {
    /// Machine-readable key (e.g., "gmail_app_password").
    pub key: String,
    /// Human-readable name (e.g., "Gmail App Password").
    pub name: String,
    /// Description of what this value is for (e.g., "App password for
    /// sending emails via Gmail SMTP. Generated in Google Account settings.").
    pub description: String,
    /// Category of the secret.
    pub kind: SecureKind,
    /// Whether the value has been stored (vs. just registered as needed).
    pub has_value: bool,
}

/// Categories of secure data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecureKind {
    /// Passwords, app passwords, PINs
    Password,
    /// API keys, access tokens
    ApiKey,
    /// Email addresses
    Email,
    /// Full name, address, phone number
    PersonalInfo,
    /// SSH keys, certificates
    CryptoKey,
    /// Other sensitive data
    Other,
}

impl SecureKind {
    /// Patterns in key names that indicate this kind.
    pub fn from_key(key: &str) -> Self {
        let lower = key.to_lowercase();
        if lower.contains("password") || lower.contains("passwd") || lower.contains("pin") {
            Self::Password
        } else if lower.contains("api_key") || lower.contains("apikey")
            || lower.contains("token") || lower.contains("secret")
        {
            Self::ApiKey
        } else if lower.contains("email") || lower.contains("e_mail") {
            Self::Email
        } else if lower.contains("name") || lower.contains("address")
            || lower.contains("phone") || lower.contains("birth")
        {
            Self::PersonalInfo
        } else if lower.contains("ssh") || lower.contains("private_key")
            || lower.contains("certificate")
        {
            Self::CryptoKey
        } else {
            Self::Other
        }
    }
}

/// Registry of secure values — metadata only, no actual secrets.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecureRegistry {
    entries: BTreeMap<String, SecureEntry>,
}

impl SecureRegistry {
    /// Load the registry from a JSON file, or create empty if it doesn't exist.
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Save the registry to a JSON file.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    /// Register a new secure entry (metadata only, no value).
    pub fn register(&mut self, key: &str, name: &str, description: &str, kind: SecureKind) {
        self.entries.insert(
            key.to_string(),
            SecureEntry {
                key: key.to_string(),
                name: name.to_string(),
                description: description.to_string(),
                kind,
                has_value: false,
            },
        );
    }

    /// Mark an entry as having a value stored in the vault.
    pub fn mark_stored(&mut self, key: &str) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.has_value = true;
        }
    }

    /// Check if a key exists and has a value.
    pub fn has_value(&self, key: &str) -> bool {
        self.entries.get(key).map(|e| e.has_value).unwrap_or(false)
    }

    /// Get entry metadata (AI-safe — no values).
    pub fn get(&self, key: &str) -> Option<&SecureEntry> {
        self.entries.get(key)
    }

    /// List all entries with their metadata (AI-safe — no values).
    pub fn list(&self) -> Vec<&SecureEntry> {
        self.entries.values().collect()
    }

    /// Remove an entry.
    pub fn remove(&mut self, key: &str) {
        self.entries.remove(key);
    }

    /// Default registry path.
    pub fn default_path() -> PathBuf {
        crate::config::ConfigManager::default_config_dir().join("secure_registry.json")
    }
}

/// Patterns that indicate a prompt is asking the user for sensitive data.
///
/// Used by the prompt security filter to detect when the AI is trying to
/// get the user to type secrets directly in the chat.
pub const SENSITIVE_ASK_PATTERNS: &[&str] = &[
    "enter your password",
    "type your password",
    "provide your password",
    "what is your password",
    "share your password",
    "enter your api key",
    "type your api key",
    "provide your api key",
    "what is your api key",
    "enter your app password",
    "type your app password",
    "provide your app password",
    "enter your email address",
    "what is your email",
    "provide your email address",
    "enter your token",
    "provide your token",
    "what is your secret",
    "enter your private key",
    "paste your key",
    "paste your password",
    "tell me your password",
    "give me your password",
    "give me your api key",
    "send me your",
    "type your credential",
    "enter your credential",
];

/// Check if a text contains patterns that ask for sensitive data.
///
/// Returns the matched pattern if found, or None.
pub fn detect_sensitive_ask(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    SENSITIVE_ASK_PATTERNS
        .iter()
        .find(|pattern| lower.contains(**pattern))
        .copied()
}

/// Check if a memory key likely holds a credential/secret value.
pub fn is_credential_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("credential")
        || lower.contains("app_password")
        || lower.contains("private_key")
        || lower.contains("auth")
        || lower.contains("pin")
}

/// Scan text for leaked secure/private values.
///
/// Checks if any stored secure or private value appears verbatim in the text.
/// This catches cases where a tool result or AI response accidentally includes
/// a raw credential or personal data value.
///
/// Returns a list of leaked key names. An empty list means the text is clean.
pub fn scan_for_leaked_values(text: &str, memory_path: &std::path::Path) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    // Load the memory store to get actual values.
    let store: std::collections::BTreeMap<String, String> = std::fs::read_to_string(memory_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let mut leaked = Vec::new();
    for (key, value) in &store {
        // Only check credential/private keys, and only values long enough to
        // avoid false positives (short values like "yes" or "80" would match everywhere).
        if value.len() >= 6 && is_credential_key(key) && text.contains(value.as_str()) {
            leaked.push(key.clone());
        }
    }
    leaked
}

/// Check if a key name indicates private (non-secure) personal data.
pub fn is_private_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("email")
        || lower.contains("e_mail")
        || lower.contains("name")
        || lower.contains("address")
        || lower.contains("phone")
        || lower.contains("birth")
        || lower.contains("age")
        || lower.contains("gender")
}

/// Check if a key is either credential (secure) or private.
pub fn is_protected_key(key: &str) -> bool {
    is_credential_key(key) || is_private_key(key)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_kind_from_key() {
        assert_eq!(SecureKind::from_key("gmail_app_password"), SecureKind::Password);
        assert_eq!(SecureKind::from_key("openai_api_key"), SecureKind::ApiKey);
        assert_eq!(SecureKind::from_key("user_email"), SecureKind::Email);
        assert_eq!(SecureKind::from_key("full_name"), SecureKind::PersonalInfo);
        assert_eq!(SecureKind::from_key("ssh_private_key"), SecureKind::CryptoKey);
        assert_eq!(SecureKind::from_key("random_note"), SecureKind::Other);
    }

    #[test]
    fn registry_register_and_check() {
        let mut reg = SecureRegistry::default();
        reg.register("gmail_pw", "Gmail Password", "App password for SMTP", SecureKind::Password);
        assert!(!reg.has_value("gmail_pw")); // registered but no value yet
        reg.mark_stored("gmail_pw");
        assert!(reg.has_value("gmail_pw")); // now has value
    }

    #[test]
    fn registry_list_returns_metadata_only() {
        let mut reg = SecureRegistry::default();
        reg.register("k1", "Key One", "First key", SecureKind::Password);
        reg.register("k2", "Key Two", "Second key", SecureKind::ApiKey);
        let entries = reg.list();
        assert_eq!(entries.len(), 2);
        // Entries have descriptions but no values — struct has no value field
        assert!(entries.iter().all(|e| !e.key.is_empty()));
    }

    #[test]
    fn detect_sensitive_ask_catches_password_requests() {
        assert!(detect_sensitive_ask("Please enter your password below:").is_some());
        assert!(detect_sensitive_ask("What is your API key?").is_some());
        assert!(detect_sensitive_ask("Type your app password here").is_some());
        assert!(detect_sensitive_ask("Please provide your email address").is_some());
    }

    #[test]
    fn detect_sensitive_ask_allows_normal_text() {
        assert!(detect_sensitive_ask("I can help you send an email").is_none());
        assert!(detect_sensitive_ask("Let me check your stored credentials").is_none());
        assert!(detect_sensitive_ask("I'll use the secure input for that").is_none());
    }

    #[test]
    fn detect_sensitive_ask_catches_sneaky_requests() {
        assert!(detect_sensitive_ask("Could you paste your key in the chat?").is_some());
        assert!(detect_sensitive_ask("Please give me your password so I can help").is_some());
        assert!(detect_sensitive_ask("Can you tell me your password?").is_some());
    }

    #[test]
    fn registry_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");

        let mut reg = SecureRegistry::default();
        reg.register("test_key", "Test", "A test entry", SecureKind::Other);
        reg.mark_stored("test_key");
        reg.save(&path).unwrap();

        let loaded = SecureRegistry::load(&path);
        assert!(loaded.has_value("test_key"));
        assert_eq!(loaded.get("test_key").unwrap().name, "Test");
    }
}
