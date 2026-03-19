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

// Model lists are defined in crate::providers::PROVIDERS.

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

/// Pre-trained wake words: (slug, human name).
const PRETRAINED_NAMES: &[(&str, &str)] = &[
    ("hey_assistant", "Hey Assistant"),
    ("hey_jarvis", "Hey Jarvis"),
    ("computer", "Computer"),
    ("ok_computer", "OK Computer"),
    ("hey_friday", "Hey Friday"),
    ("jarvis", "Jarvis"),
    ("ok_jarvis", "OK Jarvis"),
    ("skynet", "Skynet"),
    ("terminator", "Terminator"),
    ("hey_house", "Hey House"),
    ("ok_home", "OK Home"),
    ("home_assistant", "Home Assistant"),
    ("mr_anderson", "Mr. Anderson"),
    ("mr_smith", "Mr. Smith"),
    ("hey_dick_head", "Hey Dick Head"),
    ("oi_fuckwhit", "Oi Fuckwhit"),
    ("yo_homie", "Yo Homie"),
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
    use crate::providers::{PROVIDERS, current_model};

    let page = adw::PreferencesPage::builder()
        .title("AI")
        .icon_name("applications-science-symbolic")
        .build();

    // -----------------------------------------------------------------------
    // Provider group — combo row with ALL provider ids.
    // -----------------------------------------------------------------------
    let provider_group = adw::PreferencesGroup::builder()
        .title("LLM Provider")
        .build();

    let provider_ids: Vec<&str> = PROVIDERS.iter().map(|p| p.id).collect();
    let provider_list = gtk::StringList::new(&provider_ids);
    let provider_row = adw::ComboRow::builder()
        .title("Provider")
        .subtitle("Active LLM provider")
        .model(&provider_list)
        .build();
    let current = config.get_str("llm.provider", "claude");
    let provider_idx = PROVIDERS
        .iter()
        .position(|p| p.id == current.as_str())
        .unwrap_or(0) as u32;
    provider_row.set_selected(provider_idx);
    provider_group.add(&provider_row);

    page.add(&provider_group);

    // -----------------------------------------------------------------------
    // API Keys group — one PasswordEntryRow per provider that needs a key.
    // -----------------------------------------------------------------------
    let keys_group = adw::PreferencesGroup::builder()
        .title("API Keys")
        .description("Enter a new key to replace the existing one")
        .build();

    for prov in PROVIDERS.iter().filter(|p| p.needs_api_key) {
        let title = format!("{} API Key", prov.display_name);
        let key_row = adw::PasswordEntryRow::builder()
            .title(&title)
            .build();

        let stored = config.get_str(prov.api_key_config, "");
        if !stored.is_empty() {
            key_row.set_text(KEY_PLACEHOLDER);
        }
        keys_group.add(&key_row);

        // Save on change.
        let config_dir = ConfigManager::default_config_dir();
        let config_key = prov.api_key_config.to_string();
        let row_ref = key_row.clone();
        key_row.connect_changed(move |_| {
            let text = row_ref.text().to_string();
            if !text.is_empty() && text != KEY_PLACEHOLDER {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set(&config_key, serde_json::json!(text));
                }
            }
        });
    }

    // Ollama enabled switch (no API key, just a toggle).
    if let Some(ollama) = PROVIDERS.iter().find(|p| p.id == "ollama") {
        let switch = gtk::Switch::new();
        switch.set_active(config.get_bool(ollama.enabled_config, false));
        switch.set_valign(gtk::Align::Center);
        let row = adw::ActionRow::builder()
            .title("Ollama (local)")
            .subtitle("Enable local Ollama inference")
            .build();
        row.add_suffix(&switch);
        row.set_activatable_widget(Some(&switch));
        keys_group.add(&row);

        let config_dir = ConfigManager::default_config_dir();
        let enabled_key = ollama.enabled_config.to_string();
        switch.connect_state_set(move |_, active| {
            if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                let _ = cfg.set(&enabled_key, serde_json::json!(active));
            }
            gtk::glib::Propagation::Proceed
        });
    }

    page.add(&keys_group);

    // -----------------------------------------------------------------------
    // Models group — one ComboRow per provider.
    // -----------------------------------------------------------------------
    let models_group = adw::PreferencesGroup::builder()
        .title("Models")
        .build();

    for prov in PROVIDERS {
        if prov.models.is_empty() {
            continue;
        }
        let title = format!("{} Model", prov.display_name);
        let model_names: Vec<&str> = prov.models.iter().map(|(name, _)| *name).collect();
        let model_list = gtk::StringList::new(&model_names);
        let model_row = adw::ComboRow::builder()
            .title(&title)
            .model(&model_list)
            .build();

        let cur = current_model(prov, config);
        let idx = prov
            .models
            .iter()
            .position(|(_, slug)| *slug == cur.as_str())
            .unwrap_or(0) as u32;
        model_row.set_selected(idx);
        models_group.add(&model_row);

        // Save on change.
        let config_dir = ConfigManager::default_config_dir();
        let model_config_key = prov.model_config.to_string();
        let models_ref: Vec<(&str, &str)> = prov.models.to_vec();
        model_row.connect_selected_notify(move |row| {
            let sel = row.selected() as usize;
            if let Some((_, slug)) = models_ref.get(sel) {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set(&model_config_key, serde_json::json!(slug));
                }
            }
        });
    }

    let system_prompt_row = adw::EntryRow::builder()
        .title("Custom AI Instructions")
        .build();
    system_prompt_row.set_text(&config.get_str("llm.extra_system_prompt", ""));
    system_prompt_row.set_tooltip_text(Some(
        "Extra instructions added to every AI conversation.\n\
         e.g., 'Always respond in German' or 'Be concise'"
    ));
    models_group.add(&system_prompt_row);

    page.add(&models_group);

    // -----------------------------------------------------------------------
    // AI Model Assignment — show Main AI and Summarizer selections.
    // -----------------------------------------------------------------------
    let assign_group = adw::PreferencesGroup::builder()
        .title("AI Model Assignment")
        .description("Which provider/model to use for main AI and TTS summarizer")
        .build();

    // Main AI — currently the active provider + its model.
    let main_provider_display = crate::providers::find_by_id(&config.get_str("llm.provider", "claude"))
        .map(|p| p.display_name)
        .unwrap_or("claude");
    let main_model = crate::providers::find_by_id(&config.get_str("llm.provider", "claude"))
        .map(|p| current_model(p, config))
        .unwrap_or_default();
    let main_row = adw::ActionRow::builder()
        .title("Main AI")
        .subtitle(format!("{main_provider_display} \u{2014} {main_model}"))
        .build();
    assign_group.add(&main_row);

    // TTS Summarizer.
    let summary_provider = config.get_str("llm.tts_summary_provider", "");
    let summary_model = config.get_str("llm.tts_summary_model", "");
    let summary_text = if summary_provider.is_empty() {
        "Same as Main AI".to_string()
    } else {
        let display = crate::providers::find_by_id(&summary_provider)
            .map(|p| p.display_name)
            .unwrap_or(summary_provider.as_str());
        if summary_model.is_empty() {
            display.to_string()
        } else {
            format!("{display} \u{2014} {summary_model}")
        }
    };
    let summary_row = adw::ActionRow::builder()
        .title("TTS Summarizer")
        .subtitle(&summary_text)
        .build();
    assign_group.add(&summary_row);

    page.add(&assign_group);

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

    // Mic test — record button row
    let mic_test_btn = gtk::Button::with_label("Test");
    mic_test_btn.add_css_class("suggested-action");
    mic_test_btn.set_valign(gtk::Align::Center);

    let mic_test_row = adw::ActionRow::builder()
        .title("Microphone Test")
        .subtitle("Record a short clip and show signal level")
        .build();
    mic_test_row.add_suffix(&mic_test_btn);
    mic_test_row.set_activatable_widget(Some(&mic_test_btn));
    stt_group.add(&mic_test_row);

    // Mic test — playback row (play button + level bar + status)
    let mic_play_btn = gtk::Button::from_icon_name("media-playback-start-symbolic");
    mic_play_btn.set_tooltip_text(Some("Play back recording"));
    mic_play_btn.set_sensitive(false);
    mic_play_btn.set_valign(gtk::Align::Center);

    let mic_level = gtk::LevelBar::builder()
        .min_value(0.0)
        .max_value(1.0)
        .value(0.0)
        .hexpand(true)
        .build();
    mic_level.set_valign(gtk::Align::Center);

    let mic_status = gtk::Label::new(Some(""));
    mic_status.add_css_class("dim-label");

    let mic_result_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    mic_result_box.set_valign(gtk::Align::Center);
    mic_result_box.append(&mic_play_btn);
    mic_result_box.append(&mic_level);
    mic_result_box.append(&mic_status);

    let mic_result_row = adw::ActionRow::builder()
        .title("Playback")
        .build();
    mic_result_row.add_suffix(&mic_result_box);
    mic_result_row.set_visible(false); // Hidden until a recording is made
    stt_group.add(&mic_result_row);

    // Play button: plays back the recorded mic test
    {
        let play_btn = mic_play_btn.clone();
        mic_play_btn.connect_clicked(move |_| {
            play_btn.set_sensitive(false);
            let play_btn_c = play_btn.clone();
            std::thread::spawn(move || {
                let _ = std::process::Command::new("aplay")
                    .arg("/tmp/aios-mic-test.wav")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            });
            // Re-enable after playback duration (~3s)
            gtk::glib::timeout_add_local_once(
                std::time::Duration::from_secs(4),
                move || { play_btn_c.set_sensitive(true); },
            );
        });
    }

    {
        let level = mic_level.clone();
        let status = mic_status.clone();
        let btn = mic_test_btn.clone();
        let play = mic_play_btn.clone();
        let result_row = mic_result_row.clone();
        mic_test_btn.connect_clicked(move |_| {
            btn.set_sensitive(false);
            play.set_sensitive(false);
            result_row.set_visible(true); // Show playback row when test starts
            status.set_text("Recording...");
            level.set_value(0.0);

            let level_c = level.clone();
            let status_c = status.clone();
            let btn_c = btn.clone();
            let play_c = play.clone();

            // Record in background, update level bar via polling
            let (tx, rx) = std::sync::mpsc::channel::<f64>();
            std::thread::spawn(move || {
                // Record 3 seconds using arecord, analyze in chunks
                let output = std::process::Command::new("timeout")
                    .args(["4", "arecord", "-d", "3", "-f", "S16_LE", "-r", "16000", "-c", "1",
                           "-q", "/tmp/aios-mic-test.wav"])
                    .output();

                match output {
                    Ok(o) if o.status.success() || o.status.code() == Some(124) => {
                        // Analyze the recorded audio
                        if let Ok(data) = std::fs::read("/tmp/aios-mic-test.wav") {
                            if data.len() > 44 {
                                let samples: Vec<i16> = data[44..]
                                    .chunks_exact(2)
                                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                                    .collect();
                                let max = samples.iter().map(|s| s.unsigned_abs() as f64).fold(0.0f64, f64::max);
                                let energy = samples.iter().map(|s| s.unsigned_abs() as f64).sum::<f64>() / samples.len() as f64;
                                let level_norm = (max / 32768.0).min(1.0);
                                let _ = tx.send(level_norm);
                                // Also send negative to signal "done with energy info"
                                let _ = tx.send(-(energy));
                                return;
                            }
                        }
                        let _ = tx.send(-0.0); // done, no data
                    }
                    _ => {
                        let _ = tx.send(-0.0); // error
                    }
                }
            });

            // Poll for result
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                match rx.try_recv() {
                    Ok(val) if val >= 0.0 => {
                        level_c.set_value(val);
                        gtk::glib::ControlFlow::Continue
                    }
                    Ok(val) => {
                        // Negative = done, absolute value is energy
                        let energy = val.abs();
                        btn_c.set_sensitive(true);
                        play_c.set_sensitive(true);
                        if energy > 50.0 {
                            status_c.set_text("Mic working! Press play to hear.");
                            status_c.remove_css_class("error");
                        } else if energy > 5.0 {
                            status_c.set_text("Quiet signal. Press play to hear.");
                            status_c.remove_css_class("error");
                        } else {
                            status_c.set_text("No signal");
                            status_c.add_css_class("error");
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(_) => {
                        btn_c.set_sensitive(true);
                        status_c.set_text("Error");
                        status_c.add_css_class("error");
                        gtk::glib::ControlFlow::Break
                    }
                }
            });
        });
    }

    page.add(&stt_group);

    // Wake Word group.
    let wake_group = adw::PreferencesGroup::builder()
        .title("Wake Word")
        .build();

    // Wake word enabled switch.
    let wake_switch = gtk::Switch::new();
    wake_switch.set_active(config.get_bool("voice.wake_enabled", true));
    wake_switch.set_valign(gtk::Align::Center);
    let wake_enabled_row = adw::ActionRow::builder()
        .title("Wake Word Enabled")
        .subtitle("Listen for a wake word to activate the assistant")
        .build();
    wake_enabled_row.add_suffix(&wake_switch);
    wake_enabled_row.set_activatable_widget(Some(&wake_switch));
    wake_group.add(&wake_enabled_row);

    // Save wake enabled on change.
    {
        let config_dir = ConfigManager::default_config_dir();
        wake_switch.connect_state_set(move |_, active| {
            if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                let _ = cfg.set("voice.wake_enabled", serde_json::json!(active));
            }
            gtk::glib::Propagation::Proceed
        });
    }

    // Pre-trained wake word combo.
    let wake_names: Vec<&str> = PRETRAINED_NAMES.iter().map(|(_, name)| *name).collect();
    let wake_list = gtk::StringList::new(&wake_names);
    let wake_combo = adw::ComboRow::builder()
        .title("Pre-trained Wake Word")
        .subtitle("Choose a built-in wake word")
        .model(&wake_list)
        .build();
    let current_wake = config.get_str("voice.wake_word", "hey_assistant");
    let wake_idx = PRETRAINED_NAMES
        .iter()
        .position(|(slug, _)| *slug == current_wake.as_str())
        .unwrap_or(0) as u32;
    wake_combo.set_selected(wake_idx);
    wake_group.add(&wake_combo);

    // Save pre-trained wake word on change.
    {
        let config_dir = ConfigManager::default_config_dir();
        wake_combo.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if let Some((slug, _)) = PRETRAINED_NAMES.get(idx) {
                if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                    let _ = cfg.set("voice.wake_word", serde_json::json!(slug));
                    let _ = cfg.set("voice.wake_word_source", serde_json::json!("pretrained"));
                }
            }
        });
    }

    // Custom wake phrase entry.
    let custom_wake_entry = adw::EntryRow::builder()
        .title("Custom Wake Phrase")
        .build();
    custom_wake_entry.set_tooltip_text(Some(
        "Custom phrases require training (~5 min) and may be less accurate",
    ));
    wake_group.add(&custom_wake_entry);

    // Train button.
    let train_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    train_box.set_margin_top(4);

    let train_btn = gtk::Button::with_label("Train");
    train_btn.add_css_class("suggested-action");
    train_box.append(&train_btn);

    let train_status = gtk::Label::new(Some(""));
    train_status.add_css_class("dim-label");
    train_box.append(&train_status);

    let train_row = adw::ActionRow::builder()
        .title("Train Custom Wake Word")
        .subtitle("Record samples and train a model for your custom phrase")
        .build();
    train_row.add_suffix(&train_box);
    wake_group.add(&train_row);

    // Train button handler.
    {
        let config_dir = ConfigManager::default_config_dir();
        let entry = custom_wake_entry.clone();
        let status = train_status.clone();
        train_btn.connect_clicked(move |_| {
            let phrase = entry.text().to_string();
            if phrase.trim().is_empty() {
                status.set_text("Enter a phrase first");
                status.add_css_class("error");
                return;
            }
            // Update config with the custom phrase.
            if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                let _ = cfg.set("voice.wake_word", serde_json::json!(phrase.trim()));
                let _ = cfg.set("voice.wake_word_source", serde_json::json!("custom"));
            }
            status.remove_css_class("error");
            status.set_text("Config saved. Use /wake train to start training.");
        });
    }

    // Threshold spin button.
    let wake_threshold_adj = gtk::Adjustment::new(
        config.get_f64("voice.wake_threshold", 0.5),
        0.1,  // min
        1.0,  // max
        0.05, // step
        0.05, // page step
        0.0,  // page size
    );
    let threshold_spin = gtk::SpinButton::new(Some(&wake_threshold_adj), 0.05, 2);
    threshold_spin.set_valign(gtk::Align::Center);
    let threshold_row = adw::ActionRow::builder()
        .title("Threshold")
        .subtitle("Detection sensitivity (lower = more sensitive)")
        .build();
    threshold_row.add_suffix(&threshold_spin);
    wake_group.add(&threshold_row);

    // Save threshold on change.
    {
        let config_dir = ConfigManager::default_config_dir();
        threshold_spin.connect_value_changed(move |spin| {
            let val = spin.value();
            if let Ok(mut cfg) = ConfigManager::with_path(config_dir.join("config.json")) {
                let _ = cfg.set("voice.wake_threshold", serde_json::json!(val));
            }
        });
    }

    page.add(&wake_group);

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

    // TTS test button
    let tts_test_btn = gtk::Button::with_label("Test");
    tts_test_btn.add_css_class("suggested-action");
    tts_test_btn.set_valign(gtk::Align::Center);

    let tts_status = gtk::Label::new(Some(""));
    tts_status.add_css_class("dim-label");

    let tts_suffix = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    tts_suffix.set_valign(gtk::Align::Center);
    tts_suffix.append(&tts_status);
    tts_suffix.append(&tts_test_btn);

    let tts_test_row = adw::ActionRow::builder()
        .title("Audio Output Test")
        .subtitle("Play a short test message using current voice")
        .build();
    tts_test_row.add_suffix(&tts_suffix);
    tts_test_row.set_activatable_widget(Some(&tts_test_btn));
    tts_group.add(&tts_test_row);

    {
        let status = tts_status.clone();
        let btn = tts_test_btn.clone();
        let voice_slugs_c = voice_slugs.clone();
        let voice_row_c = voice_row.clone();
        tts_test_btn.connect_clicked(move |_| {
            btn.set_sensitive(false);
            status.set_text("Playing...");

            let btn_c = btn.clone();
            let status_c = status.clone();
            let voice = voice_slugs_c.get(voice_row_c.selected() as usize)
                .cloned()
                .unwrap_or_else(|| "en_US-amy-medium".to_string());

            let (tts_tx, tts_rx) = std::sync::mpsc::channel::<bool>();
            std::thread::spawn(move || {
                let test_text = "Hello! This is a test of the AiOS voice output. If you can hear me, your audio is working correctly.";
                let piper_model = format!("/home/aios/.aios/models/piper/{voice}.onnx");

                let success = if std::path::Path::new(&piper_model).exists() {
                    let child = std::process::Command::new("piper")
                        .args(["--model", &piper_model, "--output_raw"])
                        .stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::null())
                        .spawn();

                    match child {
                        Ok(mut c) => {
                            if let Some(ref mut stdin) = c.stdin {
                                use std::io::Write;
                                let _ = stdin.write_all(test_text.as_bytes());
                            }
                            let stdin = c.stdin.take();
                            drop(stdin);
                            if let Some(stdout) = c.stdout.take() {
                                let _ = std::process::Command::new("aplay")
                                    .args(["-r", "22050", "-f", "S16_LE", "-t", "raw", "-c", "1"])
                                    .stdin(stdout)
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .status();
                            }
                            let _ = c.wait();
                            true
                        }
                        Err(_) => false,
                    }
                } else {
                    std::process::Command::new("espeak-ng")
                        .args(["-v", "en", "-s", "170"])
                        .arg(test_text)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .is_ok()
                };

                let _ = tts_tx.send(success);
            });

            // Poll for TTS completion
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                match tts_rx.try_recv() {
                    Ok(success) => {
                        btn_c.set_sensitive(true);
                        if success {
                            status_c.set_text("Done!");
                            status_c.remove_css_class("error");
                        } else {
                            status_c.set_text("Failed");
                            status_c.add_css_class("error");
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(_) => {
                        btn_c.set_sensitive(true);
                        status_c.set_text("Error");
                        gtk::glib::ControlFlow::Break
                    }
                }
            });
        });
    }

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
