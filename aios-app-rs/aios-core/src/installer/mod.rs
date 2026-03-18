//! Hard drive installer -- live ISO detection, drive management, partitioning,
//! filesystem copy, bootloader installation, and locale configuration.

pub mod bootloader;
pub mod configure;
pub mod copy;
pub mod drives;
pub mod locale;
pub mod partition;

// Re-export the most commonly used items.
pub use bootloader::install_grub;
pub use configure::{cleanup_mounts, configure_system, write_fstab};
pub use copy::copy_filesystem;
pub use drives::{DriveInfo, PartitionInfo, drive_display_name, format_size, list_drives};
pub use locale::{CountryDefaults, all_countries, country_by_code, country_by_name, detect_country};
pub use partition::{
    PartitionPlan, PartitionResult, PartitionRole, PlannedPartition, execute_partition_plan,
    is_uefi, plan_partitions,
};

/// Write a file via `sudo tee` (needed for root-owned target filesystems).
pub fn sudo_write(path: &str, content: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("sudo")
        .args(["tee", path])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to write {path}: {e}"))?;
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write {path}: {e}"))?;
    }
    let output = child.wait_with_output()
        .map_err(|e| format!("Failed to write {path}: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to write {path}: {stderr}"));
    }
    Ok(())
}

/// Create a directory via `sudo mkdir -p`.
pub fn sudo_mkdir(path: &str) -> Result<(), String> {
    let output = std::process::Command::new("sudo")
        .args(["mkdir", "-p", path])
        .output()
        .map_err(|e| format!("Failed to create directory {path}: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to create directory {path}: {stderr}"));
    }
    Ok(())
}

/// Remove a file via `sudo rm -f`.
pub fn sudo_rm(path: &str) -> Result<(), String> {
    let _ = std::process::Command::new("sudo")
        .args(["rm", "-f", path])
        .output();
    Ok(())
}

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

// ---------------------------------------------------------------------------
// Install orchestrator types
// ---------------------------------------------------------------------------

/// All parameters needed to perform a hard drive installation.
#[derive(Debug, Clone)]
pub struct InstallConfig {
    /// Target drive device path (e.g. "/dev/sda").
    pub target_device: String,
    /// Total size of the target drive in bytes.
    pub target_size_bytes: u64,
    /// Country defaults (locale, keyboard, timezone).
    pub country: locale::CountryDefaults,
    /// Hostname for the new system.
    pub hostname: String,
    /// Master password (for vault, SSH, user account).
    pub master_password: String,
    /// AI assistant name.
    pub assistant_name: String,
    /// Wake word.
    pub wake_word: String,
    /// LLM provider configurations.
    pub providers: Vec<ProviderConfig>,
}

/// LLM provider configuration for the installed system.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub name: String,
    pub api_key: String,
}

/// Installation phase for progress reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallPhase {
    Partitioning,
    Formatting,
    CopyingFiles,
    InstallingBootloader,
    ConfiguringSystem,
    CreatingVault,
    SettingUpUser,
    Cleanup,
    Complete,
}

impl InstallPhase {
    /// Human-readable description for the UI.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Partitioning => "Partitioning drive...",
            Self::Formatting => "Formatting partitions...",
            Self::CopyingFiles => "Copying system files...",
            Self::InstallingBootloader => "Installing bootloader...",
            Self::ConfiguringSystem => "Configuring system...",
            Self::CreatingVault => "Creating vault...",
            Self::SettingUpUser => "Setting up user account...",
            Self::Cleanup => "Cleaning up...",
            Self::Complete => "Installation complete!",
        }
    }
}

// ---------------------------------------------------------------------------
// Install orchestrator
// ---------------------------------------------------------------------------

/// Run the complete installation process.
///
/// This is the single entry point called from the GTK installer step.
/// It runs all phases sequentially and reports progress via the callback.
/// Designed to be called from a background thread (not the GTK main thread).
///
/// Phases:
/// 1. Partition the target drive
/// 2. Copy the live filesystem (unsquashfs)
/// 3. Write fstab
/// 4. Install the bootloader (GRUB)
/// 5. Configure system (hostname, locale, timezone, keyboard)
/// 6. Set user/SSH passwords
/// 7. Create vault with API keys and master password
/// 8. Write AiOS config.json
/// 9. Cleanup (unmount)
pub fn run_install(
    config: &InstallConfig,
    progress: impl Fn(InstallPhase, &str) + Send,
) -> Result<(), String> {
    // Phase 1: Partition the target drive.
    progress(InstallPhase::Partitioning, "Planning partitions...");
    let plan = partition::plan_partitions(&config.target_device, config.target_size_bytes)?;

    progress(InstallPhase::Formatting, "Executing partition plan...");
    let part_result = partition::execute_partition_plan(&plan, |msg| {
        progress(InstallPhase::Formatting, msg);
    })?;

    // Phase 2: Locate the squashfs image and copy it.
    progress(InstallPhase::CopyingFiles, "Locating live filesystem...");
    let squashfs = find_squashfs()
        .ok_or_else(|| "Could not find squashfs filesystem on live media".to_string())?;

    let mount_point = copy::copy_filesystem(
        &part_result.root_device,
        part_result.efi_device.as_deref(),
        &squashfs,
        |msg| progress(InstallPhase::CopyingFiles, msg),
    )?;

    // Phase 3: Write fstab.
    progress(InstallPhase::ConfiguringSystem, "Writing fstab...");
    configure::write_fstab(&mount_point, &part_result)?;

    // Phase 4: Install bootloader.
    progress(InstallPhase::InstallingBootloader, "");
    bootloader::install_grub(&mount_point, &config.target_device, plan.is_uefi, |msg| {
        progress(InstallPhase::InstallingBootloader, msg);
    })?;

    // Phase 5: Configure system (hostname, locale, timezone, keyboard, passwords).
    progress(InstallPhase::ConfiguringSystem, "");
    configure::configure_system(
        &mount_point,
        &config.hostname,
        config.country.language,
        config.country.keyboard,
        config.country.timezone,
        &config.master_password,
        |msg| progress(InstallPhase::ConfiguringSystem, msg),
    )?;

    // Phase 6: Create the vault with API keys on the target.
    progress(InstallPhase::CreatingVault, "Creating encrypted vault...");
    create_vault_on_target(&mount_point, config)?;

    // Phase 7: Write AiOS config.json on the target.
    progress(InstallPhase::SettingUpUser, "Writing configuration...");
    write_config_on_target(&mount_point, config)?;

    // Fix ownership of the aios home directory.
    progress(InstallPhase::SettingUpUser, "Setting file ownership...");
    let _ = std::process::Command::new("sudo")
        .args(["chroot", &mount_point, "chown", "-R", "aios:aios", "/home/aios"])
        .output();

    // Phase 8: Cleanup — unmount everything.
    progress(InstallPhase::Cleanup, "Unmounting filesystems...");
    configure::cleanup_mounts(&mount_point, plan.is_uefi)?;

    progress(InstallPhase::Complete, "Installation complete!");
    Ok(())
}

/// Create the encrypted vault on the target system with the configured API keys.
fn create_vault_on_target(mount_point: &str, config: &InstallConfig) -> Result<(), String> {
    use crate::secure::{SecretEntry, SecretKind, Vault};

    let vault_dir = format!("{mount_point}/home/aios/.aios");
    sudo_mkdir(&vault_dir)?;

    let vault_path = std::path::PathBuf::from(format!("{vault_dir}/vault.enc"));
    let mut vault = Vault::new(vault_path);
    vault
        .create(&config.master_password)
        .map_err(|e| format!("Failed to create vault: {e}"))?;

    for provider in &config.providers {
        let key_name = format!("{}_api_key", provider.name.to_lowercase());
        let label = format!("{} API Key", provider.name);
        let entry = SecretEntry {
            kind: SecretKind::ApiKey,
            value: provider.api_key.clone(),
            label,
            created: chrono::Utc::now(),
            last_accessed: None,
        };
        vault
            .set(&key_name, entry)
            .map_err(|e| format!("Failed to store {} key: {e}", provider.name))?;
    }

    Ok(())
}

/// Write AiOS config.json on the target system.
fn write_config_on_target(mount_point: &str, config: &InstallConfig) -> Result<(), String> {
    use crate::config::ConfigManager;

    let config_path =
        std::path::PathBuf::from(format!("{mount_point}/home/aios/.aios/config.json"));

    let mut mgr = ConfigManager::with_path(config_path)
        .map_err(|e| format!("Failed to create config manager: {e}"))?;

    // Provider settings
    if let Some(primary) = config.providers.first() {
        let _ = mgr.set(
            "llm.provider",
            serde_json::json!(primary.name.to_lowercase()),
        );
    }

    // Assistant settings
    let _ = mgr.set(
        "assistant.name",
        serde_json::json!(config.assistant_name),
    );
    let _ = mgr.set("assistant.wake_word", serde_json::json!(config.wake_word));

    // Machine name
    let _ = mgr.set("system.hostname", serde_json::json!(config.hostname));

    // Locale settings from country
    let _ = mgr.set(
        "system.keyboard_layout",
        serde_json::json!(config.country.keyboard),
    );
    let _ = mgr.set(
        "system.timezone",
        serde_json::json!(config.country.timezone),
    );
    let _ = mgr.set(
        "system.language",
        serde_json::json!(config.country.language),
    );
    let _ = mgr.set(
        "system.time_format_24h",
        serde_json::json!(config.country.time_format_24h),
    );

    Ok(())
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

    #[test]
    fn test_install_phase_descriptions() {
        assert_eq!(InstallPhase::Partitioning.description(), "Partitioning drive...");
        assert_eq!(InstallPhase::Formatting.description(), "Formatting partitions...");
        assert_eq!(InstallPhase::CopyingFiles.description(), "Copying system files...");
        assert_eq!(InstallPhase::InstallingBootloader.description(), "Installing bootloader...");
        assert_eq!(InstallPhase::ConfiguringSystem.description(), "Configuring system...");
        assert_eq!(InstallPhase::CreatingVault.description(), "Creating vault...");
        assert_eq!(InstallPhase::SettingUpUser.description(), "Setting up user account...");
        assert_eq!(InstallPhase::Cleanup.description(), "Cleaning up...");
        assert_eq!(InstallPhase::Complete.description(), "Installation complete!");
    }

    #[test]
    fn test_install_config_constructible() {
        let config = InstallConfig {
            target_device: "/dev/sda".to_string(),
            target_size_bytes: 500_000_000_000,
            country: locale::CountryDefaults {
                country_code: "US",
                country_name: "United States",
                language: "en",
                keyboard: "us",
                timezone: "America/New_York",
                time_format_24h: false,
            },
            hostname: "aios".to_string(),
            master_password: "test123".to_string(),
            assistant_name: "Atlas".to_string(),
            wake_word: "Atlas".to_string(),
            providers: vec![ProviderConfig {
                name: "claude".to_string(),
                api_key: "sk-test".to_string(),
            }],
        };
        assert_eq!(config.target_device, "/dev/sda");
        assert_eq!(config.providers.len(), 1);
    }
}
