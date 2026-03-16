"""AiOS main application — GTK4/Adw AI-native desktop interface."""

import sys
import logging

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Gtk, Adw, GLib

from aios.config.manager import ConfigManager

log = logging.getLogger(__name__)


class AiOSApplication(Adw.Application):
    """The main AiOS application."""

    def __init__(self):
        super().__init__(application_id="dev.aios.app")
        self.config_mgr = ConfigManager()
        self.llm_manager = None
        self.tool_registry = None
        self.voice_controller = None
        self.window = None

    def do_startup(self):
        Adw.Application.do_startup(self)
        self._init_tools()
        self._init_llm()
        self._init_voice()

    def do_activate(self):
        if self.window is None:
            from aios.ui.main_window import MainWindow

            self.window = MainWindow(
                app=self,
                llm_manager=self.llm_manager,
                tool_registry=self.tool_registry,
                voice_controller=self.voice_controller,
                config_mgr=self.config_mgr,
            )

            # Connect voice transcription to UI
            if self.voice_controller:
                self.voice_controller.set_on_transcription(
                    lambda text: GLib.idle_add(self.window.handle_voice_input, text)
                )

            # Connect voice recording buttons
            self.window.prompt_input.set_record_callbacks(
                on_start=self._on_record_start,
                on_stop=self._on_record_stop,
            )

        self.window.present()

    def _init_tools(self):
        """Initialize the tool registry and load built-in tools."""
        try:
            from aios.tools.registry import ToolRegistry

            self.tool_registry = ToolRegistry()
            self.tool_registry.load_builtin_tools()

            # Load user plugins
            plugins_dir = self.config_mgr.plugins_dir
            self.tool_registry.load_plugins_dir(str(plugins_dir))

            log.info(f"Loaded {len(self.tool_registry.list_tools())} tools")
        except Exception as e:
            log.error(f"Failed to initialize tools: {e}")

    def _init_llm(self):
        """Initialize the LLM provider manager."""
        try:
            from aios.llm.manager import LLMManager
            from aios.llm.claude import ClaudeProvider
            from aios.llm.openai_provider import OpenAIProvider

            self.llm_manager = LLMManager()

            # Always register providers (keys can be set later via /key)
            claude_key = self.config_mgr.get("llm.claude_api_key", "")
            claude_model = self.config_mgr.get("llm.claude_model", "claude-sonnet-4-20250514")
            claude = ClaudeProvider(api_key=claude_key, model=claude_model)
            self.llm_manager.register_provider(claude)

            openai_key = self.config_mgr.get("llm.openai_api_key", "")
            openai_model = self.config_mgr.get("llm.openai_model", "gpt-4o")
            openai_prov = OpenAIProvider(api_key=openai_key, model=openai_model)
            self.llm_manager.register_provider(openai_prov)

            # Set active provider
            active = self.config_mgr.get("llm.provider", "claude")
            self.llm_manager.set_active(active)

            # Connect tool execution
            if self.tool_registry:
                self.llm_manager.tool_registry = self.tool_registry

                def execute_tool(name: str, arguments: dict):
                    tool = self.tool_registry.get(name)
                    if tool:
                        result = tool.execute(**arguments)
                        # Show tool execution in UI
                        if self.window:
                            GLib.idle_add(
                                self.window.show_tool_execution, name, arguments, result
                            )
                        return result
                    from aios.tools.base import ToolResult
                    return ToolResult(success=False, output="", error=f"Unknown tool: {name}")

                self.llm_manager.tool_executor = execute_tool

            log.info(f"LLM initialized with provider: {active}")
        except Exception as e:
            log.error(f"Failed to initialize LLM: {e}")

    def _init_voice(self):
        """Initialize the voice controller."""
        try:
            from aios.voice.controller import VoiceController

            self.voice_controller = VoiceController(self.config_mgr)
            # Lazy initialization — models are loaded on first use
            log.info("Voice controller ready (lazy init)")
        except Exception as e:
            log.warning(f"Voice system unavailable: {e}")

    def _on_record_start(self):
        if self.voice_controller:
            self.voice_controller.start_recording()

    def _on_record_stop(self):
        if self.voice_controller:
            self.voice_controller.stop_recording()


def main():
    """Entry point for the AiOS application."""
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    )

    app = AiOSApplication()
    exit_code = app.run(sys.argv)
    sys.exit(exit_code)


if __name__ == "__main__":
    main()
