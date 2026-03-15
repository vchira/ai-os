"""Slash command handler for AiOS configuration and control."""

import subprocess
import shlex
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from aios.config.manager import ConfigManager
    from aios.llm.manager import LLMManager


class CommandHandler:
    """Handles /commands typed in the prompt."""

    def __init__(self, config_mgr: "ConfigManager", llm_manager: "LLMManager", voice_controller=None):
        self.config = config_mgr
        self.llm = llm_manager
        self.voice = voice_controller
        self._commands = {
            "/help": self._cmd_help,
            "/key": self._cmd_key,
            "/provider": self._cmd_provider,
            "/model": self._cmd_model,
            "/keyboard": self._cmd_keyboard,
            "/resolution": self._cmd_resolution,
            "/theme": self._cmd_theme,
            "/voice": self._cmd_voice,
            "/mic": self._cmd_mic,
            "/speaker": self._cmd_speaker,
            "/language": self._cmd_language,
            "/selftest": self._cmd_selftest,
            "/reset": self._cmd_reset,
            "/info": self._cmd_info,
            "/tools": self._cmd_tools,
            "/plugin": self._cmd_plugin,
            "/clear": self._cmd_clear,
        }

    def execute(self, command_str: str) -> str:
        """Execute a slash command and return the result text."""
        parts = command_str.strip().split(None, 1)
        cmd = parts[0].lower()
        args = parts[1] if len(parts) > 1 else ""

        handler = self._commands.get(cmd)
        if handler:
            try:
                return handler(args)
            except Exception as e:
                return f"Error executing {cmd}: {e}"
        else:
            return f"Unknown command: {cmd}\nType /help for available commands."

    def _cmd_help(self, args: str) -> str:
        return """Available commands:

/key <provider> <api-key>   Set API key (e.g., /key claude sk-ant-...)
/provider <name>            Switch LLM provider (claude, openai)
/model <name>               Set the model for current provider
/keyboard <layout> [var]    Set keyboard layout (e.g., /keyboard de)
/resolution <WxH>           Set screen resolution (e.g., /resolution 1920x1080)
/theme <dark|light|auto>    Set UI theme
/voice <voice-id>           Set TTS voice (e.g., /voice en_US-ryan-medium)
/mic <on|off>               Enable/disable voice input
/speaker <on|off>           Enable/disable voice output
/language <code>            Set STT language (blank=auto)
/tools                      List available tools
/plugin install <name>      Install a tool plugin
/plugin remove <name>       Remove a tool plugin
/plugin list                List installed plugins
/selftest                   Run self-test (simulated conversation)
/info                       Show system information
/reset [key]                Reset config to defaults
/clear                      Clear chat history
/help                       Show this help"""

    def _cmd_key(self, args: str) -> str:
        parts = args.strip().split(None, 1)
        if len(parts) != 2:
            return "Usage: /key <provider> <api-key>\nExample: /key claude sk-ant-..."

        provider, key = parts
        provider = provider.lower()

        if provider == "claude":
            self.config.set("llm.claude_api_key", key)
            if self.llm:
                self.llm.set_api_key("claude", key)
            return f"Claude API key set ({key[:12]}...)"
        elif provider == "openai":
            self.config.set("llm.openai_api_key", key)
            if self.llm:
                self.llm.set_api_key("openai", key)
            return f"OpenAI API key set ({key[:12]}...)"
        else:
            return f"Unknown provider: {provider}. Supported: claude, openai"

    def _cmd_provider(self, args: str) -> str:
        name = args.strip().lower()
        if name not in ("claude", "openai"):
            return "Usage: /provider <claude|openai>"
        self.config.set("llm.provider", name)
        if self.llm:
            self.llm.set_active(name)
        return f"Switched to {name}"

    def _cmd_model(self, args: str) -> str:
        model = args.strip()
        if not model:
            return "Usage: /model <model-name>\nExamples: claude-sonnet-4-20250514, gpt-4o"
        provider = self.config.get("llm.provider", "claude")
        self.config.set(f"llm.{provider}_model", model)
        return f"Model set to {model} for {provider}"

    def _cmd_keyboard(self, args: str) -> str:
        parts = args.strip().split()
        if not parts:
            return "Usage: /keyboard <layout> [variant]\nExamples: /keyboard de, /keyboard us intl"

        layout = parts[0]
        variant = parts[1] if len(parts) > 1 else ""

        self.config.set("system.keyboard_layout", layout)
        self.config.set("system.keyboard_variant", variant)

        # Apply immediately
        try:
            cmd = ["setxkbmap", "-layout", layout]
            if variant:
                cmd += ["-variant", variant]
            subprocess.run(cmd, check=True, capture_output=True, timeout=5)
            return f"Keyboard layout set to {layout}" + (f" ({variant})" if variant else "")
        except FileNotFoundError:
            # Try swaymsg for Wayland
            try:
                sway_cmd = f'input "*" xkb_layout "{layout}"'
                if variant:
                    sway_cmd += f'\ninput "*" xkb_variant "{variant}"'
                subprocess.run(
                    ["swaymsg", sway_cmd] if not variant else ["bash", "-c", f'swaymsg \'input "*" xkb_layout "{layout}"\' && swaymsg \'input "*" xkb_variant "{variant}"\''],
                    check=True, capture_output=True, timeout=5,
                )
                return f"Keyboard layout set to {layout}" + (f" ({variant})" if variant else "")
            except Exception:
                return f"Keyboard layout saved as {layout}. Will apply on next session."
        except Exception as e:
            return f"Keyboard layout saved as {layout}. Could not apply immediately: {e}"

    def _cmd_resolution(self, args: str) -> str:
        res = args.strip()
        if not res or "x" not in res.lower():
            return "Usage: /resolution <WxH>\nExample: /resolution 1920x1080"

        try:
            # Try wlr-randr for Wayland
            subprocess.run(
                ["wlr-randr", "--output", "eDP-1", "--mode", res],
                check=True, capture_output=True, timeout=5,
            )
            return f"Resolution set to {res}"
        except FileNotFoundError:
            try:
                # Try xrandr as fallback
                subprocess.run(
                    ["xrandr", "--output", "eDP-1", "--mode", res],
                    check=True, capture_output=True, timeout=5,
                )
                return f"Resolution set to {res}"
            except Exception:
                return f"Resolution {res} saved. Install wlr-randr to apply: sudo apt install wlr-randr"
        except Exception as e:
            return f"Could not set resolution: {e}"

    def _cmd_theme(self, args: str) -> str:
        theme = args.strip().lower()
        if theme not in ("dark", "light", "auto"):
            return "Usage: /theme <dark|light|auto>"
        self.config.set("ui.theme", theme)
        return f"Theme set to {theme}"

    def _cmd_voice(self, args: str) -> str:
        voice_id = args.strip()
        if not voice_id:
            # List available voices
            if self.voice:
                voices = self.voice.list_voices()
                lines = ["Available voices:"]
                for v in voices:
                    dl = " [downloaded]" if v.get("downloaded") else ""
                    lines.append(f"  {v['id']} - {v['name']} ({v['language']}, {v['gender']}){dl}")
                return "\n".join(lines)
            return "Usage: /voice <voice-id>\nExample: /voice en_US-ryan-medium"

        self.config.set("voice.tts_voice", voice_id)
        if self.voice:
            self.voice.set_voice(voice_id)
        return f"Voice set to {voice_id}"

    def _cmd_mic(self, args: str) -> str:
        state = args.strip().lower()
        if state not in ("on", "off"):
            return "Usage: /mic <on|off>"
        enabled = state == "on"
        self.config.set("voice.stt_enabled", enabled)
        if self.voice:
            if enabled:
                self.voice.enable_stt()
            else:
                self.voice.disable_stt()
        return f"Voice input {'enabled' if enabled else 'disabled'}"

    def _cmd_speaker(self, args: str) -> str:
        state = args.strip().lower()
        if state not in ("on", "off"):
            return "Usage: /speaker <on|off>"
        enabled = state == "on"
        self.config.set("voice.tts_enabled", enabled)
        if self.voice:
            if enabled:
                self.voice.enable_tts()
            else:
                self.voice.disable_tts()
        return f"Voice output {'enabled' if enabled else 'disabled'}"

    def _cmd_language(self, args: str) -> str:
        lang = args.strip()
        self.config.set("voice.stt_language", lang)
        if self.voice:
            self.voice.set_language(lang)
        if lang:
            return f"STT language set to {lang}"
        return "STT language set to auto-detect"

    def _cmd_selftest(self, args: str) -> str:
        from aios.selftest.runner import SelfTestRunner

        runner = SelfTestRunner(self.config, self.llm)
        return runner.run_all()

    def _cmd_reset(self, args: str) -> str:
        key = args.strip() or None
        self.config.reset(key)
        if key:
            return f"Reset {key} to default"
        return "All settings reset to defaults"

    def _cmd_info(self, args: str) -> str:
        import platform
        import os

        lines = [
            "AiOS System Information",
            "=" * 40,
            f"AiOS Version: 2.0.0",
            f"Kernel: {platform.release()}",
            f"Architecture: {platform.machine()}",
            f"Python: {platform.python_version()}",
            f"Provider: {self.config.get('llm.provider', 'claude')}",
            f"STT: {'enabled' if self.config.get('voice.stt_enabled') else 'disabled'}",
            f"TTS: {'enabled' if self.config.get('voice.tts_enabled') else 'disabled'}",
            f"Voice: {self.config.get('voice.tts_voice', 'default')}",
            f"Theme: {self.config.get('ui.theme', 'dark')}",
            f"Keyboard: {self.config.get('system.keyboard_layout', 'us')}",
        ]
        return "\n".join(lines)

    def _cmd_tools(self, args: str) -> str:
        if self.llm and hasattr(self.llm, "tool_registry") and self.llm.tool_registry:
            tools = self.llm.tool_registry.list_tools()
            if not tools:
                return "No tools registered."
            lines = ["Available tools:"]
            for t in tools:
                lines.append(f"  {t.name} - {t.description}")
            return "\n".join(lines)
        return "Tool registry not available."

    def _cmd_plugin(self, args: str) -> str:
        parts = args.strip().split(None, 1)
        if not parts:
            return "Usage: /plugin <install|remove|list> [name]"

        action = parts[0].lower()
        name = parts[1] if len(parts) > 1 else ""

        from aios.tools.store import ToolStore

        store = ToolStore(self.config.get("tools.store_url"))

        if action == "list":
            plugins = store.list_installed()
            if not plugins:
                return "No plugins installed."
            lines = ["Installed plugins:"]
            for p in plugins:
                lines.append(f"  {p.name} v{p.version} - {p.description}")
            return "\n".join(lines)
        elif action == "install":
            if not name:
                return "Usage: /plugin install <name>"
            result = store.install(name)
            return result
        elif action == "remove":
            if not name:
                return "Usage: /plugin remove <name>"
            result = store.uninstall(name)
            return result
        else:
            return f"Unknown plugin action: {action}. Use install, remove, or list."

    def _cmd_clear(self, args: str) -> str:
        return "__CLEAR__"
