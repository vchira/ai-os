# Remote Upgrade Script — Design Spec

## Problem

During development, pushing updates to a running AiOS machine requires manual SSH + file copy. Need a simple script that automates this.

## Requirements

1. Script runs on the developer's machine (not on AiOS)
2. Discovers AiOS machine on the LAN via mDNS (`<name>.aios.local`)
3. SSH into the machine, copy new binary, restart the app
4. Saves the SSH password so it doesn't ask every time
5. Handles multiple AiOS installations (different hostnames)

## Script: `scripts/push-update.sh`

```bash
Usage: ./scripts/push-update.sh [hostname]

# Default: assistant.aios.local
# Custom: ./scripts/push-update.sh jarvis
#   → connects to jarvis.aios.local
```

### Flow

1. Build the release binary if not fresh: `cargo build --release`
2. Resolve `<hostname>.aios.local`
3. Read saved password from `~/.aios-dev/credentials` (or ask + save)
4. SCP the binary: `scp target/release/aios aios@<host>:/tmp/aios-new`
5. SSH: `aios-update /tmp/aios-new` (the update script already exists in the ISO)
6. Show success/failure

### Password Storage

```bash
# ~/.aios-dev/credentials (simple key=value, chmod 600)
assistant.aios.local=mypassword123
jarvis.aios.local=otherpassword
```

Created on first use. Permission set to 600.

### Multiple Machines

If no hostname given, scan the LAN for AiOS machines:
```bash
avahi-browse -rt _http._tcp 2>/dev/null | grep "Assistant Web Interface"
```

Show found machines and let the user pick, or update all.

## Files

| File | Purpose |
|------|---------|
| `scripts/push-update.sh` | New: the remote upgrade script |
| `scripts/push-update.sh` uses `aios-update` | Already exists in ISO at `/usr/bin/aios-update` |
