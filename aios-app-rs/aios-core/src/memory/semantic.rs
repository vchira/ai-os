//! Semantic file index — search files by content meaning using TF-IDF
//! keyword matching.
//!
//! This module provides a simple, dependency-free file indexer that extracts
//! keywords from files and allows searching by content relevance without
//! sending entire files to an LLM.
//!
//! No ML dependencies — uses classic TF-IDF-style keyword extraction:
//! 1. Split text into words
//! 2. Lowercase, remove punctuation
//! 3. Remove stop words
//! 4. Count frequency
//! 5. Return top N unique words

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tracing::debug;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A file index entry containing metadata and extracted keywords.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndex {
    /// Absolute path to the file.
    pub path: String,
    /// Title: filename or first non-empty line.
    pub title: String,
    /// First 200 characters of content for preview.
    pub content_preview: String,
    /// Extracted keywords sorted by frequency.
    pub keywords: Vec<String>,
    /// Last modification time as ISO-8601 string.
    pub last_modified: String,
    /// File size in bytes.
    pub size: u64,
    /// File type category: "text", "code", "config", "document", etc.
    pub file_type: String,
}

/// Simple text search index for the file system.
///
/// Maintains an in-memory index of file metadata and keywords, persisted
/// to a JSON file on disk.
pub struct SemanticIndex {
    entries: Vec<FileIndex>,
    path: PathBuf,
}

/// Search result with a relevance score.
#[derive(Debug, Clone)]
pub struct SearchResult<'a> {
    /// Reference to the matched file index entry.
    pub entry: &'a FileIndex,
    /// Relevance score (higher is more relevant).
    pub score: f64,
}

// ---------------------------------------------------------------------------
// Stop words
// ---------------------------------------------------------------------------

/// Common English stop words to filter out during keyword extraction.
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "shall",
    "should", "may", "might", "must", "can", "could", "am", "to", "of",
    "in", "for", "on", "with", "at", "by", "from", "as", "into", "through",
    "during", "before", "after", "above", "below", "between", "out", "off",
    "over", "under", "again", "further", "then", "once", "here", "there",
    "when", "where", "why", "how", "all", "both", "each", "few", "more",
    "most", "other", "some", "such", "no", "nor", "not", "only", "own",
    "same", "so", "than", "too", "very", "just", "because", "but", "and",
    "or", "if", "while", "about", "up", "down", "this", "that", "these",
    "those", "it", "its", "he", "she", "they", "them", "their", "we",
    "you", "your", "my", "me", "our", "his", "her", "what", "which", "who",
    "whom", "i", "also", "still", "new", "old", "let", "get", "set",
    "use", "used", "using", "true", "false", "null", "none", "return",
    "self", "pub", "fn", "let", "mut", "const", "static", "struct", "enum",
    "impl", "def", "class", "import", "from", "var", "function",
];

/// Directories to skip during indexing.
const SKIP_DIRS: &[&str] = &[
    ".git", ".svn", ".hg", "node_modules", "target", "__pycache__",
    ".cache", ".local", ".config", "venv", ".venv", "env", ".env",
    "dist", "build", ".tox", ".mypy_cache", ".pytest_cache",
    ".cargo", ".rustup",
];

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl SemanticIndex {
    /// Create a new semantic index that persists to the given path.
    pub fn new(path: PathBuf) -> Self {
        Self {
            entries: Vec::new(),
            path,
        }
    }

    /// Create a new index using the default path (~/.aios/memory/file_index.json).
    pub fn default_path() -> Self {
        let path = directories::BaseDirs::new()
            .map(|d| d.home_dir().join(".aios/memory/file_index.json"))
            .unwrap_or_else(|| PathBuf::from("/tmp/aios_file_index.json"));
        Self::new(path)
    }

    /// Index a directory, extracting keywords from each text file.
    ///
    /// Returns the number of files indexed.
    ///
    /// Skips binary files, hidden directories, and well-known build
    /// artifact directories (node_modules, target, .git, etc.).
    pub fn index_directory(&mut self, dir: &Path, recursive: bool) -> Result<usize, String> {
        if !dir.is_dir() {
            return Err(format!("Not a directory: {}", dir.display()));
        }

        let mut count = 0usize;
        self.walk_and_index(dir, recursive, &mut count)?;

        debug!(dir = %dir.display(), count, "indexed directory");
        Ok(count)
    }

    /// Search the index for entries matching the query.
    ///
    /// Scoring:
    /// - Exact match in filename: +10 per match
    /// - Keyword match: +3 per matched keyword
    /// - Content preview match: +1 per match
    /// - File type match (if query mentions a type): +5
    ///
    /// Results are sorted by score (descending) and limited to `limit`.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult<'_>> {
        let query_keywords = keyword_extract(query);

        if query_keywords.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<SearchResult<'_>> = self
            .entries
            .iter()
            .map(|entry| {
                let score = score_entry(entry, &query_keywords, query);
                SearchResult { entry, score }
            })
            .filter(|r| r.score > 0.0)
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    /// Find entries by file type.
    pub fn find_by_type(&self, file_type: &str) -> Vec<&FileIndex> {
        self.entries
            .iter()
            .filter(|e| e.file_type == file_type)
            .collect()
    }

    /// Get the N most recently modified files.
    pub fn recent_files(&self, n: usize) -> Vec<&FileIndex> {
        let mut sorted: Vec<&FileIndex> = self.entries.iter().collect();
        sorted.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
        sorted.truncate(n);
        sorted
    }

    /// Save the index to disk.
    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {e}"))?;
        }

        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| format!("Failed to serialize index: {e}"))?;

        fs::write(&self.path, json).map_err(|e| format!("Failed to write index: {e}"))?;

        debug!(path = %self.path.display(), entries = self.entries.len(), "saved index");
        Ok(())
    }

    /// Load the index from disk.
    pub fn load(&mut self) -> Result<(), String> {
        if !self.path.exists() {
            debug!(path = %self.path.display(), "index file does not exist, starting empty");
            return Ok(());
        }

        let content = fs::read_to_string(&self.path)
            .map_err(|e| format!("Failed to read index: {e}"))?;

        self.entries = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse index: {e}"))?;

        debug!(path = %self.path.display(), entries = self.entries.len(), "loaded index");
        Ok(())
    }

    /// Get the number of indexed entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries from the index.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn walk_and_index(
        &mut self,
        dir: &Path,
        recursive: bool,
        count: &mut usize,
    ) -> Result<(), String> {
        let entries = fs::read_dir(dir).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!("Permission denied: {}", dir.display())
            } else {
                format!("Failed to read directory: {e}")
            }
        })?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden entries.
            if name.starts_with('.') {
                continue;
            }

            let file_type_result = entry.file_type();
            let is_dir = file_type_result.as_ref().map(|ft| ft.is_dir()).unwrap_or(false);

            if is_dir {
                // Skip well-known build artifact directories.
                if SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                if recursive {
                    self.walk_and_index(&entry.path(), true, count)?;
                }
            } else {
                // Try to index this file.
                if let Some(file_entry) = index_file(&entry.path()) {
                    // Remove any existing entry for this path.
                    self.entries.retain(|e| e.path != file_entry.path);
                    self.entries.push(file_entry);
                    *count += 1;
                }
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// File indexing
// ---------------------------------------------------------------------------

/// Index a single file, returning None if the file is binary or unreadable.
fn index_file(path: &Path) -> Option<FileIndex> {
    let meta = path.metadata().ok()?;

    // Skip files larger than 10 MB.
    if meta.len() > 10_000_000 {
        return None;
    }

    // Skip binary files based on extension.
    if is_binary_extension(path) {
        return None;
    }

    let file_type = classify_file_type(path);

    // Read the first portion of the file for keyword extraction.
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut content_buffer = String::new();
    let mut first_line = String::new();
    let mut lines_read = 0usize;

    for line in reader.lines() {
        let line = line.ok()?; // If we can't read it as UTF-8, skip.
        lines_read += 1;

        if first_line.is_empty() && !line.trim().is_empty() {
            first_line = line.trim().to_string();
        }

        content_buffer.push_str(&line);
        content_buffer.push(' ');

        // Read up to ~8KB of content for keyword extraction.
        if content_buffer.len() > 8192 {
            break;
        }
    }

    if lines_read == 0 {
        return None; // Empty file
    }

    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let title = if first_line.len() > 80 {
        first_line[..80].to_string()
    } else if !first_line.is_empty() {
        first_line
    } else {
        filename.clone()
    };

    let content_preview: String = content_buffer.chars().take(200).collect();
    let keywords = keyword_extract(&content_buffer);

    let last_modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    Some(FileIndex {
        path: path.to_string_lossy().to_string(),
        title,
        content_preview,
        keywords,
        last_modified,
        size: meta.len(),
        file_type,
    })
}

/// Classify a file into a type category based on its extension.
fn classify_file_type(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") | Some("py") | Some("js") | Some("ts") | Some("java") | Some("c")
        | Some("cpp") | Some("h") | Some("go") | Some("rb") | Some("php") | Some("swift")
        | Some("kt") | Some("scala") | Some("cs") | Some("sh") | Some("bash")
        | Some("zsh") | Some("lua") | Some("r") | Some("pl") | Some("ex") | Some("exs")
        | Some("hs") | Some("ml") | Some("clj") => "code".to_string(),

        Some("toml") | Some("yaml") | Some("yml") | Some("json") | Some("xml")
        | Some("ini") | Some("cfg") | Some("conf") | Some("env") | Some("properties")
        | Some("lock") => "config".to_string(),

        Some("md") | Some("rst") | Some("txt") | Some("doc") | Some("docx")
        | Some("pdf") | Some("rtf") | Some("tex") | Some("org") => "document".to_string(),

        Some("csv") | Some("tsv") | Some("log") | Some("dat") => "data".to_string(),

        Some("html") | Some("htm") | Some("css") | Some("scss") | Some("less")
        | Some("svg") => "web".to_string(),

        _ => "text".to_string(),
    }
}

/// Check if a file is likely binary based on its extension.
fn is_binary_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("bmp") | Some("ico")
        | Some("webp") | Some("mp3") | Some("mp4") | Some("avi") | Some("mkv") | Some("mov")
        | Some("wav") | Some("flac") | Some("ogg") | Some("zip") | Some("tar") | Some("gz")
        | Some("bz2") | Some("xz") | Some("7z") | Some("rar") | Some("so") | Some("dll")
        | Some("dylib") | Some("exe") | Some("bin") | Some("o") | Some("a") | Some("class")
        | Some("pyc") | Some("pyo") | Some("wasm") | Some("ttf") | Some("otf") | Some("woff")
        | Some("woff2") | Some("eot") | Some("pdf") | Some("db") | Some("sqlite")
        | Some("sqlite3")
    )
}

// ---------------------------------------------------------------------------
// Keyword extraction
// ---------------------------------------------------------------------------

/// Extract keywords from text using simple TF-IDF-style analysis.
///
/// 1. Split text into words
/// 2. Lowercase, remove punctuation
/// 3. Remove stop words and short words
/// 4. Count frequency
/// 5. Return top 20 unique words sorted by frequency
pub fn keyword_extract(text: &str) -> Vec<String> {
    let mut freq: HashMap<String, usize> = HashMap::new();

    for word in text.split(|c: char| c.is_whitespace() || c == '/' || c == '\\') {
        // Strip punctuation from edges.
        let cleaned: String = word
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();

        let lower = cleaned.to_lowercase();

        // Skip short words, stop words, and purely numeric words.
        if lower.len() < 3 {
            continue;
        }
        if STOP_WORDS.contains(&lower.as_str()) {
            continue;
        }
        if lower.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        *freq.entry(lower).or_insert(0) += 1;
    }

    // Sort by frequency (descending), then alphabetically for stability.
    let mut entries: Vec<(String, usize)> = freq.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    entries.into_iter().take(20).map(|(word, _)| word).collect()
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Score a file index entry against query keywords.
fn score_entry(entry: &FileIndex, query_keywords: &[String], raw_query: &str) -> f64 {
    let mut score = 0.0;
    let raw_lower = raw_query.to_lowercase();

    // Extract filename from path.
    let filename = Path::new(&entry.path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    for qw in query_keywords {
        // Exact match in filename: +10
        if filename.contains(qw.as_str()) {
            score += 10.0;
        }

        // Keyword match: +3 per matched keyword
        for kw in &entry.keywords {
            if kw == qw {
                score += 3.0;
            } else if kw.contains(qw.as_str()) || qw.contains(kw.as_str()) {
                score += 1.5; // Partial keyword match
            }
        }

        // Content preview match: +1
        if entry.content_preview.to_lowercase().contains(qw.as_str()) {
            score += 1.0;
        }

        // Title match: +5
        if entry.title.to_lowercase().contains(qw.as_str()) {
            score += 5.0;
        }
    }

    // File type bonus: if the raw query mentions a type keyword.
    let type_keywords: HashMap<&str, &str> = HashMap::from([
        ("code", "code"),
        ("script", "code"),
        ("program", "code"),
        ("config", "config"),
        ("configuration", "config"),
        ("settings", "config"),
        ("document", "document"),
        ("docs", "document"),
        ("notes", "document"),
        ("data", "data"),
        ("csv", "data"),
        ("log", "data"),
        ("web", "web"),
        ("html", "web"),
        ("css", "web"),
    ]);

    for (keyword, file_type) in &type_keywords {
        if raw_lower.contains(keyword) && entry.file_type == *file_type {
            score += 5.0;
        }
    }

    score
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- keyword_extract --

    #[test]
    fn keyword_extract_basic() {
        let text = "The quick brown fox jumps over the lazy dog. \
                    The fox was very quick and very brown.";
        let keywords = keyword_extract(text);

        // "the", "was", "very", "and", "over" are stop words -> filtered out
        assert!(keywords.contains(&"quick".to_string()));
        assert!(keywords.contains(&"brown".to_string()));
        assert!(keywords.contains(&"fox".to_string()));
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"was".to_string()));
    }

    #[test]
    fn keyword_extract_filters_short_words() {
        let text = "a b c de fg hij klmn";
        let keywords = keyword_extract(text);
        // Only words >= 3 chars should be present.
        assert!(!keywords.contains(&"a".to_string()));
        assert!(!keywords.contains(&"de".to_string()));
        assert!(keywords.contains(&"hij".to_string()));
        assert!(keywords.contains(&"klmn".to_string()));
    }

    #[test]
    fn keyword_extract_max_20() {
        let words: Vec<String> = (0..100)
            .map(|i| format!("uniqueword{i:03}"))
            .collect();
        let text = words.join(" ");
        let keywords = keyword_extract(&text);
        assert!(keywords.len() <= 20);
    }

    #[test]
    fn keyword_extract_removes_punctuation() {
        let text = "hello, world! this... is (great)";
        let keywords = keyword_extract(text);
        assert!(keywords.contains(&"hello".to_string()));
        assert!(keywords.contains(&"world".to_string()));
        assert!(keywords.contains(&"great".to_string()));
    }

    #[test]
    fn keyword_extract_preserves_underscores() {
        let text = "my_function_name another_var";
        let keywords = keyword_extract(text);
        assert!(keywords.contains(&"my_function_name".to_string()));
    }

    #[test]
    fn keyword_extract_frequency_order() {
        let text = "apple banana apple cherry apple banana";
        let keywords = keyword_extract(text);
        // "apple" (3x) should come before "banana" (2x) and "cherry" (1x).
        let apple_pos = keywords.iter().position(|w| w == "apple");
        let banana_pos = keywords.iter().position(|w| w == "banana");
        let cherry_pos = keywords.iter().position(|w| w == "cherry");

        assert!(apple_pos.is_some());
        assert!(banana_pos.is_some());
        assert!(cherry_pos.is_some());
        assert!(apple_pos.unwrap() < banana_pos.unwrap());
        assert!(banana_pos.unwrap() < cherry_pos.unwrap());
    }

    // -- classify_file_type --

    #[test]
    fn classify_code_files() {
        assert_eq!(classify_file_type(Path::new("main.rs")), "code");
        assert_eq!(classify_file_type(Path::new("app.py")), "code");
        assert_eq!(classify_file_type(Path::new("index.js")), "code");
    }

    #[test]
    fn classify_config_files() {
        assert_eq!(classify_file_type(Path::new("Cargo.toml")), "config");
        assert_eq!(classify_file_type(Path::new("config.yaml")), "config");
        assert_eq!(classify_file_type(Path::new("data.json")), "config");
    }

    #[test]
    fn classify_document_files() {
        assert_eq!(classify_file_type(Path::new("README.md")), "document");
        assert_eq!(classify_file_type(Path::new("notes.txt")), "document");
    }

    #[test]
    fn classify_data_files() {
        assert_eq!(classify_file_type(Path::new("data.csv")), "data");
        assert_eq!(classify_file_type(Path::new("app.log")), "data");
    }

    // -- is_binary_extension --

    #[test]
    fn binary_extension_detection() {
        assert!(is_binary_extension(Path::new("image.png")));
        assert!(is_binary_extension(Path::new("lib.so")));
        assert!(is_binary_extension(Path::new("font.woff2")));
        assert!(!is_binary_extension(Path::new("main.rs")));
        assert!(!is_binary_extension(Path::new("README.md")));
    }

    // -- SemanticIndex --

    #[test]
    fn index_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("test_index.json");

        // Create test files.
        fs::write(dir.path().join("hello.py"), "# Python hello world\nprint('Hello, world!')\n").unwrap();
        fs::write(dir.path().join("config.toml"), "[settings]\ntimeout = 30\n").unwrap();
        fs::write(dir.path().join("notes.md"), "# Meeting notes\nDiscussed project timeline.\n").unwrap();

        let mut index = SemanticIndex::new(index_path);
        let count = index.index_directory(dir.path(), false).unwrap();
        assert_eq!(count, 3);

        // Search for "python".
        let results = index.search("python hello", 10);
        assert!(!results.is_empty());
        assert!(results[0].entry.path.contains("hello.py"));
    }

    #[test]
    fn index_skips_hidden_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let hidden = dir.path().join(".hidden");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("secret.txt"), "secret data").unwrap();
        fs::write(dir.path().join("visible.txt"), "visible data").unwrap();

        let index_path = dir.path().join("test_index.json");
        let mut index = SemanticIndex::new(index_path);
        let count = index.index_directory(dir.path(), true).unwrap();
        assert_eq!(count, 1); // Only visible.txt
    }

    #[test]
    fn index_skips_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        fs::write(nm.join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("app.js"), "console.log('hi')").unwrap();

        let index_path = dir.path().join("test_index.json");
        let mut index = SemanticIndex::new(index_path);
        let count = index.index_directory(dir.path(), true).unwrap();
        assert_eq!(count, 1); // Only app.js
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("roundtrip_index.json");

        fs::write(dir.path().join("test.txt"), "some test content here").unwrap();

        let mut index = SemanticIndex::new(index_path.clone());
        index.index_directory(dir.path(), false).unwrap();
        index.save().unwrap();

        let mut loaded = SemanticIndex::new(index_path);
        loaded.load().unwrap();
        assert_eq!(loaded.len(), index.len());
    }

    #[test]
    fn find_by_type() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("type_index.json");

        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("notes.md"), "# Notes").unwrap();
        fs::write(dir.path().join("config.toml"), "[cfg]").unwrap();

        let mut index = SemanticIndex::new(index_path);
        index.index_directory(dir.path(), false).unwrap();

        let code_files = index.find_by_type("code");
        assert_eq!(code_files.len(), 1);
        assert!(code_files[0].path.contains("main.rs"));
    }

    #[test]
    fn recent_files() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("recent_index.json");

        fs::write(dir.path().join("a.txt"), "aaa").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(dir.path().join("b.txt"), "bbb").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(dir.path().join("c.txt"), "ccc").unwrap();

        let mut index = SemanticIndex::new(index_path);
        index.index_directory(dir.path(), false).unwrap();

        let recent = index.recent_files(2);
        assert_eq!(recent.len(), 2);
        // Most recent should be first.
        assert!(recent[0].path.contains("c.txt"));
    }

    #[test]
    fn search_scores_filename_higher() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("score_index.json");

        // A file named "rust_guide.md" should rank higher than one that merely mentions "rust".
        fs::write(dir.path().join("rust_guide.md"), "# Guide to Rust\nRust is great").unwrap();
        fs::write(dir.path().join("notes.txt"), "Today I learned some rust programming").unwrap();

        let mut index = SemanticIndex::new(index_path);
        index.index_directory(dir.path(), false).unwrap();

        let results = index.search("rust guide", 10);
        assert!(!results.is_empty());
        // The file named "rust_guide.md" should rank first.
        assert!(results[0].entry.path.contains("rust_guide.md"));
    }

    #[test]
    fn load_nonexistent_file_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("nonexistent.json");
        let mut index = SemanticIndex::new(index_path);
        let result = index.load();
        assert!(result.is_ok());
        assert!(index.is_empty());
    }
}
