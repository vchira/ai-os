//! Filesystem copy for the AiOS installer.
//!
//! Mounts partitions and copies the live filesystem via `unsquashfs`.
//! All functions require root privileges (run via `sudo`).

use std::path::Path;

/// Default mount point for the installation target.
pub const INSTALL_MOUNT_POINT: &str = "/mnt/aios-install";

/// Mount a partition at a given mount point.
///
/// Creates the mount point directory if it does not exist.
/// Optionally accepts a filesystem type hint.
///
/// # Note
/// Requires root — all mount operations run via `sudo`.
pub fn mount(device: &str, mount_point: &str, fstype: Option<&str>) -> Result<(), String> {
    std::fs::create_dir_all(mount_point)
        .map_err(|e| format!("Failed to create mount point {mount_point}: {e}"))?;

    let mut args = vec!["mount"];
    if let Some(fs) = fstype {
        args.push("-t");
        args.push(fs);
    }
    args.push(device);
    args.push(mount_point);

    let output = std::process::Command::new("sudo")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run mount: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("mount {device} on {mount_point} failed: {stderr}"));
    }

    Ok(())
}

/// Unmount a mount point.
///
/// # Note
/// Requires root — runs via `sudo umount`.
pub fn unmount(mount_point: &str) -> Result<(), String> {
    let output = std::process::Command::new("sudo")
        .args(["umount", mount_point])
        .output()
        .map_err(|e| format!("Failed to run umount: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("umount {mount_point} failed: {stderr}"));
    }

    Ok(())
}

/// Copy the live filesystem to the target root partition.
///
/// Steps:
/// 1. Mount root partition at [`INSTALL_MOUNT_POINT`]
/// 2. Run `unsquashfs -f -d <mount_point> <squashfs_path>` to extract the live filesystem
/// 3. If UEFI, mount EFI partition at `<mount_point>/boot/efi`
///
/// Returns the mount point path (for subsequent configuration steps).
/// Does **not** unmount — the caller handles cleanup.
///
/// # Note
/// Requires root — all operations run via `sudo`.
pub fn copy_filesystem(
    root_device: &str,
    efi_device: Option<&str>,
    squashfs_path: &Path,
    progress: impl Fn(&str),
) -> Result<String, String> {
    let target = INSTALL_MOUNT_POINT;

    // Step 1: Mount root partition
    progress("Mounting root partition...");
    mount(root_device, target, Some("ext4"))?;

    // Step 2: Extract the squashfs filesystem
    let squashfs_str = squashfs_path
        .to_str()
        .ok_or_else(|| "Invalid squashfs path (non-UTF-8)".to_string())?;

    progress("Copying system files... (this may take a few minutes)");

    let output = std::process::Command::new("sudo")
        .args(["unsquashfs", "-f", "-d", target, squashfs_str])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run unsquashfs: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Try to unmount before returning the error
        let _ = unmount(target);
        return Err(format!("unsquashfs failed: {stderr}"));
    }

    progress("System files copied successfully");

    // Step 3: If UEFI, mount EFI partition
    if let Some(efi_dev) = efi_device {
        let efi_mount = format!("{target}/boot/efi");
        std::fs::create_dir_all(&efi_mount)
            .map_err(|e| format!("Failed to create EFI mount point: {e}"))?;
        progress("Mounting EFI partition...");
        mount(efi_dev, &efi_mount, Some("vfat"))?;
    }

    Ok(target.to_string())
}

#[cfg(test)]
mod tests {
    // These functions require root and real block devices, so they can only be
    // tested in an integration/VM environment. The tests here just verify
    // compilation and basic logic.

    #[test]
    fn test_install_mount_point_is_absolute() {
        assert!(super::INSTALL_MOUNT_POINT.starts_with('/'));
    }
}
