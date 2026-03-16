//! System tool — run commands, query hardware, list processes.
//!
//! Ported from `aios-app/aios/tools/builtin/system_tools.py`.

use std::fs;
use std::process::Command;
use std::time::Duration;

use aios_core::types::ToolResult;
use regex::Regex;
use tracing::{debug, warn};

use crate::sandbox::{Sandbox, SandboxType};
use crate::tool::Tool;

/// Timeout for shell commands.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Build the list of dangerous command patterns.
///
/// These patterns are compiled once and cached via `std::sync::LazyLock`.
fn dangerous_patterns() -> &'static [(Regex, &'static str)] {
    use std::sync::LazyLock;

    static PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        vec![
            (
                Regex::new(r"(?i)\brm\s+(-[a-zA-Z]*)?r").unwrap(),
                "rm -r / rm -rf",
            ),
            (Regex::new(r"(?i)\bmkfs\b").unwrap(), "mkfs"),
            (
                Regex::new(r"(?i)\bdd\b.*\bof=/dev/").unwrap(),
                "dd to device",
            ),
            (
                Regex::new(r"(?i)>\s*/dev/sd[a-z]").unwrap(),
                "write to block device",
            ),
            (Regex::new(r"(?i)\bshutdown\b").unwrap(), "shutdown"),
            (Regex::new(r"(?i)\breboot\b").unwrap(), "reboot"),
            (Regex::new(r"(?i)\binit\s+[06]\b").unwrap(), "init 0/6"),
            (
                Regex::new(r"(?i)\bsystemctl\s+(poweroff|reboot|halt)\b").unwrap(),
                "systemctl poweroff/reboot/halt",
            ),
            (
                Regex::new(r":\(\)\{ :\|:& \};:").unwrap(),
                "fork bomb",
            ),
            (
                Regex::new(r"(?i)\bchmod\s+(-[a-zA-Z]*)?\s*777\s+/").unwrap(),
                "chmod 777 /",
            ),
            (
                Regex::new(r"(?i)\bchown\s+.*\s+/").unwrap(),
                "chown /",
            ),
        ]
    });

    &PATTERNS
}

/// Check whether a command is safe to run.
///
/// Returns `Ok(())` if safe, or `Err(reason)` describing why it was blocked.
fn check_command_safety(command: &str) -> Result<(), String> {
    for (pattern, label) in dangerous_patterns() {
        if pattern.is_match(command) {
            return Err(format!("Blocked by safety rule: {label} ({pattern})"));
        }
    }
    Ok(())
}

/// System utilities: run a shell command (sandboxed), get system info,
/// get the current date/time, or list running processes.
pub struct SystemTool;

impl Tool for SystemTool {
    fn name(&self) -> &str {
        "system"
    }

    fn description(&self) -> &str {
        "System utilities: run a shell command (sandboxed), get system info, \
         get the current date/time, or list running processes."
    }

    fn category(&self) -> &str {
        "system"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["run_command", "get_system_info", "get_datetime", "list_processes"],
                    "description": "System action to perform."
                },
                "command": {
                    "type": "string",
                    "description": "Shell command to execute (for run_command)."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "run_command" => run_command(command),
            "get_system_info" => get_system_info(),
            "get_datetime" => get_datetime(),
            "list_processes" => list_processes(),
            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. \
                 Use: run_command, get_system_info, get_datetime, list_processes."
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Action implementations
// ---------------------------------------------------------------------------

/// Execute a shell command after safety checks, routed through the sandbox.
fn run_command(command: &str) -> ToolResult {
    if command.is_empty() {
        return ToolResult::fail("'command' is required for run_command.");
    }

    // Hard deny: check blocklist first (these are never allowed).
    if let Err(reason) = check_command_safety(command) {
        warn!(command, reason = %reason, "command rejected");
        return ToolResult::fail(format!("Command rejected: {reason}"));
    }

    // Classify the command and route through the appropriate sandbox.
    let sandbox_type = Sandbox::classify_command(command);
    let sandbox_label = match &sandbox_type {
        SandboxType::None => "none (direct)",
        SandboxType::Process { .. } => "process",
        SandboxType::Docker { .. } => "docker",
    };
    debug!(command, sandbox = sandbox_label, "executing via sandbox");

    let result = match Sandbox::execute(
        &sandbox_type,
        "sh",
        &["-c", command],
        None,
        None,
    ) {
        Ok(r) => r,
        Err(e) => return ToolResult::fail(format!("Failed to run command: {e}")),
    };

    // Check for timeout.
    if result.timed_out {
        return ToolResult::fail(format!(
            "Command timed out after {}s.",
            COMMAND_TIMEOUT.as_secs()
        ));
    }

    // Check for resource limit exceeded.
    if result.resource_exceeded {
        return ToolResult::fail("Command exceeded resource limits (OOM killed).");
    }

    let mut text = result.stdout.clone();
    if !result.stderr.is_empty() {
        if text.is_empty() {
            text = result.stderr.clone();
        } else {
            text.push_str("\n--- stderr ---\n");
            text.push_str(&result.stderr);
        }
    }

    let text = if text.trim().is_empty() {
        "(no output)".to_string()
    } else {
        text.trim().to_string()
    };

    let code = result.exit_code;
    if code == 0 {
        ToolResult::ok_with_data(
            text,
            serde_json::json!({ "returncode": code, "sandbox": sandbox_label }),
        )
    } else {
        ToolResult {
            success: false,
            output: text,
            data: Some(serde_json::json!({ "returncode": code, "sandbox": sandbox_label })),
            error: Some(format!("Exit code {code}")),
        }
    }
}

/// Gather system information (hostname, CPU, memory, disk, load).
fn get_system_info() -> ToolResult {
    let mut info = serde_json::Map::new();

    // Platform / architecture.
    if let Ok(uname) = Command::new("uname").arg("-a").output() {
        let platform = String::from_utf8_lossy(&uname.stdout).trim().to_string();
        info.insert("platform".into(), serde_json::Value::String(platform));
    }

    if let Ok(arch) = Command::new("uname").arg("-m").output() {
        let arch = String::from_utf8_lossy(&arch.stdout).trim().to_string();
        info.insert("architecture".into(), serde_json::Value::String(arch));
    }

    // Hostname.
    if let Ok(hostname) = hostname::get() {
        info.insert(
            "hostname".into(),
            serde_json::Value::String(hostname.to_string_lossy().to_string()),
        );
    }

    // CPU count.
    if let Some(cpus) = std::thread::available_parallelism().ok() {
        info.insert(
            "cpu_count".into(),
            serde_json::Value::Number(cpus.get().into()),
        );
    }

    // Memory from /proc/meminfo.
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                info.insert(
                    "memory_total".into(),
                    serde_json::Value::String(rest.trim().to_string()),
                );
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                info.insert(
                    "memory_available".into(),
                    serde_json::Value::String(rest.trim().to_string()),
                );
            }
        }
    }

    // Disk usage for /.
    if let Ok(stat) = nix_disk_usage("/") {
        info.insert(
            "disk_total_gb".into(),
            serde_json::json!(stat.0),
        );
        info.insert(
            "disk_used_gb".into(),
            serde_json::json!(stat.1),
        );
        info.insert(
            "disk_free_gb".into(),
            serde_json::json!(stat.2),
        );
    }

    // Load average.
    if let Ok(loadavg) = fs::read_to_string("/proc/loadavg") {
        let parts: Vec<&str> = loadavg.split_whitespace().collect();
        if parts.len() >= 3 {
            info.insert(
                "load_average".into(),
                serde_json::Value::String(format!("{}, {}, {}", parts[0], parts[1], parts[2])),
            );
        }
    }

    let lines: Vec<String> = info
        .iter()
        .map(|(k, v)| {
            let vs = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{k}: {vs}")
        })
        .collect();

    ToolResult::ok_with_data(lines.join("\n"), serde_json::Value::Object(info))
}

/// Get current date and time (local + UTC).
fn get_datetime() -> ToolResult {
    let local = chrono::Local::now();
    let utc = chrono::Utc::now();
    let tz_name = local.format("%Z").to_string();

    let text = format!(
        "Local: {}\nUTC:   {}\nTimezone: {}",
        local.format("%Y-%m-%d %H:%M:%S"),
        utc.format("%Y-%m-%d %H:%M:%S"),
        tz_name,
    );

    ToolResult::ok_with_data(
        text,
        serde_json::json!({
            "local": local.to_rfc3339(),
            "utc": utc.to_rfc3339(),
            "timezone": tz_name,
        }),
    )
}

/// List running processes sorted by memory usage.
fn list_processes() -> ToolResult {
    let result = Command::new("ps")
        .args(["aux", "--sort=-%mem"])
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                ToolResult::ok(text)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                ToolResult::fail(format!("ps failed: {stderr}"))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            ToolResult::fail("'ps' command not found on this system.")
        }
        Err(e) => ToolResult::fail(format!("Failed to list processes: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Escape a string for safe embedding inside a `sh -c '...'` invocation.
#[allow(unused)]
fn shell_escape(s: &str) -> String {
    // Wrap in single quotes and escape any embedded single quotes.
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// Get disk usage for a path using statvfs.
/// Returns `(total_gb, used_gb, free_gb)` rounded to 2 decimal places.
fn nix_disk_usage(path: &str) -> Result<(f64, f64, f64), String> {
    // Use the `df` command as a portable fallback.
    let output = Command::new("df")
        .args(["--output=size,used,avail", "-B1", path])
        .output()
        .map_err(|e| format!("df failed: {e}"))?;

    if !output.status.success() {
        return Err("df returned non-zero".to_string());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return Err("unexpected df output".to_string());
    }

    let parts: Vec<&str> = lines[1].split_whitespace().collect();
    if parts.len() < 3 {
        return Err("unexpected df output format".to_string());
    }

    let total: f64 = parts[0].parse().map_err(|_| "parse error")?;
    let used: f64 = parts[1].parse().map_err(|_| "parse error")?;
    let free: f64 = parts[2].parse().map_err(|_| "parse error")?;

    let gb = 1024.0 * 1024.0 * 1024.0;
    Ok((
        (total / gb * 100.0).round() / 100.0,
        (used / gb * 100.0).round() / 100.0,
        (free / gb * 100.0).round() / 100.0,
    ))
}

// ---------------------------------------------------------------------------
// Hostname helper (avoids pulling in nix/libc just for gethostname)
// ---------------------------------------------------------------------------

mod hostname {
    use std::ffi::OsString;

    /// Get the system hostname.
    pub fn get() -> Result<OsString, String> {
        std::fs::read_to_string("/etc/hostname")
            .map(|s| OsString::from(s.trim().to_string()))
            .or_else(|_| {
                // Fallback: use `hostname` command.
                let output = std::process::Command::new("hostname")
                    .output()
                    .map_err(|e| e.to_string())?;
                Ok(OsString::from(
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                ))
            })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_blocks_rm_rf() {
        assert!(check_command_safety("rm -rf /").is_err());
        assert!(check_command_safety("rm -r /home").is_err());
    }

    #[test]
    fn safety_blocks_mkfs() {
        assert!(check_command_safety("mkfs.ext4 /dev/sda1").is_err());
    }

    #[test]
    fn safety_blocks_dd() {
        assert!(check_command_safety("dd if=/dev/zero of=/dev/sda").is_err());
    }

    #[test]
    fn safety_blocks_shutdown() {
        assert!(check_command_safety("shutdown -h now").is_err());
        assert!(check_command_safety("reboot").is_err());
    }

    #[test]
    fn safety_blocks_systemctl_poweroff() {
        assert!(check_command_safety("systemctl poweroff").is_err());
        assert!(check_command_safety("systemctl reboot").is_err());
    }

    #[test]
    fn safety_allows_safe_commands() {
        assert!(check_command_safety("ls -la").is_ok());
        assert!(check_command_safety("echo hello").is_ok());
        assert!(check_command_safety("cat /etc/os-release").is_ok());
    }

    #[test]
    fn run_command_echo() {
        let tool = SystemTool;
        let r = tool.execute(serde_json::json!({
            "action": "run_command",
            "command": "echo hello"
        }));
        assert!(r.success);
        assert!(r.output.contains("hello"));
    }

    #[test]
    fn run_command_empty() {
        let tool = SystemTool;
        let r = tool.execute(serde_json::json!({
            "action": "run_command"
        }));
        assert!(!r.success);
    }

    #[test]
    fn run_command_blocked() {
        let tool = SystemTool;
        let r = tool.execute(serde_json::json!({
            "action": "run_command",
            "command": "rm -rf /"
        }));
        assert!(!r.success);
        assert!(r.error.unwrap_or_default().contains("rejected"));
    }

    #[test]
    fn get_datetime_works() {
        let tool = SystemTool;
        let r = tool.execute(serde_json::json!({ "action": "get_datetime" }));
        assert!(r.success);
        assert!(r.output.contains("Local:"));
        assert!(r.output.contains("UTC:"));
    }

    #[test]
    fn get_system_info_works() {
        let tool = SystemTool;
        let r = tool.execute(serde_json::json!({ "action": "get_system_info" }));
        assert!(r.success);
        // Should contain at least one piece of system info.
        assert!(!r.output.is_empty());
    }

    #[test]
    fn unknown_action() {
        let tool = SystemTool;
        let r = tool.execute(serde_json::json!({ "action": "destroy" }));
        assert!(!r.success);
    }
}
