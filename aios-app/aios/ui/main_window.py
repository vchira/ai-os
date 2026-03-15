"""Main application window for AiOS."""

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Gtk, Adw, Gdk, GLib, Gio

from aios.ui.chat_view import ChatView
from aios.ui.prompt_input import PromptInput
from aios.ui.settings_dialog import SettingsDialog


class MainWindow(Adw.ApplicationWindow):
    """The primary AiOS window — full-screen AI interrogation interface."""

    def __init__(self, app, llm_manager, tool_registry, voice_controller, config_mgr):
        super().__init__(application=app, title="AiOS")
        self.llm_manager = llm_manager
        self.tool_registry = tool_registry
        self.voice_controller = voice_controller
        self.config_mgr = config_mgr

        self.set_default_size(1200, 800)
        self._build_ui()
        self._connect_signals()
        self._apply_theme()

    def _build_ui(self):
        # Main vertical layout
        self.main_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.set_content(self.main_box)

        # Header bar
        header = Adw.HeaderBar()
        header.set_title_widget(Gtk.Label(label="AiOS"))

        # Provider selector in header
        self.provider_dropdown = Gtk.DropDown.new_from_strings(["claude", "openai"])
        self.provider_dropdown.connect("notify::selected", self._on_provider_changed)
        header.pack_start(self.provider_dropdown)

        # Voice controls in header
        voice_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=4)
        voice_box.add_css_class("linked")

        self.mic_button = Gtk.ToggleButton()
        self.mic_button.set_icon_name("audio-input-microphone-symbolic")
        self.mic_button.set_tooltip_text("Voice input (STT)")
        self.mic_button.set_active(True)
        voice_box.append(self.mic_button)

        self.speaker_button = Gtk.ToggleButton()
        self.speaker_button.set_icon_name("audio-speakers-symbolic")
        self.speaker_button.set_tooltip_text("Voice output (TTS)")
        self.speaker_button.set_active(True)
        voice_box.append(self.speaker_button)

        header.pack_end(voice_box)

        # Settings button
        settings_btn = Gtk.Button(icon_name="emblem-system-symbolic")
        settings_btn.set_tooltip_text("Settings")
        settings_btn.connect("clicked", self._on_settings_clicked)
        header.pack_end(settings_btn)

        self.main_box.append(header)

        # Chat area (scrollable)
        self.chat_view = ChatView()
        scroll = Gtk.ScrolledWindow(vexpand=True, hexpand=True)
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_child(self.chat_view)
        self.scroll = scroll
        self.main_box.append(scroll)

        # Voice activity indicator
        self.voice_indicator = Gtk.LevelBar()
        self.voice_indicator.set_min_value(0)
        self.voice_indicator.set_max_value(1.0)
        self.voice_indicator.set_value(0)
        self.voice_indicator.set_visible(False)
        self.voice_indicator.add_css_class("voice-indicator")
        self.main_box.append(self.voice_indicator)

        # Prompt input area
        self.prompt_input = PromptInput()
        self.main_box.append(self.prompt_input)

    def _connect_signals(self):
        self.prompt_input.connect("message-submitted", self._on_message_submitted)
        self.mic_button.connect("toggled", self._on_mic_toggled)
        self.speaker_button.connect("toggled", self._on_speaker_toggled)

    def _apply_theme(self):
        theme = self.config_mgr.get("ui.theme", "dark")
        style_mgr = Adw.StyleManager.get_default()
        if theme == "dark":
            style_mgr.set_color_scheme(Adw.ColorScheme.FORCE_DARK)
        elif theme == "light":
            style_mgr.set_color_scheme(Adw.ColorScheme.FORCE_LIGHT)
        else:
            style_mgr.set_color_scheme(Adw.ColorScheme.DEFAULT)

        css_provider = Gtk.CssProvider()
        css_provider.load_from_string(self._get_css())
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(),
            css_provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
        )

    def _get_css(self) -> str:
        return """
        .chat-message-user {
            background-color: alpha(@accent_bg_color, 0.15);
            border-radius: 12px;
            padding: 12px 16px;
            margin: 4px 48px 4px 96px;
        }
        .chat-message-assistant {
            background-color: alpha(@card_bg_color, 0.8);
            border-radius: 12px;
            padding: 12px 16px;
            margin: 4px 96px 4px 48px;
        }
        .chat-message-tool {
            background-color: alpha(@warning_bg_color, 0.1);
            border-radius: 8px;
            padding: 8px 12px;
            margin: 2px 96px 2px 64px;
            font-family: monospace;
            font-size: 0.9em;
        }
        .prompt-input {
            margin: 8px 16px 16px 16px;
        }
        .prompt-entry {
            min-height: 48px;
            border-radius: 24px;
            padding: 8px 16px;
            font-size: 1.05em;
        }
        .voice-indicator {
            margin: 0 16px;
            min-height: 4px;
        }
        .voice-recording {
            color: @error_color;
        }
        """

    def _on_message_submitted(self, widget, message: str):
        """Handle a message from the text prompt."""
        if message.startswith("/"):
            self._handle_command(message)
            return
        self._send_message(message)

    def _send_message(self, text: str):
        """Send a user message to the LLM."""
        self.chat_view.add_message("user", text)
        self.prompt_input.set_sensitive(False)
        self._scroll_to_bottom()

        # Run LLM call in a thread to avoid blocking UI
        import threading

        def do_llm():
            try:
                response = self.llm_manager.chat(
                    user_message=text,
                    tools=self.tool_registry.get_tools_schema(),
                )
                GLib.idle_add(self._on_llm_response, response)
            except Exception as e:
                GLib.idle_add(self._on_llm_error, str(e))

        threading.Thread(target=do_llm, daemon=True).start()

    def _on_llm_response(self, response):
        """Handle LLM response on the main thread."""
        self.chat_view.add_message("assistant", response.content)
        self.prompt_input.set_sensitive(True)
        self.prompt_input.grab_focus()
        self._scroll_to_bottom()

        # Speak the response if TTS is enabled
        if self.speaker_button.get_active() and self.voice_controller:
            self.voice_controller.speak(response.content)

    def _on_llm_error(self, error: str):
        """Handle LLM error on the main thread."""
        self.chat_view.add_message("system", f"Error: {error}")
        self.prompt_input.set_sensitive(True)
        self.prompt_input.grab_focus()

    def _handle_command(self, command: str):
        """Handle slash commands."""
        from aios.config.commands import CommandHandler

        handler = CommandHandler(self.config_mgr, self.llm_manager, self.voice_controller)
        result = handler.execute(command)
        self.chat_view.add_message("system", result)

        # Re-apply theme if it changed
        if command.startswith("/theme"):
            self._apply_theme()

    def _on_provider_changed(self, dropdown, _pspec):
        idx = dropdown.get_selected()
        providers = ["claude", "openai"]
        if idx < len(providers):
            self.llm_manager.set_active(providers[idx])

    def _on_mic_toggled(self, button):
        if self.voice_controller:
            if button.get_active():
                self.voice_controller.enable_stt()
                self.voice_indicator.set_visible(True)
            else:
                self.voice_controller.disable_stt()
                self.voice_indicator.set_visible(False)

    def _on_speaker_toggled(self, button):
        if self.voice_controller:
            if button.get_active():
                self.voice_controller.enable_tts()
            else:
                self.voice_controller.disable_tts()

    def _on_settings_clicked(self, button):
        dialog = SettingsDialog(self, self.config_mgr, self.voice_controller)
        dialog.present()

    def _scroll_to_bottom(self):
        def scroll():
            adj = self.scroll.get_vadjustment()
            adj.set_value(adj.get_upper() - adj.get_page_size())
            return False

        GLib.timeout_add(50, scroll)

    def handle_voice_input(self, text: str):
        """Called by voice controller when STT produces text."""
        self.prompt_input.set_text(text)
        self._send_message(text)

    def show_tool_execution(self, tool_name: str, args: dict, result):
        """Display a tool execution in the chat."""
        import json

        args_str = json.dumps(args, indent=2)
        self.chat_view.add_message(
            "tool",
            f"Tool: {tool_name}\nInput: {args_str}\nResult: {result.output}",
        )
        self._scroll_to_bottom()
