# AiOS — AI-Native Linux Distribution

## Core Philosophy

AiOS is an AI-first Linux distribution. **Humans interact through voice and text.** The OS exists as a substrate for AI — the AI is the primary interface between human intent and system execution.

The AI is the primary actor. Humans ask questions or express intent through voice or the text prompt. The AI interprets intent, decides what to do, and uses tools to fulfill it.

## AI Interaction Model

- Humans interact through **voice** (default) or **text prompt**
- Voice input uses local Whisper STT (multilingual, accent-aware)
- Voice output uses local Piper TTS (configurable gender, language, voice)
- Voice can be toggled independently: mic on/off, speaker on/off
- When voice is disabled, the interface falls back to text-only
- The AI receives the human's request along with system context (date/time, tools, hardware state)
- If the AI can answer directly, it does
- **If a tool is needed** to execute what the human wants:
  1. The AI responds that it needs a tool and explains exactly what it can do
  2. If the human allows it, the AI uses the tool
  3. The AI explains exactly what the tool does, what it did, and shows the result
- The AI should never silently execute something the human didn't ask for
- Tools are expandable via a plugin system — installable from an online store
- API keys are set at runtime via `/key claude <key>` or `/key openai <key>`

## Architecture

- **Base**: Debian Bookworm (12) — maximum hardware support with non-free firmware
- **Display**: Wayland compositor (labwc) — lightweight, wlroots-based
- **Application**: Python + GTK4/libadwaita — native Wayland, modern GNOME look
- **Voice STT**: faster-whisper (local Whisper inference, multilingual)
- **Voice TTS**: Piper TTS (local VITS-based, multi-language, male/female voices)
- **LLM Providers**: Claude API, OpenAI API — switchable at runtime
- **Tools**: Plugin-based system with built-in tools + online store
- **Installer**: Calamares — full system installation from live ISO
- **Audio**: PipeWire (modern Linux audio, replaces PulseAudio + JACK)

## Build

```bash
# Install the AiOS application (development)
make app

# Run in development mode (needs GTK4 + Wayland)
make run

# Run self-tests (no AI needed)
make selftest

# Run unit tests
make test

# Build the bootable ISO (requires sudo + live-build)
make iso

# Test in QEMU
make qemu
```

### API Keys

Set at runtime in the app:
```
/key claude sk-ant-...
/key openai sk-...
```

Or configure via Settings dialog (gear icon).

### Build Dependencies

For the ISO:
```bash
sudo apt install live-build live-boot live-config
```

For development:
```bash
sudo apt install python3-gi python3-gi-cairo gir1.2-gtk-4.0 gir1.2-adw-1 \
    python3-pip python3-venv portaudio19-dev
cd aios-app && pip install -e ".[dev]"
```

## Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/key <provider> <key>` | Set API key |
| `/provider <name>` | Switch LLM provider (claude, openai) |
| `/model <name>` | Set model for current provider |
| `/keyboard <layout>` | Set keyboard layout (e.g., de, fr, us) |
| `/resolution <WxH>` | Set screen resolution |
| `/theme <dark\|light\|auto>` | Set UI theme |
| `/voice <voice-id>` | Set TTS voice |
| `/mic <on\|off>` | Toggle voice input |
| `/speaker <on\|off>` | Toggle voice output |
| `/language <code>` | Set STT language (blank=auto) |
| `/tools` | List available tools |
| `/plugin install <name>` | Install a tool plugin |
| `/plugin list` | List installed plugins |
| `/selftest` | Run self-test suite |
| `/info` | Show system information |
| `/clear` | Clear chat history |

## Key Files

### Application (`aios-app/`)
- `aios/app.py` — Main GTK4 application entry point
- `aios/ui/main_window.py` — Primary window with chat, voice controls
- `aios/ui/chat_view.py` — Chat message display
- `aios/ui/prompt_input.py` — Text input + push-to-talk button
- `aios/ui/settings_dialog.py` — Settings UI (Adw.PreferencesWindow)
- `aios/llm/base.py` — LLM provider abstract interface
- `aios/llm/claude.py` — Claude API provider
- `aios/llm/openai_provider.py` — OpenAI API provider
- `aios/llm/manager.py` — Provider manager with tool-call loop
- `aios/voice/stt.py` — Speech-to-text (faster-whisper)
- `aios/voice/tts.py` — Text-to-speech (Piper)
- `aios/voice/audio.py` — Audio capture/playback (PipeWire)
- `aios/voice/controller.py` — Voice orchestration
- `aios/tools/base.py` — Tool plugin interface
- `aios/tools/registry.py` — Tool discovery and registration
- `aios/tools/store.py` — Online plugin store client
- `aios/tools/builtin/` — Built-in tools (memory, display, system, files, web)
- `aios/config/manager.py` — TOML configuration manager
- `aios/config/commands.py` — Slash command handler
- `aios/selftest/runner.py` — Self-test engine
- `aios/selftest/scenarios.py` — Test scenarios

### Distribution (`distro/`)
- `build.sh` — Main ISO build script (Debian live-build)
- `run-qemu.sh` — Quick QEMU testing
- `auto/config` — live-build auto configuration
- `config/package-lists/` — Debian package lists
- `config/hooks/` — Build hooks (user creation, app install)
- `config/includes.chroot/` — Files included in the live system

## Tool Plugin System

Tools are Python classes implementing the `Tool` interface:

```python
from aios.tools.base import Tool, ToolResult

class MyTool(Tool):
    @property
    def name(self) -> str:
        return "my_tool"

    @property
    def description(self) -> str:
        return "Does something useful"

    @property
    def parameters(self) -> dict:
        return {
            "type": "object",
            "properties": {
                "input": {"type": "string", "description": "The input"}
            },
            "required": ["input"]
        }

    def execute(self, **kwargs) -> ToolResult:
        return ToolResult(success=True, output="Done!")
```

Plugins are installed to `~/.aios/plugins/` and discovered via Python entry points.
