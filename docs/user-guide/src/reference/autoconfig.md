# Autoconfig Format

The autoconfig file is a JSON document that configures AiOS automatically at boot, skipping the interactive setup wizard. This reference describes every field.

## File locations

AiOS searches for the autoconfig file in this order:

1. **Kernel boot parameter:** `aios.autoconfig=/path/to/file`
2. **Baked into ISO:** `/opt/aios-app/autoconfig.json`
3. **USB drive:** `aios-autoconfig.json` at the root of any mounted removable drive

The first file found is used.

## Complete schema

```json
{
  "provider": {
    "primary": "claude",
    "claude_api_key": "",
    "openai_api_key": ""
  },
  "system": {
    "keyboard": "us",
    "language": "en",
    "timezone": "",
    "hostname": "assistant",
    "master_password": ""
  },
  "install": {
    "enabled": false,
    "target_disk": "",
    "confirm": false
  },
  "assistant": {
    "name": "Assistant",
    "effort": "auto"
  },
  "debug": false
}
```

## Field reference

### provider

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `primary` | string | `"claude"` | Primary LLM provider. Values: `"claude"` or `"openai"`. |
| `claude_api_key` | string | `""` | Anthropic Claude API key. Starts with `sk-ant-`. |
| `openai_api_key` | string | `""` | OpenAI API key. Starts with `sk-`. |

If both keys are provided, the `primary` field determines which is used first. The other becomes the backup provider.

### system

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `keyboard` | string | `"us"` | Keyboard layout code (XKB). Examples: `"us"`, `"de"`, `"fr"`, `"gb"`. |
| `language` | string | `"en"` | Default STT language code. Examples: `"en"`, `"de"`, `"fr"`. |
| `timezone` | string | `""` | System timezone. Examples: `"America/New_York"`, `"Europe/Berlin"`, `"Asia/Tokyo"`. Empty uses UTC. |
| `hostname` | string | `"assistant"` | System hostname. Also used for mDNS (`hostname.local`). |
| `master_password` | string | `""` | Master password for the encrypted vault. Must be at least 8 characters. |

### install

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | boolean | `false` | Whether to install AiOS to a hard drive. |
| `target_disk` | string | `""` | Target disk device. `"auto"` selects the first non-USB disk. A specific path like `"/dev/sda"` targets that exact disk. |
| `confirm` | boolean | `false` | If `false`, the user is still asked to confirm before erasing the disk. If `true`, installation proceeds without confirmation (fully unattended). |

### assistant

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | `"Assistant"` | Display name for the AI assistant. |
| `effort` | string | `"auto"` | Default effort level. Values: `"auto"`, `"low"`, `"medium"`, `"high"`. |

### debug

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `debug` | boolean | `false` | Enable debug mode. Increases logging verbosity. |

## Examples

### Minimal (just an API key)

```json
{
  "provider": {
    "claude_api_key": "sk-ant-your-key-here"
  },
  "system": {
    "master_password": "your-password"
  }
}
```

All other fields use defaults: US keyboard, English language, no installation, auto effort.

### Live mode with German keyboard

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

### Fully unattended hard drive installation

```json
{
  "provider": {
    "primary": "claude",
    "claude_api_key": "sk-ant-your-key-here",
    "openai_api_key": "sk-your-backup-key"
  },
  "system": {
    "keyboard": "us",
    "language": "en",
    "timezone": "America/New_York",
    "hostname": "aios-workstation",
    "master_password": "strong-password-here"
  },
  "install": {
    "enabled": true,
    "target_disk": "auto",
    "confirm": true
  },
  "assistant": {
    "name": "Jarvis",
    "effort": "auto"
  }
}
```

> **Warning:** `confirm: true` with `enabled: true` will erase the target disk without any user interaction. Use with caution.

### Both providers with OpenAI primary

```json
{
  "provider": {
    "primary": "openai",
    "claude_api_key": "sk-ant-your-backup-key",
    "openai_api_key": "sk-your-primary-key"
  },
  "system": {
    "master_password": "your-password"
  }
}
```

## Notes

- All sections and fields are optional. Omitted fields use their defaults.
- The `_description` field (seen in some example files) is ignored by the parser and can be used for documentation.
- The file must be valid JSON. Comments are not supported.
- API keys and passwords are stored in plain text in the autoconfig file. Handle with care.
