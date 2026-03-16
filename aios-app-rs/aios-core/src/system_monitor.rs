//! System information gathering and process monitoring.
//!
//! Reads system state from `/proc` and other Linux filesystem interfaces
//! without any external crate dependencies.

use serde::{Deserialize, Serialize};
use std::fs;

// ---------------------------------------------------------------------------
// ProcessInfo
// ---------------------------------------------------------------------------

/// Information about a running process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Process ID.
    pub pid: u32,
    /// Process name (comm).
    pub name: String,
    /// Approximate CPU usage as a percentage (0.0–100.0).
    pub cpu_percent: f32,
    /// Resident memory in kilobytes.
    pub memory_kb: u64,
    /// Owner username.
    pub user: String,
}

// ---------------------------------------------------------------------------
// SystemInfo
// ---------------------------------------------------------------------------

/// System information snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Machine hostname.
    pub hostname: String,
    /// OS version string (e.g. from `/etc/os-release`).
    pub os_version: String,
    /// Kernel version string.
    pub kernel: String,
    /// System uptime in seconds.
    pub uptime_secs: u64,
    /// Number of logical CPU cores.
    pub cpu_count: usize,
    /// Total physical memory in kilobytes.
    pub total_memory_kb: u64,
    /// Used memory in kilobytes.
    pub used_memory_kb: u64,
    /// Total disk space on the root filesystem in gigabytes.
    pub disk_total_gb: f64,
    /// Used disk space on the root filesystem in gigabytes.
    pub disk_used_gb: f64,
    /// Top processes sorted by CPU usage (descending), limited to 15.
    pub processes: Vec<ProcessInfo>,
}

impl SystemInfo {
    /// Gather current system information by reading `/proc` and related files.
    pub fn gather() -> Self {
        let hostname = read_hostname();
        let os_version = read_os_version();
        let kernel = read_kernel_version();
        let uptime_secs = read_uptime_secs();
        let cpu_count = read_cpu_count();
        let (total_memory_kb, used_memory_kb) = read_memory_info();
        let (disk_total_gb, disk_used_gb) = read_disk_usage();
        let mut processes = read_processes();

        // Sort by CPU descending, then by memory descending as tiebreaker.
        processes.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory_kb.cmp(&a.memory_kb))
        });
        processes.truncate(15);

        SystemInfo {
            hostname,
            os_version,
            kernel,
            uptime_secs,
            cpu_count,
            total_memory_kb,
            used_memory_kb,
            disk_total_gb,
            disk_used_gb,
            processes,
        }
    }

    /// Kill a process by PID.
    pub fn kill_process(pid: u32) -> Result<(), String> {
        use std::process::Command;
        let output = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output()
            .map_err(|e| format!("Failed to kill process: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// Format the system information as a human-readable text report.
    pub fn format_text(&self) -> String {
        let uptime_str = format_uptime(self.uptime_secs);

        let mem_pct = if self.total_memory_kb > 0 {
            (self.used_memory_kb as f64 / self.total_memory_kb as f64) * 100.0
        } else {
            0.0
        };
        let mem_bar = progress_bar(mem_pct, 20);

        let disk_pct = if self.disk_total_gb > 0.0 {
            (self.disk_used_gb / self.disk_total_gb) * 100.0
        } else {
            0.0
        };
        let disk_bar = progress_bar(disk_pct, 20);

        let total_mem_mb = self.total_memory_kb / 1024;
        let used_mem_mb = self.used_memory_kb / 1024;

        let mut out = String::new();

        out.push_str("System Monitor\n");
        out.push_str("========================================\n\n");

        out.push_str(&format!("Hostname:  {}\n", self.hostname));
        out.push_str(&format!("OS:        {}\n", self.os_version));
        out.push_str(&format!("Kernel:    {}\n", self.kernel));
        out.push_str(&format!("Uptime:    {uptime_str}\n"));
        out.push_str(&format!("CPUs:      {}\n", self.cpu_count));

        out.push('\n');
        out.push_str(&format!(
            "Memory:    {mem_bar}  {used_mem_mb} / {total_mem_mb} MB ({mem_pct:.1}%)\n"
        ));
        out.push_str(&format!(
            "Disk (/):  {disk_bar}  {:.1} / {:.1} GB ({disk_pct:.1}%)\n",
            self.disk_used_gb, self.disk_total_gb
        ));

        if !self.processes.is_empty() {
            out.push('\n');
            out.push_str("Top Processes\n");
            out.push_str("----------------------------------------\n");
            out.push_str(&format!(
                "{:<7} {:<20} {:>6} {:>10} {}\n",
                "PID", "NAME", "CPU%", "MEM(KB)", "USER"
            ));
            for p in &self.processes {
                out.push_str(&format!(
                    "{:<7} {:<20} {:>5.1}% {:>10} {}\n",
                    p.pid,
                    truncate_str(&p.name, 20),
                    p.cpu_percent,
                    p.memory_kb,
                    p.user
                ));
            }
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Helpers — reading /proc
// ---------------------------------------------------------------------------

/// Read the hostname from `/etc/hostname` or fall back to "unknown".
fn read_hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Read the OS version from `/etc/os-release`.
fn read_os_version() -> String {
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                return val.trim_matches('"').to_string();
            }
        }
    }
    "Linux".to_string()
}

/// Read the kernel version from `/proc/version`.
fn read_kernel_version() -> String {
    if let Ok(content) = fs::read_to_string("/proc/version") {
        // The first three words are typically "Linux version X.Y.Z-..."
        let parts: Vec<&str> = content.split_whitespace().take(3).collect();
        if parts.len() >= 3 {
            return parts[2].to_string();
        }
        return content.trim().to_string();
    }
    "unknown".to_string()
}

/// Read system uptime in seconds from `/proc/uptime`.
fn read_uptime_secs() -> u64 {
    if let Ok(content) = fs::read_to_string("/proc/uptime") {
        if let Some(first) = content.split_whitespace().next() {
            if let Ok(secs) = first.parse::<f64>() {
                return secs as u64;
            }
        }
    }
    0
}

/// Count the number of logical CPUs by counting "processor" lines in `/proc/cpuinfo`.
fn read_cpu_count() -> usize {
    if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
        return content
            .lines()
            .filter(|line| line.starts_with("processor"))
            .count();
    }
    1
}

/// Read total and used memory from `/proc/meminfo`.
///
/// Returns `(total_kb, used_kb)`. Used = Total - Available (or Total - Free
/// if MemAvailable is not present).
fn read_memory_info() -> (u64, u64) {
    let mut total: u64 = 0;
    let mut available: Option<u64> = None;
    let mut free: u64 = 0;

    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("MemTotal:") {
                total = parse_meminfo_kb(val);
            } else if let Some(val) = line.strip_prefix("MemAvailable:") {
                available = Some(parse_meminfo_kb(val));
            } else if let Some(val) = line.strip_prefix("MemFree:") {
                free = parse_meminfo_kb(val);
            }
        }
    }

    let used = total.saturating_sub(available.unwrap_or(free));
    (total, used)
}

/// Parse a value like "  16384000 kB" into a u64 of kilobytes.
fn parse_meminfo_kb(val: &str) -> u64 {
    val.split_whitespace()
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Read disk usage of the root filesystem by parsing `df /` output.
///
/// Returns `(total_gb, used_gb)`.
fn read_disk_usage() -> (f64, f64) {
    if let Ok(output) = std::process::Command::new("df")
        .args(["--block-size=1", "/"])
        .output()
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            // Second line: Filesystem  1B-blocks  Used  Available  Use%  Mounted on
            if let Some(line) = stdout.lines().nth(1) {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() >= 4 {
                    let total_bytes: f64 =
                        cols[1].parse().unwrap_or(0.0);
                    let used_bytes: f64 =
                        cols[2].parse().unwrap_or(0.0);
                    let total_gb = total_bytes / (1024.0 * 1024.0 * 1024.0);
                    let used_gb = used_bytes / (1024.0 * 1024.0 * 1024.0);
                    return (total_gb, used_gb);
                }
            }
        }
    }
    (0.0, 0.0)
}

/// Read process information from `/proc/[pid]/`.
///
/// For each numeric directory in `/proc/`, reads:
/// - `/proc/[pid]/stat` for pid, comm, and utime+stime
/// - `/proc/[pid]/status` for VmRSS (resident memory) and Uid
fn read_processes() -> Vec<ProcessInfo> {
    let mut procs = Vec::new();
    let uptime_ticks = read_uptime_ticks();
    let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;

    let proc_dir = match fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return procs,
    };

    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only numeric directories are PIDs.
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let base = format!("/proc/{pid}");

        // Read /proc/[pid]/stat for comm and CPU time.
        let stat_path = format!("{base}/stat");
        let stat_content = match fs::read_to_string(&stat_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let (comm, cpu_percent) = parse_proc_stat(&stat_content, uptime_ticks, ticks_per_sec);

        // Read /proc/[pid]/status for VmRSS and Uid.
        let status_path = format!("{base}/status");
        let (memory_kb, uid) = if let Ok(status) = fs::read_to_string(&status_path) {
            parse_proc_status(&status)
        } else {
            (0, 0)
        };

        let user = uid_to_username(uid);

        procs.push(ProcessInfo {
            pid,
            name: comm,
            cpu_percent,
            memory_kb,
            user,
        });
    }

    procs
}

/// Read total uptime in clock ticks (uptime_secs * CLK_TCK).
fn read_uptime_ticks() -> f64 {
    let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    let uptime_secs = read_uptime_secs() as f64;
    uptime_secs * ticks_per_sec
}

/// Parse `/proc/[pid]/stat` to extract comm and approximate CPU%.
///
/// The format is: `pid (comm) state ppid ... utime stime ...`
/// Fields 14 and 15 (0-indexed) are utime and stime in clock ticks.
/// Field 22 is starttime.
fn parse_proc_stat(content: &str, uptime_ticks: f64, ticks_per_sec: f64) -> (String, f32) {
    // The comm field is enclosed in parentheses and may contain spaces.
    let open = match content.find('(') {
        Some(i) => i,
        None => return ("?".to_string(), 0.0),
    };
    let close = match content.rfind(')') {
        Some(i) => i,
        None => return ("?".to_string(), 0.0),
    };

    let comm = content[open + 1..close].to_string();
    let rest = &content[close + 2..]; // skip ") "
    let fields: Vec<&str> = rest.split_whitespace().collect();

    // After the closing paren, fields are indexed from 0:
    //   0=state, 1=ppid, ..., 11=utime, 12=stime, ..., 19=starttime
    if fields.len() < 20 {
        return (comm, 0.0);
    }

    let utime: f64 = fields[11].parse().unwrap_or(0.0);
    let stime: f64 = fields[12].parse().unwrap_or(0.0);
    let starttime: f64 = fields[19].parse().unwrap_or(0.0);

    let total_time = utime + stime;
    let elapsed = uptime_ticks - starttime;

    let cpu_percent = if elapsed > 0.0 && ticks_per_sec > 0.0 {
        (total_time / elapsed) * 100.0
    } else {
        0.0
    };

    (comm, cpu_percent as f32)
}

/// Parse `/proc/[pid]/status` for VmRSS and Uid.
///
/// Returns `(memory_kb, uid)`.
fn parse_proc_status(content: &str) -> (u64, u32) {
    let mut memory_kb: u64 = 0;
    let mut uid: u32 = 0;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("VmRSS:") {
            memory_kb = parse_meminfo_kb(val);
        } else if let Some(val) = line.strip_prefix("Uid:") {
            // Uid line has: real effective saved fs
            if let Some(first) = val.split_whitespace().next() {
                uid = first.parse().unwrap_or(0);
            }
        }
    }

    (memory_kb, uid)
}

/// Map a UID to a username by reading `/etc/passwd`.
fn uid_to_username(uid: u32) -> String {
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() >= 3 {
                if let Ok(line_uid) = fields[2].parse::<u32>() {
                    if line_uid == uid {
                        return fields[0].to_string();
                    }
                }
            }
        }
    }
    uid.to_string()
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Format uptime seconds as "Xd Xh Xm".
fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Build a text progress bar: `[=========>          ]`.
fn progress_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;

    let bar_char = '=';
    let tip = if filled > 0 && filled < width {
        ">"
    } else {
        ""
    };
    let fill: String = std::iter::repeat(bar_char).take(if tip.is_empty() { filled } else { filled.saturating_sub(1) }).collect();
    let space: String = std::iter::repeat(' ').take(if tip.is_empty() { empty } else { empty }) .collect();

    format!("[{fill}{tip}{space}]")
}

/// Truncate a string to `max_len` characters, appending "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = max_len.saturating_sub(3);
        format!("{}...", &s[..end])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_minutes_only() {
        assert_eq!(format_uptime(300), "5m");
    }

    #[test]
    fn format_uptime_hours_minutes() {
        assert_eq!(format_uptime(3720), "1h 2m");
    }

    #[test]
    fn format_uptime_days() {
        assert_eq!(format_uptime(90061), "1d 1h 1m");
    }

    #[test]
    fn progress_bar_empty() {
        let bar = progress_bar(0.0, 10);
        assert_eq!(bar, "[          ]");
    }

    #[test]
    fn progress_bar_full() {
        let bar = progress_bar(100.0, 10);
        assert_eq!(bar, "[==========]");
    }

    #[test]
    fn progress_bar_half() {
        let bar = progress_bar(50.0, 10);
        assert_eq!(bar, "[====>     ]");
    }

    #[test]
    fn truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn truncate_str_long() {
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn process_info_serializes() {
        let p = ProcessInfo {
            pid: 42,
            name: "test_proc".to_string(),
            cpu_percent: 12.5,
            memory_kb: 1024,
            user: "root".to_string(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"pid\":42"));
        assert!(json.contains("\"name\":\"test_proc\""));
        assert!(json.contains("\"cpu_percent\":12.5"));
        assert!(json.contains("\"memory_kb\":1024"));
        assert!(json.contains("\"user\":\"root\""));
    }

    #[test]
    fn process_info_deserializes() {
        let json = r#"{"pid":1,"name":"init","cpu_percent":0.1,"memory_kb":2048,"user":"root"}"#;
        let p: ProcessInfo = serde_json::from_str(json).unwrap();
        assert_eq!(p.pid, 1);
        assert_eq!(p.name, "init");
        assert!((p.cpu_percent - 0.1).abs() < 0.01);
        assert_eq!(p.memory_kb, 2048);
        assert_eq!(p.user, "root");
    }

    #[test]
    fn system_info_format_text_contains_sections() {
        let info = SystemInfo {
            hostname: "test-host".to_string(),
            os_version: "Debian 12".to_string(),
            kernel: "6.1.0".to_string(),
            uptime_secs: 3661,
            cpu_count: 4,
            total_memory_kb: 8_000_000,
            used_memory_kb: 4_000_000,
            disk_total_gb: 100.0,
            disk_used_gb: 40.0,
            processes: vec![
                ProcessInfo {
                    pid: 1,
                    name: "systemd".to_string(),
                    cpu_percent: 0.5,
                    memory_kb: 8192,
                    user: "root".to_string(),
                },
                ProcessInfo {
                    pid: 100,
                    name: "aios-gtk".to_string(),
                    cpu_percent: 2.3,
                    memory_kb: 65536,
                    user: "user".to_string(),
                },
            ],
        };

        let text = info.format_text();

        assert!(text.contains("System Monitor"));
        assert!(text.contains("test-host"));
        assert!(text.contains("Debian 12"));
        assert!(text.contains("6.1.0"));
        assert!(text.contains("1h 1m"));
        assert!(text.contains("4"));
        assert!(text.contains("Memory:"));
        assert!(text.contains("Disk (/):"));
        assert!(text.contains("Top Processes"));
        assert!(text.contains("systemd"));
        assert!(text.contains("aios-gtk"));
        assert!(text.contains("PID"));
        assert!(text.contains("NAME"));
        assert!(text.contains("CPU%"));
    }

    #[test]
    fn system_info_serializes() {
        let info = SystemInfo {
            hostname: "h".to_string(),
            os_version: "os".to_string(),
            kernel: "k".to_string(),
            uptime_secs: 100,
            cpu_count: 2,
            total_memory_kb: 1000,
            used_memory_kb: 500,
            disk_total_gb: 50.0,
            disk_used_gb: 25.0,
            processes: vec![],
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: SystemInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.hostname, "h");
        assert_eq!(back.uptime_secs, 100);
        assert_eq!(back.cpu_count, 2);
        assert!(back.processes.is_empty());
    }

    #[test]
    fn parse_meminfo_kb_works() {
        assert_eq!(parse_meminfo_kb("  16384000 kB"), 16384000);
        assert_eq!(parse_meminfo_kb("0 kB"), 0);
        assert_eq!(parse_meminfo_kb("garbage"), 0);
    }
}
