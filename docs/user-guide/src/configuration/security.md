# Security & Vault

AiOS takes security seriously. API keys and sensitive data are stored in an encrypted vault, tools run in sandboxes, and the AI always asks before accessing protected resources.

## The encrypted vault

All secrets (API keys, passwords, personal information) are stored in an encrypted vault file:

```
~/.aios/vault.enc
```

The vault uses:

- **Argon2id** for key derivation -- converts your master password into an encryption key, resistant to brute-force attacks
- **AES-256-GCM** for encryption -- industry-standard authenticated encryption

The vault file structure: 16-byte salt + 12-byte nonce + encrypted ciphertext.

Your master password is never stored anywhere. It is used only to derive the encryption key in memory.

## Master password

You create your master password during the setup wizard. This password:

- Protects all secrets in the vault
- Is required to unlock the vault after a reboot (if the vault was locked)
- Should be at least 8 characters
- Is never stored in plain text

To change your master password, re-run the setup wizard:

```
/configure
```

## Permission system

When the AI needs to access a secret or perform a sensitive operation, it goes through a permission check:

1. The AI requests access, explaining **what** it needs and **why**
2. If authentication is required, you are prompted for your master password
3. You approve or deny the request
4. If approved, the AI gets temporary access

### Authentication caching

To avoid repeatedly entering your password, AiOS caches authentication for a configurable period:

- After entering your password, you are not asked again for a set number of minutes
- The "don't ask for N minutes" duration is configurable
- Each key in the vault has its own independent permission cache timeout

### What requires permission

- Reading API keys from the vault
- Accessing stored passwords
- Any tool operation marked as sensitive
- Vault operations (adding, modifying, deleting secrets)

## Tool sandboxing

Commands and code executed by the AI run in sandboxes based on risk level:

| Risk level | Sandbox | Examples |
|-----------|---------|---------|
| **Read-only** | None (direct execution) | `ls`, `cat`, `ps`, `df` |
| **Modifying** | Process isolation (timeout + resource limits) | `mv`, `cp`, `mkdir` |
| **Dangerous** | Docker container (isolated, no network, memory limits) | `rm`, `pip install`, `python script.py`, `bash -c "..."` |

Code execution through the `execute_code` tool always uses sandbox isolation -- Docker when available, process isolation as fallback.

## File access scope

The `files` tool is scoped to your home directory. The AI cannot read or write files outside of `~` through the files tool. This prevents accidental (or malicious prompt injection) access to system files.

Direct system commands through the `system` tool are subject to sandbox rules described above.

## Voice privacy

Voice recognition (Whisper) and text-to-speech (Piper/espeak-ng) run entirely on your local machine. Audio is never sent to any cloud service. Only the transcribed text is sent to the LLM provider as part of your message.

## Network security

- LLM API calls use HTTPS (TLS encrypted)
- The web channel runs on HTTP within your local network (no external exposure by default)
- The Signal channel uses the Signal protocol (end-to-end encrypted)
- No telemetry or analytics are sent from AiOS

## Autoconfig security

Autoconfig files contain sensitive data (API keys, master password) in plain text. Handle them carefully:

- Do not commit autoconfig files with real keys to version control
- Delete autoconfig files from USB drives after setup
- For production deployments, consider encrypted storage or network-mounted volumes

See [Unattended Installation](../installation/unattended.md) for more details.

## Best practices

1. **Choose a strong master password.** It protects all your secrets.
2. **Use both providers** so you have a backup if one is compromised or revoked.
3. **Keep AiOS updated** to get security patches.
4. **Disable the web channel** on untrusted networks (`/channel web off`).
5. **Review tool actions** -- the AI always tells you what it is about to do. Read the explanation before approving.
