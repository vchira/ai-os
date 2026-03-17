//! Drive detection and enumeration for the AiOS installer.
//!
//! Uses `lsblk` to discover block devices and their partitions,
//! identifying the live media so the UI can exclude or warn about it.

use serde::Deserialize;

/// Information about a block device (drive) on the system.
#[derive(Debug, Clone)]
pub struct DriveInfo {
    /// Device path (e.g. "/dev/sda").
    pub device: String,
    /// Human-readable model name (e.g. "Samsung SSD 860 EVO").
    pub model: String,
    /// Total size in bytes.
    pub size_bytes: u64,
    /// Human-readable size (e.g. "500 GB").
    pub size_human: String,
    /// Whether this is the drive the live USB is running from.
    pub is_live_media: bool,
    /// Existing partitions on this drive.
    pub partitions: Vec<PartitionInfo>,
}

/// Information about an existing partition.
#[derive(Debug, Clone)]
pub struct PartitionInfo {
    /// Partition device path (e.g. "/dev/sda1").
    pub device: String,
    /// Filesystem type (e.g. "ext4", "ntfs", "fat32").
    pub fstype: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Human-readable size.
    pub size_human: String,
    /// Mount point, if any.
    pub mount_point: Option<String>,
    /// Label, if any.
    pub label: Option<String>,
}

/// Raw JSON output from `lsblk --json`.
#[derive(Deserialize)]
struct LsblkOutput {
    blockdevices: Vec<LsblkDevice>,
}

#[derive(Deserialize)]
struct LsblkDevice {
    name: String,
    size: Option<u64>,
    model: Option<String>,
    #[serde(rename = "type")]
    device_type: Option<String>,
    fstype: Option<String>,
    mountpoint: Option<String>,
    label: Option<String>,
    #[serde(default)]
    children: Option<Vec<LsblkDevice>>,
}

/// Paths that indicate a partition belongs to the live media.
const LIVE_MOUNT_PREFIXES: &[&str] = &[
    "/lib/live/mount/",
    "/run/live/",
    "/cdrom",
];

/// Enumerate all block devices suitable for installation.
///
/// Uses `lsblk --json --bytes` for reliable parsing. Filters out loop devices
/// and CD-ROMs. Marks the live media drive based on mount points.
pub fn list_drives() -> Vec<DriveInfo> {
    let output = match std::process::Command::new("lsblk")
        .args(["-J", "-b", "-o", "NAME,SIZE,MODEL,TYPE,FSTYPE,MOUNTPOINT,LABEL", "--paths"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = match String::from_utf8(output.stdout) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let parsed: LsblkOutput = match serde_json::from_str(&stdout) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    let mut drives = Vec::new();

    for dev in &parsed.blockdevices {
        let dev_type = dev.device_type.as_deref().unwrap_or("");
        if dev_type != "disk" {
            continue;
        }

        let size_bytes = dev.size.unwrap_or(0);
        let model = dev.model.as_deref().unwrap_or("").trim().to_string();

        // Gather partitions from children
        let mut partitions = Vec::new();
        let mut is_live = false;

        if let Some(children) = &dev.children {
            for child in children {
                let child_type = child.device_type.as_deref().unwrap_or("");
                if child_type != "part" {
                    continue;
                }

                let mount = child.mountpoint.as_deref();

                // Check if this partition is part of the live media
                if let Some(mp) = mount {
                    if LIVE_MOUNT_PREFIXES.iter().any(|prefix| mp.starts_with(prefix)) {
                        is_live = true;
                    }
                }

                let part_size = child.size.unwrap_or(0);
                partitions.push(PartitionInfo {
                    device: child.name.clone(),
                    fstype: child.fstype.as_deref().unwrap_or("").to_string(),
                    size_bytes: part_size,
                    size_human: format_size(part_size),
                    mount_point: child.mountpoint.clone(),
                    label: child.label.clone(),
                });
            }
        }

        drives.push(DriveInfo {
            device: dev.name.clone(),
            model: model.clone(),
            size_bytes,
            size_human: format_size(size_bytes),
            is_live_media: is_live,
            partitions,
        });
    }

    drives
}

/// Format bytes as a human-readable string (e.g. "500.0 GB", "1.2 TB").
///
/// Uses TB for >= 1 TB, GB for >= 1 GB, MB otherwise. One decimal place.
pub fn format_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000_000 {
        format!("{:.1} TB", bytes as f64 / 1_000_000_000_000.0)
    } else if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    }
}

/// Generate a human-readable drive description for the UI dropdown.
///
/// Examples:
/// - "Samsung SSD 860 EVO — 500.0 GB (/dev/sda)"
/// - "Unknown Drive — 256.0 GB (/dev/nvme0n1)"
pub fn drive_display_name(drive: &DriveInfo) -> String {
    let model = if drive.model.is_empty() {
        "Unknown Drive"
    } else {
        &drive.model
    };
    format!("{} \u{2014} {} ({})", model, drive.size_human, drive.device)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(500_000_000), "500.0 MB");
        assert_eq!(format_size(0), "0.0 MB");
        assert_eq!(format_size(1_000_000), "1.0 MB");
    }

    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(1_000_000_000), "1.0 GB");
        assert_eq!(format_size(500_000_000_000), "500.0 GB");
        assert_eq!(format_size(256_000_000_000), "256.0 GB");
    }

    #[test]
    fn test_format_size_tb() {
        assert_eq!(format_size(1_000_000_000_000), "1.0 TB");
        assert_eq!(format_size(2_000_000_000_000), "2.0 TB");
        assert_eq!(format_size(1_500_000_000_000), "1.5 TB");
    }

    #[test]
    fn test_drive_display_name_with_model() {
        let drive = DriveInfo {
            device: "/dev/sda".into(),
            model: "Samsung SSD 860".into(),
            size_bytes: 500_000_000_000,
            size_human: "500.0 GB".into(),
            is_live_media: false,
            partitions: vec![],
        };
        assert_eq!(drive_display_name(&drive), "Samsung SSD 860 \u{2014} 500.0 GB (/dev/sda)");
    }

    #[test]
    fn test_drive_display_name_unknown() {
        let drive = DriveInfo {
            device: "/dev/nvme0n1".into(),
            model: String::new(),
            size_bytes: 256_000_000_000,
            size_human: "256.0 GB".into(),
            is_live_media: false,
            partitions: vec![],
        };
        assert_eq!(drive_display_name(&drive), "Unknown Drive \u{2014} 256.0 GB (/dev/nvme0n1)");
    }

    #[test]
    fn test_list_drives_no_panic() {
        // On a dev machine, this should not panic. It may return drives or an
        // empty list depending on the environment.
        let drives = list_drives();
        // Just verify we got a valid Vec back
        let _ = drives.len();
    }
}
