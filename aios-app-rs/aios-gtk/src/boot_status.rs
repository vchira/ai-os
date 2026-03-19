//! Boot status text generation.
//!
//! Extracted from `app.rs` to deduplicate the boot status block that was
//! repeated in `run_first_boot_setup`, `apply_autoconfig`, and
//! `activate_main`.

use aios_core::config::ConfigManager;
use aios_core::i18n::{t, t_fmt};
use aios_core::types::{BootStatus, StatusLine};

/// Build the boot status text from configuration.
///
/// Reads channel, voice, KWS, and system settings from `config` and produces
/// a formatted status string suitable for displaying in the chat view at boot.
pub(crate) fn build_boot_status(config: &ConfigManager) -> String {
    let mut status = BootStatus::new();

    // -- Channels --
    status.add(StatusLine::new(
        &t("boot.status.desktop"),
        true,
        "GTK4/libadwaita",
    ));

    let web_enabled = config.get_bool("channels.web.enabled", true);
    let web_port = config.get_str("channels.web.port", "80");
    if web_enabled {
        status.add(StatusLine::new(
            &t("boot.status.web_channel"),
            true,
            format!("http://aios.local:{web_port}"),
        ));
    } else {
        status.add(StatusLine::new(
            &t("boot.status.web_channel"),
            false,
            &t("boot.status.disabled_web"),
        ));
    }

    let signal_enabled = config.get_bool("channels.signal.enabled", false);
    let signal_phone = config.get_str("channels.signal.phone", "");
    if signal_enabled && !signal_phone.is_empty() {
        status.add(StatusLine::new(
            &t("boot.status.signal"),
            true,
            &signal_phone,
        ));
    } else {
        status.add(StatusLine::new(
            &t("boot.status.signal"),
            false,
            &t("boot.status.disabled_signal"),
        ));
    }

    // -- LLM Provider + Model --
    let provider_id = config.get_str("llm.provider", "claude");
    let active_def = crate::providers::find_by_id(&provider_id);
    let has_key = active_def
        .map(|p| crate::providers::is_configured(p, config))
        .unwrap_or(false);
    let model = active_def
        .map(|p| crate::providers::current_model(p, config))
        .unwrap_or_else(|| provider_id.clone());
    let display = active_def
        .map(|p| p.display_name)
        .unwrap_or_else(|| provider_id.as_str());
    if has_key {
        status.add(StatusLine::new(
            &t("boot.status.llm_provider"),
            true,
            format!("{display} ({model})"),
        ));
    } else {
        status.add(StatusLine::new(
            &t("boot.status.llm_provider"),
            false,
            t_fmt("boot.status.no_api_key_hint", &[("provider", display)]),
        ));
    }

    // Show backup providers — any configured provider that is not the active one.
    for p in crate::providers::PROVIDERS {
        if p.id == provider_id {
            continue;
        }
        if crate::providers::is_configured(p, config) {
            status.add(StatusLine::new(
                &t("boot.status.backup"),
                true,
                t_fmt(
                    "boot.status.backup_available",
                    &[("provider", p.display_name)],
                ),
            ));
        }
    }

    // -- Voice --
    let stt = config.get_bool("voice.stt_enabled", true);
    let tts = config.get_bool("voice.tts_enabled", true);
    let tts_voice = config.get_str("voice.tts_voice", "en_US-amy-medium");
    let has_piper = std::path::Path::new("/usr/bin/piper").exists();
    let has_espeak = std::path::Path::new("/usr/bin/espeak-ng").exists();
    let has_whisper = std::path::Path::new("/usr/bin/whisper-cpp-cli").exists();
    let tts_backend = if has_piper {
        "Piper"
    } else if has_espeak {
        "espeak-ng"
    } else {
        "none"
    };
    let not_installed = t("boot.status.not_installed");
    let stt_backend = if has_whisper { "Whisper" } else { &not_installed };
    status.add(StatusLine::new(
        &t("boot.status.audio_output"),
        tts && (has_piper || has_espeak),
        format!("{tts_backend} \u{2014} voice: {tts_voice}"),
    ));
    status.add(StatusLine::new(
        &t("boot.status.audio_input"),
        stt && has_whisper,
        format!(
            "{stt_backend}{}",
            if !stt { " \u{2014} disabled" } else { "" }
        ),
    ));

    // -- KWS (Wake Word Detection) --
    append_kws_status(&mut status, config);

    // -- System --
    let kb_layout = config.get_str("system.keyboard_layout", "us");
    let timezone = std::fs::read_to_string("/etc/timezone")
        .unwrap_or_else(|_| "UTC".into())
        .trim()
        .to_string();
    let boot_time = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    status.add(StatusLine::new(
        &t("boot.status.keyboard"),
        true,
        kb_layout,
    ));
    status.add(StatusLine::new(
        &t("boot.status.timezone"),
        true,
        &timezone,
    ));
    status.add(StatusLine::new(
        &t("boot.status.boot_time"),
        true,
        boot_time,
    ));

    status.format()
}

/// Append KWS (wake word) status lines to a [`BootStatus`].
fn append_kws_status(status: &mut BootStatus, config: &ConfigManager) {
    let wake_enabled = config.get_bool("voice.wake_enabled", true);
    if !wake_enabled {
        status.add(StatusLine::new(
            &t("boot.status.kws"),
            false,
            &t("boot.status.kws_disabled"),
        ));
        return;
    }

    // Check system path first (ISO), then user path.
    let system_kws = std::path::PathBuf::from("/opt/aios-app/models/kws");
    let user_kws = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/home/aios"))
        .join(".aios/models/kws");
    let kws_models_dir = if system_kws.join("infrastructure/melspectrogram.onnx").exists() {
        system_kws
    } else {
        user_kws
    };
    let wake_word = config.get_str("voice.wake_word", "hey_assistant");
    let wake_source = config.get_str("voice.wake_word_source", "pretrained");
    let infra_present = kws_models_dir
        .join("infrastructure/melspectrogram.onnx")
        .exists()
        || kws_models_dir
            .join("infrastructure/embedding_model.onnx")
            .exists();

    if !infra_present {
        status.add(StatusLine::new(
            &t("boot.status.kws"),
            false,
            &t("boot.status.kws_models_not_found"),
        ));
        return;
    }

    if wake_source == "training" {
        status.add(StatusLine::new(
            &t("boot.status.kws"),
            false,
            t_fmt("boot.status.kws_training", &[("wake_word", &wake_word)]),
        ));
        return;
    }

    // Check whether the actual model file exists.
    let model_available = if let Some(pretrained) = aios_voice::find_pretrained(&wake_word) {
        aios_voice::pretrained_model_path(&kws_models_dir, pretrained).exists()
    } else {
        let sanitized = wake_word
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        kws_models_dir
            .join("custom")
            .join(format!("{sanitized}.onnx"))
            .exists()
    };

    if model_available {
        status.add(StatusLine::new(
            &t("boot.status.kws"),
            true,
            format!("{wake_word} ({wake_source})"),
        ));
    } else {
        status.add(StatusLine::new(
            &t("boot.status.kws"),
            false,
            &t("boot.status.kws_models_not_found"),
        ));
    }
}
