# Hard Drive Installation

AiOS can be installed permanently to an internal hard drive or SSD. The installer erases the target disk and sets up a bootable AiOS system.

## Before you install

- **Back up any data** on the target disk. The installer formats the entire drive.
- AiOS requires at least **8 GB** of disk space. A 16 GB or larger drive is recommended.
- The target disk must be a different device than the USB drive you are booting from.

## Installation methods

There are two ways to install AiOS to a hard drive:

### 1. Through the setup wizard

During the first-boot setup wizard, if AiOS detects internal hard drives, the AI will ask whether you want to install to a hard drive or continue in live mode.

If you choose to install:
1. The AI shows available disks with their sizes and names.
2. Select the target disk.
3. Confirm that you want to erase the disk and install.
4. The installer runs -- this takes 2-5 minutes depending on disk speed.
5. The AI prompts you to remove the USB drive and reboot.

### 2. Through autoconfig

Set the installation fields in your autoconfig file:

```json
{
  "install": {
    "enabled": true,
    "target_disk": "auto",
    "confirm": false
  }
}
```

See [Unattended Installation](unattended.md) for full details on autoconfig.

## What the installer does

The installer performs these steps:

1. **Partitions the disk** using GPT:
   - A 512 MB EFI System Partition (FAT32) for UEFI boot
   - The remaining space as a single ext4 root partition

2. **Copies the live system** to the root partition. This is a direct copy of the running system, so the installed version is identical to what you see in live mode.

3. **Installs the bootloader** (GRUB) to the EFI partition.

4. **Configures fstab** for the new partition layout.

5. **Copies your vault and settings** from the live session to the installed system, so you do not need to run the setup wizard again.

## After installation

Once installation completes:

1. Shut down or reboot the computer.
2. Remove the USB drive.
3. The computer will now boot directly into AiOS from the hard drive.

Your vault, API keys, and any settings from the live session are preserved. The system is ready to use immediately.

## Disk selection

When using `"target_disk": "auto"`, the installer automatically selects the first non-USB, non-removable disk. This is usually the right choice for machines with a single internal drive.

If your machine has multiple internal drives, specify the exact device:

```json
{
  "install": {
    "enabled": true,
    "target_disk": "/dev/sda",
    "confirm": false
  }
}
```

To see which disks are available, you can ask the AI: "What disks are available?" or use the system tool to run `lsblk`.

## Troubleshooting

- **Installer says no disks found:** The disk controller may need a kernel driver. Try a different SATA/NVMe port or check BIOS settings for AHCI mode.
- **Boot fails after installation:** Enter BIOS and ensure the installed disk is first in the boot order. If Secure Boot is enabled, try disabling it.
- **"Disk is busy" error:** Make sure no partitions on the target disk are mounted. The installer will attempt to unmount them, but manual intervention may be needed in rare cases.
