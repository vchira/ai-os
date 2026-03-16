//! Settings dialog using `adw::PreferencesWindow`.
//!
//! Displays configuration pages for AI, Voice, and System settings.
//! Values are read from the [`ConfigManager`] on open and written back
//! on change.

use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;
use libadwaita::prelude::*;

use aios_core::config::ConfigManager;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Show the settings dialog as a modal window.
pub fn show_settings(parent: &adw::ApplicationWindow, config: &ConfigManager) {
    let dialog = adw::PreferencesWindow::builder()
        .title("AiOS Settings")
        .transient_for(parent)
        .modal(true)
        .build();

    // Build pages.
    dialog.add(&build_ai_page(config));
    dialog.add(&build_voice_page(config));
    dialog.add(&build_system_page(config));

    dialog.present();
}

// ---------------------------------------------------------------------------
// AI page
// ---------------------------------------------------------------------------

fn build_ai_page(config: &ConfigManager) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("AI")
        .icon_name("applications-science-symbolic")
        .build();

    // Provider group.
    let provider_group = adw::PreferencesGroup::builder()
        .title("LLM Provider")
        .build();

    // Provider combo row.
    let providers = gtk::StringList::new(&["claude", "openai"]);
    let provider_row = adw::ComboRow::builder()
        .title("Provider")
        .subtitle("Active LLM provider")
        .model(&providers)
        .build();
    let current = config.get_str("llm.provider", "claude");
    provider_row.set_selected(if current == "openai" { 1 } else { 0 });
    provider_group.add(&provider_row);

    page.add(&provider_group);

    // API keys group.
    let keys_group = adw::PreferencesGroup::builder()
        .title("API Keys")
        .description("Set your API keys for each provider")
        .build();

    let claude_key_row = adw::PasswordEntryRow::builder()
        .title("Claude API Key")
        .build();
    let stored_claude = config.get_str("llm.claude_api_key", "");
    if !stored_claude.is_empty() {
        claude_key_row.set_text(&stored_claude);
    }
    keys_group.add(&claude_key_row);

    let openai_key_row = adw::PasswordEntryRow::builder()
        .title("OpenAI API Key")
        .build();
    let stored_openai = config.get_str("llm.openai_api_key", "");
    if !stored_openai.is_empty() {
        openai_key_row.set_text(&stored_openai);
    }
    keys_group.add(&openai_key_row);

    page.add(&keys_group);

    // Models group.
    let models_group = adw::PreferencesGroup::builder()
        .title("Models")
        .build();

    let claude_model_row = adw::EntryRow::builder()
        .title("Claude Model")
        .build();
    claude_model_row.set_text(&config.get_str("llm.claude_model", "claude-sonnet-4-20250514"));
    models_group.add(&claude_model_row);

    let openai_model_row = adw::EntryRow::builder()
        .title("OpenAI Model")
        .build();
    openai_model_row.set_text(&config.get_str("llm.openai_model", "gpt-4o"));
    models_group.add(&openai_model_row);

    let system_prompt_row = adw::EntryRow::builder()
        .title("Extra System Prompt")
        .build();
    system_prompt_row.set_text(&config.get_str("llm.extra_system_prompt", ""));
    models_group.add(&system_prompt_row);

    page.add(&models_group);

    page
}

// ---------------------------------------------------------------------------
// Voice page
// ---------------------------------------------------------------------------

fn build_voice_page(config: &ConfigManager) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Voice")
        .icon_name("audio-input-microphone-symbolic")
        .build();

    // STT group.
    let stt_group = adw::PreferencesGroup::builder()
        .title("Speech-to-Text")
        .build();

    let stt_switch = gtk::Switch::new();
    stt_switch.set_active(config.get_bool("voice.stt_enabled", true));
    stt_switch.set_valign(gtk::Align::Center);
    let stt_enabled = adw::ActionRow::builder()
        .title("STT Enabled")
        .subtitle("Enable voice input via microphone")
        .build();
    stt_enabled.add_suffix(&stt_switch);
    stt_enabled.set_activatable_widget(Some(&stt_switch));
    stt_group.add(&stt_enabled);

    let stt_backends = gtk::StringList::new(&["Whisper", "Vosk"]);
    let stt_backend_row = adw::ComboRow::builder()
        .title("STT Backend")
        .model(&stt_backends)
        .build();
    stt_backend_row.set_selected(0);
    stt_group.add(&stt_backend_row);

    let stt_models = gtk::StringList::new(&["tiny", "base", "small", "medium", "large"]);
    let stt_model_row = adw::ComboRow::builder()
        .title("STT Model")
        .subtitle("Larger models are more accurate but slower")
        .model(&stt_models)
        .build();
    let current_model = config.get_str("voice.stt_model", "medium");
    let model_idx = match current_model.as_str() {
        "tiny" => 0,
        "base" => 1,
        "small" => 2,
        "medium" => 3,
        "large" => 4,
        _ => 3,
    };
    stt_model_row.set_selected(model_idx);
    stt_group.add(&stt_model_row);

    page.add(&stt_group);

    // TTS group.
    let tts_group = adw::PreferencesGroup::builder()
        .title("Text-to-Speech")
        .build();

    let tts_switch = gtk::Switch::new();
    tts_switch.set_active(config.get_bool("voice.tts_enabled", true));
    tts_switch.set_valign(gtk::Align::Center);
    let tts_enabled = adw::ActionRow::builder()
        .title("TTS Enabled")
        .subtitle("Enable spoken responses")
        .build();
    tts_enabled.add_suffix(&tts_switch);
    tts_enabled.set_activatable_widget(Some(&tts_switch));
    tts_group.add(&tts_enabled);

    let tts_backends = gtk::StringList::new(&["Piper", "Espeak", "CoquiXtts"]);
    let tts_backend_row = adw::ComboRow::builder()
        .title("TTS Backend")
        .model(&tts_backends)
        .build();
    tts_backend_row.set_selected(0);
    tts_group.add(&tts_backend_row);

    let voice_row = adw::EntryRow::builder()
        .title("Voice")
        .build();
    voice_row.set_text(&config.get_str("voice.tts_voice", "en_US-amy-medium"));
    tts_group.add(&voice_row);

    let adjustment = gtk::Adjustment::new(
        config.get_f64("voice.tts_rate", 1.0),
        0.5,  // min
        2.0,  // max
        0.1,  // step
        0.1,  // page step
        0.0,  // page size
    );
    let rate_spin = gtk::SpinButton::new(Some(&adjustment), 0.1, 1);
    rate_spin.set_valign(gtk::Align::Center);
    let rate_row = adw::ActionRow::builder()
        .title("Speech Rate")
        .subtitle("Playback speed multiplier")
        .build();
    rate_row.add_suffix(&rate_spin);
    tts_group.add(&rate_row);

    page.add(&tts_group);

    page
}

// ---------------------------------------------------------------------------
// System page
// ---------------------------------------------------------------------------

fn build_system_page(config: &ConfigManager) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("System")
        .icon_name("preferences-system-symbolic")
        .build();

    // Appearance group.
    let appearance_group = adw::PreferencesGroup::builder()
        .title("Appearance")
        .build();

    let themes = gtk::StringList::new(&["dark", "light", "auto"]);
    let theme_row = adw::ComboRow::builder()
        .title("Theme")
        .model(&themes)
        .build();
    let current_theme = config.get_str("ui.theme", "dark");
    let theme_idx = match current_theme.as_str() {
        "dark" => 0,
        "light" => 1,
        "auto" => 2,
        _ => 0,
    };
    theme_row.set_selected(theme_idx);
    appearance_group.add(&theme_row);

    page.add(&appearance_group);

    // Input group.
    let input_group = adw::PreferencesGroup::builder()
        .title("Input")
        .build();

    let keyboard_row = adw::EntryRow::builder()
        .title("Keyboard Layout")
        .build();
    keyboard_row.set_text(&config.get_str("system.keyboard_layout", "us"));
    input_group.add(&keyboard_row);

    page.add(&input_group);

    // Locale group.
    let locale_group = adw::PreferencesGroup::builder()
        .title("Locale")
        .build();

    let language_row = adw::EntryRow::builder()
        .title("Language / Locale")
        .build();
    language_row.set_text(&config.get_str("system.locale", "en_US.UTF-8"));
    locale_group.add(&language_row);

    page.add(&locale_group);

    page
}
