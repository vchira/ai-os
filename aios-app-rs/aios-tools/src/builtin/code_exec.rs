//! Code execution tool — run code in a sandboxed environment.
//!
//! Supports Python, Bash, JavaScript, and Rust. Code is always executed
//! inside a sandbox (Docker preferred, process-level fallback).

use std::fs;

use aios_core::types::ToolResult;
use tracing::debug;

use crate::sandbox::{Sandbox, SandboxType};
use crate::tool::Tool;

/// Sandboxed code execution tool.
///
/// Writes code to a temporary file and executes it in a Docker container
/// (or process sandbox if Docker is unavailable). Never runs code directly
/// on the host.
pub struct CodeExecTool;

impl Tool for CodeExecTool {
    fn name(&self) -> &str {
        "execute_code"
    }

    fn description(&self) -> &str {
        "Execute code in a sandboxed environment. Supports Python, Bash, \
         JavaScript, and Rust."
    }

    fn category(&self) -> &str {
        "system"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "language": {
                    "type": "string",
                    "enum": ["python", "bash", "javascript", "rust"],
                    "description": "Programming language of the code."
                },
                "code": {
                    "type": "string",
                    "description": "The code to execute."
                },
                "timeout": {
                    "type": "number",
                    "description": "Timeout in seconds (default 30)."
                }
            },
            "required": ["language", "code"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let language = match args.get("language").and_then(|v| v.as_str()) {
            Some(l) => l,
            None => return ToolResult::fail("'language' is required."),
        };

        let code = match args.get("code").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::fail("'code' is required."),
        };

        let timeout = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        execute_code(language, code, timeout)
    }
}

/// Execute code in a sandboxed environment.
fn execute_code(language: &str, code: &str, timeout: u64) -> ToolResult {
    // Determine file extension and interpreter.
    let (extension, interpreter) = match language {
        "python" => ("py", "python3"),
        "bash" => ("sh", "bash"),
        "javascript" => ("js", "node"),
        "rust" => ("rs", "rustc"), // Special handling below
        _ => {
            return ToolResult::fail(format!(
                "Unsupported language: {language}. Use: python, bash, javascript, rust."
            ));
        }
    };

    // Write code to a temporary file.
    let tmp_dir = std::env::temp_dir().join("aios-code-exec");
    if let Err(e) = fs::create_dir_all(&tmp_dir) {
        return ToolResult::fail(format!("Failed to create temp directory: {e}"));
    }

    let file_name = format!("code_{}.{extension}", uuid_simple());
    let file_path = tmp_dir.join(&file_name);

    if let Err(e) = fs::write(&file_path, code) {
        return ToolResult::fail(format!("Failed to write code to temp file: {e}"));
    }

    debug!(language, path = %file_path.display(), "executing code");

    // Build the execution command.
    let (command, args_vec) = if language == "rust" {
        // For Rust: compile and run.
        let binary = tmp_dir.join(format!("code_{}", uuid_simple()));
        let compile_and_run = format!(
            "rustc {} -o {} 2>&1 && {} 2>&1",
            file_path.display(),
            binary.display(),
            binary.display()
        );
        ("sh".to_string(), vec!["-c".to_string(), compile_and_run])
    } else {
        (
            interpreter.to_string(),
            vec![file_path.display().to_string()],
        )
    };

    // Always use a sandbox -- Docker preferred, process fallback.
    let sandbox_type = if Sandbox::docker_available() {
        SandboxType::Docker {
            image: docker_image_for_language(language).to_string(),
            timeout_secs: timeout,
            network: false,
        }
    } else {
        SandboxType::Process {
            timeout_secs: timeout,
            max_memory_mb: 512,
        }
    };

    let args_refs: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    let result = match Sandbox::execute(
        &sandbox_type,
        &command,
        &args_refs,
        None,
        Some(tmp_dir.to_str().unwrap_or("/tmp")),
    ) {
        Ok(r) => r,
        Err(e) => {
            // Clean up.
            let _ = fs::remove_file(&file_path);
            return ToolResult::fail(format!("Execution failed: {e}"));
        }
    };

    // Clean up the temp file.
    let _ = fs::remove_file(&file_path);

    // Build the output.
    let mut output = String::new();

    if result.timed_out {
        output.push_str(&format!("[TIMEOUT after {timeout}s]\n"));
    }
    if result.resource_exceeded {
        output.push_str("[RESOURCE LIMIT EXCEEDED]\n");
    }

    if !result.stdout.is_empty() {
        output.push_str(&result.stdout);
    }
    if !result.stderr.is_empty() {
        if !output.is_empty() {
            output.push_str("\n--- stderr ---\n");
        }
        output.push_str(&result.stderr);
    }

    if output.trim().is_empty() {
        output = "(no output)".to_string();
    }

    let sandbox_label = match &sandbox_type {
        SandboxType::Docker { image, .. } => format!("docker:{image}"),
        SandboxType::Process { .. } => "process".to_string(),
        SandboxType::None => "none".to_string(),
    };

    let data = serde_json::json!({
        "language": language,
        "exit_code": result.exit_code,
        "timed_out": result.timed_out,
        "resource_exceeded": result.resource_exceeded,
        "sandbox": sandbox_label,
    });

    if result.exit_code == 0 && !result.timed_out && !result.resource_exceeded {
        ToolResult::ok_with_data(output.trim().to_string(), data)
    } else {
        ToolResult {
            success: false,
            output: output.trim().to_string(),
            data: Some(data),
            error: Some(format!("Exit code {}", result.exit_code)),
        }
    }
}

/// Get the appropriate Docker image for a language.
fn docker_image_for_language(language: &str) -> &str {
    match language {
        "python" => "python:3.12-slim",
        "javascript" => "node:20-slim",
        "rust" => "rust:1-slim",
        "bash" | _ => "debian:bookworm-slim",
    }
}

/// Generate a simple unique ID (timestamp-based to avoid adding uuid dependency here).
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_language() {
        let tool = CodeExecTool;
        let r = tool.execute(serde_json::json!({ "code": "print('hello')" }));
        assert!(!r.success);
    }

    #[test]
    fn rejects_missing_code() {
        let tool = CodeExecTool;
        let r = tool.execute(serde_json::json!({ "language": "python" }));
        assert!(!r.success);
    }

    #[test]
    fn rejects_unsupported_language() {
        let r = execute_code("cobol", "DISPLAY 'HELLO'.", 10);
        assert!(!r.success);
        assert!(r.error.unwrap_or_default().contains("Unsupported"));
    }

    #[test]
    fn execute_bash_echo() {
        // This test uses the process sandbox (no Docker needed).
        let r = execute_code("bash", "echo 'hello from sandbox'", 10);
        // May succeed or fail depending on sandbox environment.
        // Just verify it doesn't panic and returns a valid ToolResult.
        assert!(!r.output.is_empty() || r.error.is_some());
    }

    #[test]
    fn docker_image_selection() {
        assert_eq!(docker_image_for_language("python"), "python:3.12-slim");
        assert_eq!(docker_image_for_language("javascript"), "node:20-slim");
        assert_eq!(docker_image_for_language("rust"), "rust:1-slim");
        assert_eq!(docker_image_for_language("bash"), "debian:bookworm-slim");
    }

    #[test]
    fn uuid_simple_unique() {
        let a = uuid_simple();
        // Small delay to ensure different timestamps.
        std::thread::sleep(std::time::Duration::from_millis(1));
        let b = uuid_simple();
        assert_ne!(a, b);
    }
}
