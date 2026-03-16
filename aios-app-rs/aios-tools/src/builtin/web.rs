//! Web tool — fetch URLs, search the web, and download files.
//!
//! Uses `reqwest` (blocking client) for HTTP requests and the DuckDuckGo
//! Instant Answer API for web searches.
//!
//! Ported from `aios-app/aios/tools/builtin/web.py`.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use aios_core::types::ToolResult;
use tracing::debug;

use crate::tool::Tool;

/// Maximum response body to keep in memory (~2 MB).
const MAX_FETCH_BYTES: usize = 2_000_000;

/// Default timeout for HTTP requests.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Download timeout (2 minutes).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// User-Agent header to identify AiOS requests.
const USER_AGENT: &str = "AiOS/2.0 (WebTool; Rust)";

/// Default download directory.
fn download_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().join("Downloads"))
        .unwrap_or_else(|| PathBuf::from("Downloads"))
}

/// Web utilities: fetch a URL (returns text content), search the web,
/// or download a file to a local path.
pub struct WebTool {
    client: reqwest::blocking::Client,
}

impl WebTool {
    /// Create a new `WebTool` with a pre-configured HTTP client.
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self { client }
    }
}

impl Default for WebTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for WebTool {
    fn name(&self) -> &str {
        "web"
    }

    fn description(&self) -> &str {
        "Web utilities: fetch a URL (returns text content), search the web, \
         or download a file to a local path."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["fetch_url", "search_web", "download_file"],
                    "description": "Web action to perform."
                },
                "url": {
                    "type": "string",
                    "description": "URL to fetch or download."
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search_web)."
                },
                "destination": {
                    "type": "string",
                    "description": "Local file path for download_file (defaults to ~/Downloads/<filename>)."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let destination = args.get("destination").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "fetch_url" => self.fetch_url(url),
            "search_web" => self.search_web(query),
            "download_file" => self.download_file(url, destination),
            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. Use: fetch_url, search_web, download_file."
            )),
        }
    }
}

impl WebTool {
    /// Fetch a URL and return its text content (up to [`MAX_FETCH_BYTES`]).
    fn fetch_url(&self, url: &str) -> ToolResult {
        if url.is_empty() {
            return ToolResult::fail("'url' is required for fetch_url.");
        }

        let response = match self.client.get(url).send() {
            Ok(r) => r,
            Err(e) => {
                if e.is_timeout() {
                    return ToolResult::fail(format!("Request timed out: {e}"));
                }
                return ToolResult::fail(format!("Could not reach URL: {e}"));
            }
        };

        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if !status.is_success() {
            return ToolResult::fail(format!("HTTP error {}: {}", status.as_u16(), status));
        }

        // Read up to MAX_FETCH_BYTES.
        let bytes = match response.bytes() {
            Ok(b) => {
                if b.len() > MAX_FETCH_BYTES {
                    b.slice(..MAX_FETCH_BYTES)
                } else {
                    b
                }
            }
            Err(e) => return ToolResult::fail(format!("Failed to read response body: {e}")),
        };

        let text = String::from_utf8_lossy(&bytes).to_string();

        ToolResult::ok_with_data(
            text,
            serde_json::json!({
                "url": url,
                "status": status.as_u16(),
                "content_type": content_type,
                "length": bytes.len(),
            }),
        )
    }

    /// Search the web using the DuckDuckGo Instant Answer API.
    fn search_web(&self, query: &str) -> ToolResult {
        if query.is_empty() {
            return ToolResult::fail("'query' is required for search_web.");
        }

        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
            urlencoding(query),
        );

        debug!(query, "searching DuckDuckGo");

        let response = match self.client.get(&url).send() {
            Ok(r) => r,
            Err(e) => {
                if e.is_timeout() {
                    return ToolResult::fail(format!("Search API timed out: {e}"));
                }
                return ToolResult::fail(format!("Could not reach search API: {e}"));
            }
        };

        if !response.status().is_success() {
            return ToolResult::fail(format!(
                "Search API error {}: {}",
                response.status().as_u16(),
                response.status()
            ));
        }

        let data: serde_json::Value = match response.json() {
            Ok(v) => v,
            Err(e) => return ToolResult::fail(format!("Failed to parse search response: {e}")),
        };

        let mut results: Vec<serde_json::Value> = Vec::new();

        // Abstract (main answer).
        if let Some(abstract_text) = data.get("Abstract").and_then(|v| v.as_str()) {
            if !abstract_text.is_empty() {
                results.push(serde_json::json!({
                    "title": data.get("Heading").and_then(|v| v.as_str()).unwrap_or(""),
                    "snippet": abstract_text,
                    "url": data.get("AbstractURL").and_then(|v| v.as_str()).unwrap_or(""),
                }));
            }
        }

        // Related topics.
        if let Some(topics) = data.get("RelatedTopics").and_then(|v| v.as_array()) {
            for topic in topics {
                if let Some(text) = topic.get("Text").and_then(|v| v.as_str()) {
                    let first_url = topic
                        .get("FirstURL")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let title = first_url
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .replace('_', " ");
                    results.push(serde_json::json!({
                        "title": title,
                        "snippet": text,
                        "url": first_url,
                    }));
                }

                // Sub-topics (grouped).
                if let Some(sub_topics) = topic.get("Topics").and_then(|v| v.as_array()) {
                    for sub in sub_topics {
                        if let Some(text) = sub.get("Text").and_then(|v| v.as_str()) {
                            let first_url = sub
                                .get("FirstURL")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let title = first_url
                                .rsplit('/')
                                .next()
                                .unwrap_or("")
                                .replace('_', " ");
                            results.push(serde_json::json!({
                                "title": title,
                                "snippet": text,
                                "url": first_url,
                            }));
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            return ToolResult::ok_with_data(
                "No results found.".to_string(),
                serde_json::json!({ "query": query, "results": [] }),
            );
        }

        // Take at most 10 results.
        let results: Vec<serde_json::Value> = results.into_iter().take(10).collect();

        let mut lines: Vec<String> = Vec::new();
        for (i, r) in results.iter().enumerate() {
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let snippet = r.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");

            lines.push(format!("{}. {}", i + 1, title));
            // Truncate snippet to 200 chars.
            let snippet_truncated: String = snippet.chars().take(200).collect();
            lines.push(format!("   {snippet_truncated}"));
            if !url.is_empty() {
                lines.push(format!("   {url}"));
            }
            lines.push(String::new());
        }

        let output = lines.join("\n").trim().to_string();

        ToolResult::ok_with_data(
            output,
            serde_json::json!({
                "query": query,
                "results": results,
            }),
        )
    }

    /// Download a file from a URL to a local path.
    fn download_file(&self, url: &str, destination: &str) -> ToolResult {
        if url.is_empty() {
            return ToolResult::fail("'url' is required for download_file.");
        }

        // Determine destination path.
        let mut dest_path = if !destination.is_empty() {
            let expanded = if destination.starts_with('~') {
                let home = directories::BaseDirs::new()
                    .map(|d| d.home_dir().to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("/tmp"));
                if destination == "~" {
                    home
                } else if let Some(rest) = destination.strip_prefix("~/") {
                    home.join(rest)
                } else {
                    PathBuf::from(destination)
                }
            } else {
                PathBuf::from(destination)
            };
            match expanded.canonicalize() {
                Ok(p) => p,
                Err(_) => expanded,
            }
        } else {
            // Extract filename from URL.
            let filename = url
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("download")
                .split('?')
                .next()
                .unwrap_or("download");
            let filename = if filename.is_empty() {
                "download"
            } else {
                filename
            };

            let dl_dir = download_dir();
            fs::create_dir_all(&dl_dir).ok();
            dl_dir.join(filename)
        };

        // Do not overwrite — append a numeric suffix.
        if dest_path.exists() {
            let stem = dest_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let ext = dest_path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let parent = dest_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            let mut counter = 1u32;
            loop {
                let candidate = parent.join(format!("{stem}_{counter}{ext}"));
                if !candidate.exists() {
                    dest_path = candidate;
                    break;
                }
                counter += 1;
            }
        }

        // Build a client with a longer timeout for downloads.
        let dl_client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(DOWNLOAD_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        let response = match dl_client.get(url).send() {
            Ok(r) => r,
            Err(e) => {
                if e.is_timeout() {
                    return ToolResult::fail(format!("Download timed out: {e}"));
                }
                return ToolResult::fail(format!("Could not reach URL: {e}"));
            }
        };

        if !response.status().is_success() {
            return ToolResult::fail(format!(
                "Download HTTP error {}: {}",
                response.status().as_u16(),
                response.status()
            ));
        }

        // Ensure parent directory exists.
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let bytes = match response.bytes() {
            Ok(b) => b,
            Err(e) => return ToolResult::fail(format!("Failed to read download body: {e}")),
        };

        let total = bytes.len();
        if let Err(e) = fs::write(&dest_path, &bytes) {
            return ToolResult::fail(format!("Failed to write downloaded file: {e}"));
        }

        debug!(url, path = %dest_path.display(), bytes = total, "downloaded file");

        ToolResult::ok_with_data(
            format!("Downloaded {total} bytes to {}", dest_path.display()),
            serde_json::json!({
                "url": url,
                "path": dest_path.display().to_string(),
                "bytes_downloaded": total,
            }),
        )
    }
}

/// Simple percent-encoding for URL query parameters.
fn urlencoding(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
        }
    }
    encoded
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_basic() {
        assert_eq!(urlencoding("hello world"), "hello+world");
        assert_eq!(urlencoding("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn fetch_url_requires_url() {
        let tool = WebTool::new();
        let r = tool.execute(serde_json::json!({ "action": "fetch_url" }));
        assert!(!r.success);
    }

    #[test]
    fn search_web_requires_query() {
        let tool = WebTool::new();
        let r = tool.execute(serde_json::json!({ "action": "search_web" }));
        assert!(!r.success);
    }

    #[test]
    fn download_file_requires_url() {
        let tool = WebTool::new();
        let r = tool.execute(serde_json::json!({ "action": "download_file" }));
        assert!(!r.success);
    }

    #[test]
    fn unknown_action() {
        let tool = WebTool::new();
        let r = tool.execute(serde_json::json!({ "action": "hack" }));
        assert!(!r.success);
    }
}
