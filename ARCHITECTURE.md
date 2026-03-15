# AiOS v2.0 — Architecture

> *An AI-native Linux distribution where the AI is the primary interface
> between human intent and system execution.*

## Vision

Users don't navigate menus or manage files. They speak or type what they want,
and the AI resolves intent into action — using tools, system commands, and
knowledge to fulfill the request. AiOS provides the OS substrate that makes
this seamless: voice I/O, tool execution, and a minimal Wayland desktop focused
entirely on the AI conversation.

**AiOS is not an AI chatbot running on Linux. It is a Linux distribution
designed from the ground up so that AI is the primary user interface.**

---

## System Architecture

```
╔══════════════════════════════════════════════════════════════════╗
║                        HUMAN                                     ║
║                  Voice / Text / Gestures                         ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  ┌─────────────────── AiOS Application ───────────────────────┐  ║
║  │                                                             │  ║
║  │  ┌──────────┐  ┌──────────────┐  ┌────────────────────┐   │  ║
║  │  │  Voice    │  │   Chat UI    │  │   Settings UI      │   │  ║
║  │  │  I/O      │  │  (GTK4/Adw) │  │   (Preferences)    │   │  ║
║  │  │          │  │              │  │                    │   │  ║
║  │  │ STT(Whisper) │  │ Messages     │  │ Provider, Voice,  │   │  ║
║  │  │ TTS(Piper)│  │ Tool results │  │ Theme, Keyboard   │   │  ║
║  │  │ Audio I/O │  │ Images       │  │ Plugins           │   │  ║
║  │  └─────┬────┘  └──────┬───────┘  └────────┬───────────┘   │  ║
║  │        │               │                    │               │  ║
║  │  ┌─────┴───────────────┴────────────────────┴──────────┐   │  ║
║  │  │              LLM Manager (Tool-Call Loop)            │   │  ║
║  │  │                                                      │   │  ║
║  │  │  ┌─────────┐  ┌──────────┐  ┌───────────────────┐  │   │  ║
║  │  │  │ Claude  │  │ OpenAI   │  │ (Future providers)│  │   │  ║
║  │  │  │ Provider│  │ Provider │  │                   │  │   │  ║
║  │  │  └─────────┘  └──────────┘  └───────────────────┘  │   │  ║
║  │  └──────────────────────┬──────────────────────────────┘   │  ║
║  │                         │                                   │  ║
║  │  ┌──────────────────────┴──────────────────────────────┐   │  ║
║  │  │              Tool Registry & Executor                │   │  ║
║  │  │                                                      │   │  ║
║  │  │  Built-in:   memory, display, system, files, web    │   │  ║
║  │  │  Plugins:    ~/.aios/plugins/ (entry points)        │   │  ║
║  │  │  Store:      https://store.aios.dev/api/v1          │   │  ║
║  │  └─────────────────────────────────────────────────────┘   │  ║
║  └─────────────────────────────────────────────────────────────┘  ║
║                                                                  ║
╠══════════════════════════════════════════════════════════════════╣
║  ┌────────────────────────────────────────────────────────────┐  ║
║  │                    Wayland Desktop                          │  ║
║  │                                                             │  ║
║  │  labwc (compositor)    foot (terminal)    fuzzel (launcher) │  ║
║  │  mako (notifications)  swaybg (wallpaper) grim (screenshot)│  ║
║  └────────────────────────────────────────────────────────────┘  ║
║                                                                  ║
╠══════════════════════════════════════════════════════════════════╣
║  ┌────────────────────────────────────────────────────────────┐  ║
║  │                    Linux Userspace                          │  ║
║  │                                                             │  ║
║  │  PipeWire (audio)    NetworkManager    systemd    greetd   │  ║
║  │  BlueZ (bluetooth)   PulseAudio compat  udev     polkit   │  ║
║  └────────────────────────────────────────────────────────────┘  ║
║                                                                  ║
╠══════════════════════════════════════════════════════════════════╣
║  ┌────────────────────────────────────────────────────────────┐  ║
║  │              Linux Kernel (Debian Bookworm)                 │  ║
║  │                                                             │  ║
║  │  All drivers: WiFi, GPU, USB, NVMe, Bluetooth, Audio, ...  │  ║
║  │  Non-free firmware: Intel, Realtek, Atheros, Broadcom, ... │  ║
║  └────────────────────────────────────────────────────────────┘  ║
║                                                                  ║
╠══════════════════════════════════════════════════════════════════╣
║  HARDWARE: x86_64 | ARM64 | Any Linux-supported platform       ║
╚══════════════════════════════════════════════════════════════════╝
```

---

## Core Design Principles

### 1. AI-First, Not AI-Bolted-On

The entire desktop session is the AI conversation. There is no traditional
desktop with app icons. labwc runs a single auto-started application: the
AiOS AI interface. The AI can launch other applications via tools, but
the conversational interface is always primary.

### 2. Voice-Native

Voice is the default input/output modality:
- **STT**: faster-whisper running locally — no cloud dependency
- **TTS**: Piper TTS running locally — configurable voices, languages, gender
- **Fallback**: Text prompt always available; voice can be toggled off
- **Push-to-talk**: Hold the record button, release to transcribe

### 3. Tools as the Execution Layer

The AI doesn't execute code directly. It uses **tools** — structured,
sandboxed functions that the AI can call to interact with the system:

```
User: "What time is it in Tokyo?"
AI → tool_call: system.get_datetime(timezone="Asia/Tokyo")
Tool → result: "2025-01-15 22:30:00 JST"
AI: "It's 10:30 PM in Tokyo."
```

Tools are the API between AI and system. They are:
- **Typed**: JSON Schema parameters, validated before execution
- **Sandboxed**: No arbitrary code execution
- **Pluggable**: Install new tools from the online store
- **Transparent**: Every tool call is shown to the user

### 4. Maximum Hardware Support

By building on Debian + Linux kernel, AiOS inherits support for virtually
all hardware: WiFi, GPU, Bluetooth, USB, NVMe, audio, webcams, printers.
Non-free firmware is included by default.

---

## Component Details

### Voice System

```
                    ┌─────────────────┐
                    │ Voice Controller │
                    │                  │
                    │ Orchestrates     │
                    │ STT/TTS/Audio    │
                    └───────┬──────────┘
                            │
              ┌─────────────┼─────────────┐
              │             │             │
    ┌─────────┴───┐ ┌──────┴─────┐ ┌─────┴────────┐
    │ Audio I/O   │ │    STT     │ │     TTS      │
    │             │ │            │ │              │
    │ PipeWire/   │ │ faster-    │ │ Piper TTS   │
    │ ALSA via    │ │ whisper    │ │              │
    │ sounddevice │ │            │ │ Voices:      │
    │             │ │ Models:    │ │ en_US-amy-f  │
    │ 16kHz mono  │ │ tiny/base/ │ │ en_US-ryan-m │
    │ float32     │ │ small/     │ │ de_DE-thorsten│
    │             │ │ medium/    │ │ fr_FR-siwis  │
    │ VAD energy  │ │ large-v3   │ │ es/it/pt/... │
    └─────────────┘ └────────────┘ └──────────────┘
```

- Models stored in `~/.aios/models/{whisper,piper}/`
- Downloaded on first use (or pre-loaded in ISO)
- STT supports all Whisper languages with accent robustness
- TTS voices configurable: gender (male/female/neutral), speed, pitch

### LLM Provider System

```
    ┌──────────────────────────────┐
    │        LLM Manager           │
    │                              │
    │  send_message() ─────────┐  │
    │                          │  │
    │  if response.tool_calls: │  │
    │    execute tools         │  │
    │    feed results back     │  │
    │    loop until done       │  │
    └──────────┬───────────────┘  │
               │                  │
    ┌──────────┴──────────┐      │
    │                     │      │
┌───┴─────────┐ ┌─────────┴──┐  │
│   Claude    │ │   OpenAI   │  │
│   Provider  │ │   Provider │  │
│             │ │            │  │
│ anthropic   │ │ openai SDK │  │
│ SDK         │ │            │  │
│             │ │ gpt-4o     │  │
│ claude-     │ │ gpt-4-turbo│  │
│ sonnet-4   │ │ ...        │  │
└─────────────┘ └────────────┘
```

The tool-call loop:
1. Send user message + tool schemas to LLM
2. LLM responds (possibly with tool_calls)
3. Execute each tool_call, collect results
4. Feed tool results back to LLM
5. Repeat until LLM responds with plain text

### Tool Plugin System

```
~/.aios/plugins/
  weather/
    __init__.py
    plugin.toml          # name, version, description, author
    weather_tool.py      # Tool implementation

# plugin.toml
[plugin]
name = "weather"
version = "1.0.0"
description = "Get weather forecasts"
author = "community"
tools = ["weather_tool:WeatherTool"]
```

**Built-in tools:**

| Tool | Actions | Description |
|------|---------|-------------|
| `memory` | memorize, recall, forget, list_keys | Persistent key-value store |
| `display` | show_image, show_notification, show_markdown | UI display actions |
| `system` | run_command, get_system_info, get_datetime, list_processes | System operations |
| `files` | read_file, write_file, list_directory, search_files | File operations |
| `web` | fetch_url, search_web, download_file | Web access |

**Online store:**
- `https://store.aios.dev/api/v1` (configurable)
- `/plugin install <name>` — download + install
- `/plugin remove <name>` — uninstall
- `/plugin list` — show installed

### Configuration

All config in `~/.aios/config.json` (JSON, managed by ConfigManager):

```json
{
  "llm": {
    "provider": "claude",
    "claude_api_key": "sk-ant-...",
    "claude_model": "claude-sonnet-4-20250514",
    "openai_api_key": "sk-...",
    "openai_model": "gpt-4o"
  },
  "voice": {
    "stt_enabled": true,
    "stt_model": "medium",
    "stt_language": "",
    "tts_enabled": true,
    "tts_voice": "en_US-amy-medium",
    "tts_gender": "female",
    "tts_rate": 1.0
  },
  "ui": { "theme": "dark" },
  "system": { "keyboard_layout": "us" }
}
```

Configurable via:
- Slash commands (`/keyboard de`, `/voice en_US-ryan-medium`)
- Settings dialog (gear icon in header bar)
- Direct config file edit

---

## Distribution Build

### How the ISO is built

```
Debian Bookworm ──► live-build ──► AiOS ISO
                        │
                        ├── Package lists (base, desktop, aios, installer)
                        ├── Build hooks (create user, install app, configure greetd)
                        ├── Chroot includes (labwc config, AiOS app, wallpaper)
                        └── Calamares config (for installation to disk)
```

The ISO is both **live** (boots directly) and **installable** (via Calamares).

### Boot flow

```
BIOS/UEFI → GRUB → Linux kernel → systemd → greetd → labwc → AiOS app
```

1. greetd auto-logs in as `aios` user
2. labwc starts as Wayland compositor
3. labwc autostart runs `aios-session.sh`
4. Session script starts AiOS app (full-screen AI interface)

### Desktop Environment

- **labwc**: wlroots-based Wayland compositor (lightweight, configurable)
- **foot**: Terminal emulator (Alt+Enter to open)
- **fuzzel**: Application launcher (Super key)
- **mako**: Notification daemon
- **grim/slurp**: Screenshot tools

---

## Self-Test System

`/selftest` runs without a real LLM, testing:

1. **Config roundtrip** — write/read config values
2. **Tool registry** — built-in tools load with valid schemas
3. **Memory tool** — memorize/recall/forget cycle
4. **System tool** — datetime and system info retrieval
5. **Command handler** — /help, /info, /theme parsing
6. **Mock conversation** — simulated tool-call flow
7. **Tool schema validation** — schemas conform to JSON Schema
8. **Keyboard config** — layout change persists
9. **Provider switching** — provider change persists

---

## Directory Structure

```
ai-os/
  aios-app/                     # Main AI application (Python + GTK4)
    pyproject.toml              # Package config + entry points
    aios/
      app.py                    # GTK4 application entry point
      ui/
        main_window.py          # Primary window
        chat_view.py            # Chat message display
        prompt_input.py         # Text input + push-to-talk
        settings_dialog.py      # Preferences dialog
      voice/
        controller.py           # Voice orchestration
        stt.py                  # Speech-to-text (Whisper)
        tts.py                  # Text-to-speech (Piper)
        audio.py                # Audio capture/playback
      llm/
        base.py                 # Provider interface
        claude.py               # Claude API provider
        openai_provider.py      # OpenAI API provider
        manager.py              # Provider manager + tool loop
      tools/
        base.py                 # Tool interface
        registry.py             # Tool discovery
        store.py                # Online plugin store
        builtin/                # Built-in tools
          memory.py             # Key-value memory
          display.py            # Image/notification display
          system_tools.py       # System commands
          files.py              # File operations
          web.py                # Web fetch/search
      config/
        manager.py              # Config persistence
        commands.py             # Slash command handler
      selftest/
        runner.py               # Test runner
        scenarios.py            # Test scenario definitions
    tests/                      # pytest unit tests

  distro/                       # Debian live-build config
    build.sh                    # Main build script
    run-qemu.sh                 # QEMU test launcher
    auto/                       # live-build auto scripts
    config/
      package-lists/            # Debian package lists
      hooks/                    # Build hooks
      includes.chroot/          # Files for the live system

  Makefile                      # Build targets
  CLAUDE.md                     # Project instructions
  ARCHITECTURE.md               # Detailed architecture
```

---

## Roadmap

### v2.0 (Current)
- [x] Linux-based distribution (Debian Bookworm)
- [x] Wayland desktop (labwc)
- [x] GTK4 AI interface
- [x] Voice I/O (Whisper STT + Piper TTS)
- [x] Multi-provider LLM (Claude + OpenAI)
- [x] Tool plugin system
- [x] Self-test framework
- [x] Bootable/installable ISO

### v2.1 (Planned)
- [ ] Online tool store (store.aios.dev)
- [ ] Streaming LLM responses
- [ ] Image generation tools
- [ ] Multi-monitor support
- [ ] Accessibility improvements

### v2.2 (Future)
- [ ] Local LLM support (llama.cpp, ollama)
- [ ] Multi-agent workflows
- [ ] Sandboxed code execution tool
- [ ] Screen sharing / remote assistance
- [ ] Mobile companion app
