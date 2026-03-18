# Creating a Bootable USB

Once you have downloaded the AiOS ISO, you need to write it to a USB drive. This will make the USB drive bootable so you can start AiOS on your computer.

> **Warning:** Writing the ISO to a USB drive will erase all data on that drive. Back up any important files first.

## Requirements

- A USB drive, 4 GB or larger
- The downloaded AiOS ISO file

## Linux

### Using dd (command line)

1. Insert your USB drive.

2. Identify the device name. Run `lsblk` and look for your USB drive (usually `/dev/sdb` or `/dev/sdc`):

    ```bash
    lsblk
    ```

    Look for the device matching your USB drive's size. Make absolutely sure you identify the correct device -- writing to the wrong device will destroy data.

3. Unmount the USB drive if it was auto-mounted:

    ```bash
    sudo umount /dev/sdX*
    ```

    Replace `sdX` with your actual device name.

4. Write the ISO:

    ```bash
    sudo dd if=aios-2.0.31.iso of=/dev/sdX bs=4M status=progress conv=fsync
    ```

    Replace `aios-2.0.31.iso` with your filename and `/dev/sdX` with your device name. Do not include a partition number (use `/dev/sdb`, not `/dev/sdb1`).

5. Wait for the command to finish. This typically takes 1-3 minutes.

### Using GNOME Disks (graphical)

1. Open the Disks application.
2. Select your USB drive from the left panel.
3. Click the menu button (three dots) and choose "Restore Disk Image..."
4. Select the AiOS ISO file.
5. Click "Start Restoring..." and confirm.

## macOS

### Using dd

1. Insert your USB drive.

2. Identify the device:

    ```bash
    diskutil list
    ```

    Find your USB drive (e.g., `/dev/disk2`).

3. Unmount the drive:

    ```bash
    diskutil unmountDisk /dev/diskN
    ```

4. Write the ISO:

    ```bash
    sudo dd if=aios-2.0.31.iso of=/dev/rdiskN bs=4m
    ```

    Note: Use `/dev/rdiskN` (with the `r` prefix) for significantly faster writing.

5. Eject the drive:

    ```bash
    diskutil eject /dev/diskN
    ```

### Using balenaEtcher

1. Download balenaEtcher from [etcher.balena.io](https://etcher.balena.io).
2. Open Etcher, select the AiOS ISO, select your USB drive, and click Flash.

## Windows

### Using Rufus

1. Download Rufus from [rufus.ie](https://rufus.ie).
2. Insert your USB drive.
3. In Rufus, select your USB drive under "Device."
4. Click "SELECT" and choose the AiOS ISO file.
5. Leave all other settings at their defaults.
6. Click "START" and confirm.

### Using balenaEtcher

1. Download balenaEtcher from [etcher.balena.io](https://etcher.balena.io).
2. Open Etcher, select the AiOS ISO, select your USB drive, and click Flash.

## Next step

Once the USB drive is ready, proceed to [Booting AiOS](booting.md).
