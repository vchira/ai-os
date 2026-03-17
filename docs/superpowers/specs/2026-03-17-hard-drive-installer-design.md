# Hard Drive Installer — Design Spec

## Problem

AiOS currently only runs as a live ISO. Users need the ability to install it to a hard drive from the live USB, with a conversational installation flow.

## Requirements

1. Only available when booted from live ISO (detect squashfs/overlay root)
2. Conversational installer integrated into the first-boot setup flow
3. Asks for: country (derives locale/timezone/keyboard/time format), drive, space, assistant name, master password
4. AI handles partitioning (EFI + root + swap)
5. After install → modal dialog: "Remove USB, press Reboot" (only option)
6. After reboot from hard drive → first-boot setup runs, skips already-configured items
7. SSH default password `aios:aios` until master password is set, then SSH password = master password
8. All settings stored in encrypted vault
9. All UI strings use i18n framework (`t()` calls)

## Detection: Live ISO vs Installed

```bash
# Live: root is overlay/squashfs/tmpfs
# Installed: root is ext4/btrfs/xfs on a real partition
mountpoint -q / && findmnt -n -o FSTYPE / | grep -qE "overlay|squashfs|tmpfs"
```

In Rust:
```rust
fn is_live_iso() -> bool {
    std::process::Command::new("findmnt")
        .args(["-n", "-o", "FSTYPE", "/"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|fs| fs.contains("overlay") || fs.contains("squashfs") || fs.contains("tmpfs"))
        .unwrap_or(false)
}
```

## Installer Conversation Flow

Inserted into first-boot setup after audio test:

1. **Country** — dropdown auto-detected via IP geolocation (freegeoip/ipapi). Derives:
   - Language (e.g. Germany → German)
   - Keyboard layout (e.g. Germany → de)
   - Timezone (e.g. Germany → Europe/Berlin)
   - Time format (e.g. Germany → 24h)
   - Show summary: "Based on Germany: German, de keyboard, Europe/Berlin, 24h — change any?"

2. **Install to hard drive?** — "Would you like to install AiOS to a hard drive, or continue running from USB?" Yes/No buttons. Only shown on live ISO.

3. **Drive selection** — dropdown with human-readable names auto-detected via `lsblk`:
   - "Samsung SSD 500GB (/dev/sda)"
   - "WD Blue 1TB (/dev/sdb)"
   - Shows size, model, current partitions

4. **Space allocation**:
   - If drive has < 10GB free → list existing partitions, ask which to delete
   - If drive has free space → "How much space for AiOS?" with default = all available
   - Minimum: 10GB

5. **Partitioning plan** — AI decides and shows:
   - EFI System Partition: 512MB (FAT32) — if UEFI boot
   - Swap: min(RAM, 8GB)
   - Root: remaining space (ext4)
   - "This will create 3 partitions on Samsung SSD. Continue?" Confirm/Cancel

6. **Assistant name / wake word / machine name** — same as current setup step

7. **Master password** + confirm — same as current setup, but also sets SSH password

8. **Confirm summary** — full summary card:
   - Country, language, keyboard, timezone
   - Target drive and partition plan
   - Assistant name, wake word, hostname
   - "Install AiOS?" with Install/Cancel buttons

9. **Installation progress** — messages in chat:
   - "Partitioning drive..."
   - "Formatting partitions..."
   - "Copying system files..."
   - "Installing bootloader..."
   - "Configuring system..."
   - "Creating vault..."
   - "Setting up user account..."

10. **Complete** — GTK modal dialog (not chat card):
    - "Installation complete! Remove the USB drive and press Reboot."
    - Single "Reboot" button, no dismiss, no close

## What Gets Installed

1. **Partition and format** target drive (gdisk/sgdisk for GPT, mkfs.fat/mkfs.ext4/mkswap)
2. **Copy filesystem** — unsquashfs the live squashfs to the root partition
3. **Install bootloader** — GRUB (grub-install + grub-mkconfig), detect UEFI vs BIOS
4. **Write /etc/fstab** — mount entries for root, EFI, swap (by UUID)
5. **Set hostname** — from installer choice
6. **Set locale/timezone/keyboard** — from country selection
7. **Create vault** — with master password, store all settings
8. **Set SSH password** — change from default `aios` to master password
9. **Set user password** — change from default `aios` to master password

## After Reboot (Installed System)

- Boots from hard drive, GRUB → Linux → greetd → labwc → AiOS
- Vault exists → `activate_main()` runs (not first-boot setup)
- Locale, timezone, keyboard already configured
- Provider/API keys: if from `.env` at build time, already in vault; otherwise setup asks
- SSH secured with master password

## SSH Password

- Default: `aios:aios` (set at build time in `_inner_build.sh`)
- After installer sets master password: `echo "aios:<master_password>" | sudo chpasswd`
- After first-boot setup (non-installer path): same — when vault is created, update SSH password

## Files

| File | Purpose |
|------|---------|
| `aios-core/src/installer.rs` | New: live ISO detection, drive enumeration, partitioning, file copy, bootloader, fstab |
| `aios-core/src/installer/drives.rs` | New: lsblk parsing, human-readable drive names |
| `aios-core/src/installer/partition.rs` | New: partition planning, sgdisk/mkfs execution |
| `aios-core/src/installer/copy.rs` | New: unsquashfs, file copy, progress reporting |
| `aios-core/src/installer/bootloader.rs` | New: GRUB install (UEFI + BIOS) |
| `aios-core/src/installer/locale.rs` | New: country → locale/timezone/keyboard mapping, IP geolocation |
| `aios-gtk/src/ui/first_boot.rs` | Add installer steps to setup conversation |
| `aios-gtk/src/app.rs` | Live ISO detection, modal reboot dialog |
| `distro/_inner_build.sh` | Ensure squashfs-tools, grub-install, sgdisk in ISO packages |

## Country → Locale Mapping

Embedded mapping (no network dependency for the mapping itself, only for auto-detection):

```rust
struct CountryDefaults {
    language: &'static str,      // "de"
    keyboard: &'static str,      // "de"
    timezone: &'static str,      // "Europe/Berlin"
    time_format_24h: bool,       // true
}
```

~30 countries mapped. Auto-detection of current country via `http://ip-api.com/json/` (free, no key needed).

## Packages Required in ISO

Already present or need to add to `_inner_build.sh` package lists:
- `squashfs-tools` — for `unsquashfs`
- `grub-efi-amd64` + `grub-pc` — bootloader for UEFI and BIOS
- `sgdisk` (from `gdisk` package) — GPT partitioning
- `dosfstools` — for `mkfs.fat` (EFI partition)
- `e2fsprogs` — for `mkfs.ext4` (already present)
