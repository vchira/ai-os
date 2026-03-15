"""Settings dialog for AiOS configuration."""

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Gtk, Adw


class SettingsDialog(Adw.PreferencesWindow):
    """Settings dialog for configuring AiOS."""

    def __init__(self, parent, config_mgr, voice_controller):
        super().__init__(transient_for=parent, modal=True)
        self.set_title("AiOS Settings")
        self.set_default_size(600, 700)
        self.config_mgr = config_mgr
        self.voice_controller = voice_controller

        self._build_llm_page()
        self._build_voice_page()
        self._build_system_page()

    def _build_llm_page(self):
        page = Adw.PreferencesPage(title="AI", icon_name="dialog-information-symbolic")

        # Provider group
        provider_group = Adw.PreferencesGroup(title="LLM Provider")

        # Active provider
        provider_row = Adw.ComboRow(title="Active Provider")
        provider_row.set_model(Gtk.StringList.new(["claude", "openai"]))
        current = self.config_mgr.get("llm.provider", "claude")
        provider_row.set_selected(0 if current == "claude" else 1)
        provider_row.connect("notify::selected", self._on_provider_changed)
        provider_group.add(provider_row)

        # Claude API key
        claude_key_row = Adw.PasswordEntryRow(title="Claude API Key")
        claude_key_row.set_text(self.config_mgr.get("llm.claude_api_key", ""))
        claude_key_row.connect("changed", lambda r: self.config_mgr.set("llm.claude_api_key", r.get_text()))
        provider_group.add(claude_key_row)

        # Claude model
        claude_model_row = Adw.EntryRow(title="Claude Model")
        claude_model_row.set_text(self.config_mgr.get("llm.claude_model", "claude-sonnet-4-20250514"))
        claude_model_row.connect("changed", lambda r: self.config_mgr.set("llm.claude_model", r.get_text()))
        provider_group.add(claude_model_row)

        # OpenAI API key
        openai_key_row = Adw.PasswordEntryRow(title="OpenAI API Key")
        openai_key_row.set_text(self.config_mgr.get("llm.openai_api_key", ""))
        openai_key_row.connect("changed", lambda r: self.config_mgr.set("llm.openai_api_key", r.get_text()))
        provider_group.add(openai_key_row)

        # OpenAI model
        openai_model_row = Adw.EntryRow(title="OpenAI Model")
        openai_model_row.set_text(self.config_mgr.get("llm.openai_model", "gpt-4o"))
        openai_model_row.connect("changed", lambda r: self.config_mgr.set("llm.openai_model", r.get_text()))
        provider_group.add(openai_model_row)

        page.add(provider_group)

        # System prompt group
        prompt_group = Adw.PreferencesGroup(title="System Prompt")
        prompt_row = Adw.EntryRow(title="Additional system prompt")
        prompt_row.set_text(self.config_mgr.get("llm.extra_system_prompt", ""))
        prompt_row.connect("changed", lambda r: self.config_mgr.set("llm.extra_system_prompt", r.get_text()))
        prompt_group.add(prompt_row)
        page.add(prompt_group)

        self.add(page)

    def _build_voice_page(self):
        page = Adw.PreferencesPage(title="Voice", icon_name="audio-speakers-symbolic")

        # STT group
        stt_group = Adw.PreferencesGroup(title="Speech-to-Text")

        stt_enabled = Adw.SwitchRow(title="Enable voice input")
        stt_enabled.set_active(self.config_mgr.get("voice.stt_enabled", True))
        stt_enabled.connect("notify::active", lambda r, _: self.config_mgr.set("voice.stt_enabled", r.get_active()))
        stt_group.add(stt_enabled)

        stt_model = Adw.ComboRow(title="Whisper model")
        models = ["tiny", "base", "small", "medium", "large-v3"]
        stt_model.set_model(Gtk.StringList.new(models))
        current_model = self.config_mgr.get("voice.stt_model", "medium")
        if current_model in models:
            stt_model.set_selected(models.index(current_model))
        stt_model.connect("notify::selected", self._on_stt_model_changed)
        stt_group.add(stt_model)
        self._stt_models = models

        stt_lang = Adw.EntryRow(title="Language (blank=auto-detect)")
        stt_lang.set_text(self.config_mgr.get("voice.stt_language", ""))
        stt_lang.connect("changed", lambda r: self.config_mgr.set("voice.stt_language", r.get_text()))
        stt_group.add(stt_lang)

        page.add(stt_group)

        # TTS group
        tts_group = Adw.PreferencesGroup(title="Text-to-Speech")

        tts_enabled = Adw.SwitchRow(title="Enable voice output")
        tts_enabled.set_active(self.config_mgr.get("voice.tts_enabled", True))
        tts_enabled.connect("notify::active", lambda r, _: self.config_mgr.set("voice.tts_enabled", r.get_active()))
        tts_group.add(tts_enabled)

        # Voice gender
        gender_row = Adw.ComboRow(title="Voice gender")
        genders = ["female", "male", "neutral"]
        gender_row.set_model(Gtk.StringList.new(genders))
        current_gender = self.config_mgr.get("voice.tts_gender", "female")
        if current_gender in genders:
            gender_row.set_selected(genders.index(current_gender))
        gender_row.connect("notify::selected", self._on_gender_changed)
        tts_group.add(gender_row)
        self._genders = genders

        # Voice selection
        voice_row = Adw.EntryRow(title="Voice ID (e.g., en_US-amy-medium)")
        voice_row.set_text(self.config_mgr.get("voice.tts_voice", "en_US-amy-medium"))
        voice_row.connect("changed", lambda r: self.config_mgr.set("voice.tts_voice", r.get_text()))
        tts_group.add(voice_row)

        # Speech rate
        rate_row = Adw.SpinRow.new_with_range(0.5, 2.0, 0.1)
        rate_row.set_title("Speech rate")
        rate_row.set_value(self.config_mgr.get("voice.tts_rate", 1.0))
        rate_row.connect("notify::value", lambda r, _: self.config_mgr.set("voice.tts_rate", r.get_value()))
        tts_group.add(rate_row)

        page.add(tts_group)
        self.add(page)

    def _build_system_page(self):
        page = Adw.PreferencesPage(title="System", icon_name="preferences-system-symbolic")

        # Display group
        display_group = Adw.PreferencesGroup(title="Display")

        theme_row = Adw.ComboRow(title="Theme")
        themes = ["dark", "light", "auto"]
        theme_row.set_model(Gtk.StringList.new(themes))
        current_theme = self.config_mgr.get("ui.theme", "dark")
        if current_theme in themes:
            theme_row.set_selected(themes.index(current_theme))
        theme_row.connect("notify::selected", self._on_theme_changed)
        display_group.add(theme_row)
        self._themes = themes

        page.add(display_group)

        # Keyboard group
        kb_group = Adw.PreferencesGroup(title="Keyboard")

        layout_row = Adw.EntryRow(title="Keyboard layout (e.g., us, de, fr)")
        layout_row.set_text(self.config_mgr.get("system.keyboard_layout", "us"))
        layout_row.connect("changed", lambda r: self.config_mgr.set("system.keyboard_layout", r.get_text()))
        kb_group.add(layout_row)

        page.add(kb_group)

        # Tools group
        tools_group = Adw.PreferencesGroup(title="Tools & Plugins")

        plugins_row = Adw.ActionRow(title="Manage plugins", subtitle="Install and remove AI tool plugins")
        plugins_row.set_activatable(True)
        plugins_row.add_suffix(Gtk.Image(icon_name="go-next-symbolic"))
        tools_group.add(plugins_row)

        page.add(tools_group)
        self.add(page)

    def _on_provider_changed(self, row, _pspec):
        providers = ["claude", "openai"]
        idx = row.get_selected()
        if idx < len(providers):
            self.config_mgr.set("llm.provider", providers[idx])

    def _on_stt_model_changed(self, row, _pspec):
        idx = row.get_selected()
        if idx < len(self._stt_models):
            self.config_mgr.set("voice.stt_model", self._stt_models[idx])

    def _on_gender_changed(self, row, _pspec):
        idx = row.get_selected()
        if idx < len(self._genders):
            self.config_mgr.set("voice.tts_gender", self._genders[idx])

    def _on_theme_changed(self, row, _pspec):
        idx = row.get_selected()
        if idx < len(self._themes):
            self.config_mgr.set("ui.theme", self._themes[idx])
