//! Memory tool — persistent key-value store for the AI.
//!
//! Data is stored as a single JSON file at `~/.aios/memory.json`.
//! Ported from `aios-app/aios/tools/builtin/memory.py`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use aios_core::types::ToolResult;
use tracing::debug;

use crate::tool::Tool;

/// Default storage path: `~/.aios/memory.json`.
fn default_memory_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().join(".aios").join("memory.json"))
        .unwrap_or_else(|| PathBuf::from(".aios/memory.json"))
}

/// Persistent key-value memory store.
///
/// Supports four actions:
/// - **memorize** — store a key-value pair
/// - **recall** — retrieve the value for a key
/// - **forget** — delete a key
/// - **list_keys** — list all stored keys
pub struct MemoryTool {
    storage_path: PathBuf,
}

impl MemoryTool {
    /// Create a new `MemoryTool`.
    ///
    /// If `storage_path` is `None`, the default `~/.aios/memory.json` is used.
    pub fn new(storage_path: Option<PathBuf>) -> Self {
        Self {
            storage_path: storage_path.unwrap_or_else(default_memory_path),
        }
    }

    /// Load the JSON store from disk.  Returns an empty map if the file does
    /// not exist or is empty.
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        if !self.storage_path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = fs::read_to_string(&self.storage_path)
            .map_err(|e| format!("Failed to read memory file: {e}"))?;
        if text.trim().is_empty() {
            return Ok(BTreeMap::new());
        }
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse memory JSON: {e}"))
    }

    /// Save the store back to disk, creating parent directories as needed.
    fn save(&self, store: &BTreeMap<String, String>) -> Result<(), String> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create memory directory: {e}"))?;
        }
        let json =
            serde_json::to_string_pretty(store).map_err(|e| format!("Failed to serialize: {e}"))?;
        fs::write(&self.storage_path, json)
            .map_err(|e| format!("Failed to write memory file: {e}"))?;
        Ok(())
    }
}

impl Tool for MemoryTool {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "Persistent key-value memory store. \
         Memorize facts, recall them later, forget them, or list all keys."
    }

    fn category(&self) -> &str {
        "memory"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["memorize", "recall", "forget", "list_keys"],
                    "description": "The memory operation to perform."
                },
                "key": {
                    "type": "string",
                    "description": "The key to memorize/recall/forget (not required for list_keys)."
                },
                "value": {
                    "type": "string",
                    "description": "The value to memorize (required for 'memorize' action)."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let key = args.get("key").and_then(|v| v.as_str());
        let value = args.get("value").and_then(|v| v.as_str());

        let mut store = match self.load() {
            Ok(s) => s,
            Err(e) => return ToolResult::fail(format!("Failed to load memory store: {e}")),
        };

        match action {
            "memorize" => {
                let Some(key) = key.filter(|k| !k.is_empty()) else {
                    return ToolResult::fail("'key' is required for memorize.");
                };
                let Some(value) = value else {
                    return ToolResult::fail("'value' is required for memorize.");
                };
                store.insert(key.to_string(), value.to_string());
                if let Err(e) = self.save(&store) {
                    return ToolResult::fail(e);
                }
                debug!(key, "memorized");
                ToolResult::ok(format!("Memorized key {key:?}."))
            }

            "recall" => {
                let Some(key) = key.filter(|k| !k.is_empty()) else {
                    return ToolResult::fail("'key' is required for recall.");
                };
                match store.get(key) {
                    Some(val) => ToolResult::ok_with_data(
                        val.clone(),
                        serde_json::json!({ "key": key, "value": val }),
                    ),
                    None => ToolResult::fail(format!("No memory found for key {key:?}.")),
                }
            }

            "forget" => {
                let Some(key) = key.filter(|k| !k.is_empty()) else {
                    return ToolResult::fail("'key' is required for forget.");
                };
                if !store.contains_key(key) {
                    return ToolResult::fail(format!("No memory found for key {key:?}."));
                }
                store.remove(key);
                if let Err(e) = self.save(&store) {
                    return ToolResult::fail(e);
                }
                debug!(key, "forgot");
                ToolResult::ok(format!("Forgot key {key:?}."))
            }

            "list_keys" => {
                let keys: Vec<&String> = store.keys().collect();
                let output = if keys.is_empty() {
                    "(no memories stored)".to_string()
                } else {
                    keys.iter()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                ToolResult::ok_with_data(output, serde_json::json!({ "keys": keys }))
            }

            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. Use: memorize, recall, forget, list_keys."
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tool() -> (MemoryTool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.json");
        (MemoryTool::new(Some(path)), dir)
    }

    #[test]
    fn memorize_and_recall() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "memorize",
            "key": "name",
            "value": "Alice"
        }));
        assert!(r.success);

        let r = tool.execute(serde_json::json!({
            "action": "recall",
            "key": "name"
        }));
        assert!(r.success);
        assert_eq!(r.output, "Alice");
    }

    #[test]
    fn recall_missing_key() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "recall",
            "key": "missing"
        }));
        assert!(!r.success);
    }

    #[test]
    fn forget_key() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "memorize",
            "key": "x",
            "value": "1"
        }));
        let r = tool.execute(serde_json::json!({
            "action": "forget",
            "key": "x"
        }));
        assert!(r.success);

        let r = tool.execute(serde_json::json!({
            "action": "recall",
            "key": "x"
        }));
        assert!(!r.success);
    }

    #[test]
    fn list_keys_empty() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "list_keys" }));
        assert!(r.success);
        assert_eq!(r.output, "(no memories stored)");
    }

    #[test]
    fn list_keys_populated() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "memorize", "key": "b", "value": "2"
        }));
        tool.execute(serde_json::json!({
            "action": "memorize", "key": "a", "value": "1"
        }));
        let r = tool.execute(serde_json::json!({ "action": "list_keys" }));
        assert!(r.success);
        // BTreeMap keeps keys sorted
        assert_eq!(r.output, "a\nb");
    }

    #[test]
    fn unknown_action() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "nope" }));
        assert!(!r.success);
    }

    #[test]
    fn memorize_requires_key() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "memorize",
            "value": "orphan"
        }));
        assert!(!r.success);
    }

    #[test]
    fn memorize_requires_value() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "memorize",
            "key": "k"
        }));
        assert!(!r.success);
    }

    // -- New comprehensive tests --

    #[test]
    fn memorize_and_recall_roundtrip() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "memorize",
            "key": "color",
            "value": "blue"
        }));
        assert!(r.success);
        assert!(r.output.contains("color"));

        let r = tool.execute(serde_json::json!({
            "action": "recall",
            "key": "color"
        }));
        assert!(r.success);
        assert_eq!(r.output, "blue");
        // Check structured data is present.
        let data = r.data.unwrap();
        assert_eq!(data["key"], "color");
        assert_eq!(data["value"], "blue");
    }

    #[test]
    fn recall_nonexistent_key() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "recall",
            "key": "does_not_exist"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("does_not_exist"));
    }

    #[test]
    fn forget_nonexistent_key() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "forget",
            "key": "ghost"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("ghost"));
    }

    #[test]
    fn list_keys_empty_store() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "list_keys" }));
        assert!(r.success);
        assert_eq!(r.output, "(no memories stored)");
        let data = r.data.unwrap();
        assert!(data["keys"].as_array().unwrap().is_empty());
    }

    #[test]
    fn list_keys_with_data() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "memorize", "key": "z", "value": "last"
        }));
        tool.execute(serde_json::json!({
            "action": "memorize", "key": "a", "value": "first"
        }));
        tool.execute(serde_json::json!({
            "action": "memorize", "key": "m", "value": "middle"
        }));

        let r = tool.execute(serde_json::json!({ "action": "list_keys" }));
        assert!(r.success);
        // BTreeMap keeps keys sorted: a, m, z.
        assert_eq!(r.output, "a\nm\nz");
        let data = r.data.unwrap();
        let keys: Vec<&str> = data["keys"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(keys, vec!["a", "m", "z"]);
    }

    #[test]
    fn overwrite_existing_key() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "memorize", "key": "k", "value": "old"
        }));
        tool.execute(serde_json::json!({
            "action": "memorize", "key": "k", "value": "new"
        }));

        let r = tool.execute(serde_json::json!({
            "action": "recall", "key": "k"
        }));
        assert!(r.success);
        assert_eq!(r.output, "new");
    }

    #[test]
    fn forget_then_recall_fails() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "memorize", "key": "temp", "value": "data"
        }));
        let r = tool.execute(serde_json::json!({
            "action": "forget", "key": "temp"
        }));
        assert!(r.success);

        let r = tool.execute(serde_json::json!({
            "action": "recall", "key": "temp"
        }));
        assert!(!r.success);
    }

    #[test]
    fn memorize_empty_key_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "memorize",
            "key": "",
            "value": "val"
        }));
        assert!(!r.success);
    }

    #[test]
    fn recall_empty_key_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "recall",
            "key": ""
        }));
        assert!(!r.success);
    }

    #[test]
    fn forget_empty_key_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "forget",
            "key": ""
        }));
        assert!(!r.success);
    }

    #[test]
    fn tool_name_and_category() {
        let (tool, _dir) = temp_tool();
        assert_eq!(tool.name(), "memory");
        assert_eq!(tool.category(), "memory");
    }

    #[test]
    fn persistence_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.json");

        // First instance: write.
        let tool1 = MemoryTool::new(Some(path.clone()));
        tool1.execute(serde_json::json!({
            "action": "memorize", "key": "persist", "value": "yes"
        }));

        // Second instance: read.
        let tool2 = MemoryTool::new(Some(path));
        let r = tool2.execute(serde_json::json!({
            "action": "recall", "key": "persist"
        }));
        assert!(r.success);
        assert_eq!(r.output, "yes");
    }

    #[test]
    fn invalid_action_returns_error() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "explode" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown action"));
        assert!(r.error.as_deref().unwrap().contains("explode"));
    }

    #[test]
    fn memorize_overwrites_existing_key_value() {
        let (tool, _dir) = temp_tool();
        // Store initial value.
        let r = tool.execute(serde_json::json!({
            "action": "memorize", "key": "color", "value": "red"
        }));
        assert!(r.success);

        // Overwrite with new value.
        let r = tool.execute(serde_json::json!({
            "action": "memorize", "key": "color", "value": "green"
        }));
        assert!(r.success);

        // Recall should return the updated value.
        let r = tool.execute(serde_json::json!({
            "action": "recall", "key": "color"
        }));
        assert!(r.success);
        assert_eq!(r.output, "green");

        // list_keys should still show only one "color" entry.
        let r = tool.execute(serde_json::json!({ "action": "list_keys" }));
        assert!(r.success);
        assert_eq!(r.output, "color");
    }

    #[test]
    fn memorize_multiple_keys_and_list() {
        let (tool, _dir) = temp_tool();
        for i in 0..5 {
            tool.execute(serde_json::json!({
                "action": "memorize",
                "key": format!("key_{i}"),
                "value": format!("val_{i}")
            }));
        }
        let r = tool.execute(serde_json::json!({ "action": "list_keys" }));
        assert!(r.success);
        let keys: Vec<&str> = r.output.split('\n').collect();
        assert_eq!(keys.len(), 5);
        // BTreeMap keeps sorted order.
        assert_eq!(keys[0], "key_0");
        assert_eq!(keys[4], "key_4");
    }
}
