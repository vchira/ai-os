//! Speculative pre-fetch engine.
//!
//! When a user sends a message, the [`Prefetcher`] analyses the text and
//! starts fetching data that is likely to be needed by tools — in parallel
//! with the LLM call.  When the LLM eventually requests a tool call, the
//! tool execution can check the prefetch cache first to avoid redundant work.
//!
//! The pre-fetcher uses simple keyword/pattern heuristics (no LLM call) and
//! enforces a 2-second timeout so it never blocks the main LLM request.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tracing::{debug, warn};

/// Timeout for individual pre-fetch operations.
const PREFETCH_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum size of a pre-fetched result (512 KB).
const MAX_PREFETCH_SIZE: usize = 512 * 1024;

/// Speculative data pre-fetcher.
///
/// Analyses user messages and starts background tasks to fetch data that
/// tools are likely to need.  Results are cached in memory and can be
/// retrieved by tool implementations via [`get_cached`](Self::get_cached).
pub struct Prefetcher {
    cache: Arc<Mutex<HashMap<String, String>>>,
}

impl Prefetcher {
    /// Create a new, empty pre-fetcher.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Analyse a user message and start pre-fetching likely needed data.
    ///
    /// Runs individual prefetch operations as background tasks with a
    /// 2-second timeout.  Results are stored in the cache for later
    /// retrieval by tool execution.
    ///
    /// This method returns quickly — the actual fetching happens in spawned
    /// tasks.
    pub async fn prefetch(&self, user_message: &str) {
        let msg_lower = user_message.to_lowercase();

        // Heuristic 1: file paths — pre-read the file.
        if let Some(path) = extract_file_path(user_message) {
            let cache = self.cache.clone();
            tokio::spawn(async move {
                let result = tokio::time::timeout(
                    PREFETCH_TIMEOUT,
                    tokio::fs::read_to_string(&path),
                )
                .await;

                match result {
                    Ok(Ok(content)) => {
                        let content = if content.len() > MAX_PREFETCH_SIZE {
                            content[..MAX_PREFETCH_SIZE].to_string()
                        } else {
                            content
                        };
                        debug!(path = %path, bytes = content.len(), "prefetched file");
                        cache.lock().unwrap().insert(
                            format!("file:{path}"),
                            content,
                        );
                    }
                    Ok(Err(e)) => {
                        debug!(path = %path, error = %e, "prefetch file read failed");
                    }
                    Err(_) => {
                        warn!(path = %path, "prefetch file read timed out");
                    }
                }
            });
        }

        // Heuristic 2: system info / processes.
        if msg_lower.contains("system info")
            || msg_lower.contains("processes")
            || msg_lower.contains("cpu")
            || msg_lower.contains("memory usage")
        {
            let cache = self.cache.clone();
            tokio::spawn(async move {
                let result = tokio::time::timeout(PREFETCH_TIMEOUT, async {
                    let output = tokio::process::Command::new("uname")
                        .arg("-a")
                        .output()
                        .await;
                    match output {
                        Ok(o) if o.status.success() => {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        }
                        _ => None,
                    }
                })
                .await;

                if let Ok(Some(info)) = result {
                    debug!("prefetched system info");
                    cache
                        .lock()
                        .unwrap()
                        .insert("system:uname".to_string(), info);
                }
            });
        }

        // Heuristic 3: URL in message — start fetching.
        if let Some(url) = extract_url(user_message) {
            let cache = self.cache.clone();
            tokio::spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(PREFETCH_TIMEOUT)
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());

                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.text().await {
                            Ok(text) => {
                                let text = if text.len() > MAX_PREFETCH_SIZE {
                                    text[..MAX_PREFETCH_SIZE].to_string()
                                } else {
                                    text
                                };
                                debug!(url = %url, bytes = text.len(), "prefetched URL");
                                cache
                                    .lock()
                                    .unwrap()
                                    .insert(format!("url:{url}"), text);
                            }
                            Err(e) => {
                                debug!(url = %url, error = %e, "prefetch URL body read failed");
                            }
                        }
                    }
                    Ok(resp) => {
                        debug!(url = %url, status = %resp.status(), "prefetch URL non-success");
                    }
                    Err(e) => {
                        debug!(url = %url, error = %e, "prefetch URL failed");
                    }
                }
            });
        }

        // Heuristic 4: "last files" / "recent files" / "last logs".
        if (msg_lower.contains("last") || msg_lower.contains("recent"))
            && (msg_lower.contains("file") || msg_lower.contains("log"))
        {
            let cache = self.cache.clone();
            tokio::spawn(async move {
                let result = tokio::time::timeout(PREFETCH_TIMEOUT, async {
                    let output = tokio::process::Command::new("ls")
                        .args(["-lt", "--time=modification"])
                        .arg(
                            std::env::var("HOME")
                                .unwrap_or_else(|_| "/tmp".to_string()),
                        )
                        .output()
                        .await;
                    match output {
                        Ok(o) if o.status.success() => {
                            // Take first 50 lines.
                            let text = String::from_utf8_lossy(&o.stdout);
                            let lines: Vec<&str> = text.lines().take(50).collect();
                            Some(lines.join("\n"))
                        }
                        _ => None,
                    }
                })
                .await;

                if let Ok(Some(listing)) = result {
                    debug!("prefetched recent files listing");
                    cache
                        .lock()
                        .unwrap()
                        .insert("files:recent".to_string(), listing);
                }
            });
        }
    }

    /// Get a pre-fetched result from the cache, if available.
    ///
    /// Keys follow the pattern `"type:identifier"`:
    /// - `"file:/path/to/file"` — pre-read file content
    /// - `"system:uname"` — pre-fetched system info
    /// - `"url:https://..."` — pre-fetched URL content
    /// - `"files:recent"` — recent file listing
    pub fn get_cached(&self, key: &str) -> Option<String> {
        self.cache.lock().unwrap().get(key).cloned()
    }

    /// Clear the entire pre-fetch cache.
    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
    }

    /// Return the number of cached items.
    pub fn cache_size(&self) -> usize {
        self.cache.lock().unwrap().len()
    }
}

impl Default for Prefetcher {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Pattern extraction helpers
// ---------------------------------------------------------------------------

/// Extract a likely file path from a user message.
///
/// Looks for tokens that start with `/` or `~/` and contain path-like
/// characters.
fn extract_file_path(message: &str) -> Option<String> {
    for word in message.split_whitespace() {
        // Strip common punctuation from edges.
        let clean = word
            .trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == '.' || c == '?' || c == '!');
        if (clean.starts_with('/') || clean.starts_with("~/"))
            && clean.len() > 2
            && clean.chars().all(|c| {
                c.is_alphanumeric()
                    || c == '/'
                    || c == '.'
                    || c == '_'
                    || c == '-'
                    || c == '~'
            })
        {
            // Expand ~ to $HOME.
            let expanded = if let Some(rest) = clean.strip_prefix("~/") {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                format!("{home}/{rest}")
            } else {
                clean.to_string()
            };
            return Some(expanded);
        }
    }
    None
}

/// Extract a URL from a user message.
///
/// Looks for tokens starting with `http://` or `https://`.
fn extract_url(message: &str) -> Option<String> {
    for word in message.split_whitespace() {
        let clean = word.trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == '>' || c == '<');
        if clean.starts_with("http://") || clean.starts_with("https://") {
            return Some(clean.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_file_path_absolute() {
        assert_eq!(
            extract_file_path("Please read /etc/hostname"),
            Some("/etc/hostname".to_string())
        );
    }

    #[test]
    fn extract_file_path_home() {
        let result = extract_file_path("Check ~/Documents/notes.txt");
        assert!(result.is_some());
        let path = result.unwrap();
        assert!(path.ends_with("/Documents/notes.txt"));
    }

    #[test]
    fn extract_file_path_none() {
        assert_eq!(extract_file_path("Hello world, how are you?"), None);
    }

    #[test]
    fn extract_file_path_quoted() {
        assert_eq!(
            extract_file_path(r#"Read the file "/etc/os-release" please"#),
            Some("/etc/os-release".to_string())
        );
    }

    #[test]
    fn extract_url_https() {
        assert_eq!(
            extract_url("Fetch https://example.com/api"),
            Some("https://example.com/api".to_string())
        );
    }

    #[test]
    fn extract_url_http() {
        assert_eq!(
            extract_url("Get http://localhost:8080/health"),
            Some("http://localhost:8080/health".to_string())
        );
    }

    #[test]
    fn extract_url_none() {
        assert_eq!(extract_url("What is the weather today?"), None);
    }

    #[test]
    fn prefetcher_cache_operations() {
        let pf = Prefetcher::new();
        assert_eq!(pf.cache_size(), 0);

        // Manually insert into cache.
        pf.cache
            .lock()
            .unwrap()
            .insert("test:key".to_string(), "value".to_string());

        assert_eq!(pf.get_cached("test:key"), Some("value".to_string()));
        assert_eq!(pf.get_cached("missing"), None);
        assert_eq!(pf.cache_size(), 1);

        pf.clear();
        assert_eq!(pf.cache_size(), 0);
    }

    #[tokio::test]
    async fn prefetch_no_crash_on_empty_message() {
        let pf = Prefetcher::new();
        pf.prefetch("").await;
        // Should not crash.
    }

    #[tokio::test]
    async fn prefetch_file_path_spawns_task() {
        let pf = Prefetcher::new();
        // Use a file that exists on all Linux systems.
        pf.prefetch("Read /etc/hostname for me").await;
        // Give the spawned task time to complete.
        tokio::time::sleep(Duration::from_millis(500)).await;
        // The file should be cached if it exists.
        let result = pf.get_cached("file:/etc/hostname");
        // On CI, /etc/hostname should exist.
        if std::path::Path::new("/etc/hostname").exists() {
            assert!(result.is_some(), "expected /etc/hostname to be cached");
        }
    }

    #[tokio::test]
    async fn prefetch_nonexistent_file() {
        let pf = Prefetcher::new();
        pf.prefetch("Read /nonexistent/path/12345.txt").await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Should not be cached (file doesn't exist).
        assert!(pf.get_cached("file:/nonexistent/path/12345.txt").is_none());
    }

    #[tokio::test]
    async fn prefetch_system_info() {
        let pf = Prefetcher::new();
        pf.prefetch("Show me the system info").await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let result = pf.get_cached("system:uname");
        // uname should work on Linux.
        assert!(result.is_some(), "expected system info to be cached");
    }

    // -----------------------------------------------------------------------
    // detect_prefetch_hints for file paths
    // -----------------------------------------------------------------------

    #[test]
    fn detect_file_hint_in_read_log_message() {
        // "read my log file /var/log/syslog" should detect a file path.
        let path = extract_file_path("read my log file /var/log/syslog");
        assert_eq!(path, Some("/var/log/syslog".to_string()));
    }

    #[test]
    fn detect_file_hint_absolute_path() {
        let path = extract_file_path("check /etc/passwd for users");
        assert_eq!(path, Some("/etc/passwd".to_string()));
    }

    #[test]
    fn detect_file_hint_home_relative() {
        let path = extract_file_path("edit ~/notes.txt");
        assert!(path.is_some());
        assert!(path.unwrap().ends_with("/notes.txt"));
    }

    // -----------------------------------------------------------------------
    // detect_prefetch_hints for URLs
    // -----------------------------------------------------------------------

    #[test]
    fn detect_url_hint_in_search_message() {
        let url = extract_url("fetch https://api.example.com/data");
        assert_eq!(url, Some("https://api.example.com/data".to_string()));
    }

    #[test]
    fn detect_url_hint_http() {
        let url = extract_url("open http://localhost:3000");
        assert_eq!(url, Some("http://localhost:3000".to_string()));
    }

    // -----------------------------------------------------------------------
    // detect_prefetch_hints for plain question — no hints
    // -----------------------------------------------------------------------

    #[test]
    fn no_file_hint_in_plain_question() {
        assert_eq!(extract_file_path("What is the weather today?"), None);
    }

    #[test]
    fn no_url_hint_in_plain_question() {
        assert_eq!(extract_url("What is the capital of France?"), None);
    }

    #[test]
    fn no_file_hint_in_empty_string() {
        assert_eq!(extract_file_path(""), None);
    }

    #[test]
    fn no_url_hint_in_empty_string() {
        assert_eq!(extract_url(""), None);
    }

    // -----------------------------------------------------------------------
    // Prefetcher caches values and returns them
    // -----------------------------------------------------------------------

    #[test]
    fn prefetcher_get_cached_returns_inserted_value() {
        let pf = Prefetcher::new();
        pf.cache.lock().unwrap().insert(
            "file:/etc/hostname".to_string(),
            "my-machine".to_string(),
        );
        assert_eq!(
            pf.get_cached("file:/etc/hostname"),
            Some("my-machine".to_string()),
        );
    }

    #[test]
    fn prefetcher_get_cached_returns_none_for_missing_key() {
        let pf = Prefetcher::new();
        assert_eq!(pf.get_cached("file:/nonexistent"), None);
    }

    // -----------------------------------------------------------------------
    // Prefetcher clear and cache_size
    // -----------------------------------------------------------------------

    #[test]
    fn prefetcher_clear_removes_all_entries() {
        let pf = Prefetcher::new();
        pf.cache.lock().unwrap().insert("a".to_string(), "1".to_string());
        pf.cache.lock().unwrap().insert("b".to_string(), "2".to_string());
        assert_eq!(pf.cache_size(), 2);
        pf.clear();
        assert_eq!(pf.cache_size(), 0);
    }

    #[test]
    fn prefetcher_default_creates_empty() {
        let pf = Prefetcher::default();
        assert_eq!(pf.cache_size(), 0);
    }

    // -----------------------------------------------------------------------
    // Prefetch triggers for system info keywords
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn prefetch_cpu_keyword_triggers_system_info() {
        let pf = Prefetcher::new();
        pf.prefetch("How is my cpu doing?").await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let result = pf.get_cached("system:uname");
        assert!(result.is_some(), "cpu keyword should trigger system info prefetch");
    }

    #[tokio::test]
    async fn prefetch_memory_usage_keyword_triggers_system_info() {
        let pf = Prefetcher::new();
        pf.prefetch("Show memory usage").await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let result = pf.get_cached("system:uname");
        assert!(result.is_some(), "memory usage keyword should trigger system info prefetch");
    }

    #[tokio::test]
    async fn prefetch_plain_question_no_system_info() {
        let pf = Prefetcher::new();
        pf.prefetch("What is the capital of France?").await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        // No system info keywords — cache should be empty.
        assert!(pf.get_cached("system:uname").is_none());
    }
}
