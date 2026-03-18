# Interactive Setup Wizard

The first time AiOS boots (when no encrypted vault exists), it launches an interactive setup wizard. The AI assistant walks you through initial configuration using voice and on-screen panels.

## What the wizard configures

1. **AI provider** -- which LLM service to use (Claude or OpenAI)
2. **API key** -- your authentication key for the chosen provider
3. **Master password** -- the password that protects your encrypted vault
4. **Backup provider** (optional) -- a second provider as a fallback

## Step by step

### 1. Welcome

The AI greets you with a spoken message: "Welcome to AiOS!" A panel appears on screen.

If your speakers or microphone are not working, do not worry -- everything is also displayed on screen as text, and you can type your responses.

### 2. Choose your AI provider

A selection panel appears with two options:
- **Claude** (Anthropic)
- **OpenAI**

Click your choice or speak it. This becomes your primary LLM provider.

### 3. Enter your API key

A secure text input appears. Type your API key -- the characters are masked for security.

- For Claude: your key starts with `sk-ant-`
- For OpenAI: your key starts with `sk-`

The AI verifies the key by making a test request. If the key is invalid, you will be prompted to try again.

### 4. Create a master password

A password input appears. Choose a password that will protect your encrypted vault. This vault stores your API keys and other secrets.

Requirements:
- At least 8 characters
- You will need this password if the vault needs to be unlocked after a reboot

The password is processed through Argon2id key derivation and used for AES-256-GCM encryption. It is never stored in plain text.

### 5. Backup provider (optional)

The AI asks if you would like to add a second provider. If you choose yes:
- Select the other provider
- Enter its API key
- Choose which provider is primary and which is the fallback

Having a backup provider means AiOS can switch automatically if your primary provider is unavailable.

### 6. Setup complete

Once all steps are finished, the AI confirms that setup is complete. Your API keys are encrypted and stored in the vault at `~/.aios/vault.enc`. You are now in the main chat interface, ready to interact.

## Re-running the wizard

If you need to change your initial configuration later, you can re-run the setup wizard at any time:

```
/configure
```

This opens the same wizard interface but preserves your existing vault. You can also change individual settings using specific commands like `/key`, `/provider`, and `/model`.

## Skipping the wizard

If an autoconfig file is detected, the setup wizard is skipped entirely. See [Unattended Installation](unattended.md) for details on automated configuration.
