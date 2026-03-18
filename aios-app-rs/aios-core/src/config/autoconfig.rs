//! Unattended auto-configuration for AiOS.
//!
//! Loads `autoconfig.json` from:
//! 1. Kernel boot parameter: `aios.autoconfig=/path/to/file`
//! 2. Baked into ISO: `/opt/aios-app/autoconfig.json`
//! 3. USB drive root: any mounted FAT/ext volume with `aios-autoconfig.json`
//!
//! If found, the setup wizard is skipped and the system is configured
//! automatically.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Auto-configuration loaded from JSON.
#[derive(Debug, Clone, Deserialize)]
pub struct AutoConfig {
    /// LLM provider settings.
    #[serde(default)]
    pub provider: ProviderConfig,

    /// System settings (keyboard, language, password, etc.).
    #[serde(default)]
    pub system: SystemConfig,

    /// Installation settings.
    #[serde(default)]
    pub install: InstallConfig,

    /// Assistant personality.
    #[serde(default)]
    pub assistant: AssistantConfig,

    /// Enable debug mode.
    #[serde(default)]
    pub debug: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// Primary provider: "claude" or "openai".
    #[serde(default = "default_provider")]
    pub primary: String,
    /// Claude API key.
    #[serde(default)]
    pub claude_api_key: String,
    /// OpenAI API key.
    #[serde(default)]
    pub openai_api_key: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            primary: default_provider(),
            claude_api_key: String::new(),
            openai_api_key: String::new(),
        }
    }
}

fn default_provider() -> String {
    "claude".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemConfig {
    /// Keyboard layout (e.g. "de", "us").
    #[serde(default = "default_keyboard")]
    pub keyboard: String,
    /// Language code (e.g. "en", "de").
    #[serde(default = "default_language")]
    pub language: String,
    /// Timezone (e.g. "Europe/Berlin").
    #[serde(default)]
    pub timezone: String,
    /// Hostname.
    #[serde(default = "default_hostname")]
    pub hostname: String,
    /// Master password for the vault.
    #[serde(default)]
    pub master_password: String,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            keyboard: default_keyboard(),
            language: default_language(),
            timezone: String::new(),
            hostname: default_hostname(),
            master_password: String::new(),
        }
    }
}

fn default_keyboard() -> String { "us".to_string() }
fn default_language() -> String { "en".to_string() }
fn default_hostname() -> String { "assistant".to_string() }

#[derive(Debug, Clone, Deserialize, Default)]
pub struct InstallConfig {
    /// Whether to install to a hard drive.
    #[serde(default)]
    pub enabled: bool,
    /// Target disk: "auto" picks the first non-USB disk, or "/dev/sdX" for explicit.
    #[serde(default)]
    pub target_disk: String,
    /// If false, still ask for confirmation before wiping the disk.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantConfig {
    /// Assistant display name.
    #[serde(default = "default_assistant_name")]
    pub name: String,
    /// Effort level: "auto", "low", "medium", "high".
    #[serde(default = "default_effort")]
    pub effort: String,
    /// Wake word phrase (e.g., "hey assistant", "jarvis").
    #[serde(default = "default_wake_word")]
    pub wake_word: String,
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self {
            name: default_assistant_name(),
            effort: default_effort(),
            wake_word: default_wake_word(),
        }
    }
}

fn default_assistant_name() -> String { "Assistant".to_string() }
fn default_effort() -> String { "auto".to_string() }
fn default_wake_word() -> String { "hey assistant".to_string() }

/// Search for and load an autoconfig file.
///
/// Returns `Some(AutoConfig)` if found and valid, `None` otherwise.
pub fn load_autoconfig() -> Option<AutoConfig> {
    // 1. Check kernel boot parameter
    if let Some(path) = kernel_param_path() {
        if let Some(cfg) = try_load(&path) {
            tracing::info!("Loaded autoconfig from kernel param: {}", path.display());
            return Some(cfg);
        }
    }

    // 2. Check baked-in location
    let baked = Path::new("/opt/aios-app/autoconfig.json");
    if let Some(cfg) = try_load(baked) {
        tracing::info!("Loaded autoconfig from ISO: {}", baked.display());
        return Some(cfg);
    }

    // 3. Check USB drives
    if let Some((path, cfg)) = scan_usb_drives() {
        tracing::info!("Loaded autoconfig from USB: {}", path.display());
        return Some(cfg);
    }

    None
}

/// Try to load and parse autoconfig from a path.
fn try_load(path: &Path) -> Option<AutoConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Read `aios.autoconfig=` from `/proc/cmdline`.
fn kernel_param_path() -> Option<PathBuf> {
    let cmdline = std::fs::read_to_string("/proc/cmdline").ok()?;
    for param in cmdline.split_whitespace() {
        if let Some(path) = param.strip_prefix("aios.autoconfig=") {
            return Some(PathBuf::from(path));
        }
    }
    None
}

/// Scan mounted USB/removable drives for `aios-autoconfig.json`.
fn scan_usb_drives() -> Option<(PathBuf, AutoConfig)> {
    // Check common mount points for removable media
    let mount_dirs = ["/media", "/mnt", "/run/media"];

    for base in &mount_dirs {
        let base_path = Path::new(base);
        if !base_path.exists() {
            continue;
        }
        // Walk one or two levels deep
        if let Ok(entries) = std::fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let candidate = path.join("aios-autoconfig.json");
                if let Some(cfg) = try_load(&candidate) {
                    return Some((candidate, cfg));
                }
                // One level deeper (e.g. /media/user/USB_DRIVE/)
                if path.is_dir() {
                    if let Ok(sub_entries) = std::fs::read_dir(&path) {
                        for sub in sub_entries.flatten() {
                            let sub_candidate = sub.path().join("aios-autoconfig.json");
                            if let Some(cfg) = try_load(&sub_candidate) {
                                return Some((sub_candidate, cfg));
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_autoconfig() {
        let json = r#"{
            "provider": {
                "primary": "claude",
                "claude_api_key": "sk-test",
                "openai_api_key": ""
            },
            "system": {
                "keyboard": "de",
                "language": "en",
                "timezone": "Europe/Berlin",
                "hostname": "mybox",
                "master_password": "12345678"
            },
            "install": {
                "enabled": true,
                "target_disk": "auto",
                "confirm": false
            },
            "assistant": {
                "name": "Buddy",
                "effort": "high",
                "wake_word": "hey computer"
            },
            "debug": true
        }"#;

        let cfg: AutoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.provider.primary, "claude");
        assert_eq!(cfg.provider.claude_api_key, "sk-test");
        assert_eq!(cfg.system.keyboard, "de");
        assert_eq!(cfg.system.master_password, "12345678");
        assert!(cfg.install.enabled);
        assert_eq!(cfg.install.target_disk, "auto");
        assert!(!cfg.install.confirm);
        assert_eq!(cfg.assistant.name, "Buddy");
        assert_eq!(cfg.assistant.wake_word, "hey computer");
        assert!(cfg.debug);
    }

    #[test]
    fn test_parse_minimal_autoconfig() {
        let json = r#"{
            "provider": { "claude_api_key": "sk-test" },
            "system": { "master_password": "12345678" }
        }"#;

        let cfg: AutoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.provider.primary, "claude");
        assert_eq!(cfg.system.keyboard, "us");
        assert!(!cfg.install.enabled);
        assert_eq!(cfg.assistant.name, "Assistant");
        assert_eq!(cfg.assistant.wake_word, "hey assistant");
    }

    #[test]
    fn test_parse_empty_autoconfig() {
        let json = "{}";
        let cfg: AutoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.provider.primary, "claude");
        assert_eq!(cfg.system.keyboard, "us");
        assert!(!cfg.debug);
        assert_eq!(cfg.assistant.wake_word, "hey assistant");
    }
}
