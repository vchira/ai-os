//! Data processing tool — process data files locally without sending
//! the entire file to the AI.
//!
//! All operations stream the file line-by-line to avoid loading large files
//! into memory. Results are compact strings suitable for the LLM context.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use aios_core::types::ToolResult;
use regex::Regex;

use crate::tool::Tool;

/// Maximum number of matching lines to return for grep/log_errors.
const MAX_MATCH_LINES: usize = 100;

/// Maximum number of lines to load for sort/unique operations.
const MAX_SORT_LINES: usize = 100_000;

// ---------------------------------------------------------------------------
// Tool definition
// ---------------------------------------------------------------------------

/// Local data processing tool.
///
/// Processes files without sending their full content to the LLM. Supports
/// line counts, head/tail, grep, sort, CSV summary, JSON queries, and more.
pub struct DataProcessTool;

impl Tool for DataProcessTool {
    fn name(&self) -> &str {
        "process_data"
    }

    fn description(&self) -> &str {
        "Process data files locally without sending the entire file to the AI. \
         Use this for large files, CSVs, logs, etc."
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
                    "enum": [
                        "count_lines", "head", "tail", "grep", "sort", "unique",
                        "csv_summary", "csv_query", "file_stats", "json_query",
                        "log_errors", "diff"
                    ],
                    "description": "Data processing action to perform."
                },
                "path": {
                    "type": "string",
                    "description": "File path to process."
                },
                "path2": {
                    "type": "string",
                    "description": "Second file path (for diff action)."
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (for grep action)."
                },
                "n": {
                    "type": "number",
                    "description": "Number of lines (for head/tail, default 10)."
                },
                "query": {
                    "type": "string",
                    "description": "Query expression (for csv_query, json_query)."
                }
            },
            "required": ["action", "path"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let path2 = args.get("path2").and_then(|v| v.as_str()).unwrap_or("");
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        if path.is_empty() {
            return ToolResult::fail("'path' is required.");
        }

        // Resolve the path, scoping to home directory.
        let resolved = match resolve_path(path) {
            Ok(p) => p,
            Err(e) => return ToolResult::fail(e),
        };

        match action {
            "count_lines" => count_lines(&resolved),
            "head" => head(&resolved, n),
            "tail" => tail(&resolved, n),
            "grep" => grep(&resolved, pattern),
            "sort" => sort_lines(&resolved),
            "unique" => unique_lines(&resolved),
            "csv_summary" => csv_summary(&resolved),
            "csv_query" => csv_query(&resolved, query),
            "file_stats" => file_stats(&resolved),
            "json_query" => json_query(&resolved, query),
            "log_errors" => log_errors(&resolved),
            "diff" => {
                if path2.is_empty() {
                    return ToolResult::fail("'path2' is required for diff action.");
                }
                let resolved2 = match resolve_path(path2) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::fail(e),
                };
                diff_files(&resolved, &resolved2)
            }
            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. Use: count_lines, head, tail, grep, sort, unique, \
                 csv_summary, csv_query, file_stats, json_query, log_errors, diff."
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Resolve a path string, expanding ~ to the home directory.
fn resolve_path(path_str: &str) -> Result<PathBuf, String> {
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

    // Ensure the path is under the home directory.
    let home = home_dir();
    let resolved = expanded.canonicalize().unwrap_or(expanded);
    let home_resolved = home.canonicalize().unwrap_or(home);

    if !resolved.starts_with(&home_resolved) && !resolved.starts_with("/tmp") {
        return Err(format!(
            "Access denied: {} is outside the home directory.",
            resolved.display()
        ));
    }

    Ok(resolved)
}

fn home_dir() -> PathBuf {
    crate::home_dir()
}

// ---------------------------------------------------------------------------
// Action implementations
// ---------------------------------------------------------------------------

/// Count lines in a file using streaming BufReader.
fn count_lines(path: &Path) -> ToolResult {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let count = reader.lines().count();

    ToolResult::ok_with_data(
        format!("{count} lines"),
        serde_json::json!({ "path": path.display().to_string(), "lines": count }),
    )
}

/// Read the first N lines of a file.
fn head(path: &Path, n: usize) -> ToolResult {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .take(n)
        .filter_map(|l| l.ok())
        .collect();

    let output = lines.join("\n");
    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": path.display().to_string(),
            "lines_shown": lines.len(),
            "n": n,
        }),
    )
}

/// Read the last N lines of a file.
///
/// Uses a ring buffer to stream through the file without loading it all.
fn tail(path: &Path, n: usize) -> ToolResult {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let mut ring: Vec<String> = Vec::with_capacity(n);
    let mut total = 0usize;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        total += 1;
        if ring.len() < n {
            ring.push(line);
        } else {
            ring[total % n] = line;
        }
    }

    // Reconstruct the correct order from the ring buffer.
    let lines = if total <= n {
        ring
    } else {
        let start = (total + 1) % n;
        let mut ordered = Vec::with_capacity(n);
        for i in 0..n {
            ordered.push(ring[(start + i) % n].clone());
        }
        ordered
    };

    let output = lines.join("\n");
    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": path.display().to_string(),
            "lines_shown": lines.len(),
            "total_lines": total,
            "n": n,
        }),
    )
}

/// Search for lines matching a regex pattern.
fn grep(path: &Path, pattern: &str) -> ToolResult {
    if pattern.is_empty() {
        return ToolResult::fail("'pattern' is required for grep action.");
    }

    let regex = match Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return ToolResult::fail(format!("Invalid regex pattern: {e}")),
    };

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let mut matches: Vec<String> = Vec::new();
    let mut line_num = 0usize;
    let mut total_matches = 0usize;

    for line in reader.lines() {
        line_num += 1;
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if regex.is_match(&line) {
            total_matches += 1;
            if matches.len() < MAX_MATCH_LINES {
                matches.push(format!("{line_num}: {line}"));
            }
        }
    }

    let truncated = total_matches > MAX_MATCH_LINES;
    let mut output = if matches.is_empty() {
        "(no matches)".to_string()
    } else {
        matches.join("\n")
    };

    if truncated {
        output.push_str(&format!(
            "\n... ({total_matches} total matches, showing first {MAX_MATCH_LINES})"
        ));
    }

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": path.display().to_string(),
            "pattern": pattern,
            "total_matches": total_matches,
            "shown": matches.len().min(total_matches),
            "truncated": truncated,
        }),
    )
}

/// Sort all lines alphabetically (up to MAX_SORT_LINES).
fn sort_lines(path: &Path) -> ToolResult {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader
        .lines()
        .take(MAX_SORT_LINES)
        .filter_map(|l| l.ok())
        .collect();

    let truncated = lines.len() >= MAX_SORT_LINES;
    lines.sort();

    let mut output = lines.join("\n");
    if truncated {
        output.push_str(&format!(
            "\n... (truncated at {MAX_SORT_LINES} lines)"
        ));
    }

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": path.display().to_string(),
            "lines": lines.len(),
            "truncated": truncated,
        }),
    )
}

/// Unique lines with counts, sorted by frequency.
fn unique_lines(path: &Path) -> ToolResult {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut total = 0usize;

    for line in reader.lines().take(MAX_SORT_LINES) {
        if let Ok(l) = line {
            total += 1;
            *counts.entry(l).or_insert(0) += 1;
        }
    }

    let mut entries: Vec<(String, usize)> = counts.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    let lines: Vec<String> = entries
        .iter()
        .take(100) // Show top 100
        .map(|(line, count)| format!("{count:>6}  {line}"))
        .collect();

    let output = if lines.is_empty() {
        "(empty file)".to_string()
    } else {
        lines.join("\n")
    };

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": path.display().to_string(),
            "total_lines": total,
            "unique_lines": entries.len(),
        }),
    )
}

/// Summarize a CSV file: column names, row count, sample rows.
fn csv_summary(path: &Path) -> ToolResult {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let mut lines_iter = reader.lines();

    // Read header.
    let header_line = match lines_iter.next() {
        Some(Ok(h)) => h,
        Some(Err(e)) => return ToolResult::fail(format!("Failed to read CSV header: {e}")),
        None => return ToolResult::fail("Empty CSV file."),
    };

    let headers: Vec<&str> = header_line.split(',').map(|s| s.trim()).collect();

    // Collect sample rows (first 5) and count total.
    let mut sample_rows: Vec<String> = Vec::new();
    let mut row_count = 0usize;

    for line in lines_iter {
        if let Ok(l) = line {
            row_count += 1;
            if sample_rows.len() < 5 {
                sample_rows.push(l);
            }
        }
    }

    let mut output = String::new();
    output.push_str(&format!("Columns ({}):\n", headers.len()));
    for (i, h) in headers.iter().enumerate() {
        output.push_str(&format!("  {}: {h}\n", i + 1));
    }
    output.push_str(&format!("\nRows: {row_count}\n"));
    output.push_str("\nSample rows:\n");
    output.push_str(&format!("{header_line}\n"));
    for row in &sample_rows {
        output.push_str(&format!("{row}\n"));
    }

    ToolResult::ok_with_data(
        output.trim().to_string(),
        serde_json::json!({
            "path": path.display().to_string(),
            "columns": headers,
            "row_count": row_count,
            "sample_rows": sample_rows.len(),
        }),
    )
}

/// Simple CSV query: filter rows by a column condition.
///
/// Query format: `column_name operator value`
/// Operators: `>`, `<`, `>=`, `<=`, `==`, `!=`, `contains`
fn csv_query(path: &Path, query: &str) -> ToolResult {
    if query.is_empty() {
        return ToolResult::fail("'query' is required for csv_query action.");
    }

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let mut lines_iter = reader.lines();

    // Read header.
    let header_line = match lines_iter.next() {
        Some(Ok(h)) => h,
        Some(Err(e)) => return ToolResult::fail(format!("Failed to read CSV header: {e}")),
        None => return ToolResult::fail("Empty CSV file."),
    };

    let headers: Vec<String> = header_line
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    // Parse the query.
    let parsed = match parse_csv_filter(query, &headers) {
        Ok(p) => p,
        Err(e) => return ToolResult::fail(e),
    };

    // Filter rows.
    let mut matched_rows: Vec<String> = Vec::new();
    let mut total_rows = 0usize;

    for line in lines_iter {
        if let Ok(l) = line {
            total_rows += 1;
            let fields: Vec<&str> = l.split(',').map(|s| s.trim()).collect();
            if parsed.col_idx < fields.len() && apply_filter(fields[parsed.col_idx], &parsed) {
                if matched_rows.len() < MAX_MATCH_LINES {
                    matched_rows.push(l);
                }
            }
        }
    }

    let mut output = format!("{header_line}\n");
    if matched_rows.is_empty() {
        output.push_str("(no matching rows)");
    } else {
        output.push_str(&matched_rows.join("\n"));
    }

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": path.display().to_string(),
            "query": query,
            "total_rows": total_rows,
            "matched_rows": matched_rows.len(),
        }),
    )
}

/// File statistics: size, line count, word count, encoding guess.
fn file_stats(path: &Path) -> ToolResult {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return ToolResult::fail(format!("Failed to get file metadata: {e}")),
    };

    let size = meta.len();

    // Stream through the file to count lines and words.
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let mut line_count = 0usize;
    let mut word_count = 0usize;
    let mut is_utf8 = true;
    let mut has_binary = false;

    for line in reader.lines() {
        match line {
            Ok(l) => {
                line_count += 1;
                word_count += l.split_whitespace().count();
            }
            Err(_) => {
                is_utf8 = false;
                has_binary = true;
                break;
            }
        }
    }

    let encoding = if has_binary {
        "binary"
    } else if is_utf8 {
        "UTF-8"
    } else {
        "unknown"
    };

    let size_human = if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    };

    let output = format!(
        "Size: {size_human} ({size} bytes)\n\
         Lines: {line_count}\n\
         Words: {word_count}\n\
         Encoding: {encoding}"
    );

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": path.display().to_string(),
            "size": size,
            "size_human": size_human,
            "lines": line_count,
            "words": word_count,
            "encoding": encoding,
        }),
    )
}

/// Extract fields from a JSON file using dot notation.
///
/// Supports paths like: `data.users[0].name`, `config.timeout`, `items[2]`.
fn json_query(path: &Path, query: &str) -> ToolResult {
    if query.is_empty() {
        return ToolResult::fail("'query' is required for json_query action.");
    }

    // Read the JSON file (with a size limit to prevent OOM).
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return ToolResult::fail(format!("Failed to get file metadata: {e}")),
    };

    if meta.len() > 50_000_000 {
        return ToolResult::fail("JSON file too large (>50MB). Use grep or head instead.");
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return ToolResult::fail(format!("Failed to read file: {e}")),
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => return ToolResult::fail(format!("Invalid JSON: {e}")),
    };

    let result = navigate_json(&json, query);

    let output = match &result {
        serde_json::Value::Null => "(null)".to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| format!("{other}")),
    };

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path": path.display().to_string(),
            "query": query,
        }),
    )
}

/// Extract error and warning lines from log files.
fn log_errors(path: &Path) -> ToolResult {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open file: {e}")),
    };

    let reader = BufReader::new(file);
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut fatals: Vec<String> = Vec::new();
    let mut line_num = 0usize;

    let patterns = [
        ("FATAL", "fatal"),
        ("ERROR", "error"),
        ("WARN", "warn"),
    ];

    for line in reader.lines() {
        line_num += 1;
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        let lower = line.to_lowercase();

        for (label, pattern) in &patterns {
            if lower.contains(pattern) {
                let entry = format!("{line_num}: {line}");
                match *label {
                    "FATAL" => {
                        if fatals.len() < MAX_MATCH_LINES {
                            fatals.push(entry);
                        }
                    }
                    "ERROR" => {
                        if errors.len() < MAX_MATCH_LINES {
                            errors.push(entry);
                        }
                    }
                    "WARN" => {
                        if warnings.len() < MAX_MATCH_LINES {
                            warnings.push(entry);
                        }
                    }
                    _ => {}
                }
                break; // Only count each line once (highest severity).
            }
        }
    }

    let mut output = String::new();

    if !fatals.is_empty() {
        output.push_str(&format!("=== FATAL ({}) ===\n", fatals.len()));
        for f in &fatals {
            output.push_str(&format!("{f}\n"));
        }
        output.push('\n');
    }

    if !errors.is_empty() {
        output.push_str(&format!("=== ERRORS ({}) ===\n", errors.len()));
        for e in &errors {
            output.push_str(&format!("{e}\n"));
        }
        output.push('\n');
    }

    if !warnings.is_empty() {
        output.push_str(&format!("=== WARNINGS ({}) ===\n", warnings.len()));
        for w in &warnings {
            output.push_str(&format!("{w}\n"));
        }
    }

    if output.is_empty() {
        output = "No errors, warnings, or fatal messages found.".to_string();
    }

    ToolResult::ok_with_data(
        output.trim().to_string(),
        serde_json::json!({
            "path": path.display().to_string(),
            "total_lines": line_num,
            "fatals": fatals.len(),
            "errors": errors.len(),
            "warnings": warnings.len(),
        }),
    )
}

/// Compare two files line by line.
fn diff_files(path1: &Path, path2: &Path) -> ToolResult {
    let file1 = match File::open(path1) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open first file: {e}")),
    };
    let file2 = match File::open(path2) {
        Ok(f) => f,
        Err(e) => return ToolResult::fail(format!("Failed to open second file: {e}")),
    };

    let reader1 = BufReader::new(file1);
    let reader2 = BufReader::new(file2);

    // Read both files line by line (up to a limit).
    let max_lines = 10_000;
    let lines1: Vec<String> = reader1
        .lines()
        .take(max_lines)
        .filter_map(|l| l.ok())
        .collect();
    let lines2: Vec<String> = reader2
        .lines()
        .take(max_lines)
        .filter_map(|l| l.ok())
        .collect();

    let mut diffs: Vec<String> = Vec::new();
    let max_len = lines1.len().max(lines2.len());
    let mut additions = 0usize;
    let mut deletions = 0usize;
    let mut changes = 0usize;

    for i in 0..max_len {
        if diffs.len() >= MAX_MATCH_LINES {
            diffs.push("... (truncated)".to_string());
            break;
        }

        match (lines1.get(i), lines2.get(i)) {
            (Some(a), Some(b)) if a != b => {
                changes += 1;
                diffs.push(format!("{}:- {a}", i + 1));
                diffs.push(format!("{}:+ {b}", i + 1));
            }
            (Some(a), None) => {
                deletions += 1;
                diffs.push(format!("{}:- {a}", i + 1));
            }
            (None, Some(b)) => {
                additions += 1;
                diffs.push(format!("{}:+ {b}", i + 1));
            }
            _ => {} // Lines are equal or both None.
        }
    }

    let output = if diffs.is_empty() {
        "Files are identical.".to_string()
    } else {
        format!(
            "{} changes, {} additions, {} deletions\n\n{}",
            changes,
            additions,
            deletions,
            diffs.join("\n")
        )
    };

    ToolResult::ok_with_data(
        output,
        serde_json::json!({
            "path1": path1.display().to_string(),
            "path2": path2.display().to_string(),
            "lines1": lines1.len(),
            "lines2": lines2.len(),
            "changes": changes,
            "additions": additions,
            "deletions": deletions,
        }),
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parsed CSV filter.
struct CsvFilter {
    col_idx: usize,
    operator: FilterOp,
    value: String,
}

enum FilterOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    Ne,
    Contains,
}

/// Parse a CSV filter query like "column_name > 100".
fn parse_csv_filter(query: &str, headers: &[String]) -> Result<CsvFilter, String> {
    // Supported operators (check longest first to avoid ambiguity).
    let operators = [">=", "<=", "!=", "==", ">", "<", "contains"];

    for op_str in &operators {
        if let Some(pos) = query.find(op_str) {
            let col_name = query[..pos].trim();
            let value = query[pos + op_str.len()..].trim().to_string();

            let col_idx = headers
                .iter()
                .position(|h| h == col_name)
                .ok_or_else(|| format!("Column '{col_name}' not found. Available: {}", headers.join(", ")))?;

            let operator = match *op_str {
                ">" => FilterOp::Gt,
                "<" => FilterOp::Lt,
                ">=" => FilterOp::Gte,
                "<=" => FilterOp::Lte,
                "==" => FilterOp::Eq,
                "!=" => FilterOp::Ne,
                "contains" => FilterOp::Contains,
                _ => return Err(format!("Unknown operator: {op_str}")),
            };

            return Ok(CsvFilter {
                col_idx,
                operator,
                value,
            });
        }
    }

    Err(format!(
        "Invalid query format. Use: column_name operator value\n\
         Operators: >, <, >=, <=, ==, !=, contains\n\
         Example: age > 30"
    ))
}

/// Apply a filter to a field value.
fn apply_filter(field: &str, filter: &CsvFilter) -> bool {
    match filter.operator {
        FilterOp::Contains => field.contains(&filter.value),
        FilterOp::Eq => field == filter.value,
        FilterOp::Ne => field != filter.value,
        FilterOp::Gt | FilterOp::Lt | FilterOp::Gte | FilterOp::Lte => {
            // Try numeric comparison first, fall back to string comparison.
            if let (Ok(fv), Ok(cv)) = (field.parse::<f64>(), filter.value.parse::<f64>()) {
                match filter.operator {
                    FilterOp::Gt => fv > cv,
                    FilterOp::Lt => fv < cv,
                    FilterOp::Gte => fv >= cv,
                    FilterOp::Lte => fv <= cv,
                    _ => unreachable!(),
                }
            } else {
                match filter.operator {
                    FilterOp::Gt => field > filter.value.as_str(),
                    FilterOp::Lt => field < filter.value.as_str(),
                    FilterOp::Gte => field >= filter.value.as_str(),
                    FilterOp::Lte => field <= filter.value.as_str(),
                    _ => unreachable!(),
                }
            }
        }
    }
}

/// Navigate a JSON value using dot-notation path.
///
/// Supports: `data.users[0].name`, `items[2].value`, `config.timeout`
fn navigate_json<'a>(value: &'a serde_json::Value, path: &str) -> serde_json::Value {
    if path.is_empty() {
        return value.clone();
    }

    let mut current = value;

    for segment in split_json_path(path) {
        match &segment {
            JsonPathSegment::Key(key) => {
                current = match current.get(key.as_str()) {
                    Some(v) => v,
                    None => return serde_json::Value::Null,
                };
            }
            JsonPathSegment::Index(idx) => {
                current = match current.get(*idx) {
                    Some(v) => v,
                    None => return serde_json::Value::Null,
                };
            }
        }
    }

    current.clone()
}

enum JsonPathSegment {
    Key(String),
    Index(usize),
}

/// Split a JSON path like "data.users[0].name" into segments.
fn split_json_path(path: &str) -> Vec<JsonPathSegment> {
    let mut segments = Vec::new();
    let mut current_key = String::new();

    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !current_key.is_empty() {
                    segments.push(JsonPathSegment::Key(std::mem::take(&mut current_key)));
                }
            }
            '[' => {
                if !current_key.is_empty() {
                    segments.push(JsonPathSegment::Key(std::mem::take(&mut current_key)));
                }
                // Read the index.
                let mut index_str = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == ']' {
                        chars.next();
                        break;
                    }
                    index_str.push(next_ch);
                    chars.next();
                }
                if let Ok(idx) = index_str.parse::<usize>() {
                    segments.push(JsonPathSegment::Index(idx));
                }
            }
            _ => {
                current_key.push(ch);
            }
        }
    }

    if !current_key.is_empty() {
        segments.push(JsonPathSegment::Key(current_key));
    }

    segments
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_temp_file(content: &str) -> (tempfile::NamedTempFile, PathBuf) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        let path = f.path().to_path_buf();
        (f, path)
    }

    // -- count_lines --

    #[test]
    fn count_lines_basic() {
        let (_f, path) = create_temp_file("line1\nline2\nline3\n");
        let r = count_lines(&path);
        assert!(r.success);
        assert!(r.output.contains("3"));
    }

    #[test]
    fn count_lines_empty() {
        let (_f, path) = create_temp_file("");
        let r = count_lines(&path);
        assert!(r.success);
        assert!(r.output.contains("0"));
    }

    // -- head --

    #[test]
    fn head_basic() {
        let content = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let (_f, path) = create_temp_file(&content);
        let r = head(&path, 5);
        assert!(r.success);
        assert!(r.output.contains("line 1"));
        assert!(r.output.contains("line 5"));
        assert!(!r.output.contains("line 6"));
    }

    // -- tail --

    #[test]
    fn tail_basic() {
        let content = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let (_f, path) = create_temp_file(&content);
        let r = tail(&path, 3);
        assert!(r.success);
        assert!(r.output.contains("line 18"));
        assert!(r.output.contains("line 19"));
        assert!(r.output.contains("line 20"));
        assert!(!r.output.contains("line 17"));
    }

    #[test]
    fn tail_fewer_lines_than_n() {
        let (_f, path) = create_temp_file("a\nb\n");
        let r = tail(&path, 10);
        assert!(r.success);
        assert!(r.output.contains("a"));
        assert!(r.output.contains("b"));
    }

    // -- grep --

    #[test]
    fn grep_basic() {
        let (_f, path) = create_temp_file("apple\nbanana\napricot\ncherry\n");
        let r = grep(&path, "^a");
        assert!(r.success);
        assert!(r.output.contains("apple"));
        assert!(r.output.contains("apricot"));
        assert!(!r.output.contains("banana"));
    }

    #[test]
    fn grep_no_match() {
        let (_f, path) = create_temp_file("apple\nbanana\n");
        let r = grep(&path, "zzz");
        assert!(r.success);
        assert!(r.output.contains("no matches"));
    }

    #[test]
    fn grep_empty_pattern() {
        let (_f, path) = create_temp_file("test\n");
        let r = grep(&path, "");
        assert!(!r.success);
    }

    // -- sort --

    #[test]
    fn sort_basic() {
        let (_f, path) = create_temp_file("cherry\napple\nbanana\n");
        let r = sort_lines(&path);
        assert!(r.success);
        let lines: Vec<&str> = r.output.lines().collect();
        assert_eq!(lines, vec!["apple", "banana", "cherry"]);
    }

    // -- unique --

    #[test]
    fn unique_basic() {
        let (_f, path) = create_temp_file("a\nb\na\nb\na\nc\n");
        let r = unique_lines(&path);
        assert!(r.success);
        // 'a' should have highest count (3).
        let first_line = r.output.lines().next().unwrap();
        assert!(first_line.contains("3"));
        assert!(first_line.contains("a"));
    }

    // -- csv_summary --

    #[test]
    fn csv_summary_basic() {
        let csv = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,Chicago\n";
        let (_f, path) = create_temp_file(csv);
        let r = csv_summary(&path);
        assert!(r.success);
        assert!(r.output.contains("name"));
        assert!(r.output.contains("age"));
        assert!(r.output.contains("city"));
        assert!(r.output.contains("Rows: 3"));
    }

    // -- csv_query --

    #[test]
    fn csv_query_numeric() {
        let csv = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,Chicago\n";
        let (_f, path) = create_temp_file(csv);
        let r = csv_query(&path, "age > 28");
        assert!(r.success);
        assert!(r.output.contains("Alice"));
        assert!(r.output.contains("Charlie"));
        assert!(!r.output.contains("Bob"));
    }

    #[test]
    fn csv_query_contains() {
        let csv = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,Chicago\n";
        let (_f, path) = create_temp_file(csv);
        let r = csv_query(&path, "city contains Chi");
        assert!(r.success);
        assert!(r.output.contains("Chicago"));
        assert!(!r.output.contains("NYC"));
    }

    // -- file_stats --

    #[test]
    fn file_stats_basic() {
        let content = "hello world\nfoo bar baz\n";
        let (_f, path) = create_temp_file(content);
        let r = file_stats(&path);
        assert!(r.success);
        assert!(r.output.contains("Lines: 2"));
        assert!(r.output.contains("Words: 5"));
        assert!(r.output.contains("UTF-8"));
    }

    // -- json_query --

    #[test]
    fn json_query_basic() {
        let json = r#"{"data": {"users": [{"name": "Alice"}, {"name": "Bob"}]}}"#;
        let (_f, path) = create_temp_file(json);
        let r = json_query(&path, "data.users[0].name");
        assert!(r.success);
        assert_eq!(r.output.trim(), "Alice");
    }

    #[test]
    fn json_query_nested_array() {
        let json = r#"{"items": [10, 20, 30]}"#;
        let (_f, path) = create_temp_file(json);
        let r = json_query(&path, "items[1]");
        assert!(r.success);
        assert!(r.output.contains("20"));
    }

    #[test]
    fn json_query_missing_path() {
        let json = r#"{"a": 1}"#;
        let (_f, path) = create_temp_file(json);
        let r = json_query(&path, "b.c");
        assert!(r.success);
        assert!(r.output.contains("null"));
    }

    // -- log_errors --

    #[test]
    fn log_errors_finds_all_levels() {
        let log = "\
2024-01-01 INFO Starting up\n\
2024-01-01 WARNING Low disk space\n\
2024-01-01 ERROR Connection failed\n\
2024-01-01 FATAL Out of memory\n\
2024-01-01 INFO Shutting down\n";
        let (_f, path) = create_temp_file(log);
        let r = log_errors(&path);
        assert!(r.success);
        assert!(r.output.contains("FATAL"));
        assert!(r.output.contains("ERRORS"));
        assert!(r.output.contains("WARNINGS"));
    }

    #[test]
    fn log_errors_clean_log() {
        let log = "2024-01-01 INFO All good\n2024-01-01 INFO Still good\n";
        let (_f, path) = create_temp_file(log);
        let r = log_errors(&path);
        assert!(r.success);
        assert!(r.output.contains("No errors"));
    }

    // -- diff --

    #[test]
    fn diff_identical() {
        let (_f1, path1) = create_temp_file("a\nb\nc\n");
        let (_f2, path2) = create_temp_file("a\nb\nc\n");
        let r = diff_files(&path1, &path2);
        assert!(r.success);
        assert!(r.output.contains("identical"));
    }

    #[test]
    fn diff_different() {
        let (_f1, path1) = create_temp_file("a\nb\nc\n");
        let (_f2, path2) = create_temp_file("a\nB\nc\n");
        let r = diff_files(&path1, &path2);
        assert!(r.success);
        assert!(r.output.contains("- b"));
        assert!(r.output.contains("+ B"));
    }

    // -- navigate_json --

    #[test]
    fn navigate_json_simple() {
        let json: serde_json::Value = serde_json::json!({"a": {"b": 42}});
        let result = navigate_json(&json, "a.b");
        assert_eq!(result, serde_json::json!(42));
    }

    #[test]
    fn navigate_json_array() {
        let json: serde_json::Value = serde_json::json!({"items": [1, 2, 3]});
        let result = navigate_json(&json, "items[1]");
        assert_eq!(result, serde_json::json!(2));
    }

    // -- split_json_path --

    #[test]
    fn split_path_simple() {
        let segments = split_json_path("data.users[0].name");
        assert_eq!(segments.len(), 4);
        assert!(matches!(&segments[0], JsonPathSegment::Key(k) if k == "data"));
        assert!(matches!(&segments[1], JsonPathSegment::Key(k) if k == "users"));
        assert!(matches!(&segments[2], JsonPathSegment::Index(0)));
        assert!(matches!(&segments[3], JsonPathSegment::Key(k) if k == "name"));
    }

    // -- Tool interface --

    #[test]
    fn tool_rejects_missing_path() {
        let tool = DataProcessTool;
        let r = tool.execute(serde_json::json!({ "action": "count_lines" }));
        assert!(!r.success);
    }

    #[test]
    fn tool_unknown_action() {
        let tool = DataProcessTool;
        let r = tool.execute(serde_json::json!({
            "action": "destroy",
            "path": "/tmp/test"
        }));
        assert!(!r.success);
    }
}
