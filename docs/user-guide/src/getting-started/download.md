# Download

## Getting the ISO

Download the latest AiOS ISO image from the official releases page:

> **[github.com/AiOS-Project/ai-os/releases](https://github.com/AiOS-Project/ai-os/releases)**

The ISO file is approximately 900 MB. The filename follows the pattern:

```
aios-<version>.iso
```

For example: `aios-2.0.31.iso`

## Verify the download

Each release includes a SHA256 checksum file. After downloading, verify the integrity of your ISO:

**Linux / macOS:**
```bash
sha256sum aios-2.0.31.iso
```

Compare the output with the checksum listed on the release page.

**Windows (PowerShell):**
```powershell
Get-FileHash aios-2.0.31.iso -Algorithm SHA256
```

## What is included

The AiOS ISO is a complete, self-contained system. It includes:

- Debian Bookworm (12) base with non-free firmware
- Wayland compositor (labwc)
- AiOS application (Rust binary, approximately 6 MB)
- Voice recognition (whisper.cpp with a bundled model)
- Text-to-speech (Piper with a bundled voice, espeak-ng as fallback)
- PipeWire audio system
- GTK4/libadwaita desktop interface
- Web channel server
- All required libraries and dependencies

No additional downloads are needed after booting. The only external requirement is an internet connection to reach your LLM provider's API.

## Building from source

If you prefer to build your own ISO, you can do so from the source repository. This requires Docker:

```bash
git clone https://github.com/AiOS-Project/ai-os.git
cd ai-os
./start.sh          # Build and boot in QEMU
./start.sh --clean  # Full clean rebuild
```

See the project README for detailed build instructions and prerequisites.
