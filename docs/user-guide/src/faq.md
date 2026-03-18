# FAQ

## General

### What is AiOS?

AiOS is an AI-native Linux distribution. It is a complete operating system where the primary interface is an AI assistant. You interact through voice and text, and the AI uses tools to control the system on your behalf.

### Is AiOS free?

The AiOS operating system is free to use. However, the AI assistant requires an API key from Anthropic (Claude) or OpenAI, which have their own pricing. AiOS implements extensive cost optimization to minimize API spending.

### Does AiOS require internet?

Yes. The AI assistant communicates with cloud LLM providers (Anthropic or OpenAI) over HTTPS. Without internet, the AI cannot respond. However, voice recognition and text-to-speech run locally and do not require internet.

### Can I dual-boot AiOS with another OS?

AiOS currently installs to an entire disk. It does not support dual-boot configurations. For testing alongside another OS, use live mode (boot from USB) or a virtual machine.

### What hardware does AiOS support?

AiOS runs on any x86_64 machine with at least 4 GB of RAM. It includes non-free firmware from Debian, so most Wi-Fi adapters, graphics cards, and other hardware work out of the box.

## AI and Privacy

### Is my voice recorded or sent to the cloud?

No. Voice recognition (Whisper) runs entirely on your local machine. Only the transcribed text is sent to the LLM provider as part of your conversation. Audio never leaves your computer.

### Are my conversations stored?

Conversations exist in memory during your session. In live mode, they are lost at shutdown. With a hard drive installation, conversation history may persist in the episodic memory system. Conversations are sent to the LLM provider (Anthropic or OpenAI) for processing -- refer to their privacy policies for how they handle API data.

### Can the AI run commands without my permission?

The AI always tells you what it plans to do before using a tool. For sensitive operations, it requests explicit permission. The AI will never silently execute something you did not ask for.

### Where are my API keys stored?

API keys are stored in an encrypted vault (`~/.aios/vault.enc`) using AES-256-GCM encryption with an Argon2id-derived key from your master password. They are never stored in plain text on disk (except temporarily in autoconfig files, which you should delete after use).

## Setup and Installation

### I lost my master password. Can I recover it?

No. The master password is used to derive the encryption key and is never stored. If you lose it, delete the vault file and reconfigure:

```bash
rm ~/.aios/vault.enc
```

Then restart AiOS or run `/configure` to set up a new vault.

### Can I change my API key after setup?

Yes. Use the `/key` command at any time:

```
/key claude sk-ant-your-new-key
```

### Can I use both Claude and OpenAI?

Yes. Configure both with `/key`, then set one as primary with `/provider`. The other becomes a backup. You can switch between them at any time.

### How do I skip the setup wizard?

Place an autoconfig file on the USB drive or bake it into the ISO. See [Unattended Installation](installation/unattended.md).

## Usage

### Why is the AI slow to respond?

Response time depends on your internet connection, the LLM provider's load, and the model being used. More expensive models (Opus, GPT-4) take longer. Try:
- `/effort low` for faster responses
- `/mode saver` for cheaper, faster models
- Check your internet speed from a terminal

### Can I use AiOS without voice?

Yes. Disable voice with `/mic off` and `/speaker off`. The text input is always available and provides full functionality.

### How do I open a file manager?

AiOS does not include a traditional file manager. Ask the AI to manage files: "List files in my home directory," "Find large files," "Move this file." You can also open a terminal with `Alt+Enter` for direct file system access.

### Can I install regular Linux applications?

AiOS is based on Debian. You can open a terminal (`Alt+Enter`) and use `apt` to install packages:

```bash
sudo apt update
sudo apt install package-name
```

However, AiOS is designed as a single-purpose AI interface. Installing desktop applications may not integrate well with the minimal compositor.

### How do I take a screenshot?

Press the `Print` key. The screenshot is captured by `grim` and saved to your home directory.

## Channels

### Can multiple people use AiOS at the same time?

AiOS has one active channel at a time. Multiple people can view the web interface simultaneously, but only one channel (the one that sent the most recent message) is active. The others see a notification that the conversation has moved.

### Can I access AiOS from outside my local network?

The web channel is designed for local network access only. For remote access, the Signal channel works from anywhere with internet. Setting up a VPN or reverse proxy to the web channel is possible but not officially supported.

### Does the web channel work on my phone?

Yes. Open `http://aios.local` (or the IP address) in your phone's browser. The web interface is responsive and works on mobile devices.

## Cost

### How much does it cost to use AiOS?

The AiOS software is free. API costs depend on your usage and provider:
- Light usage (casual questions): a few dollars per month
- Heavy usage (complex analysis, long conversations): potentially more

AiOS's cost optimization features (cascading models, caching, tool bundling) typically reduce costs by 70-90% compared to naive API usage. The `/mode saver` setting minimizes costs further.

### Which provider is cheaper?

Both providers have similar pricing tiers. Claude Haiku and GPT-4o-mini are the cheapest options. AiOS's `/mode saver` automatically uses these cheaper models where possible.
