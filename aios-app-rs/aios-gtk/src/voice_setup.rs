//! KWS engine, voice listener, and VU meter setup.
//!
//! Extracted from `app.rs`. Contains the full KWS model loading, training
//! recovery, and voice listener startup logic used by both `activate_main`
//! and `transition_to_normal_mode`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use libadwaita as adw;
use tracing::{info, warn};

use aios_core::config::ConfigManager;

use crate::app::AiosApp;
use crate::llm_handler;
use crate::tts::stop_tts;
use crate::ui::chat_view::ChatView;
use crate::ui::main_window;
use crate::ui::prompt_input::PromptInput;
use crate::voice_listener::start_voice_listener;

// ---------------------------------------------------------------------------
// Full KWS + voice setup (used by activate_main)
// ---------------------------------------------------------------------------

/// Set up KWS engine, voice listener, VU meter, and speaker toggle.
///
/// This is the full setup path used during normal boot. It includes:
/// - Mic toggle wiring
/// - KWS model copying from ISO
/// - KWS engine creation + wake word model loading
/// - Training recovery for interrupted training
/// - Voice listener start
/// - VU meter polling
/// - STT transcription polling
/// - Speaker toggle wiring
pub(crate) fn setup_kws_and_voice(
    state: &Rc<RefCell<AiosApp>>,
    chat_view: &ChatView,
    prompt_input: &PromptInput,
    window: &adw::ApplicationWindow,
    vu_meter_widget: gtk4::LevelBar,
    kws_models_dir: &std::path::Path,
) {
    let stt_enabled = Arc::new(std::sync::atomic::AtomicBool::new(
        state.borrow().config.get_bool("voice.stt_enabled", true),
    ));
    let stt_flag = stt_enabled.clone();
    let state_ref = state.clone();
    main_window::connect_mic_toggle(window, move |active| {
        let mut s = state_ref.borrow_mut();
        let _ = s.config.set("voice.stt_enabled", serde_json::json!(active));
        stt_flag.store(active, std::sync::atomic::Ordering::Relaxed);
        info!("Mic toggled: {active}");
    });

    // Read wake word configuration.
    let wake_word_cfg = {
        let s = state.borrow();
        let wake_phrase = s.config.get_str("voice.wake_word", "hey jarvis");
        let wake_on = s.config.get_bool("voice.wake_enabled", true);
        let wake_source = s.config.get_str("voice.wake_word_source", "");
        (wake_phrase, wake_on, wake_source)
    };

    let wake_enabled = Arc::new(std::sync::atomic::AtomicBool::new(wake_word_cfg.1));
    let wake_training_in_progress = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Copy KWS infrastructure models from ISO on first run.
    {
        let iso_kws_dir = std::path::Path::new("/opt/aios-app/models/kws");
        if iso_kws_dir.exists() && !kws_models_dir.exists() {
            if let Err(e) = copy_dir_recursive(iso_kws_dir, kws_models_dir) {
                warn!("KWS: failed to copy models from ISO: {e}");
            } else {
                info!(
                    "KWS: copied infrastructure models from ISO to {}",
                    kws_models_dir.display()
                );
            }
        }
    }

    // Create KWS engine.
    let kws_engine: Arc<std::sync::Mutex<Option<aios_voice::KwsEngine>>> = {
        match aios_voice::KwsEngine::new(kws_models_dir) {
            Ok(mut engine) => {
                // Apply configured threshold.
                let threshold = state.borrow().config.get_f64("voice.wake_threshold", 0.5) as f32;
                engine.set_threshold(threshold);
                info!("KWS: threshold set to {threshold}");

                load_wake_model(
                    &mut engine,
                    &wake_word_cfg.0,
                    &wake_word_cfg.2,
                    kws_models_dir,
                );
                Arc::new(std::sync::Mutex::new(Some(engine)))
            }
            Err(e) => {
                warn!("KWS: failed to create engine (infrastructure models missing?): {e}");
                Arc::new(std::sync::Mutex::new(None))
            }
        }
    };

    // Store KWS state in the app.
    {
        let mut s = state.borrow_mut();
        s.kws_engine = Some(kws_engine.clone());
        s.wake_training_in_progress = Some(wake_training_in_progress.clone());
        s.wake_enabled = Some(wake_enabled.clone());
        s.kws_models_dir = kws_models_dir.to_path_buf();
    }

    // Training recovery.
    recover_interrupted_training(
        state,
        chat_view,
        &wake_enabled,
        &wake_training_in_progress,
        &kws_engine,
        kws_models_dir,
    );

    // Start voice listener.
    let (stt_tx, stt_rx) = std::sync::mpsc::channel::<String>();
    let audio_level = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let _voice_handle = start_voice_listener(
        stt_tx,
        stt_enabled,
        wake_enabled.clone(),
        wake_training_in_progress.clone(),
        kws_engine.clone(),
        audio_level.clone(),
    );

    // VU meter.
    let vu_level = audio_level.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        let level = vu_level.load(std::sync::atomic::Ordering::Relaxed);
        vu_meter_widget.set_value(level as f64 / 5.0);
        glib::ControlFlow::Continue
    });

    // Poll for STT transcriptions.
    let state_ref = state.clone();
    let chat_view_ref = chat_view.clone();
    let prompt_ref = prompt_input.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(text) = stt_rx.try_recv() {
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            info!(
                "STT transcription received: {}",
                &text[..text.len().min(50)]
            );
            chat_view_ref.add_message("user", &format!("\u{1f3a4} {text}"));
            llm_handler::send_to_llm(&state_ref, &chat_view_ref, &prompt_ref, text);
        }
        glib::ControlFlow::Continue
    });

    // Speaker toggle.
    let state_ref = state.clone();
    main_window::connect_speaker_toggle(window, move |active| {
        let mut s = state_ref.borrow_mut();
        let _ = s.config.set("voice.tts_enabled", serde_json::json!(active));
        if !active {
            stop_tts();
        }
        info!("Speaker toggled: {active}");
    });
}

// ---------------------------------------------------------------------------
// Training recovery
// ---------------------------------------------------------------------------

/// Recover from interrupted wake word training.
fn recover_interrupted_training(
    state: &Rc<RefCell<AiosApp>>,
    chat_view: &ChatView,
    wake_enabled: &Arc<std::sync::atomic::AtomicBool>,
    wake_training_in_progress: &Arc<std::sync::atomic::AtomicBool>,
    kws_engine: &Arc<std::sync::Mutex<Option<aios_voice::KwsEngine>>>,
    kws_models_dir: &std::path::Path,
) {
    let wake_source = state
        .borrow()
        .config
        .get_str("voice.wake_word_source", "pretrained");
    if wake_source != "training" {
        return;
    }

    let phrase = state
        .borrow()
        .config
        .get_str("voice.wake_word", "hey jarvis");
    let retries = state
        .borrow()
        .config
        .get_f64("voice.wake_training_retries", 0.0) as i64;
    info!("KWS: detected interrupted training for \"{phrase}\" (attempt {retries})");

    if retries >= 3 {
        {
            let mut s = state.borrow_mut();
            let _ = s
                .config
                .set("voice.wake_word", serde_json::json!("hey jarvis"));
            let _ = s
                .config
                .set("voice.wake_word_source", serde_json::json!("pretrained"));
            let _ = s
                .config
                .set("voice.wake_training_retries", serde_json::json!(0));
        }
        chat_view.add_message(
            "system",
            "[SYSTEM] Wake word training failed after 3 attempts. Reset to \"Hey Assistant\".",
        );
        return;
    }

    {
        let mut s = state.borrow_mut();
        let _ = s.config.set(
            "voice.wake_training_retries",
            serde_json::json!(retries + 1),
        );
    }
    chat_view.add_message(
        "system",
        &format!(
            "[SYSTEM] Restarting wake word training for \"{}\" (attempt {})...",
            phrase,
            retries + 1
        ),
    );

    wake_training_in_progress.store(true, std::sync::atomic::Ordering::Relaxed);

    let output_dir = kws_models_dir.join("custom");
    let chat_for_recovery = chat_view.clone();
    let state_for_recovery = state.clone();
    let kws_engine_for_recovery = kws_engine.clone();
    let wake_enabled_for_recovery = wake_enabled.clone();
    let training_flag_for_recovery = wake_training_in_progress.clone();
    let phrase_clone = phrase.clone();
    let (train_tx, train_rx) =
        std::sync::mpsc::channel::<Result<std::path::PathBuf, String>>();

    std::thread::spawn(move || {
        let result = aios_voice::KwsTrainer::train(&phrase_clone, &output_dir);
        let _ = train_tx.send(result.map_err(|e| e.to_string()));
    });

    glib::timeout_add_local(
        std::time::Duration::from_millis(500),
        move || match train_rx.try_recv() {
            Ok(Ok(model_path)) => {
                info!(
                    "KWS recovery training complete: {}",
                    model_path.display()
                );
                chat_for_recovery.add_message(
                    "system",
                    &format!("Wake word training complete for \"{phrase}\". Model saved."),
                );

                if let Ok(mut guard) = kws_engine_for_recovery.lock() {
                    if let Some(ref mut engine) = *guard {
                        match engine.load_wake_model(&model_path, &phrase) {
                            Ok(()) => {
                                info!("KWS: hot-swapped model for \"{}\"", phrase);
                                chat_for_recovery.add_message(
                                    "system",
                                    &format!("Wake word \"{phrase}\" is now active."),
                                );
                            }
                            Err(e) => {
                                warn!("KWS: failed to load trained model: {e}");
                            }
                        }
                    }
                }

                wake_enabled_for_recovery.store(true, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut cfg) = ConfigManager::new() {
                    let _ = cfg.set("voice.wake_word_source", serde_json::json!("custom"));
                    let _ = cfg.set("voice.wake_training_retries", serde_json::json!(0));
                }

                training_flag_for_recovery
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                glib::ControlFlow::Break
            }
            Ok(Err(e)) => {
                warn!("KWS recovery training failed: {e}");
                chat_for_recovery.add_level_message(
                    aios_core::types::MessageLevel::Warning,
                    &format!("Wake word training failed: {e}"),
                );
                let s = state_for_recovery.borrow();
                if let Some(ref flag) = s.wake_training_in_progress {
                    flag.store(false, std::sync::atomic::Ordering::Relaxed);
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let s = state_for_recovery.borrow();
                if let Some(ref flag) = s.wake_training_in_progress {
                    flag.store(false, std::sync::atomic::Ordering::Relaxed);
                }
                glib::ControlFlow::Break
            }
        },
    );
}

// ---------------------------------------------------------------------------
// Remote channel message loop
// ---------------------------------------------------------------------------

/// Set up the remote-channel message loop (Web, Signal).
///
/// Routes messages from Web/Signal channels to the LLM and back.
pub(crate) fn start_remote_channel_loop(
    rt: &tokio::runtime::Handle,
    runtime: &Arc<aios_core::channel::AppRuntime>,
    state: &Rc<RefCell<AiosApp>>,
    chat_view: &ChatView,
    web_tx: Option<tokio::sync::broadcast::Sender<String>>,
    signal_sender: Option<Arc<aios_signal::SignalSender>>,
) {
    let msg_rx_holder = runtime.clone();
    let state_for_loop = state.clone();
    let chat_for_loop = chat_view.clone();
    let switcher_for_loop = runtime.switcher.clone();
    let rt = rt.clone();

    let (bridge_tx, bridge_rx) =
        std::sync::mpsc::channel::<aios_core::channel::IncomingMessage>();

    rt.spawn(async move {
        if let Some(mut rx) = msg_rx_holder.take_message_rx().await {
            while let Some(msg) = rx.recv().await {
                if bridge_tx.send(msg).is_err() {
                    break;
                }
            }
        }
    });

    let web_tx_clone = web_tx.clone();
    let sig_clone = signal_sender.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        while let Ok(msg) = bridge_rx.try_recv() {
            if switcher_for_loop.active_kind() != msg.channel {
                let _ = switcher_for_loop.switch_to(msg.channel);
            }

            chat_for_loop.add_message("user", &msg.text);

            if msg.text.starts_with('/') {
                AiosApp::handle_command(&state_for_loop, &chat_for_loop, &msg.text);
                continue;
            }

            {
                let mut s = state_for_loop.borrow_mut();
                s.conversation
                    .push(aios_core::types::Message::user(&msg.text));
            }

            llm_handler::handle_remote_llm_message(
                &state_for_loop,
                &chat_for_loop,
                &msg.text,
                msg.sender_id.clone(),
                msg.channel,
                web_tx_clone.clone(),
                sig_clone.clone(),
            );
        }
        glib::ControlFlow::Continue
    });
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Recursively copy all files from `src` directory into `dst` directory.
pub(crate) fn copy_dir_recursive(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Try to load the active wake word model into the KWS engine.
fn load_wake_model(
    engine: &mut aios_voice::KwsEngine,
    wake_phrase: &str,
    wake_source: &str,
    kws_models_dir: &std::path::Path,
) {
    if let Some(pretrained) = aios_voice::find_pretrained(wake_phrase) {
        let model_path = aios_voice::pretrained_model_path(kws_models_dir, pretrained);
        if model_path.exists() {
            match engine.load_wake_model(&model_path, pretrained.display_name) {
                Ok(()) => info!(
                    "KWS: loaded pretrained model for \"{}\"",
                    pretrained.display_name
                ),
                Err(e) => warn!("KWS: failed to load pretrained model: {e}"),
            }
        } else {
            warn!(
                "KWS: pretrained model file not found at {}",
                model_path.display()
            );
        }
    } else if wake_source == "training" || wake_source == "custom" {
        let sanitized = wake_phrase
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        let custom_path = kws_models_dir
            .join("custom")
            .join(format!("{sanitized}.onnx"));
        if custom_path.exists() {
            match engine.load_wake_model(&custom_path, wake_phrase) {
                Ok(()) => info!("KWS: loaded custom model for \"{}\"", wake_phrase),
                Err(e) => warn!("KWS: failed to load custom model: {e}"),
            }
        } else {
            warn!(
                "KWS: custom model not found at {} -- will use fallback detection",
                custom_path.display()
            );
        }
    } else if !wake_phrase.is_empty() {
        info!("KWS: no model for \"{wake_phrase}\" -- will use Whisper fallback detection");
    }
}
