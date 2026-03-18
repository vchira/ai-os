//! Partition planning and execution for the AiOS installer.
//!
//! Generates a partition layout (EFI + swap + root) based on the target drive
//! and system characteristics, then executes it using `sgdisk` and `mkfs`.

use super::drives::format_size;

/// A planned partition layout for installation.
#[derive(Debug, Clone)]
pub struct PartitionPlan {
    /// Target device (e.g. "/dev/sda").
    pub device: String,
    /// Whether the system boots via UEFI (true) or BIOS/legacy (false).
    pub is_uefi: bool,
    /// Planned partitions in order.
    pub partitions: Vec<PlannedPartition>,
}

/// A single planned partition.
#[derive(Debug, Clone)]
pub struct PlannedPartition {
    /// Partition purpose.
    pub role: PartitionRole,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Human-readable size.
    pub size_human: String,
    /// Filesystem to create.
    pub filesystem: &'static str,
}

/// The purpose of a planned partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionRole {
    /// BIOS Boot Partition (1 MB, unformatted — required for GRUB on BIOS+GPT).
    BiosBoot,
    /// EFI System Partition (only on UEFI systems).
    Efi,
    /// Linux swap.
    Swap,
    /// Root filesystem.
    Root,
}

impl std::fmt::Display for PartitionRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BiosBoot => write!(f, "BIOS Boot Partition"),
            Self::Efi => write!(f, "EFI System Partition"),
            Self::Swap => write!(f, "Swap"),
            Self::Root => write!(f, "Root"),
        }
    }
}

/// Result of executing a partition plan — holds device paths and UUIDs.
#[derive(Debug, Clone)]
pub struct PartitionResult {
    /// EFI partition device path (None on BIOS systems).
    pub efi_device: Option<String>,
    /// Swap partition device path.
    pub swap_device: String,
    /// Root partition device path.
    pub root_device: String,
    /// UUID of root partition (for fstab).
    pub root_uuid: String,
    /// UUID of EFI partition (for fstab), if present.
    pub efi_uuid: Option<String>,
    /// UUID of swap partition (for fstab).
    pub swap_uuid: String,
}

/// BIOS Boot Partition size: 1 MB (required for GRUB on BIOS+GPT).
const BIOS_BOOT_SIZE: u64 = 1 * 1024 * 1024;

/// EFI partition size: 512 MB.
const EFI_SIZE: u64 = 512 * 1024 * 1024;

/// Maximum swap size: 8 GB.
const MAX_SWAP: u64 = 8 * 1024 * 1024 * 1024;

/// Minimum root partition size: 8 GB.
const MIN_ROOT: u64 = 8 * 1024 * 1024 * 1024;

/// Returns `true` if the system booted via UEFI.
///
/// Checks for the existence of `/sys/firmware/efi`.
pub fn is_uefi() -> bool {
    std::path::Path::new("/sys/firmware/efi").exists()
}

/// Generate a partition plan for the given drive.
///
/// Layout:
/// - If UEFI: 512 MB EFI (FAT32) + swap (min(RAM, 8GB)) + root (remaining, ext4)
/// - If BIOS: swap (min(RAM, 8GB)) + root (remaining, ext4)
///
/// Returns `Err` if the drive is too small (root would be < 8 GB).
pub fn plan_partitions(device: &str, total_bytes: u64) -> Result<PartitionPlan, String> {
    let uefi = is_uefi();
    plan_partitions_inner(device, total_bytes, uefi, get_ram_bytes())
}

/// Inner implementation that accepts UEFI and RAM as parameters for testability.
fn plan_partitions_inner(device: &str, total_bytes: u64, uefi: bool, ram_bytes: u64) -> Result<PartitionPlan, String> {
    let boot_size = if uefi { EFI_SIZE } else { BIOS_BOOT_SIZE };
    let swap_size = ram_bytes.min(MAX_SWAP);

    let overhead = boot_size + swap_size;
    if total_bytes <= overhead {
        return Err("Not enough space for installation (need at least 10 GB)".into());
    }

    let root_size = total_bytes - overhead;
    if root_size < MIN_ROOT {
        return Err(format!(
            "Not enough space for root partition: {} available, {} required",
            format_size(root_size),
            format_size(MIN_ROOT)
        ));
    }

    let mut partitions = Vec::new();

    if uefi {
        partitions.push(PlannedPartition {
            role: PartitionRole::Efi,
            size_bytes: EFI_SIZE,
            size_human: format_size(EFI_SIZE),
            filesystem: "vfat",
        });
    } else {
        // BIOS+GPT requires a BIOS Boot Partition for GRUB to embed its core.img.
        partitions.push(PlannedPartition {
            role: PartitionRole::BiosBoot,
            size_bytes: BIOS_BOOT_SIZE,
            size_human: format_size(BIOS_BOOT_SIZE),
            filesystem: "none",
        });
    }

    partitions.push(PlannedPartition {
        role: PartitionRole::Swap,
        size_bytes: swap_size,
        size_human: format_size(swap_size),
        filesystem: "swap",
    });

    partitions.push(PlannedPartition {
        role: PartitionRole::Root,
        size_bytes: root_size,
        size_human: format_size(root_size),
        filesystem: "ext4",
    });

    Ok(PartitionPlan {
        device: device.to_string(),
        is_uefi: uefi,
        partitions,
    })
}

/// Get total RAM in bytes by reading `/proc/meminfo`.
fn get_ram_bytes() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse::<u64>().ok())
                .map(|kb| kb * 1024)
        })
        .unwrap_or(4 * 1024 * 1024 * 1024) // default 4 GB
}

/// Run a command and return its output, or an error message.
fn run_cmd(program: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run {program}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{program} failed: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Determine the partition device path from a base device and partition number.
///
/// For `/dev/sda` partition 1 -> `/dev/sda1`.
/// For `/dev/nvme0n1` partition 1 -> `/dev/nvme0n1p1`.
fn partition_device(base: &str, number: u32) -> String {
    if base.chars().last().map_or(false, |c| c.is_ascii_digit()) {
        format!("{base}p{number}")
    } else {
        format!("{base}{number}")
    }
}

/// Execute a partition plan -- wipe the drive, create GPT, partition, and format.
///
/// Calls: `sgdisk --zap-all`, `sgdisk --new`, `mkfs.fat`, `mkfs.ext4`, `mkswap`.
/// Each step reports progress via the callback.
///
/// # Safety
/// This DESTROYS all data on the target drive. The caller must have confirmed
/// with the user.
pub fn execute_partition_plan(
    plan: &PartitionPlan,
    progress: impl Fn(&str),
) -> Result<PartitionResult, String> {
    let dev = &plan.device;

    // Step 1: Wipe existing partition table
    progress("Wiping existing partition table...");
    run_cmd("sudo", &["sgdisk", "--zap-all", dev])?;

    // Step 2: Create fresh GPT
    progress("Creating GPT partition table...");
    run_cmd("sudo", &["sgdisk", "--clear", dev])?;

    let mut part_num: u32 = 0;
    let mut efi_device = None;
    let mut swap_device = String::new();
    let mut root_device = String::new();

    for planned in &plan.partitions {
        part_num += 1;
        match planned.role {
            PartitionRole::BiosBoot => {
                progress("Creating BIOS boot partition...");
                let size_mb = planned.size_bytes / (1024 * 1024);
                run_cmd("sudo", &[
                    "sgdisk",
                    &format!("--new={part_num}:0:+{size_mb}M"),
                    &format!("--typecode={part_num}:EF02"),
                    dev,
                ])?;
                // No formatting — GRUB writes directly to this partition.
            }
            PartitionRole::Efi => {
                progress("Creating EFI system partition...");
                let size_mb = planned.size_bytes / (1024 * 1024);
                run_cmd("sudo", &[
                    "sgdisk",
                    &format!("--new={part_num}:0:+{size_mb}M"),
                    &format!("--typecode={part_num}:EF00"),
                    dev,
                ])?;
                efi_device = Some(partition_device(dev, part_num));
            }
            PartitionRole::Swap => {
                progress("Creating swap partition...");
                let size_mb = planned.size_bytes / (1024 * 1024);
                run_cmd("sudo", &[
                    "sgdisk",
                    &format!("--new={part_num}:0:+{size_mb}M"),
                    &format!("--typecode={part_num}:8200"),
                    dev,
                ])?;
                swap_device = partition_device(dev, part_num);
            }
            PartitionRole::Root => {
                progress("Creating root partition...");
                run_cmd("sudo", &[
                    "sgdisk",
                    &format!("--new={part_num}:0:0"),
                    &format!("--typecode={part_num}:8300"),
                    dev,
                ])?;
                root_device = partition_device(dev, part_num);
            }
        }
    }

    // Step 3: Wait for kernel to re-read partition table
    progress("Waiting for kernel to recognize partitions...");
    let _ = run_cmd("sudo", &["partprobe", dev]);
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Step 4: Format partitions
    if let Some(ref efi_dev) = efi_device {
        progress("Formatting EFI partition (FAT32)...");
        run_cmd("sudo", &["mkfs.fat", "-F", "32", efi_dev])?;
    }

    progress("Creating swap...");
    run_cmd("sudo", &["mkswap", &swap_device])?;

    progress("Formatting root partition (ext4)...");
    run_cmd("sudo", &["mkfs.ext4", "-F", &root_device])?;

    // Step 5: Retrieve UUIDs via blkid
    progress("Reading partition UUIDs...");

    let root_uuid = get_uuid(&root_device)?;
    let swap_uuid = get_uuid(&swap_device)?;
    let efi_uuid = if let Some(ref efi_dev) = efi_device {
        Some(get_uuid(efi_dev)?)
    } else {
        None
    };

    Ok(PartitionResult {
        efi_device,
        swap_device,
        root_device,
        root_uuid,
        efi_uuid,
        swap_uuid,
    })
}

/// Get the UUID of a partition using `blkid`.
fn get_uuid(device: &str) -> Result<String, String> {
    let output = run_cmd("sudo", &["blkid", "-s", "UUID", "-o", "value", device])?;
    let uuid = output.trim().to_string();
    if uuid.is_empty() {
        return Err(format!("Could not read UUID for {device}"));
    }
    Ok(uuid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_uefi_no_panic() {
        // Just verify it returns a bool without panicking.
        let _result: bool = is_uefi();
    }

    #[test]
    fn test_plan_partitions_uefi() {
        let plan = plan_partitions_inner(
            "/dev/sda",
            500_000_000_000, // 500 GB
            true,            // UEFI
            4_000_000_000,   // 4 GB RAM
        )
        .expect("Should succeed for 500GB drive");

        assert_eq!(plan.device, "/dev/sda");
        assert!(plan.is_uefi);
        assert_eq!(plan.partitions.len(), 3);

        assert_eq!(plan.partitions[0].role, PartitionRole::Efi);
        assert_eq!(plan.partitions[0].size_bytes, EFI_SIZE);
        assert_eq!(plan.partitions[0].filesystem, "vfat");

        assert_eq!(plan.partitions[1].role, PartitionRole::Swap);
        assert_eq!(plan.partitions[1].size_bytes, 4_000_000_000);
        assert_eq!(plan.partitions[1].filesystem, "swap");

        assert_eq!(plan.partitions[2].role, PartitionRole::Root);
        assert_eq!(plan.partitions[2].filesystem, "ext4");

        // Root should be total - EFI - swap
        let expected_root = 500_000_000_000 - EFI_SIZE - 4_000_000_000;
        assert_eq!(plan.partitions[2].size_bytes, expected_root);
    }

    #[test]
    fn test_plan_partitions_bios() {
        let plan = plan_partitions_inner(
            "/dev/sda",
            500_000_000_000, // 500 GB
            false,           // BIOS
            4_000_000_000,   // 4 GB RAM
        )
        .expect("Should succeed for 500GB BIOS drive");

        assert!(!plan.is_uefi);
        assert_eq!(plan.partitions.len(), 3);

        assert_eq!(plan.partitions[0].role, PartitionRole::BiosBoot);
        assert_eq!(plan.partitions[0].size_bytes, BIOS_BOOT_SIZE);

        assert_eq!(plan.partitions[1].role, PartitionRole::Swap);
        assert_eq!(plan.partitions[1].size_bytes, 4_000_000_000);

        assert_eq!(plan.partitions[2].role, PartitionRole::Root);
        let expected_root = 500_000_000_000 - BIOS_BOOT_SIZE - 4_000_000_000;
        assert_eq!(plan.partitions[2].size_bytes, expected_root);
    }

    #[test]
    fn test_plan_partitions_too_small() {
        let result = plan_partitions_inner(
            "/dev/sda",
            5_000_000_000, // 5 GB -- too small
            true,
            4_000_000_000,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not enough space"));
    }

    #[test]
    fn test_plan_partitions_minimum_bios() {
        // BIOS: 4GB swap + 8GB root minimum = 12GB needed
        let result = plan_partitions_inner(
            "/dev/sda",
            13_000_000_000, // 13 GB -- should be enough for BIOS with 4GB swap
            false,
            4_000_000_000,
        );
        assert!(result.is_ok());
        let plan = result.unwrap();
        assert!(plan.partitions.last().unwrap().size_bytes >= MIN_ROOT);
    }

    #[test]
    fn test_plan_partitions_minimum_uefi() {
        // UEFI: 512 MiB EFI + 4 GiB swap + 8 GiB root minimum = ~12.5 GiB needed
        // Use 14 GB to comfortably exceed the binary-unit thresholds.
        let result = plan_partitions_inner(
            "/dev/sda",
            14_000_000_000, // 14 GB
            true,
            4_000_000_000,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_plan_partitions_swap_capped_at_8gb() {
        let plan = plan_partitions_inner(
            "/dev/sda",
            500_000_000_000,
            false,
            32_000_000_000, // 32 GB RAM
        )
        .unwrap();

        // Swap should be capped at 8 GB (index 1 on BIOS: BiosBoot, Swap, Root)
        let swap = plan.partitions.iter().find(|p| p.role == PartitionRole::Swap).unwrap();
        assert_eq!(swap.size_bytes, MAX_SWAP);
    }

    #[test]
    fn test_partition_device_sda() {
        assert_eq!(partition_device("/dev/sda", 1), "/dev/sda1");
        assert_eq!(partition_device("/dev/sda", 3), "/dev/sda3");
    }

    #[test]
    fn test_partition_device_nvme() {
        assert_eq!(partition_device("/dev/nvme0n1", 1), "/dev/nvme0n1p1");
        assert_eq!(partition_device("/dev/nvme0n1", 2), "/dev/nvme0n1p2");
    }
}
