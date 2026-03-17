//! Bootloader installation for the AiOS installer.
//!
//! Installs GRUB (UEFI or BIOS) into the target system via chroot.
//! All functions require root privileges (run via `sudo`).

/// Install GRUB bootloader on the target system.
///
/// Must be called while the target is mounted at `mount_point`.
/// Bind-mounts `/dev`, `/proc`, `/sys` (and `/sys/firmware/efi/efivars` on UEFI)
/// into the target and runs `grub-install` + `grub-mkconfig` inside a chroot.
///
/// Unbinds all mounts on completion (even on error).
///
/// # Note
/// Requires root — all operations run via `sudo`.
pub fn install_grub(
    mount_point: &str,
    device: &str,
    is_uefi: bool,
    progress: impl Fn(&str),
) -> Result<(), String> {
    progress("Preparing chroot environment...");

    // Bind-mount virtual filesystems needed inside the chroot.
    let mut bind_mounts: Vec<(&str, String)> = vec![
        ("/dev", format!("{mount_point}/dev")),
        ("/proc", format!("{mount_point}/proc")),
        ("/sys", format!("{mount_point}/sys")),
    ];

    if is_uefi {
        bind_mounts.push((
            "/sys/firmware/efi/efivars",
            format!("{mount_point}/sys/firmware/efi/efivars"),
        ));
    }

    for (src, dst) in &bind_mounts {
        std::fs::create_dir_all(dst).ok();
        let output = std::process::Command::new("sudo")
            .args(["mount", "--bind", src, dst])
            .output()
            .map_err(|e| format!("Failed to bind-mount {src} -> {dst}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Attempt to clean up anything we already mounted.
            unbind_mounts(mount_point, is_uefi);
            return Err(format!("Bind-mount {src} -> {dst} failed: {stderr}"));
        }
    }

    // Run grub-install inside the chroot.
    let grub_result = if is_uefi {
        install_grub_uefi(mount_point, device, &progress)
    } else {
        install_grub_bios(mount_point, device, &progress)
    };

    // Generate grub config regardless of mode (but only if install succeeded).
    if grub_result.is_ok() {
        progress("Generating GRUB configuration...");
        let output = std::process::Command::new("sudo")
            .args(["chroot", mount_point, "grub-mkconfig", "-o", "/boot/grub/grub.cfg"])
            .output()
            .map_err(|e| format!("grub-mkconfig failed to start: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            unbind_mounts(mount_point, is_uefi);
            return Err(format!("grub-mkconfig failed: {stderr}"));
        }
    }

    // Always clean up bind mounts.
    unbind_mounts(mount_point, is_uefi);

    grub_result?;

    progress("Bootloader installed");
    Ok(())
}

/// Install GRUB for UEFI systems.
fn install_grub_uefi(mount_point: &str, _device: &str, progress: &dyn Fn(&str)) -> Result<(), String> {
    progress("Installing GRUB (UEFI)...");

    let output = std::process::Command::new("sudo")
        .args([
            "chroot", mount_point,
            "grub-install",
            "--target=x86_64-efi",
            "--efi-directory=/boot/efi",
            "--bootloader-id=AiOS",
            "--recheck",
        ])
        .output()
        .map_err(|e| format!("grub-install (UEFI) failed to start: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("grub-install (UEFI) failed: {stderr}"));
    }

    Ok(())
}

/// Install GRUB for BIOS/legacy systems.
fn install_grub_bios(mount_point: &str, device: &str, progress: &dyn Fn(&str)) -> Result<(), String> {
    progress("Installing GRUB (BIOS)...");

    let output = std::process::Command::new("sudo")
        .args([
            "chroot", mount_point,
            "grub-install",
            "--target=i386-pc",
            "--recheck",
            device,
        ])
        .output()
        .map_err(|e| format!("grub-install (BIOS) failed to start: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("grub-install (BIOS) failed: {stderr}"));
    }

    Ok(())
}

/// Unmount bind mounts in reverse order. Best-effort — ignores errors.
fn unbind_mounts(mount_point: &str, is_uefi: bool) {
    // Reverse order of mounting.
    let mut paths = Vec::new();

    if is_uefi {
        paths.push(format!("{mount_point}/sys/firmware/efi/efivars"));
    }
    paths.push(format!("{mount_point}/sys"));
    paths.push(format!("{mount_point}/proc"));
    paths.push(format!("{mount_point}/dev"));

    for path in &paths {
        let _ = std::process::Command::new("sudo")
            .args(["umount", path])
            .output();
    }
}

#[cfg(test)]
mod tests {
    // Bootloader installation requires root, a real disk, and a chroot
    // environment. These functions can only be tested in a VM/integration
    // environment. The test here just verifies compilation.

    #[test]
    fn test_module_compiles() {
        // Compilation check — nothing to assert at the unit level.
    }
}
