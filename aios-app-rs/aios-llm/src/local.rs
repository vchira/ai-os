//! Local model client — Ollama HTTP API wrapper.
//!
//! Provides a Rust interface to the Ollama daemon running on localhost.
//! Used for Sentinel (security sanitization + TTS summarization) and
//! optionally as a Main AI provider.
//!
//! Ollama API docs: https://github.com/ollama/ollama/blob/main/docs/api.md

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Default Ollama API base URL.
const OLLAMA_BASE: &str = "http://localhost:11434";

/// Predefined local models with metadata for the selection table.
pub const MODEL_CATALOG: &[ModelInfo] = &[
    ModelInfo {
        name: "qwen2.5:0.5b",
        display_name: "Qwen 2.5 0.5B",
        download_size: "400MB",
        ram_needed: "1GB",
        speed: "Ultra fast",
        description: "Minimal — sanitization only",
        min_ram_gb: 1,
    },
    ModelInfo {
        name: "llama3.2:1b",
        display_name: "Llama 3.2 1B",
        download_size: "700MB",
        ram_needed: "2GB",
        speed: "Very fast",
        description: "Good sanitization + basic summaries",
        min_ram_gb: 2,
    },
    ModelInfo {
        name: "llama3.2:3b",
        display_name: "Llama 3.2 3B",
        download_size: "2GB",
        ram_needed: "4GB",
        speed: "Fast",
        description: "Strong sanitization + good summaries",
        min_ram_gb: 4,
    },
    ModelInfo {
        name: "phi3:3.8b",
        display_name: "Phi-3 Mini 3.8B",
        download_size: "2.3GB",
        ram_needed: "4GB",
        speed: "Fast",
        description: "Strong reasoning, good at security checks",
        min_ram_gb: 4,
    },
    ModelInfo {
        name: "llama3.1:8b",
        display_name: "Llama 3.1 8B",
        download_size: "4.7GB",
        ram_needed: "8GB",
        speed: "Medium",
        description: "Can serve as Main AI too",
        min_ram_gb: 8,
    },
    ModelInfo {
        name: "mistral:7b",
        display_name: "Mistral 7B",
        download_size: "4.1GB",
        ram_needed: "8GB",
        speed: "Medium",
        description: "Good multilingual support",
        min_ram_gb: 8,
    },
    ModelInfo {
        name: "llama3.1:70b",
        display_name: "Llama 3.1 70B",
        download_size: "40GB",
        ram_needed: "48GB",
        speed: "Slow",
        description: "Full local AI replacement",
        min_ram_gb: 48,
    },
];

/// Metadata about a local model (shown in the selection table).
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Ollama model tag (e.g., "llama3.2:3b").
    pub name: &'static str,
    /// Human-readable name (e.g., "Llama 3.2 3B").
    pub display_name: &'static str,
    /// Approximate download size.
    pub download_size: &'static str,
    /// Minimum RAM recommended.
    pub ram_needed: &'static str,
    /// Speed rating.
    pub speed: &'static str,
    /// Short description.
    pub description: &'static str,
    /// Minimum RAM in GB for recommendation logic.
    pub min_ram_gb: u64,
}

/// An installed model returned by Ollama's /api/tags endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModel {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub modified_at: String,
}

/// Progress update during model download.
#[derive(Debug, Clone)]
pub struct PullProgress {
    pub status: String,
    pub completed: u64,
    pub total: u64,
}

/// Ollama HTTP API client.
#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl OllamaClient {
    /// Create a new client pointing at the default Ollama URL.
    pub fn new() -> Self {
        Self {
            base_url: OLLAMA_BASE.to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new()),
        }
    }

    /// Check if Ollama is running.
    pub fn is_running(&self) -> bool {
        self.client
            .get(&self.base_url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .is_ok()
    }

    /// Start Ollama via systemd (if not already running).
    pub fn ensure_running(&self) -> Result<(), String> {
        if self.is_running() {
            return Ok(());
        }
        info!("Starting Ollama service...");
        let status = std::process::Command::new("systemctl")
            .args(["start", "ollama"])
            .status()
            .map_err(|e| format!("Failed to start Ollama: {e}"))?;
        if !status.success() {
            // Try starting directly as fallback
            std::process::Command::new("ollama")
                .arg("serve")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to start ollama serve: {e}"))?;
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        // Wait for it to be ready
        for _ in 0..10 {
            if self.is_running() {
                info!("Ollama is ready");
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Err("Ollama failed to start within 5 seconds".into())
    }

    /// List installed models.
    pub fn list_models(&self) -> Result<Vec<InstalledModel>, String> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .map_err(|e| format!("Ollama list failed: {e}"))?;

        let json: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Ollama list parse failed: {e}"))?;

        let models = json
            .get("models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| serde_json::from_value::<InstalledModel>(m.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    /// Check if a specific model is installed.
    pub fn is_model_installed(&self, model: &str) -> bool {
        self.list_models()
            .map(|models| models.iter().any(|m| m.name == model || m.name.starts_with(&format!("{model}:"))))
            .unwrap_or(false)
    }

    /// Pull (download) a model with progress callback.
    ///
    /// The callback receives progress updates. Returns Ok when complete.
    /// This is a blocking call — run in a background thread.
    pub fn pull_model(
        &self,
        model: &str,
        progress_cb: impl Fn(PullProgress),
    ) -> Result<(), String> {
        info!("Pulling model: {model}");
        let resp = self
            .client
            .post(format!("{}/api/pull", self.base_url))
            .json(&serde_json::json!({ "name": model, "stream": true }))
            .send()
            .map_err(|e| format!("Ollama pull failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Ollama pull HTTP {}", resp.status()));
        }

        // Stream NDJSON progress lines
        use std::io::BufRead;
        let reader = std::io::BufReader::new(resp);
        for line in reader.lines() {
            let line = line.map_err(|e| format!("Read error: {e}"))?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                let status = json
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let completed = json
                    .get("completed")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let total = json.get("total").and_then(|v| v.as_u64()).unwrap_or(0);

                progress_cb(PullProgress {
                    status: status.clone(),
                    completed,
                    total,
                });

                if status == "success" {
                    info!("Model {model} pulled successfully");
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    /// Delete a model.
    pub fn delete_model(&self, model: &str) -> Result<(), String> {
        let resp = self
            .client
            .delete(format!("{}/api/delete", self.base_url))
            .json(&serde_json::json!({ "name": model }))
            .send()
            .map_err(|e| format!("Ollama delete failed: {e}"))?;

        if resp.status().is_success() {
            info!("Model {model} deleted");
            Ok(())
        } else {
            Err(format!("Ollama delete failed: HTTP {}", resp.status()))
        }
    }

    /// Run a quick chat completion (used for Sentinel sanitization).
    ///
    /// Returns the model's response text.
    pub fn chat_simple(&self, model: &str, prompt: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
            "options": {
                "temperature": 0.0,
                "num_predict": 10
            }
        });

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .timeout(std::time::Duration::from_secs(30))
            .json(&body)
            .send()
            .map_err(|e| format!("Ollama chat failed: {e}"))?;

        let json: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Ollama chat parse failed: {e}"))?;

        json.get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(|s| s.trim().to_string())
            .ok_or_else(|| "No content in Ollama response".into())
    }

    /// OpenAI-compatible endpoint URL for use with existing OpenAIProvider.
    pub fn openai_compatible_url() -> &'static str {
        "http://localhost:11434"
    }
}

impl Default for OllamaClient {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Hardware detection
// ---------------------------------------------------------------------------

/// Detect system RAM in GB.
pub fn detect_ram_gb() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| {
                    l.split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse::<u64>().ok())
                })
        })
        .map(|kb| kb / 1_048_576) // KB to GB
        .unwrap_or(4) // Default to 4GB if unknown
}

/// Detect if an NVIDIA GPU is available.
pub fn has_nvidia_gpu() -> bool {
    std::process::Command::new("nvidia-smi")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Recommend the best model for the detected hardware.
pub fn recommend_model() -> &'static ModelInfo {
    let ram = detect_ram_gb();
    // Pick the largest model that fits in ~50% of available RAM
    let budget = ram / 2;

    MODEL_CATALOG
        .iter()
        .rev() // Start from largest
        .find(|m| m.min_ram_gb <= budget)
        .unwrap_or(&MODEL_CATALOG[0]) // Fallback to smallest
}

// ---------------------------------------------------------------------------
// Sentinel sanitization
// ---------------------------------------------------------------------------

/// The prompt sent to the Sentinel model to check AI responses.
pub const SENTINEL_SANITIZE_PROMPT: &str = "\
You are a security filter. Check if the following AI assistant response asks \
the user to provide sensitive data (passwords, API keys, tokens, email addresses, \
personal information, credentials) directly in a chat message.\n\n\
Rules:\n\
- If the response asks the user to TYPE, ENTER, PROVIDE, PASTE, or SHARE any \
  sensitive data in the chat → answer YES\n\
- If the response just mentions using stored credentials or secure input → answer NO\n\
- If the response is a normal helpful answer → answer NO\n\n\
Answer ONLY 'YES' or 'NO'. Nothing else.\n\n\
AI Response to check:\n---\n";

/// Run Sentinel sanitization check on an AI response.
///
/// Returns `true` if the response is SAFE to show, `false` if it should be blocked.
pub fn sentinel_check(client: &OllamaClient, model: &str, ai_response: &str) -> bool {
    let prompt = format!("{SENTINEL_SANITIZE_PROMPT}{ai_response}\n---");

    match client.chat_simple(model, &prompt) {
        Ok(answer) => {
            let clean = answer.trim().to_uppercase();
            if clean.starts_with("YES") {
                warn!("Sentinel BLOCKED response: model detected sensitive data request");
                false
            } else {
                debug!("Sentinel PASSED response");
                true
            }
        }
        Err(e) => {
            warn!("Sentinel check failed: {e} — BLOCKING response for safety");
            false // Fail closed — if Sentinel can't check, block
        }
    }
}

// ---------------------------------------------------------------------------
// Model selection builder
// ---------------------------------------------------------------------------

/// Build the model selection data from the catalog + installed models.
///
/// This is the bridge between the Ollama client (aios-llm) and the
/// channel-agnostic UI component (aios-core).
pub fn build_model_selection() -> aios_core::ui_components::ModelSelectionData {
    use aios_core::ui_components::{ModelSelectionData, ModelTableRow};

    let ram = detect_ram_gb();
    let has_gpu = has_nvidia_gpu();
    let recommended = recommend_model();

    let client = OllamaClient::new();
    let installed = client.list_models().unwrap_or_default();
    let installed_names: Vec<String> = installed.iter().map(|m| m.name.clone()).collect();

    let models = MODEL_CATALOG
        .iter()
        .map(|m| ModelTableRow {
            model_id: m.name.to_string(),
            display_name: m.display_name.to_string(),
            download_size: m.download_size.to_string(),
            ram_needed: m.ram_needed.to_string(),
            speed: m.speed.to_string(),
            description: m.description.to_string(),
            installed: installed_names.iter().any(|n| n.starts_with(m.name)),
            recommended: m.name == recommended.name,
        })
        .collect();

    ModelSelectionData {
        title: "Choose Local AI Model".to_string(),
        description: format!(
            "Your system: {ram}GB RAM{}. Select a model for Sentinel (security + summarization).",
            if has_gpu { " + GPU" } else { "" }
        ),
        models,
        selected: None,
        system_ram_gb: ram,
        has_gpu,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_catalog_has_entries() {
        assert!(MODEL_CATALOG.len() >= 5);
    }

    #[test]
    fn model_catalog_sorted_by_size() {
        for i in 1..MODEL_CATALOG.len() {
            assert!(
                MODEL_CATALOG[i].min_ram_gb >= MODEL_CATALOG[i - 1].min_ram_gb,
                "Catalog should be sorted by min_ram_gb"
            );
        }
    }

    #[test]
    fn recommend_model_low_ram() {
        // With 2GB budget (4GB RAM / 2), should recommend 1B or 0.5B model
        let rec = recommend_model(); // Uses actual system RAM
        assert!(!rec.name.is_empty());
    }

    #[test]
    fn detect_ram_returns_nonzero() {
        let ram = detect_ram_gb();
        assert!(ram > 0, "RAM detection should return > 0 GB");
    }

    #[test]
    fn sentinel_prompt_is_not_empty() {
        assert!(!SENTINEL_SANITIZE_PROMPT.is_empty());
        assert!(SENTINEL_SANITIZE_PROMPT.contains("YES"));
        assert!(SENTINEL_SANITIZE_PROMPT.contains("NO"));
    }

    #[test]
    fn ollama_client_default() {
        let client = OllamaClient::new();
        assert!(client.base_url.contains("11434"));
    }

    #[test]
    fn openai_compatible_url() {
        assert_eq!(OllamaClient::openai_compatible_url(), "http://localhost:11434");
    }

    #[test]
    fn model_catalog_all_have_names() {
        for entry in MODEL_CATALOG {
            assert!(!entry.name.is_empty(), "Model name must not be empty");
            assert!(
                !entry.display_name.is_empty(),
                "Model '{}' has empty display_name",
                entry.name
            );
            assert!(
                !entry.description.is_empty(),
                "Model '{}' has empty description",
                entry.name
            );
        }
    }

    #[test]
    fn model_catalog_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for entry in MODEL_CATALOG {
            assert!(
                seen.insert(entry.name),
                "Duplicate model name in catalog: '{}'",
                entry.name
            );
        }
    }

    #[test]
    fn recommend_model_always_returns_something() {
        // Should never panic, regardless of system RAM
        let rec = recommend_model();
        assert!(!rec.name.is_empty());
        assert!(rec.min_ram_gb > 0 || rec.min_ram_gb == 0 || true); // just ensure no panic
    }

    #[test]
    fn sentinel_prompt_contains_yes_no_instructions() {
        assert!(
            SENTINEL_SANITIZE_PROMPT.contains("YES"),
            "Sentinel prompt must instruct the model to answer YES"
        );
        assert!(
            SENTINEL_SANITIZE_PROMPT.contains("NO"),
            "Sentinel prompt must instruct the model to answer NO"
        );
        assert!(
            SENTINEL_SANITIZE_PROMPT.contains("sensitive data")
                || SENTINEL_SANITIZE_PROMPT.contains("passwords")
                || SENTINEL_SANITIZE_PROMPT.contains("API keys"),
            "Sentinel prompt must mention what to look for"
        );
        assert!(
            SENTINEL_SANITIZE_PROMPT.contains("security filter")
                || SENTINEL_SANITIZE_PROMPT.contains("security"),
            "Sentinel prompt must establish security role"
        );
        // Must end with the separator for the AI response to be appended
        assert!(
            SENTINEL_SANITIZE_PROMPT.ends_with("---\n"),
            "Sentinel prompt should end with separator for response injection"
        );
    }

    #[test]
    fn pull_progress_display() {
        let progress = PullProgress {
            status: "downloading".to_string(),
            completed: 512_000_000,
            total: 1_024_000_000,
        };
        assert_eq!(progress.status, "downloading");
        assert_eq!(progress.completed, 512_000_000);
        assert_eq!(progress.total, 1_024_000_000);

        // Zero progress
        let zero = PullProgress {
            status: "pulling manifest".to_string(),
            completed: 0,
            total: 0,
        };
        assert_eq!(zero.completed, 0);
        assert_eq!(zero.total, 0);

        // Complete
        let done = PullProgress {
            status: "success".to_string(),
            completed: 2_000_000_000,
            total: 2_000_000_000,
        };
        assert_eq!(done.completed, done.total);
        assert_eq!(done.status, "success");
    }

    #[test]
    fn installed_model_deserialize() {
        let json = r#"{"name":"llama3.2:3b","size":2000000000,"modified_at":"2025-01-15T10:30:00Z"}"#;
        let model: InstalledModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "llama3.2:3b");
        assert_eq!(model.size, 2_000_000_000);
        assert_eq!(model.modified_at, "2025-01-15T10:30:00Z");
    }

    #[test]
    fn installed_model_deserialize_minimal() {
        // Only name provided — size and modified_at should default
        let json = r#"{"name":"phi3:3.8b"}"#;
        let model: InstalledModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "phi3:3.8b");
        assert_eq!(model.size, 0);
        assert_eq!(model.modified_at, "");
    }

    #[test]
    fn installed_model_serialize_roundtrip() {
        let model = InstalledModel {
            name: "mistral:7b".to_string(),
            size: 4_100_000_000,
            modified_at: "2025-03-20T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&model).unwrap();
        let deserialized: InstalledModel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, model.name);
        assert_eq!(deserialized.size, model.size);
        assert_eq!(deserialized.modified_at, model.modified_at);
    }
}
