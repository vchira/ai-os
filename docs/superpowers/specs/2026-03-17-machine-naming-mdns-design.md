# Machine Naming + mDNS — Design Spec

## Problem

All AiOS machines currently use `aios.local` as their hostname. Multiple installations on the same network collide. There's no way for users to name their machine, and no collision detection.

## Requirements

1. Default hostname is `assistant` → reachable as `assistant.aios.local`
2. Default wake word is "Assistant" (same as hostname, independently changeable)
3. At install time (hard drive installer), user picks a machine name (default: `assistant`)
4. First-boot setup asks for wake word (default: "Assistant")
5. On every boot, check for name collisions on the local network
6. If collision found → show popup card with pre-filled suggestion (`<name>-<random4>`)
7. User accepts or types a new name → system updates hostname + Avahi + config
8. Hostname validation: lowercase alphanumeric + hyphens, 1-63 chars, no leading/trailing hyphens
9. Live ISO uses same default (`assistant.aios.local`) — no special treatment

## Architecture

### Hostname Components

| Component | How it's set | File/Config |
|-----------|-------------|-------------|
| System hostname | `hostnamectl set-hostname <name>` | `/etc/hostname` |
| Avahi mDNS | `host-name=<name>` in config, then restart | `/etc/avahi/avahi-daemon.conf` |
| AiOS config | `system.machine_name` | `~/.aios/config.json` |
| Web channel URL | Derived: `http://<name>.aios.local` | Boot status display |
| SSH | Derived from system hostname | Reachable as `<name>.aios.local` |
| Wake word | `voice.wake_word` | `~/.aios/config.json` |

### Boot Collision Check

Runs as a systemd oneshot service (`aios-hostname-check.service`) before `greetd`:

1. Read configured name from `/etc/hostname`
2. Use `avahi-resolve -n <name>.local` to check if another machine responds
3. If collision → write a flag file (`/tmp/aios-name-conflict`) with the conflicting name
4. AiOS app checks for this flag on startup and shows the rename popup card
5. If no collision → boot continues normally

### Rename Popup Card

When `/tmp/aios-name-conflict` exists:
- Show a setup card: "Another machine is using 'assistant' on this network. Choose a different name:"
- Pre-filled with `<name>-<random 4 digits>` (e.g. `assistant-7392`)
- User accepts or types a new name
- On submit: validate name, update `/etc/hostname`, Avahi config, AiOS config, restart Avahi
- Remove the flag file

### Hostname Validation

```
fn is_valid_hostname(name: &str) -> bool {
    let name = name.to_lowercase();
    name.len() >= 1
        && name.len() <= 63
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}
```

### First-Boot Setup Flow (Updated)

1. Welcome
2. Screen resolution (default 1920x1080, confirm or change)
3. Audio test (TTS + mic, per channel)
4. **Install to hard drive?** (ISO only — if yes, hand off to installer conversation)
5. **Wake word** (default "Assistant", text field, user can change)
6. Provider/API key (pre-filled from config)
7. Master password + confirm
8. Summary INFO

### Config Keys

| Key | Default | Description |
|-----|---------|-------------|
| `system.machine_name` | `"assistant"` | The machine's hostname |
| `voice.wake_word` | `"Assistant"` | The wake word for voice activation |

### Build-Time Changes

In `distro/_inner_build.sh`:
- Change default hostname from `aios` to `assistant`
- Update Avahi service files to use `assistant.local`
- Add `aios-hostname-check.service` systemd unit
- Update SSH Avahi service advertisement

### Files to Create/Modify

| File | Change |
|------|--------|
| `aios-core/src/hostname.rs` | New: validation, rename logic, collision flag check |
| `aios-gtk/src/app.rs` | Check for collision flag on startup, show rename card |
| `aios-gtk/src/ui/first_boot.rs` | Add wake word step, screen resolution step |
| `distro/_inner_build.sh` | Default hostname `assistant`, collision check service |
| `aios-core/src/config/defaults.rs` | Add `system.machine_name`, `voice.wake_word` defaults |

### What Stays the Same

- `/wake` command for changing wake word at runtime (already exists)
- Web server code (just reads hostname for display)
- Signal channel (unaffected)
