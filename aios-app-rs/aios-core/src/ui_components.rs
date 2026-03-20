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
}
