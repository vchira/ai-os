# Unattended Installation

AiOS supports fully unattended configuration through a JSON autoconfig file. When this file is detected at boot, the setup wizard is skipped and the system configures itself automatically.

This is useful for deploying multiple AiOS machines, kiosk setups, or simply saving time when you know exactly how you want the system configured.

## How autoconfig is detected

AiOS searches for the autoconfig file in three locations, in this order:

1. **Kernel boot parameter** -- `aios.autoconfig=/path/to/file`
2. **Baked into the ISO** -- `/opt/aios-app/autoconfig.json`
3. **USB drive** -- `aios-autoconfig.json` at the root of any mounted removable drive

The first file found is used. If no autoconfig file is found, the interactive setup wizard runs instead.

## Creating an autoconfig file

Create a JSON file with your desired settings. Here is a minimal example:

```json
{
  "provider": {
    "primary": "claude",
    "claude_api_key": "sk-ant-your-key-here"
  },
  "system": {
    "master_password": "your-secure-password"
  }
}
```

All fields except the ones you specify will use sensible defaults.

## Full autoconfig reference

```json
{
  "provider": {
    "primary": "claude",
    "claude_api_key": "sk-ant-...",
    "openai_api_key": "sk-..."
  },
  "system": {
    "keyboard": "us",
    "language": "en",
    "timezone": "America/New_York",
    "hostname": "assistant",
    "master_password": "your-password"
  },
  "install": {
    "enabled": false,
    "target_disk": "auto",
    "confirm": false
  },
  "assistant": {
    "name": "Assistant",
    "effort": "auto"
  },
  "debug": false
}
```

See [Autoconfig Format](../reference/autoconfig.md) for a detailed description of every field.

## Using autoconfig on a USB drive

The simplest approach for personal use:

1. Create a file named `aios-autoconfig.json` with your settings.
2. After writing the AiOS ISO to your USB drive, mount the drive and copy the file to its root directory.

   On some USB writing methods, the drive is not easily writable after flashing. An alternative is to place the file on a second USB drive -- AiOS scans all mounted removable media.

## Using autoconfig for live mode

For a live USB that configures itself every time it boots, create a file with installation disabled:

```json
{
  "provider": {
    "primary": "claude",
    "claude_api_key": "sk-ant-your-key-here"
  },
  "system": {
    "keyboard": "de",
    "language": "en",
    "timezone": "Europe/Berlin",
    "hostname": "assistant",
    "master_password": "your-password"
  },
  "install": {
    "enabled": false
  }
}
```

## Using autoconfig for hard drive installation

To have AiOS install itself to a hard drive without any interaction:

```json
{
  "provider": {
    "primary": "claude",
    "claude_api_key": "sk-ant-your-key-here"
  },
  "system": {
    "keyboard": "us",
    "language": "en",
    "timezone": "America/New_York",
    "hostname": "assistant",
    "master_password": "your-password"
  },
  "install": {
    "enabled": true,
    "target_disk": "auto",
    "confirm": false
  }
}
```

When `install.enabled` is `true`:
- `target_disk: "auto"` selects the first non-USB disk automatically
- `target_disk: "/dev/sda"` targets a specific disk
- `confirm: false` means the user is still asked to confirm before the disk is wiped
- `confirm: true` skips the confirmation (fully unattended -- use with caution)

> **Warning:** Setting both `install.enabled: true` and `install.confirm: true` will erase the target disk without any confirmation prompt. Use this only when you are certain of the target hardware.

## Baking autoconfig into the ISO

If you are building AiOS from source, place your `autoconfig.json` file at:

```
distro/build/config/includes.chroot/opt/aios-app/autoconfig.json
```

It will be included in the ISO at `/opt/aios-app/autoconfig.json` and detected automatically at boot.

## Security considerations

The autoconfig file contains your API key and master password in plain text. Handle it with care:

- Do not commit autoconfig files with real API keys to version control
- Delete the autoconfig file from the USB drive after setup if it contains sensitive data
- For production deployments, consider using the kernel boot parameter to point to a file on an encrypted or network-mounted volume
