# /upgrade Command — Design Spec

## Problem

Installed AiOS systems need a way to check for and install updates. Currently there's no upgrade mechanism.

## Requirements

1. `/upgrade` command checks for available updates
2. If no update → tells user, done
3. If update available → shows version info, asks "Install update?"
4. If yes → downloads, installs, reboots
5. Works on installed systems only (not live ISO)

## How It Works

### Version Check

AiOS has a `VERSION` file at the project root. The installed binary knows its version. The update server (or GitHub release) hosts the latest version info.

Check mechanism:
1. Read current version from compiled-in constant or `/usr/bin/aios --version`
2. Fetch latest version from update URL (configurable, default: GitHub releases API)
3. Compare versions (semver)
4. If newer → show changelog summary + ask to update

### Update Process

1. Download new AiOS binary from release URL
2. Verify checksum (SHA256)
3. Replace `/usr/bin/aios` with new binary
4. Apply setcap for port 80 binding
5. Show "Update installed. Reboot to apply?"
6. If yes → reboot

### Update URL

Default: GitHub releases or a simple static URL. Configurable via `system.update_url` config key.

The update is just the AiOS binary — not a full OS upgrade. Full OS upgrades (kernel, packages) are a separate concern.

## `/upgrade` Command Flow

```
User: /upgrade
System: Checking for updates...
System: ✅ Update available: v2.1.0 (current: v2.0.6)
        Changes: Bug fixes, new tools, improved voice
        Install now?
        [Install] [Skip]

User clicks Install:
System: Downloading update...
System: Installing...
System: ✅ Update installed! Reboot to apply.
        [Reboot] [Later]
```

Or if no update:
```
User: /upgrade
System: ✅ You're running the latest version (v2.0.6)
```

## Files

| File | Purpose |
|------|---------|
| `aios-core/src/config/commands.rs` | Add `/upgrade` command handler |
| `aios-core/src/upgrade.rs` | New: version check, download, install logic |
| `aios-gtk/src/app.rs` | Handle upgrade command result (reboot dialog) |
