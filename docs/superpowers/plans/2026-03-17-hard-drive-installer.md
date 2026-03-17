# Hard Drive Installer Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable AiOS to install itself from a live USB to a hard drive through a conversational installer integrated into the first-boot setup flow.

**Architecture:** The installer consists of a backend module in `aios-core` (live ISO detection, drive enumeration, partitioning, filesystem copy, bootloader installation, locale mapping) and new `SetupStep` variants in `aios-gtk`'s `first_boot.rs` that present the installer conversation. The backend executes privileged system commands (`sgdisk`, `mkfs`, `unsquashfs`, `grub-install`) as `sudo` subprocesses with progress reporting via callbacks. A country-to-locale mapping module provides automatic derivation of language, keyboard, timezone, and time format from the user's country (auto-detected via IP geolocation).

**Tech Stack:** Rust, GTK4/libadwaita, sgdisk, mkfs.ext4, mkfs.fat, mkswap, unsquashfs, grub-install, grub-mkconfig, lsblk, ip-api.com (geolocation)

---

### Task 1: Country-to-Locale Mapping Module

**Files:**
- Create: `aios-app-rs/aios-core/src/installer/locale.rs`
- Create: `aios-app-rs/aios-core/src/installer/mod.rs`
- Modify: `aios-app-rs/aios-core/src/lib.rs` — add `pub mod installer;`

- [ ] **Step 1: Create the installer module directory and `mod.rs`**

Create `aios-app-rs/aios-core/src/installer/mod.rs` with:

```rust
//! Hard drive installer — live ISO detection, drive management, partitioning,
//! filesystem copy, bootloader installation, and locale configuration.

pub mod locale;
```

- [ ] **Step 2: Create the `CountryDefaults` struct and static mapping**

In `aios-app-rs/aios-core/src/installer/locale.rs`, define:

```rust
/// Locale defaults derived from a country selection.
#[derive(Debug, Clone)]
pub struct CountryDefaults {
    /// ISO 3166-1 alpha-2 country code (e.g. "DE").
    pub country_code: &'static str,
    /// Country display name in English (e.g. "Germany").
    pub country_name: &'static str,
    /// Language code for locale (e.g. "de").
    pub language: &'static str,
    /// Keyboard layout code (e.g. "de").
    pub keyboard: &'static str,
    /// IANA timezone (e.g. "Europe/Berlin").
    pub timezone: &'static str,
    /// Whether the country uses 24-hour time format.
    pub time_format_24h: bool,
}
```

Embed a static array of ~30 `CountryDefaults` entries covering at minimum: US, GB, CA, AU, DE, FR, ES, IT, PT, BR, NL, BE, AT, CH, SE, NO, DK, FI, PL, CZ, RO, HU, GR, JP, CN, KR, IN, RU, MX, AR, TR, IE, NZ, ZA, IL, SA, AE, TW, SG, MY.

Provide lookup functions:
- `fn all_countries() -> &'static [CountryDefaults]` — returns the full list
- `fn country_by_code(code: &str) -> Option<&'static CountryDefaults>` — case-insensitive lookup
- `fn country_by_name(name: &str) -> Option<&'static CountryDefaults>` — case-insensitive substring match

- [ ] **Step 3: Add IP geolocation for country auto-detection**

Add an async function that queries `http://ip-api.com/json/` (free, no API key) and returns the detected country code:

```rust
/// Auto-detect the user's country via IP geolocation.
///
/// Returns `None` if the request fails or the response is unparseable.
/// Uses a 5-second timeout so the UI is not blocked for long.
pub async fn detect_country() -> Option<&'static CountryDefaults> {
    // GET http://ip-api.com/json/ → parse "countryCode" field
    // Look up in the static table
}
```

The response JSON has `{"countryCode": "DE", "country": "Germany", ...}`. Use `reqwest` (already a workspace dep) with a 5-second timeout. If the request fails or the country is not in our table, return `None`.

- [ ] **Step 4: Add a synchronous blocking wrapper for GTK thread use**

Since the GTK setup steps run on the main thread and spawn background work, provide:

```rust
/// Blocking version of `detect_country` — spawns a thread.
/// Returns the result via a callback on the GTK main thread.
pub fn detect_country_blocking(callback: impl FnOnce(Option<&'static CountryDefaults>) + Send + 'static)
```

Use `std::thread::spawn` + `reqwest::blocking::get` (blocking feature is already enabled in workspace).

- [ ] **Step 5: Register the installer module in `lib.rs`**

Add `pub mod installer;` to `aios-app-rs/aios-core/src/lib.rs`.

- [ ] **Step 6: Add `reqwest` dependency to `aios-core/Cargo.toml`**

Add `reqwest = { workspace = true }` to `[dependencies]` in `aios-app-rs/aios-core/Cargo.toml`. This is needed for the IP geolocation HTTP request.

- [ ] **Step 7: Write unit tests**

Add tests in `locale.rs`:
- `test_all_countries_not_empty` — ensures the table has entries
- `test_lookup_by_code` — looks up "US", "DE", "JP" by code
- `test_lookup_by_name` — looks up "Germany", "United States" by name
- `test_lookup_case_insensitive` — "de" and "DE" both work
- `test_unknown_country_returns_none` — "XX" returns None

- [ ] **Step 8: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 2: Live ISO Detection

**Files:**
- Modify: `aios-app-rs/aios-core/src/installer/mod.rs` — add `is_live_iso()` and `find_squashfs()` functions

- [ ] **Step 1: Implement `is_live_iso()` detection**

Add to `installer/mod.rs`:

```rust
/// Returns `true` if the system is running from a live ISO (overlay/squashfs/tmpfs root).
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
```

- [ ] **Step 2: Implement `find_squashfs()` to locate the live filesystem**

```rust
/// Locate the squashfs filesystem image on the live media.
///
/// Searches common live-build paths: `/lib/live/mount/medium/live/filesystem.squashfs`,
/// `/run/live/medium/live/filesystem.squashfs`, and `/cdrom/live/filesystem.squashfs`.
/// Returns `None` if no squashfs is found (not a live system).
pub fn find_squashfs() -> Option<std::path::PathBuf>
```

Check each candidate path with `.exists()`. Return the first match.

- [ ] **Step 3: Write tests**

- `test_is_live_iso_returns_bool` — just ensure the function doesn't panic on the dev machine
- `test_find_squashfs_dev_machine` — on dev machine, should return None (not a live ISO)

- [ ] **Step 4: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 3: Drive Detection and Enumeration

**Files:**
- Create: `aios-app-rs/aios-core/src/installer/drives.rs`
- Modify: `aios-app-rs/aios-core/src/installer/mod.rs` — add `pub mod drives;`

- [ ] **Step 1: Define the drive info types**

```rust
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
```

- [ ] **Step 2: Implement `list_drives()` using `lsblk`**

```rust
/// Enumerate all block devices suitable for installation.
///
/// Filters out: loop devices, CD-ROMs, removable USB (the live media),
/// devices smaller than 10 GB. Uses `lsblk --json --bytes` for parsing.
pub fn list_drives() -> Vec<DriveInfo>
```

Run `lsblk -J -b -o NAME,SIZE,MODEL,TYPE,FSTYPE,MOUNTPOINT,LABEL,RM,TRAN` and parse the JSON output. Filter for `TYPE == "disk"`. Detect the live media drive by checking if any of its partitions are mounted under `/lib/live/mount/` or `/run/live/`. Mark it as `is_live_media = true` and include it in the list (but the UI will show a warning). Gather partitions from child entries where `TYPE == "part"`.

- [ ] **Step 3: Implement human-readable size formatting**

```rust
/// Format bytes as a human-readable string (e.g. "500 GB", "1.2 TB").
pub fn format_size(bytes: u64) -> String
```

Use TB for >= 1 TB, GB for >= 1 GB, MB otherwise. One decimal place.

- [ ] **Step 4: Implement display name generation**

```rust
/// Generate a human-readable drive description for the UI dropdown.
///
/// Examples:
/// - "Samsung SSD 860 EVO — 500 GB (/dev/sda)"
/// - "WD Blue 1TB — 1.0 TB (/dev/sdb)"
/// - "Unknown Drive — 256 GB (/dev/nvme0n1)"
pub fn drive_display_name(drive: &DriveInfo) -> String
```

If `model` is empty or "Unknown", use "Unknown Drive". Include size and device path.

- [ ] **Step 5: Write unit tests**

- `test_format_size` — check GB, TB, MB formatting
- `test_drive_display_name` — check various model/size combos
- `test_list_drives_no_panic` — ensure `list_drives()` doesn't panic on dev machine (may return empty list)

- [ ] **Step 6: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 4: Partition Planning and Execution

**Files:**
- Create: `aios-app-rs/aios-core/src/installer/partition.rs`
- Modify: `aios-app-rs/aios-core/src/installer/mod.rs` — add `pub mod partition;`

- [ ] **Step 1: Define the partition plan types**

```rust
/// A planned partition layout for installation.
#[derive(Debug, Clone)]
pub struct PartitionPlan {
    /// Target device (e.g. "/dev/sda").
    pub device: String,
    /// Whether the system boots via UEFI (true) or BIOS/legacy (false).
    pub is_uefi: bool,
    /// Planned partitions.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionRole {
    /// EFI System Partition (only on UEFI systems).
    Efi,
    /// Linux swap.
    Swap,
    /// Root filesystem.
    Root,
}
```

- [ ] **Step 2: Implement UEFI detection**

```rust
/// Returns `true` if the system booted via UEFI.
///
/// Checks for the existence of `/sys/firmware/efi`.
pub fn is_uefi() -> bool {
    std::path::Path::new("/sys/firmware/efi").exists()
}
```

- [ ] **Step 3: Implement partition plan generation**

```rust
/// Generate a partition plan for the given drive.
///
/// Layout:
/// - If UEFI: 512 MB EFI (FAT32) + swap (min(RAM, 8GB)) + root (remaining, ext4)
/// - If BIOS: swap (min(RAM, 8GB)) + root (remaining, ext4)
///
/// Returns `Err` if the drive is too small (< 10 GB usable).
pub fn plan_partitions(device: &str, total_bytes: u64) -> Result<PartitionPlan, String>
```

Detect RAM via `/proc/meminfo` (parse `MemTotal` line). Swap = min(RAM, 8 GB). Validate minimum 10 GB for root after EFI + swap. Call `is_uefi()` to decide whether to include an EFI partition.

- [ ] **Step 4: Implement partition execution (sgdisk + mkfs)**

```rust
/// Execute a partition plan — wipe the drive, create GPT, partition, and format.
///
/// Calls: `sgdisk --zap-all`, `sgdisk --new`, `mkfs.fat`, `mkfs.ext4`, `mkswap`.
/// Each step reports progress via the callback.
///
/// # Safety
/// This DESTROYS all data on the target drive. The caller must have confirmed with the user.
pub fn execute_partition_plan(
    plan: &PartitionPlan,
    progress: impl Fn(&str),
) -> Result<PartitionResult, String>
```

The `PartitionResult` struct holds the device paths of the created partitions (e.g., `/dev/sda1` for EFI, `/dev/sda2` for swap, `/dev/sda3` for root) and their UUIDs (obtained by running `blkid`).

```rust
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
```

Implementation details:
1. `sgdisk --zap-all <device>` — wipe existing partition table
2. `sgdisk --new=1:0:+512M --typecode=1:EF00 <device>` — EFI (if UEFI)
3. `sgdisk --new=2:0:+<swap_size> --typecode=2:8200 <device>` — swap
4. `sgdisk --new=3:0:0 --typecode=3:8300 <device>` — root (remaining)
5. Wait for kernel to re-read partition table: `partprobe <device>` or `sleep 1` + re-read
6. `mkfs.fat -F 32 <efi_partition>` (if UEFI)
7. `mkswap <swap_partition>`
8. `mkfs.ext4 -F <root_partition>`
9. Retrieve UUIDs via `blkid -s UUID -o value <partition>`

All commands run via `sudo`. Each step calls `progress()` with a description.

- [ ] **Step 5: Write unit tests**

- `test_is_uefi_no_panic` — just ensure it returns a bool
- `test_plan_partitions_uefi` — mock a 500GB drive, verify 3 partitions (EFI + swap + root)
- `test_plan_partitions_bios` — mock a 500GB drive with `is_uefi = false`, verify 2 partitions
- `test_plan_partitions_too_small` — 5 GB drive returns error
- `test_plan_partitions_minimum` — 10 GB drive succeeds

Note: `execute_partition_plan` cannot be unit-tested safely; it requires integration testing in a VM.

- [ ] **Step 6: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 5: Filesystem Copy (unsquashfs)

**Files:**
- Create: `aios-app-rs/aios-core/src/installer/copy.rs`
- Modify: `aios-app-rs/aios-core/src/installer/mod.rs` — add `pub mod copy;`

- [ ] **Step 1: Implement mount helpers**

```rust
/// Mount a partition at a given mount point.
pub fn mount(device: &str, mount_point: &str, fstype: Option<&str>) -> Result<(), String>

/// Unmount a mount point.
pub fn unmount(mount_point: &str) -> Result<(), String>
```

Use `sudo mount` / `sudo umount`. Create the mount point directory if it doesn't exist.

- [ ] **Step 2: Implement the squashfs copy with progress**

```rust
/// Copy the live filesystem to the target root partition.
///
/// 1. Mount root partition at `/mnt/aios-install`
/// 2. If UEFI, mount EFI partition at `/mnt/aios-install/boot/efi`
/// 3. Run `unsquashfs -f -d /mnt/aios-install <squashfs_path>`
/// 4. Report progress via callback
///
/// The squashfs path is found via `find_squashfs()`.
pub fn copy_filesystem(
    root_device: &str,
    efi_device: Option<&str>,
    squashfs_path: &std::path::Path,
    progress: impl Fn(&str),
) -> Result<String, String>
```

The mount point is `/mnt/aios-install`. Steps:
1. `mkdir -p /mnt/aios-install`
2. `sudo mount <root_device> /mnt/aios-install`
3. Report: "Copying system files... (this may take a few minutes)"
4. `sudo unsquashfs -f -d /mnt/aios-install <squashfs_path>`
5. If UEFI: `mkdir -p /mnt/aios-install/boot/efi && sudo mount <efi_device> /mnt/aios-install/boot/efi`
6. Return the mount point path

Returns the mount point path (for subsequent configuration steps). Does NOT unmount — the caller handles cleanup.

- [ ] **Step 3: Write tests**

- `test_mount_unmount` — just compile check, actual execution needs root
- Add doc comments explaining these are integration-test-only functions

- [ ] **Step 4: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 6: Bootloader Installation and System Configuration

**Files:**
- Create: `aios-app-rs/aios-core/src/installer/bootloader.rs`
- Create: `aios-app-rs/aios-core/src/installer/configure.rs`
- Modify: `aios-app-rs/aios-core/src/installer/mod.rs` — add `pub mod bootloader; pub mod configure;`

- [ ] **Step 1: Implement GRUB installation**

In `bootloader.rs`:

```rust
/// Install GRUB bootloader on the target system.
///
/// Must be called while the target is mounted at `mount_point`.
/// Bind-mounts `/dev`, `/proc`, `/sys` into the target and runs
/// `grub-install` + `grub-mkconfig` inside a chroot.
pub fn install_grub(
    mount_point: &str,
    device: &str,
    is_uefi: bool,
    progress: impl Fn(&str),
) -> Result<(), String>
```

Steps:
1. Bind-mount: `mount --bind /dev <mount>/dev`, same for `/proc`, `/sys`, `/sys/firmware/efi/efivars` (if UEFI)
2. If UEFI: `chroot <mount> grub-install --target=x86_64-efi --efi-directory=/boot/efi --bootloader-id=AiOS --recheck <device>`
3. If BIOS: `chroot <mount> grub-install --target=i386-pc --recheck <device>`
4. `chroot <mount> grub-mkconfig -o /boot/grub/grub.cfg`
5. Unbind-mount in reverse order (even on error)

- [ ] **Step 2: Implement fstab generation**

In `configure.rs`:

```rust
/// Write /etc/fstab on the installed system.
///
/// Entries are referenced by UUID (not device path) for robustness.
pub fn write_fstab(
    mount_point: &str,
    partition_result: &super::partition::PartitionResult,
) -> Result<(), String>
```

Generate content:
```
# <filesystem>  <mount point>  <type>  <options>  <dump>  <pass>
UUID=<root_uuid>  /            ext4    errors=remount-ro  0  1
UUID=<efi_uuid>   /boot/efi    vfat    umask=0077         0  1  # if UEFI
UUID=<swap_uuid>  none         swap    sw                 0  0
```

Write to `<mount_point>/etc/fstab`.

- [ ] **Step 3: Implement system configuration (hostname, locale, timezone, keyboard, SSH password, user password)**

```rust
/// Configure the installed system's identity and locale settings.
pub fn configure_system(
    mount_point: &str,
    hostname: &str,
    language: &str,
    keyboard: &str,
    timezone: &str,
    master_password: &str,
    progress: impl Fn(&str),
) -> Result<(), String>
```

Steps (all writing to files under `mount_point` or running `chroot` commands):
1. Write `<mount>/etc/hostname` with the hostname
2. Write `<mount>/etc/hosts` with `127.0.0.1 localhost <hostname>`
3. Set locale: `chroot <mount> sed -i "s/^# *\(<lang>\)/\1/" /etc/locale.gen && chroot <mount> locale-gen`
4. Write `<mount>/etc/default/locale` with `LANG=<language>.UTF-8`
5. Set timezone: `chroot <mount> ln -sf /usr/share/zoneinfo/<timezone> /etc/localtime`
6. Set keyboard: write `<mount>/etc/default/keyboard` with `XKBLAYOUT=<keyboard>`
7. Set user password: `chroot <mount> sh -c "echo 'aios:<master_password>' | chpasswd"`
8. Set root password (same): `chroot <mount> sh -c "echo 'root:<master_password>' | chpasswd"`

Each step calls `progress()`.

- [ ] **Step 4: Implement cleanup (unmount everything)**

```rust
/// Unmount all installer mount points in reverse order.
pub fn cleanup_mounts(mount_point: &str, is_uefi: bool) -> Result<(), String>
```

Unmount in order: `<mount>/sys/firmware/efi/efivars`, `<mount>/sys`, `<mount>/proc`, `<mount>/dev`, `<mount>/boot/efi` (if UEFI), `<mount>`.

- [ ] **Step 5: Write tests**

- `test_fstab_content_uefi` — generate fstab for a mock UEFI partition result, verify content
- `test_fstab_content_bios` — generate fstab for a BIOS partition result (no EFI line)

- [ ] **Step 6: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 7: Top-Level Install Orchestrator

**Files:**
- Modify: `aios-app-rs/aios-core/src/installer/mod.rs` — add the `InstallConfig` type and `run_install()` function

- [ ] **Step 1: Define the complete install configuration**

```rust
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
```

- [ ] **Step 2: Implement the orchestrator function**

```rust
/// Run the complete installation process.
///
/// This is the single entry point called from the GTK installer step.
/// It runs all phases sequentially and reports progress via the callback.
/// Designed to be called from a background thread (not the GTK main thread).
///
/// Phases:
/// 1. Partition the target drive
/// 2. Copy the live filesystem (unsquashfs)
/// 3. Install the bootloader (GRUB)
/// 4. Write fstab
/// 5. Configure system (hostname, locale, timezone, keyboard)
/// 6. Set user/SSH passwords
/// 7. Create vault with API keys and master password
/// 8. Cleanup (unmount)
pub fn run_install(
    config: &InstallConfig,
    progress: impl Fn(InstallPhase, &str) + Send,
) -> Result<(), String>
```

- [ ] **Step 3: Define the progress phases enum**

```rust
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
```

- [ ] **Step 4: Implement vault creation on the target system**

Inside `run_install`, after system configuration, create the vault at `<mount_point>/home/aios/.aios/vault.enc`:

```rust
// Create the vault directory
let vault_dir = format!("{mount_point}/home/aios/.aios");
std::fs::create_dir_all(&vault_dir).map_err(|e| format!("Failed to create vault dir: {e}"))?;

// Create and populate the vault
let vault_path = std::path::PathBuf::from(format!("{vault_dir}/vault.enc"));
let mut vault = aios_core::secure::Vault::new(vault_path);
vault.create(&config.master_password).map_err(|e| format!("Failed to create vault: {e}"))?;

for provider in &config.providers {
    let key_name = format!("{}_api_key", provider.name);
    let entry = SecretEntry { /* ... */ };
    vault.set(&key_name, entry).map_err(|e| format!("Failed to store key: {e}"))?;
}

// Fix ownership: chown aios:aios -R <mount>/home/aios/.aios
```

- [ ] **Step 5: Implement config.json creation on the target system**

Write the AiOS config to `<mount_point>/home/aios/.aios/config.json` with provider, assistant name, wake word, machine name, locale settings. Use `ConfigManager::with_path()` pointed at the target path.

- [ ] **Step 6: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 8: First-Boot Setup Steps — Country + Install Decision

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs` — add new `SetupStep` variants and rendering methods

This is the GTK UI integration. New steps are inserted between `TestAudioInput` and `NameAssistant`.

- [ ] **Step 1: Add new `SetupStep` variants**

Add to the `SetupStep` enum:

```rust
enum SetupStep {
    Welcome,
    TestAudioOutput,
    TestAudioInput,
    Country,                  // NEW: country selection
    InstallDecision,          // NEW: install to hard drive? (only on live ISO)
    DriveSelection,           // NEW: choose target drive
    PartitionPlan,            // NEW: review partition plan
    InstallConfirm,           // NEW: final confirmation summary
    InstallProgress,          // NEW: installation in progress
    NameAssistant,
    ChooseProvider,
    EnterApiKey { provider: String },
    CreatePassword,
    ConfirmPassword,
    AddBackup,
    EnterBackupKey { provider: String },
    ProviderOrder,
    Complete,
}
```

- [ ] **Step 2: Add installer state fields to `SetupState`**

```rust
struct SetupState {
    // ... existing fields ...

    /// Selected country defaults (from Country step).
    country: Option<aios_core::installer::locale::CountryDefaults>,
    /// Whether the user chose to install to hard drive.
    install_to_drive: bool,
    /// Selected target drive (from DriveSelection step).
    target_drive: Option<aios_core::installer::drives::DriveInfo>,
    /// Generated partition plan (from PartitionPlan step).
    partition_plan: Option<aios_core::installer::partition::PartitionPlan>,
}
```

- [ ] **Step 3: Update `SetupResult` to include installer data**

Add new fields to `SetupResult`:

```rust
pub struct SetupResult {
    // ... existing fields ...

    /// Country-derived locale settings (if country was selected).
    pub country: Option<aios_core::installer::locale::CountryDefaults>,
    /// Whether installation to hard drive was performed.
    pub installed_to_drive: bool,
}
```

- [ ] **Step 4: Update the `show_step()` match to include new variants**

Add match arms in `show_step()` for each new variant, dispatching to `show_country()`, `show_install_decision()`, etc.

- [ ] **Step 5: Update `on_voice_input()` for new steps**

Add voice handling:
- `Country` — ignore voice (dropdown interaction)
- `InstallDecision` — "yes"/"install" advances to DriveSelection, "no"/"skip"/"usb" skips to NameAssistant
- `DriveSelection` — ignore voice (dropdown interaction)
- `PartitionPlan` — "yes"/"continue"/"confirm" advances, "no"/"cancel" goes back
- `InstallConfirm` — "install"/"yes"/"confirm" starts install, "cancel" goes back
- `InstallProgress` — ignore voice

- [ ] **Step 6: Update the flow in `advance()` / step transitions**

After `TestAudioInput`, advance to `Country` (instead of `NameAssistant`). After `Country`:
- If `is_live_iso()` → advance to `InstallDecision`
- If not live ISO → advance to `NameAssistant`

After `InstallDecision`:
- If user says "yes" → `DriveSelection`
- If user says "no" → `NameAssistant`

After `PartitionPlan` confirmation → `NameAssistant` (collect name/password before installing).
After `ProviderOrder` / before `Complete`:
- If `install_to_drive` is true → `InstallConfirm`
- Otherwise → `Complete`

After `InstallConfirm` "Install" button → `InstallProgress` → then shows modal reboot dialog (handled in Task 9).

- [ ] **Step 7: Implement `show_country()`**

Render a setup card with:
- Title: "Choose Your Country" (use i18n `t()` key)
- A `gtk::DropDown` populated with all country names from `locale::all_countries()`
- Auto-select the detected country (run `detect_country_blocking` when the step shows, update dropdown when result arrives)
- A summary label below the dropdown showing: "Language: German | Keyboard: de | Timezone: Europe/Berlin | Time: 24h"
- Update the summary when the dropdown selection changes
- "Next" button that stores the selected `CountryDefaults` in state and advances

```rust
fn show_country(&self) {
    let countries = aios_core::installer::locale::all_countries();
    // Build dropdown with country names
    // Spawn background country detection
    // On detection result: update dropdown selection via glib::idle_add_local_once
    // On "Next" click: store in state, advance
}
```

- [ ] **Step 8: Implement `show_install_decision()`**

Render a setup card with:
- Title: "Install to Hard Drive?"
- Description: "Would you like to install AiOS to a hard drive, or continue running from USB?"
- Two buttons: "Install to hard drive" (suggested-action) and "Continue from USB"

Only shown when `is_live_iso()` returns true (checked before advancing to this step).

- [ ] **Step 9: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 9: First-Boot Setup Steps — Drive Selection, Partition Plan, Install

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs` — implement remaining installer steps

- [ ] **Step 1: Implement `show_drive_selection()`**

Render a setup card with:
- Title: "Select Target Drive"
- A `gtk::DropDown` populated with `drives::list_drives()` display names
- Exclude drives marked `is_live_media` or show them greyed out with "(Live USB — cannot install here)"
- Below the dropdown: show drive details (existing partitions, total size)
- "Next" button stores the selected `DriveInfo` in state

Warning: if no eligible drives found, show an error message and a "Back" button.

- [ ] **Step 2: Implement `show_partition_plan()`**

After drive is selected, call `partition::plan_partitions()` and render:
- Title: "Partition Plan"
- Show each planned partition as a row: "EFI System Partition: 512 MB (FAT32)", "Swap: 4 GB", "Root: 495 GB (ext4)"
- Warning text: "This will ERASE all data on <drive model>."
- "Continue" button (destructive-action CSS class) and "Back" button

- [ ] **Step 3: Implement `show_install_confirm()`**

After all setup data is collected (name, password, providers, country, drive, partitions), show a full summary card:
- Title: "Ready to Install"
- Summary using `BootStatus` format:
  - Country: Germany
  - Language: German | Keyboard: de | Timezone: Europe/Berlin
  - Target: Samsung SSD 500GB (/dev/sda)
  - Partitions: EFI 512MB + Swap 4GB + Root 495GB
  - Assistant: <name> | Wake word: <word> | Hostname: <name>.local
  - Providers: Claude (primary), ChatGPT (backup)
  - Master password: set
- Two buttons: "Install AiOS" (destructive-action) and "Cancel"

- [ ] **Step 4: Implement `show_install_progress()`**

When the user clicks "Install AiOS":
1. Disable the button
2. Advance to `InstallProgress` step
3. Show a setup card with title "Installing AiOS..." and a progress area
4. Spawn a background thread that calls `installer::run_install()`
5. The progress callback sends messages back to the GTK thread via `glib::MainContext::channel`
6. Each progress message is added to the chat as a system message: "Partitioning drive...", "Copying system files...", etc.
7. On completion: show the reboot dialog (Step 5)
8. On error: show error in chat, provide a "Retry" button

```rust
fn show_install_progress(&self) {
    let config = self.build_install_config(); // Assemble InstallConfig from state

    let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);

    // Background thread
    let config_clone = config.clone();
    std::thread::spawn(move || {
        let result = aios_core::installer::run_install(&config_clone, |phase, msg| {
            let _ = tx.send((phase, msg.to_string()));
        });
        // Send completion or error
        match result {
            Ok(()) => { let _ = tx.send((InstallPhase::Complete, String::new())); }
            Err(e) => { let _ = tx.send((InstallPhase::Complete, e)); }
        }
    });

    // GTK receiver
    let chat = self.chat_view.clone();
    let this = self.clone();
    rx.attach(None, move |(phase, msg)| {
        if phase == InstallPhase::Complete {
            if msg.is_empty() {
                this.show_reboot_dialog();
            } else {
                chat.add_message("system", &format!("Installation failed: {msg}"));
            }
            return glib::ControlFlow::Break;
        }
        chat.add_message("system", &format!("{} {}", phase.description(), msg));
        glib::ControlFlow::Continue
    });
}
```

- [ ] **Step 5: Implement `show_reboot_dialog()` as a modal GTK dialog**

This is NOT a chat card — it's a real modal dialog:

```rust
fn show_reboot_dialog(&self) {
    // Find the top-level window
    // Create an adw::MessageDialog (or gtk::MessageDialog)
    // Title: "Installation Complete!"
    // Body: "Remove the USB drive and press Reboot."
    // Single button: "Reboot"
    // Modal, not closeable (no close button, no Escape dismiss)
    // On "Reboot" click: execute `sudo reboot`
}
```

Use `adw::MessageDialog` with `set_close_response("")` (empty string = no close response) so the user cannot dismiss it. Connect the "Reboot" response to `std::process::Command::new("sudo").args(["reboot"]).status()`.

- [ ] **Step 6: Implement helper method `build_install_config()`**

Assemble an `InstallConfig` from the current `SetupState`:

```rust
fn build_install_config(&self) -> aios_core::installer::InstallConfig {
    let s = self.state.borrow();
    InstallConfig {
        target_device: s.target_drive.as_ref().unwrap().device.clone(),
        target_size_bytes: s.target_drive.as_ref().unwrap().size_bytes,
        country: s.country.clone().unwrap(),
        hostname: if s.use_same_name {
            s.assistant_name.to_lowercase().replace(' ', "-")
        } else {
            s.machine_name_custom.clone()
        },
        master_password: s.master_password.clone(),
        assistant_name: s.assistant_name.clone(),
        wake_word: if s.use_same_name { s.assistant_name.clone() } else { s.wake_word_custom.clone() },
        providers: s.providers.iter().map(|p| ProviderConfig {
            name: p.name.clone(),
            api_key: p.api_key.clone(),
        }).collect(),
    }
}
```

- [ ] **Step 7: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 10: App.rs Integration — Live ISO Detection and Setup Wiring

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs` — update on_complete callback to handle installer results

- [ ] **Step 1: Update the `on_complete` callback to apply country settings**

In `run_first_boot_setup`, within the `setup.on_complete(move |result| { ... })` closure, add country/locale handling:

```rust
// Apply country-derived settings to the installed or live system
if let Some(ref country) = result.country {
    // Set keyboard layout
    let _ = std::process::Command::new("sudo")
        .args(["localectl", "set-x11-keymap", country.keyboard])
        .status();

    // Set timezone
    let _ = std::process::Command::new("sudo")
        .args(["timedatectl", "set-timezone", country.timezone])
        .status();

    // Store in config
    if let Ok(mut config) = ConfigManager::new() {
        let _ = config.set("system.keyboard_layout", serde_json::json!(country.keyboard));
        let _ = config.set("system.timezone", serde_json::json!(country.timezone));
        let _ = config.set("system.language", serde_json::json!(country.language));
        let _ = config.set("system.time_format_24h", serde_json::json!(country.time_format_24h));
    }
}
```

- [ ] **Step 2: Update on_complete to handle the install-to-drive case**

If `result.installed_to_drive` is true, the reboot dialog was already shown by the installer step — do NOT call `transition_to_normal_mode()`. The system will reboot from the hard drive and start fresh (vault already exists on the target).

```rust
if result.installed_to_drive {
    // Installation completed — reboot dialog is already showing.
    // Do not transition to normal mode.
    return;
}

// Normal path: transition to chat mode
Self::transition_to_normal_mode(/* ... */);
```

- [ ] **Step 3: Update `on_complete` to set SSH password**

After vault creation, set the SSH password to the master password (applies to both live and installed systems):

```rust
// Update SSH password from default 'aios' to the master password
let _ = std::process::Command::new("sudo")
    .args(["chpasswd"])
    .stdin(std::process::Stdio::piped())
    .spawn()
    .and_then(|mut child| {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(format!("aios:{}\n", result.master_password).as_bytes());
        }
        child.wait()
    });
```

- [ ] **Step 4: Verify compilation**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```

---

### Task 11: ISO Packages for Installer

**Files:**
- Modify: `distro/_inner_build.sh` — add required packages for the installer

- [ ] **Step 1: Add installer packages to the aios package list**

In `distro/_inner_build.sh`, add the following packages to `config/package-lists/aios.list.chroot` (after the existing entries but before the closing `EOF`):

```
squashfs-tools
gdisk
dosfstools
grub-efi-amd64
grub-pc
```

Check which of these are already pulled in by other packages:
- `e2fsprogs` (for `mkfs.ext4`) is a core package, should already be present
- `grub-efi-amd64` and `grub-pc` may already be present via the bootloader config in `lb config`
- `dosfstools` (for `mkfs.fat`) may not be present
- `gdisk` (for `sgdisk`) is likely not present
- `squashfs-tools` is in the Docker build container but may not be in the live ISO itself

Only add packages that are not already in the lists. Add them to the `aios.list.chroot` section.

- [ ] **Step 2: Verify the package list doesn't break the build**

```bash
# No automated test — this is verified during the next ./start.sh build
```

---

### Task 12: Tests and Compilation Verification

**Files:**
- Modify: `aios-app-rs/aios-core/src/installer/mod.rs` — add integration test stubs

- [ ] **Step 1: Run full workspace check**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -20
```

- [ ] **Step 2: Run all existing tests to ensure no regressions**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test 2>&1 | tail -30
```

- [ ] **Step 3: Run installer-specific tests**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test -p aios-core installer 2>&1
```

- [ ] **Step 4: Update `installer/mod.rs` to export all public types**

Ensure `mod.rs` has all the necessary re-exports:

```rust
pub mod locale;
pub mod drives;
pub mod partition;
pub mod copy;
pub mod bootloader;
pub mod configure;

pub use locale::{CountryDefaults, detect_country, all_countries, country_by_code};
pub use drives::{DriveInfo, PartitionInfo, list_drives};
pub use partition::{PartitionPlan, PartitionRole, PartitionResult, plan_partitions, execute_partition_plan, is_uefi};
pub use copy::copy_filesystem;
pub use bootloader::install_grub;
pub use configure::{write_fstab, configure_system, cleanup_mounts};
```

- [ ] **Step 5: Final compilation check**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check 2>&1 | tail -5
```
