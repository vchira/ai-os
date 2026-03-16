//! Semantic file search tool — search files by content, meaning, or
//! description using the [`SemanticIndex`].
//!
//! This tool provides natural-language file search: instead of needing
//! an exact path or filename pattern, users can describe what they're
//! looking for (e.g., "python scripts about data processing").

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aios_core::memory::semantic::SemanticIndex;
use aios_core::types::ToolResult;
use tracing::debug;

use crate::tool::Tool;

/// Semantic content search tool.
///
/// Wraps a shared [`SemanticIndex`] and provides natural-language file search.
/// If the index is empty or stale, it triggers a re-index first.
pub struct FindContentTool {
    index: Arc<Mutex<SemanticIndex>>,
}

impl FindContentTool {
    /// Create a new `FindContentTool` with a shared semantic index.
    pub fn new(index: Arc<Mutex<SemanticIndex>>) -> Self {
        Self { index }
    }

    /// Create a new `FindContentTool` using the default index path.
    pub fn with_default_index() -> Self {
        let index = SemanticIndex::default_path();
        Self {
            index: Arc::new(Mutex::new(index)),
        }
    }
}

impl Tool for FindContentTool {
    fn name(&self) -> &str {
        "find_content"
    }

    fn description(&self) -> &str {
        "Search files by content, meaning, or description. Don't need to know \
         the exact path — describe what you're looking for."
    }

    fn category(&self) -> &str {
        "filesystem"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to search for (e.g., 'meeting notes from last week', 'python scripts about data processing')."
                },
                "file_type": {
                    "type": "string",
                    "description": "Filter by type: text, code, config, document, data, web (optional)."
                },
                "directory": {
                    "type": "string",
                    "description": "Directory to search in (default: home directory)."
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of results (default 10)."
                }
            },
            "required": ["query"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) if !q.is_empty() => q,
            _ => return ToolResult::fail("'query' is required."),
        };

        let file_type = args.get("file_type").and_then(|v| v.as_str()).unwrap_or("");
        let directory = args.get("directory").and_then(|v| v.as_str()).unwrap_or("");
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        // Determine the directory to search.
        let search_dir = if directory.is_empty() {
            home_dir()
        } else if directory.starts_with('~') {
            if directory == "~" {
                home_dir()
            } else if let Some(rest) = directory.strip_prefix("~/") {
                home_dir().join(rest)
            } else {
                PathBuf::from(directory)
            }
        } else {
            PathBuf::from(directory)
        };

        // Lock the index.
        let mut index = match self.index.lock() {
            Ok(idx) => idx,
            Err(e) => return ToolResult::fail(format!("Failed to acquire index lock: {e}")),
        };

        // Try loading the persisted index first.
        if index.is_empty() {
            if let Err(e) = index.load() {
                debug!(error = %e, "failed to load index, will re-index");
            }
        }

        // Re-index if the index is still empty.
        if index.is_empty() {
            debug!(dir = %search_dir.display(), "index empty, re-indexing");
            match index.index_directory(&search_dir, true) {
                Ok(count) => {
                    debug!(count, "indexed files");
                    // Save for future use.
                    if let Err(e) = index.save() {
                        debug!(error = %e, "failed to save index");
                    }
                }
                Err(e) => return ToolResult::fail(format!("Failed to index directory: {e}")),
            }
        }

        // Search the index.
        let results = index.search(query, limit);

        // Filter by file type if specified.
        let results: Vec<_> = if file_type.is_empty() {
            results
        } else {
            results
                .into_iter()
                .filter(|r| r.entry.file_type == file_type)
                .collect()
        };

        if results.is_empty() {
            return ToolResult::ok_with_data(
                "No matching files found.".to_string(),
                serde_json::json!({
                    "query": query,
                    "results": [],
                    "total_indexed": index.len(),
                }),
            );
        }

        // Format results.
        let mut lines: Vec<String> = Vec::new();
        let mut result_data: Vec<serde_json::Value> = Vec::new();

        for (i, r) in results.iter().enumerate() {
            let preview: String = r.entry.content_preview.chars().take(100).collect();
            lines.push(format!(
                "{}. {} (score: {:.1})",
                i + 1,
                r.entry.path,
                r.score
            ));
            lines.push(format!("   Type: {} | Size: {} bytes", r.entry.file_type, r.entry.size));
            lines.push(format!("   Keywords: {}", r.entry.keywords.iter().take(5).cloned().collect::<Vec<_>>().join(", ")));
            if !preview.is_empty() {
                lines.push(format!("   Preview: {preview}..."));
            }
            lines.push(String::new());

            result_data.push(serde_json::json!({
                "path": r.entry.path,
                "title": r.entry.title,
                "file_type": r.entry.file_type,
                "score": r.score,
                "size": r.entry.size,
                "keywords": r.entry.keywords,
                "last_modified": r.entry.last_modified,
            }));
        }

        let output = lines.join("\n").trim().to_string();

        ToolResult::ok_with_data(
            output,
            serde_json::json!({
                "query": query,
                "results": result_data,
                "total_indexed": index.len(),
            }),
        )
    }
}

fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_query() {
        let tool = FindContentTool::with_default_index();
        let r = tool.execute(serde_json::json!({}));
        assert!(!r.success);
    }

    #[test]
    fn rejects_empty_query() {
        let tool = FindContentTool::with_default_index();
        let r = tool.execute(serde_json::json!({ "query": "" }));
        assert!(!r.success);
    }

    #[test]
    fn search_with_test_files() {
        let dir = tempfile::tempdir().unwrap();

        // Create test files.
        std::fs::write(
            dir.path().join("recipe.txt"),
            "Chocolate cake recipe\nMix flour sugar and eggs\nBake for 30 minutes",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("budget.csv"),
            "item,cost\ngroceries,200\nrent,1500",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("main.py"),
            "# Data processing script\nimport pandas as pd\ndf = pd.read_csv('data.csv')",
        )
        .unwrap();

        // Build an index directly.
        let index_path = dir.path().join("test_index.json");
        let mut index = SemanticIndex::new(index_path);
        index.index_directory(dir.path(), false).unwrap();

        let tool = FindContentTool::new(Arc::new(Mutex::new(index)));

        // Search for recipe.
        let r = tool.execute(serde_json::json!({
            "query": "chocolate cake recipe"
        }));
        assert!(r.success);
        assert!(r.output.contains("recipe.txt"));

        // Search for data processing.
        let r = tool.execute(serde_json::json!({
            "query": "data processing python"
        }));
        assert!(r.success);
        assert!(r.output.contains("main.py"));
    }

    #[test]
    fn search_with_file_type_filter() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("app.py"), "# Python application\nprint('hello')").unwrap();
        std::fs::write(dir.path().join("notes.md"), "# Python notes\nLearning python").unwrap();

        let index_path = dir.path().join("type_filter_index.json");
        let mut index = SemanticIndex::new(index_path);
        index.index_directory(dir.path(), false).unwrap();

        let tool = FindContentTool::new(Arc::new(Mutex::new(index)));

        let r = tool.execute(serde_json::json!({
            "query": "python",
            "file_type": "code"
        }));
        assert!(r.success);
        assert!(r.output.contains("app.py"));
        // notes.md is a "document", not "code", so should be filtered out.
        assert!(!r.output.contains("notes.md"));
    }
}
