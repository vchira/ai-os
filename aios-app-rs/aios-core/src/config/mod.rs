//! Configuration management — persistent JSON settings and slash commands.

pub mod commands;
pub mod defaults;

use std::path::PathBuf;

use serde_json::Value;
use tracing::{debug, warn};

use crate::error::Result;
use defaults::defaults;

// ---------------------------------------------------------------------------
// ConfigManager
// ---------------------------------------------------------------------------

/// Manages AiOS configuration with persistent JSON storage.
///
/// On creation the manager loads `~/.aios/config.json` (or a custom path),
/// deep-merges it with [`defaults::defaults()`], and exposes dotted-key
/// accessors (`get`, `set`, `get_str`, …).  Every `set` call persists to
/// disk immediately.
#[derive(Debug)]
pub struct ConfigManager {
    /// The in-memory configuration tree.
    config: Value,
    /// Path to the JSON config file on disk.
    config_path: PathBuf,
}

impl ConfigManager {
    /// Create a new `ConfigManager`, loading from the default path
    /// (`~/.aios/config.json`).
    pub fn new() -> Result<Self> {
        let config_dir = Self::default_config_dir();
        let config_path = config_dir.join("config.json");
        Self::with_path(config_path)
    }

    /// Create a `ConfigManager` that reads/writes at `path`.
    pub fn with_path(path: PathBuf) -> Result<Self> {
        let mut mgr = Self {
            config: Value::Object(serde_json::Map::new()),
            config_path: path,
        };
        mgr.load()?;
        Ok(mgr)
    }

    // -- loading / saving ---------------------------------------------------

    /// Load configuration from disk and deep-merge with defaults.
    fn load(&mut self) -> Result<()> {
        // Ensure the parent directory exists.
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let user_config = if self.config_path.exists() {
            let data = std::fs::read_to_string(&self.config_path)?;
            match serde_json::from_str::<Value>(&data) {
                Ok(v) => v,
                Err(e) => {
                    warn!(path = %self.config_path.display(), error = %e, "corrupt config — using defaults");
                    Value::Object(serde_json::Map::new())
                }
            }
        } else {
            debug!(path = %self.config_path.display(), "no config file — using defaults");
            Value::Object(serde_json::Map::new())
        };

        self.config = deep_merge(&defaults(), &user_config);
        Ok(())
    }

    /// Persist the current in-memory config to disk.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(&self.config)?;
        std::fs::write(&self.config_path, data)?;
        Ok(())
    }

    // -- getters ------------------------------------------------------------

    /// Get a config value by dotted key path (e.g. `"voice.tts_voice"`).
    ///
    /// Returns `default` if the key does not exist.
    pub fn get(&self, key: &str, default: Value) -> Value {
        self.resolve(key).cloned().unwrap_or(default)
    }

    /// Get a string value, returning `default` if absent or not a string.
    pub fn get_str(&self, key: &str, default: &str) -> String {
        self.resolve(key)
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| default.to_owned())
    }

    /// Get a boolean value, returning `default` if absent or not a bool.
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.resolve(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }

    /// Get an `f64` value, returning `default` if absent or not a number.
    pub fn get_f64(&self, key: &str, default: f64) -> f64 {
        self.resolve(key)
            .and_then(|v| v.as_f64())
            .unwrap_or(default)
    }

    /// Walk the dotted key path and return a reference to the leaf value.
    fn resolve(&self, key: &str) -> Option<&Value> {
        let mut current = &self.config;
        for part in key.split('.') {
            current = current.get(part)?;
        }
        Some(current)
    }

    // -- setter -------------------------------------------------------------

    /// Set a value by dotted key path, then persist to disk.
    ///
    /// Intermediate objects are created if they do not already exist.
    pub fn set(&mut self, key: &str, value: Value) -> Result<()> {
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = &mut self.config;

        for &part in &parts[..parts.len() - 1] {
            if !current.get(part).is_some_and(|v| v.is_object()) {
                current[part] = Value::Object(serde_json::Map::new());
            }
            current = current.get_mut(part).unwrap();
        }

        if let Some(last) = parts.last() {
            current[*last] = value;
        }

        self.save()
    }

    // -- path helpers -------------------------------------------------------

    /// The default AiOS config directory (`~/.aios`).
    pub fn default_config_dir() -> PathBuf {
        directories::BaseDirs::new()
            .map(|d| d.home_dir().join(".aios"))
            .unwrap_or_else(|| PathBuf::from("/tmp/.aios"))
    }

    /// Return the config directory for this instance (parent of the config file).
    pub fn config_dir(&self) -> PathBuf {
        self.config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    }

    /// Return the plugins directory, creating it if necessary.
    pub fn plugins_dir(&self) -> PathBuf {
        let raw = self.get_str("tools.plugins_dir", "~/.aios/plugins");
        let expanded = if raw.starts_with("~/") {
            if let Some(home) = directories::BaseDirs::new() {
                home.home_dir().join(&raw[2..])
            } else {
                PathBuf::from(&raw)
            }
        } else {
            PathBuf::from(&raw)
        };
        let _ = std::fs::create_dir_all(&expanded);
        expanded
    }

    /// Return the models directory, creating it if necessary.
    pub fn models_dir(&self) -> PathBuf {
        let dir = self.config_dir().join("models");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// Return the path to the config file on disk.
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Return a reference to the entire in-memory config tree.
    pub fn raw(&self) -> &Value {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// Deep merge
// ---------------------------------------------------------------------------

/// Recursively merge `overrides` into `defaults`.
///
/// Object keys from `overrides` take precedence. Non-object values in
/// `overrides` replace the corresponding default entirely.
pub fn deep_merge(defaults: &Value, overrides: &Value) -> Value {
    match (defaults, overrides) {
        (Value::Object(def), Value::Object(ovr)) => {
            let mut result = def.clone();
            for (key, ovr_val) in ovr {
                let merged = if let Some(def_val) = def.get(key) {
                    deep_merge(def_val, ovr_val)
                } else {
                    ovr_val.clone()
                };
                result.insert(key.clone(), merged);
            }
            Value::Object(result)
        }
        // Non-object: override wins.
        (_, ovr) => ovr.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deep_merge_basic() {
        let a = json!({"a": 1, "b": {"c": 2, "d": 3}});
        let b = json!({"b": {"c": 99}, "e": 5});
        let m = deep_merge(&a, &b);
        assert_eq!(m["a"], 1);
        assert_eq!(m["b"]["c"], 99);
        assert_eq!(m["b"]["d"], 3);
        assert_eq!(m["e"], 5);
    }

    #[test]
    fn config_manager_with_temp_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mgr = ConfigManager::with_path(path).unwrap();

        // Should have defaults
        assert_eq!(mgr.get_str("llm.provider", ""), "claude");
        assert!(mgr.get_bool("voice.stt_enabled", false));
        assert_eq!(mgr.get_str("ui.theme", ""), "dark");
    }

    #[test]
    fn config_manager_set_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        {
            let mut mgr = ConfigManager::with_path(path.clone()).unwrap();
            mgr.set("llm.provider", json!("openai")).unwrap();
        }

        // Reload and verify
        let mgr2 = ConfigManager::with_path(path).unwrap();
        assert_eq!(mgr2.get_str("llm.provider", ""), "openai");
        // Other defaults should still be present
        assert_eq!(mgr2.get_str("ui.theme", ""), "dark");
    }

    #[test]
    fn config_manager_get_f64() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mgr = ConfigManager::with_path(path).unwrap();
        assert!((mgr.get_f64("voice.tts_rate", 0.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn config_manager_directory_helpers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mgr = ConfigManager::with_path(path).unwrap();
        assert_eq!(mgr.config_dir(), dir.path());
        assert!(mgr.models_dir().ends_with("models"));
    }
}
