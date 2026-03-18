# AiOS — The AI-Native Linux Distribution

[![License: BSL 1.1](https://img.shields.io/badge/License-BSL_1.1-blue.svg)](LICENSE)
[![Status: Alpha](https://img.shields.io/badge/Status-Alpha-orange.svg)](https://github.com/swit-work/ai-os/releases)

**Humans interact through voice and text. The AI is the interface.**

AiOS is a complete Linux distribution where AI is the primary way you interact with your computer. Instead of clicking through menus and windows, you talk to your AI assistant — by voice or text — and it uses tools to get things done. The OS exists as a substrate for AI.

## Key Features

- **Voice-first interaction** — wake word detection, speech-to-text, text-to-speech, all running locally on your hardware
- **Multi-channel** — talk to your AI from the desktop (GTK4), a web browser, or Signal messenger
- **Built-in AI tool system** — file operations, web search, system commands, code execution, data processing, and more — extensible via plugins
- **Local voice processing** — all STT (Whisper) and TTS (Piper) runs on-device, nothing leaves your machine
- **VM compatible** — runs in QEMU, VirtualBox, and VMware for easy testing
- **Encrypted vault** — API keys and secrets stored with AES-256-GCM + Argon2id encryption

## Quick Start

### Download

Get the latest ISO from [GitHub Releases](https://github.com/swit-work/ai-os/releases).

### Boot in a VM (recommended for first try)

```bash
qemu-system-x86_64 \
  -m 4G -smp 2 -enable-kvm \
  -cdrom aios-*-amd64.iso \
  -device virtio-vga-gl -display sdl,gl=on \
  -device intel-hda -device hda-duplex \
  -nic user,model=virtio-net-pci
```

### Write to USB

```bash
sudo dd if=aios-*-amd64.iso of=/dev/sdX bs=4M status=progress
```

## Documentation

Full user guide: [aios.pages.dev/docs](https://aios.pages.dev/docs/)

Covers installation, configuration, voice setup, all commands, troubleshooting, and more.

## Building from Source

### Prerequisites

```bash
# Docker is required (builds inside Debian Bookworm container)
docker --version

# For local Rust development
sudo apt install libgtk-4-dev libadwaita-1-dev libasound2-dev libssl-dev
```

### Build and boot

```bash
git clone https://github.com/swit-work/ai-os.git
cd ai-os
./start.sh          # Incremental build + boot in QEMU
./start.sh --clean  # Full clean rebuild
```

## License

AiOS is licensed under the [Business Source License 1.1](LICENSE).

**Free to use** for personal, educational, internal business, and evaluation purposes.

**Commercial use** (distribution, derivative distros, hosted services, hardware bundling) requires written approval from [swIT.work GmbH](https://swit.work).

After 5 years from each release, the code converts to [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).

## Links

- **Website:** [aios.pages.dev](https://aios.pages.dev)
- **Releases:** [GitHub Releases](https://github.com/swit-work/ai-os/releases)
- **Documentation:** [aios.pages.dev/docs](https://aios.pages.dev/docs/)
- **Issues:** [GitHub Issues](https://github.com/swit-work/ai-os/issues)

---

*AiOS is developed by [swIT.work GmbH](https://swit.work), Austria.*
