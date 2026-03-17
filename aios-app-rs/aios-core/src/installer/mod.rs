//! Hard drive installer -- live ISO detection, drive management, partitioning,
//! filesystem copy, bootloader installation, and locale configuration.

pub mod drives;
pub mod locale;
pub mod partition;

// Re-export the most commonly used items.
pub use drives::{DriveInfo, PartitionInfo, drive_display_name, format_size, list_drives};
pub use locale::{CountryDefaults, all_countries, country_by_code, country_by_name, detect_country};
pub use partition::{
    PartitionPlan, PartitionResult, PartitionRole, PlannedPartition, execute_partition_plan,
    is_uefi, plan_partitions,
};

/// Returns `true` if the system is running from a live ISO (overlay/squashfs/tmpfs root).
///
/// Queries the filesystem type of `/` via `findmnt`. Live ISOs typically use
/// overlay, squashfs, or tmpfs as the root filesystem.
pub fn is_live_iso() -> bool {
    std::process::Command::new("findmnt")
        .args(["-n", "-o", "FSTYPE", "/"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|fs| {
            let fs = fs.trim();
            fs.contains("overlay") || fs.contains("squashfs") || fs.contains("tmpfs")
        })
        .unwrap_or(false)
}

/// Locate the squashfs filesystem image on the live media.
///
/// Searches common live-build paths. Returns `None` if no squashfs is found
/// (i.e. not running from a live ISO).
pub fn find_squashfs() -> Option<std::path::PathBuf> {
    let candidates = [
        "/lib/live/mount/medium/live/filesystem.squashfs",
        "/run/live/medium/live/filesystem.squashfs",
        "/cdrom/live/filesystem.squashfs",
    ];

    for path in &candidates {
        let p = std::path::Path::new(path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_live_iso_returns_bool() {
        // On a dev machine this should return false without panicking.
        let result = is_live_iso();
        // We're on a dev machine, not a live ISO.
        assert!(!result, "Dev machine should not be detected as live ISO");
    }

    #[test]
    fn test_find_squashfs_dev_machine() {
        // On a dev machine, no squashfs should be found.
        assert!(find_squashfs().is_none());
    }
}
