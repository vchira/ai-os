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
// Constants — model lists
// ---------------------------------------------------------------------------

/// Claude models: (human name, slug).
const CLAUDE_MODELS: &[(&str, &str)] = &[
    ("Claude Sonnet 4", "claude-sonnet-4-20250514"),
    ("Claude Opus 4", "claude-opus-4-20250514"),
    ("Claude Haiku 3.5", "claude-haiku-4-5-20251001"),
];

/// OpenAI / ChatGPT models: (human name, slug).
const OPENAI_MODELS: &[(&str, &str)] = &[
    ("GPT-4o", "gpt-4o"),
    ("GPT-4o Mini", "gpt-4o-mini"),
    ("GPT-4 Turbo", "gpt-4-turbo"),
];

/// Known Piper voices: (slug, human name).
const PIPER_VOICES: &[(&str, &str)] = &[
    ("en_US-amy-medium", "Amy (English US)"),
    ("en_US-arctic-medium", "Arctic (English US)"),
    ("en_US-danny-low", "Danny (English US)"),
    ("en_US-joe-medium", "Joe (English US)"),
    ("en_US-kathleen-low", "Kathleen (English US)"),
    ("en_US-lessac-medium", "Lessac (English US)"),
    ("en_US-libritts-high", "LibriTTS (English US)"),
    ("en_US-ryan-medium", "Ryan (English US)"),
    ("en_GB-alan-medium", "Alan (English GB)"),
    ("en_GB-alba-medium", "Alba (English GB)"),
    ("de_DE-thorsten-medium", "Thorsten (German)"),
    ("fr_FR-siwis-medium", "Siwis (French)"),
    ("es_ES-sharvard-medium", "Sharvard (Spanish)"),
    ("it_IT-riccardo-x_low", "Riccardo (Italian)"),
    ("pt_BR-faber-medium", "Faber (Portuguese BR)"),
    ("ro_RO-mihai-medium", "Mihai (Romanian)"),
];

/// Keyboard layouts: (code, human name).
const KEYBOARD_LAYOUTS: &[(&str, &str)] = &[
    ("us", "US English"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("gb", "British English"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("se", "Swedish"),
    ("no", "Norwegian"),
    ("dk", "Danish"),
    ("fi", "Finnish"),
    ("ru", "Russian"),
    ("tr", "Turkish"),
    ("jp", "Japanese"),
    ("kr", "Korean"),
    ("ar", "Arabic"),
    ("hu", "Hungarian"),
    ("cz", "Czech"),
];

// ---------------------------------------------------------------------------
// Placeholder used for masked API keys
// ---------------------------------------------------------------------------

const KEY_PLACEHOLDER: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

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

    // API keys group — edit-only, never show the actual key (Task 10).
    let keys_group = adw::PreferencesGroup::builder()
        .title("API Keys")
        .description("Enter a new key to replace the existing one")
        .build();

    let claude_key_row = adw::PasswordEntryRow::builder()
        .title("Claude API Key")
        .build();
    let stored_claude = config.get_str("llm.claude_api_key", "");
    if !stored_claude.is_empty() {
        // Show placeholder dots — not the actual value.
        claude_key_row.set_text(KEY_PLACEHOLDER);
    }
    keys_group.add(&claude_key_row);

    let openai_key_row = adw::PasswordEntryRow::builder()
        .title("OpenAI API Key")
        .build();
    let stored_openai = config.get_str("llm.openai_api_key", "");
    if !stored_openai.is_empty() {
        openai_key_row.set_text(KEY_PLACEHOLDER);
    }
    keys_group.add(&openai_key_row);

    // Save new key values on change (only if user typed something real).
    {
        let config_dir = ConfigManager::default_config_dir();
        let claude_row = claude_key_row.clone();
        claude_key_row.connect_changed(move |_| {
            let text = claude_row.text().to_string();
            // Only save if the user actually typed a new key, not the placeholder.
            if !text.is_empty() && text != KEY_PLACEHOLDER {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set("llm.claude_api_key", serde_json::json!(text));
                }
            }
        });
    }
    {
        let config_dir = ConfigManager::default_config_dir();
        let openai_row = openai_key_row.clone();
        openai_key_row.connect_changed(move |_| {
            let text = openai_row.text().to_string();
            if !text.is_empty() && text != KEY_PLACEHOLDER {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set("llm.openai_api_key", serde_json::json!(text));
                }
            }
        });
    }

    page.add(&keys_group);

    // Models group — dropdowns instead of free-text (Task 11).
    let models_group = adw::PreferencesGroup::builder()
        .title("Models")
        .build();

    // Claude model dropdown.
    let claude_model_names: Vec<&str> = CLAUDE_MODELS.iter().map(|(name, _)| *name).collect();
    let claude_model_list = gtk::StringList::new(&claude_model_names);
    let claude_model_row = adw::ComboRow::builder()
        .title("Claude Model")
        .model(&claude_model_list)
        .build();
    let current_claude_model = config.get_str("llm.claude_model", "claude-sonnet-4-20250514");
    let claude_model_idx = CLAUDE_MODELS
        .iter()
        .position(|(_, slug)| *slug == current_claude_model.as_str())
        .unwrap_or(0) as u32;
    claude_model_row.set_selected(claude_model_idx);
    models_group.add(&claude_model_row);

    // OpenAI model dropdown.
    let openai_model_names: Vec<&str> = OPENAI_MODELS.iter().map(|(name, _)| *name).collect();
    let openai_model_list = gtk::StringList::new(&openai_model_names);
    let openai_model_row = adw::ComboRow::builder()
        .title("ChatGPT Model")
        .model(&openai_model_list)
        .build();
    let current_openai_model = config.get_str("llm.openai_model", "gpt-4o");
    let openai_model_idx = OPENAI_MODELS
        .iter()
        .position(|(_, slug)| *slug == current_openai_model.as_str())
        .unwrap_or(0) as u32;
    openai_model_row.set_selected(openai_model_idx);
    models_group.add(&openai_model_row);

    let system_prompt_row = adw::EntryRow::builder()
        .title("Custom AI Instructions")
        .build();
    system_prompt_row.set_text(&config.get_str("llm.extra_system_prompt", ""));
    system_prompt_row.set_tooltip_text(Some(
        "Extra instructions added to every AI conversation.\n\
         e.g., 'Always respond in German' or 'Be concise'"
    ));
    models_group.add(&system_prompt_row);

    // Save model selection on change.
    {
        let config_dir = ConfigManager::default_config_dir();
        claude_model_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if let Some((_, slug)) = CLAUDE_MODELS.get(idx) {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set("llm.claude_model", serde_json::json!(slug));
                }
            }
        });
    }
    {
        let config_dir = ConfigManager::default_config_dir();
        openai_model_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if let Some((_, slug)) = OPENAI_MODELS.get(idx) {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set("llm.openai_model", serde_json::json!(slug));
                }
            }
        });
    }

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

    // Voice selection as dropdown (Task 12).
    // Build the list: known Piper voices + espeak-ng fallback.
    let mut voice_names: Vec<String> = Vec::new();
    let mut voice_slugs: Vec<String> = Vec::new();

    for (slug, human) in PIPER_VOICES {
        voice_names.push(human.to_string());
        voice_slugs.push(slug.to_string());
    }
    // Add espeak-ng as fallback option.
    voice_names.push("espeak-ng (fallback)".to_string());
    voice_slugs.push("espeak-ng".to_string());

    let voice_name_refs: Vec<&str> = voice_names.iter().map(|s| s.as_str()).collect();
    let voice_list = gtk::StringList::new(&voice_name_refs);
    let voice_row = adw::ComboRow::builder()
        .title("Voice")
        .subtitle("TTS voice to use")
        .model(&voice_list)
        .build();
    let current_voice = config.get_str("voice.tts_voice", "en_US-amy-medium");
    let voice_idx = voice_slugs
        .iter()
        .position(|s| s == &current_voice)
        .unwrap_or(0) as u32;
    voice_row.set_selected(voice_idx);
    tts_group.add(&voice_row);

    // Save voice selection on change.
    {
        let config_dir = ConfigManager::default_config_dir();
        let slugs = voice_slugs.clone();
        voice_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if let Some(slug) = slugs.get(idx) {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set("voice.tts_voice", serde_json::json!(slug));
                }
            }
        });
    }

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

    // Input group — keyboard layout as dropdown (Task 13).
    let input_group = adw::PreferencesGroup::builder()
        .title("Input")
        .build();

    let kb_names: Vec<String> = KEYBOARD_LAYOUTS
        .iter()
        .map(|(code, name)| format!("{name} ({code})"))
        .collect();
    let kb_name_refs: Vec<&str> = kb_names.iter().map(|s| s.as_str()).collect();
    let kb_list = gtk::StringList::new(&kb_name_refs);
    let keyboard_row = adw::ComboRow::builder()
        .title("Keyboard Layout")
        .model(&kb_list)
        .build();
    let current_kb = config.get_str("system.keyboard_layout", "us");
    let kb_idx = KEYBOARD_LAYOUTS
        .iter()
        .position(|(code, _)| *code == current_kb.as_str())
        .unwrap_or(0) as u32;
    keyboard_row.set_selected(kb_idx);
    input_group.add(&keyboard_row);

    // Save keyboard layout on change.
    {
        let config_dir = ConfigManager::default_config_dir();
        keyboard_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if let Some((code, _)) = KEYBOARD_LAYOUTS.get(idx) {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set("system.keyboard_layout", serde_json::json!(code));
                }
            }
        });
    }

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
