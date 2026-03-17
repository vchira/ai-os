//! Files tool — read, write, list, search, and inspect files.
//!
//! All operations are scoped to the user's home directory to prevent path
//! traversal attacks.
//!
//! Ported from `aios-app/aios/tools/builtin/files.py`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use aios_core::types::ToolResult;

use crate::tool::Tool;

/// Maximum bytes to read from a file in a single request (~1 MB).
const MAX_READ_BYTES: u64 = 1_000_000;

/// Maximum number of search results.
const MAX_SEARCH_RESULTS: usize = 200;

/// Resolve a path string and verify it lives under `root`.
///
/// Returns `Ok(resolved)` or `Err(error_message)`.
fn resolve_safe(path_str: &str, root: &Path) -> Result<PathBuf, String> {
    if path_str.is_empty() {
        return Err("path must not be empty".to_string());
    }

    // Expand ~ to home directory.
    let expanded = if path_str.starts_with('~') {
        let home = home_dir();
        if path_str == "~" {
            home
        } else if let Some(rest) = path_str.strip_prefix("~/") {
            home.join(rest)
        } else {
            PathBuf::from(path_str)
        }
    } else {
        PathBuf::from(path_str)
    };

    let resolved = match expanded.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // If the file doesn't exist yet (e.g. write_file), resolve the
            // parent and then append the file name.
            if let Some(parent) = expanded.parent() {
                let parent_resolved = parent
                    .canonicalize()
                    .or_else(|_| {
                        // Parent might not exist either; try to resolve as much as we can.
                        Ok::<PathBuf, std::io::Error>(expanded.clone())
                    })
                    .unwrap();
                if let Some(file_name) = expanded.file_name() {
                    parent_resolved.join(file_name)
                } else {
                    parent_resolved
                }
            } else {
                expanded
            }
        }
    };

    let root_resolved = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf());

    if !resolved.starts_with(&root_resolved) {
        return Err(format!(
            "Access denied: {} is outside the allowed root ({}).",
            resolved.display(),
            root_resolved.display(),
        ));
    }

    Ok(resolved)
}

/// Get the user's home directory (shared implementation).
fn home_dir() -> PathBuf {
    crate::home_dir()
}

/// File-system operations scoped to the user's home directory.
pub struct FilesTool;

impl Tool for FilesTool {
    fn name(&self) -> &str {
        "files"
    }

    fn description(&self) -> &str {
        "File operations: read a file, write a file, list a directory, \
         search for files by name pattern, or get file info."
    }

    fn category(&self) -> &str {
        "filesystem"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read_file", "write_file", "list_directory", "search_files", "file_info"],
                    "description": "File operation to perform."
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path (relative to home, or absolute)."
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (for write_file)."
                },
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern for search_files (e.g. '*.py')."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");

        let root = home_dir();

        match action {
            "read_file" => read_file(path, &root),
            "write_file" => write_file(path, content, &root),
            "list_directory" => {
                let dir = if path.is_empty() { "~" } else { path };
                list_directory(dir, &root)
            }
            "search_files" => {
                let dir = if path.is_empty() { "~" } else { path };
                search_files(dir, pattern, &root)
            }
            "file_info" => file_info(path, &root),
            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. \
                 Use: read_file, write_file, list_directory, search_files, file_info."
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Action implementations
// ---------------------------------------------------------------------------

/// Read a file's contents (up to [`MAX_READ_BYTES`]).
fn read_file(path_str: &str, root: &Path) -> ToolResult {
    if path_str.is_empty() {
        return ToolResult::fail("'path' is required for read_file.");
    }

    let resolved = match resolve_safe(path_str, root) {
        Ok(p) => p,
        Err(e) => return ToolResult::fail(e),
    };

    if !resolved.is_file() {
        return ToolResult::fail(format!(
            "Not a file or does not exist: {}",
            resolved.display()
        ));
    }

    let meta = match resolved.metadata() {
        Ok(m) => m,
        Err(e) => return ToolResult::fail(format!("Failed to read file metadata: {e}")),
    };

    if meta.len() > MAX_READ_BYTES {
        return ToolResult::fail(format!(
            "File too large ({} bytes). Max is {} bytes.",
            meta.len(),
            MAX_READ_BYTES
        ));
    }

    match fs::read_to_string(&resolved) {
        Ok(content) => ToolResult::ok_with_data(
            content,
            serde_json::json!({
                "path": resolved.display().to_string(),
                "size": meta.len(),
            }),
        ),
        Err(e) => ToolResult::fail(format!("Failed to read file: {e}")),
    }
}

/// Write content to a file, creating parent directories as needed.
fn write_file(path_str: &str, content: &str, root: &Path) -> ToolResult {
    if path_str.is_empty() {
        return ToolResult::fail("'path' is required for write_file.");
    }

    let resolved = match resolve_safe(path_str, root) {
        Ok(p) => p,
        Err(e) => return ToolResult::fail(e),
    };

    if let Some(parent) = resolved.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return ToolResult::fail(format!("Failed to create directories: {e}"));
        }
    }

    match fs::write(&resolved, content) {
        Ok(()) => {
            let bytes = content.len();
            ToolResult::ok_with_data(
                format!("Wrote {bytes} bytes to {}", resolved.display()),
                serde_json::json!({
                    "path": resolved.display().to_string(),
                    "bytes_written": bytes,
                }),
            )
        }
        Err(e) => ToolResult::fail(format!("Failed to write file: {e}")),
    }
}

/// List entries in a directory.
fn list_directory(path_str: &str, root: &Path) -> ToolResult {
    let resolved = match resolve_safe(path_str, root) {
        Ok(p) => p,
        Err(e) => return ToolResult::fail(e),
    };

    if !resolved.is_dir() {
        return ToolResult::fail(format!(
            "Not a directory or does not exist: {}",
            resolved.display()
        ));
    }

    let entries = match fs::read_dir(&resolved) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return ToolResult::fail(format!("Permission denied: {}", resolved.display()));
        }
        Err(e) => return ToolResult::fail(format!("Failed to list directory: {e}")),
    };

    let mut items: Vec<serde_json::Value> = Vec::new();
    let mut lines: Vec<String> = Vec::new();

    // Collect and sort by name.
    let mut sorted_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted_entries.sort_by_key(|e| e.file_name());

    for entry in sorted_entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let size = if is_dir {
            0u64
        } else {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        };

        let kind = if is_dir { "dir" } else { "file" };

        items.push(serde_json::json!({
            "name": name,
            "type": kind,
            "size": size,
        }));

        if is_dir {
            lines.push(format!("[DIR] {name}"));
        } else {
            lines.push(format!("      {name} ({size} B)"));
        }
    }

    let output = if lines.is_empty() {
        "(empty directory)".to_string()
    } else {
        lines.join("\n")
    };

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": resolved.display().to_string(),
            "entries": items,
        }),
    )
}

/// Search for files matching a glob pattern.
fn search_files(path_str: &str, pattern: &str, root: &Path) -> ToolResult {
    let resolved = match resolve_safe(path_str, root) {
        Ok(p) => p,
        Err(e) => return ToolResult::fail(e),
    };

    if !resolved.is_dir() {
        return ToolResult::fail(format!("Not a directory: {}", resolved.display()));
    }

    let mut matches: Vec<String> = Vec::new();

    if let Err(e) = walk_and_match(&resolved, pattern, &mut matches) {
        return ToolResult::fail(e);
    }

    let truncated = matches.len() >= MAX_SEARCH_RESULTS;
    let mut output = if matches.is_empty() {
        "(no matches)".to_string()
    } else {
        matches.join("\n")
    };

    if truncated {
        output.push_str(&format!("\n... (truncated at {MAX_SEARCH_RESULTS} results)"));
    }

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "matches": matches,
            "truncated": truncated,
        }),
    )
}

/// Recursively walk a directory tree and collect matching file names.
fn walk_and_match(dir: &Path, pattern: &str, matches: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            format!("Permission denied while searching {}", dir.display())
        } else {
            format!("Search failed: {e}")
        }
    })?;

    for entry in entries.flatten() {
        if matches.len() >= MAX_SEARCH_RESULTS {
            break;
        }

        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden directories.
        if name.starts_with('.') {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }
        }

        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            walk_and_match(&entry.path(), pattern, matches)?;
        } else if glob_match(pattern, &name) {
            matches.push(entry.path().display().to_string());
        }
    }

    Ok(())
}

/// Simple glob matching supporting `*` and `?` wildcards.
///
/// This is a basic implementation equivalent to Python's `fnmatch.fnmatch`.
fn glob_match(pattern: &str, name: &str) -> bool {
    let regex_str = glob_to_regex(pattern);
    regex::Regex::new(&regex_str)
        .map(|re| re.is_match(name))
        .unwrap_or(false)
}

/// Convert a simple glob pattern to a regex string.
fn glob_to_regex(pattern: &str) -> String {
    let mut regex = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' | '+' | '(' | ')' | '{' | '}' | '[' | ']' | '|' | '^' | '$' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }
    regex.push('$');
    regex
}

/// Get metadata information about a file or directory.
fn file_info(path_str: &str, root: &Path) -> ToolResult {
    if path_str.is_empty() {
        return ToolResult::fail("'path' is required for file_info.");
    }

    let resolved = match resolve_safe(path_str, root) {
        Ok(p) => p,
        Err(e) => return ToolResult::fail(e),
    };

    if !resolved.exists() {
        return ToolResult::fail(format!("Path does not exist: {}", resolved.display()));
    }

    let meta = match resolved.metadata() {
        Ok(m) => m,
        Err(e) => return ToolResult::fail(format!("Failed to get file info: {e}")),
    };

    let file_type = if meta.is_dir() {
        "directory"
    } else {
        "file"
    };

    let mime_type = guess_mime_type(&resolved);

    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let created = meta
        .created()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let permissions = format!("{:o}", meta.permissions().mode() & 0o777);

    let info = serde_json::json!({
        "path": resolved.display().to_string(),
        "type": file_type,
        "size": meta.len(),
        "mime_type": mime_type,
        "modified": modified,
        "created": created,
        "permissions": permissions,
    });

    let lines = [
        format!("path: {}", resolved.display()),
        format!("type: {file_type}"),
        format!("size: {}", meta.len()),
        format!("mime_type: {mime_type}"),
        format!("modified: {modified}"),
        format!("created: {created}"),
        format!("permissions: {permissions}"),
    ];

    ToolResult::ok_with_data(lines.join("\n"), info)
}

/// Guess a MIME type from the file extension.
fn guess_mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("txt") => "text/plain",
        Some("html") | Some("htm") => "text/html",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("xml") => "application/xml",
        Some("py") => "text/x-python",
        Some("rs") => "text/x-rust",
        Some("toml") => "application/toml",
        Some("yaml") | Some("yml") => "application/x-yaml",
        Some("md") => "text/markdown",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("tar") => "application/x-tar",
        Some("gz") => "application/gzip",
        Some("sh") => "application/x-sh",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_star() {
        assert!(glob_match("*.py", "test.py"));
        assert!(!glob_match("*.py", "test.rs"));
    }

    #[test]
    fn glob_match_question() {
        assert!(glob_match("?.txt", "a.txt"));
        assert!(!glob_match("?.txt", "ab.txt"));
    }

    #[test]
    fn glob_match_all() {
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn resolve_safe_rejects_traversal() {
        let root = PathBuf::from("/home/testuser");
        let result = resolve_safe("/etc/passwd", &root);
        assert!(result.is_err());
    }

    #[test]
    fn read_file_requires_path() {
        let tool = FilesTool;
        let r = tool.execute(serde_json::json!({ "action": "read_file" }));
        assert!(!r.success);
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let file_path = root.join("test.txt");

        let w = write_file(file_path.to_str().unwrap(), "hello world", &root);
        assert!(w.success, "write failed: {:?}", w.error);

        let r = read_file(file_path.to_str().unwrap(), &root);
        assert!(r.success, "read failed: {:?}", r.error);
        assert_eq!(r.output, "hello world");
    }

    #[test]
    fn list_directory_works() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("a.txt"), "aaa").unwrap();
        fs::create_dir(root.join("subdir")).unwrap();

        let r = list_directory(root.to_str().unwrap(), &root);
        assert!(r.success);
        assert!(r.output.contains("a.txt"));
        assert!(r.output.contains("[DIR] subdir"));
    }

    #[test]
    fn search_files_works() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("hello.py"), "").unwrap();
        fs::write(root.join("hello.rs"), "").unwrap();

        let r = search_files(root.to_str().unwrap(), "*.py", &root);
        assert!(r.success);
        assert!(r.output.contains("hello.py"));
        assert!(!r.output.contains("hello.rs"));
    }

    #[test]
    fn file_info_works() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let file_path = root.join("info_test.txt");
        fs::write(&file_path, "content").unwrap();

        let r = file_info(file_path.to_str().unwrap(), &root);
        assert!(r.success);
        assert!(r.output.contains("type: file"));
        assert!(r.output.contains("text/plain"));
    }

    #[test]
    fn file_info_requires_path() {
        let root = PathBuf::from("/tmp");
        let r = file_info("", &root);
        assert!(!r.success);
    }

    #[test]
    fn guess_mime_known() {
        assert_eq!(guess_mime_type(Path::new("test.json")), "application/json");
        assert_eq!(guess_mime_type(Path::new("test.png")), "image/png");
    }

    #[test]
    fn guess_mime_unknown() {
        assert_eq!(guess_mime_type(Path::new("test.xyz123")), "unknown");
    }

    #[test]
    fn unknown_action() {
        let tool = FilesTool;
        let r = tool.execute(serde_json::json!({ "action": "delete_all" }));
        assert!(!r.success);
    }
}
