//! Reusable UI component definitions — channel-agnostic.
//!
//! These structs define UI components as pure data. Each channel (Desktop/GTK,
//! Web/HTML, Signal/text) renders them using its own implementation.
//!
//! This avoids duplicating UI logic across channels.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Model Selection Table
// ---------------------------------------------------------------------------

/// A row in the model selection table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTableRow {
    /// Ollama model tag (e.g., "llama3.2:3b").
    pub model_id: String,
    /// Human-readable name.
    pub display_name: String,
    /// Download size (e.g., "2GB").
    pub download_size: String,
    /// RAM requirement (e.g., "4GB").
    pub ram_needed: String,
    /// Speed rating (e.g., "Fast").
    pub speed: String,
    /// Short description.
    pub description: String,
    /// Whether this model is installed.
    pub installed: bool,
    /// Whether this is the recommended default for the current hardware.
    pub recommended: bool,
}

/// The full model selection component data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelectionData {
    /// Title (e.g., "Choose Sentinel Model").
    pub title: String,
    /// Description text.
    pub description: String,
    /// Available models.
    pub models: Vec<ModelTableRow>,
    /// Currently selected model ID (if any).
    pub selected: Option<String>,
    /// Detected system RAM in GB.
    pub system_ram_gb: u64,
    /// Whether a GPU was detected.
    pub has_gpu: bool,
}

impl ModelSelectionData {
    /// Render as a plain text table (for Signal channel or logs).
    pub fn to_text_table(&self) -> String {
        let mut lines = vec![
            self.title.clone(),
            self.description.clone(),
            String::new(),
            format!(
                "{:<25} {:>8} {:>6} {:>12} {}",
                "Model", "Size", "RAM", "Speed", "Status"
            ),
            "-".repeat(75),
        ];

        for m in &self.models {
            let status = if m.installed {
                "\u{2705}"
            } else if m.recommended {
                "\u{2b50} recommended"
            } else {
                ""
            };
            lines.push(format!(
                "{:<25} {:>8} {:>6} {:>12} {}",
                m.display_name, m.download_size, m.ram_needed, m.speed, status
            ));
        }

        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Download Progress
// ---------------------------------------------------------------------------

/// Progress state for a model download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgressData {
    /// Model being downloaded.
    pub model_id: String,
    /// Human-readable model name.
    pub display_name: String,
    /// Download size (e.g., "2GB").
    pub download_size: String,
    /// Current status text (e.g., "pulling manifest", "downloading 45%").
    pub status: String,
    /// Bytes completed.
    pub completed: u64,
    /// Total bytes.
    pub total: u64,
    /// Whether the download is finished.
    pub done: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

impl DownloadProgressData {
    /// Progress as a fraction (0.0 to 1.0).
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.completed as f64 / self.total as f64
        }
    }

    /// Progress as a human-readable string.
    pub fn progress_text(&self) -> String {
        if self.total == 0 {
            self.status.clone()
        } else {
            let mb_done = self.completed / 1_048_576;
            let mb_total = self.total / 1_048_576;
            let pct = (self.fraction() * 100.0) as u32;
            format!("{} — {}MB / {}MB ({}%)", self.status, mb_done, mb_total, pct)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_progress_fraction() {
        let p = DownloadProgressData {
            model_id: "test".into(),
            display_name: "Test".into(),
            download_size: "1GB".into(),
            status: "downloading".into(),
            completed: 500,
            total: 1000,
            done: false,
            error: None,
        };
        assert!((p.fraction() - 0.5).abs() < 0.01);
    }

    #[test]
    fn download_progress_zero_total() {
        let p = DownloadProgressData {
            model_id: "test".into(),
            display_name: "Test".into(),
            download_size: "1GB".into(),
            status: "pulling manifest".into(),
            completed: 0,
            total: 0,
            done: false,
            error: None,
        };
        assert_eq!(p.fraction(), 0.0);
        assert_eq!(p.progress_text(), "pulling manifest");
    }

    #[test]
    fn model_table_text_rendering() {
        let data = ModelSelectionData {
            title: "Test".into(),
            description: "Pick a model".into(),
            models: vec![ModelTableRow {
                model_id: "llama3.2:3b".into(),
                display_name: "Llama 3.2 3B".into(),
                download_size: "2GB".into(),
                ram_needed: "4GB".into(),
                speed: "Fast".into(),
                description: "Test model".into(),
                installed: false,
                recommended: true,
            }],
            selected: None,
            system_ram_gb: 8,
            has_gpu: false,
        };
        let text = data.to_text_table();
        assert!(text.contains("Llama 3.2 3B"));
        assert!(text.contains("recommended"));
    }

    #[test]
    fn model_selection_to_text_table_format() {
        let data = ModelSelectionData {
            title: "Choose Local AI Model".into(),
            description: "Your system: 16GB RAM. Select a model.".into(),
            models: vec![
                ModelTableRow {
                    model_id: "qwen2.5:0.5b".into(),
                    display_name: "Qwen 2.5 0.5B".into(),
                    download_size: "400MB".into(),
                    ram_needed: "1GB".into(),
                    speed: "Ultra fast".into(),
                    description: "Minimal".into(),
                    installed: true,
                    recommended: false,
                },
                ModelTableRow {
                    model_id: "llama3.2:3b".into(),
                    display_name: "Llama 3.2 3B".into(),
                    download_size: "2GB".into(),
                    ram_needed: "4GB".into(),
                    speed: "Fast".into(),
                    description: "Strong sanitization".into(),
                    installed: false,
                    recommended: true,
                },
            ],
            selected: None,
            system_ram_gb: 16,
            has_gpu: false,
        };
        let text = data.to_text_table();

        // Title and description on first lines
        assert!(text.starts_with("Choose Local AI Model"));
        assert!(text.contains("Your system: 16GB RAM"));

        // Column headers present
        assert!(text.contains("Model"));
        assert!(text.contains("Size"));
        assert!(text.contains("RAM"));
        assert!(text.contains("Speed"));
        assert!(text.contains("Status"));

        // Separator line
        assert!(text.contains("---"));

        // Installed model shows checkmark
        assert!(text.contains("\u{2705}"), "Installed model should show checkmark");

        // Recommended model shows star + "recommended"
        assert!(text.contains("\u{2b50} recommended"), "Recommended model should show star");

        // Both model names present
        assert!(text.contains("Qwen 2.5 0.5B"));
        assert!(text.contains("Llama 3.2 3B"));
    }

    #[test]
    fn download_progress_percentage() {
        // 0% — initial state
        let p0 = DownloadProgressData {
            model_id: "test".into(),
            display_name: "Test".into(),
            download_size: "1GB".into(),
            status: "pulling manifest".into(),
            completed: 0,
            total: 0,
            done: false,
            error: None,
        };
        assert_eq!(p0.fraction(), 0.0);
        // When total is 0, progress_text should just return the status
        assert_eq!(p0.progress_text(), "pulling manifest");

        // 50%
        let p50 = DownloadProgressData {
            model_id: "test".into(),
            display_name: "Test".into(),
            download_size: "1GB".into(),
            status: "downloading".into(),
            completed: 524_288_000, // 500MB
            total: 1_048_576_000,   // 1000MB
            done: false,
            error: None,
        };
        assert!((p50.fraction() - 0.5).abs() < 0.01);
        let text50 = p50.progress_text();
        assert!(text50.contains("50%"), "Should show 50%, got: {text50}");
        assert!(text50.contains("500MB"), "Should show 500MB done, got: {text50}");
        assert!(text50.contains("1000MB"), "Should show 1000MB total, got: {text50}");

        // 100%
        let p100 = DownloadProgressData {
            model_id: "test".into(),
            display_name: "Test".into(),
            download_size: "2GB".into(),
            status: "verifying".into(),
            completed: 2_097_152_000,
            total: 2_097_152_000,
            done: true,
            error: None,
        };
        assert!((p100.fraction() - 1.0).abs() < 0.01);
        let text100 = p100.progress_text();
        assert!(text100.contains("100%"), "Should show 100%, got: {text100}");
    }

    #[test]
    fn model_table_row_default_not_installed() {
        let row = ModelTableRow {
            model_id: "test:1b".into(),
            display_name: "Test 1B".into(),
            download_size: "500MB".into(),
            ram_needed: "1GB".into(),
            speed: "Fast".into(),
            description: "A test model".into(),
            installed: false,
            recommended: false,
        };
        assert!(!row.installed, "Default row should not be installed");
        assert!(!row.recommended, "Default row should not be recommended");
        assert_eq!(row.model_id, "test:1b");
    }
}
