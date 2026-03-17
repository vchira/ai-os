//! AiOS application orchestrator.
//!
//! [`AiosApp`] is a plain Rust struct (no GObject subclassing) that holds
//! all subsystem managers and wires them together via closures and
//! [`glib::MainContext::channel`] for async communication from Tokio back
//! to the GTK main thread.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use tracing::{debug, error, info, warn};

use aios_core::config::commands::{CommandHandler, CommandResult, PanelFieldKind};
use aios_core::config::ConfigManager;
use aios_core::secure::Vault;
use aios_core::secure::vault::{SecretEntry, SecretKind};
use aios_llm::{ClaudeProvider, LlmManager, OpenAIProvider};
use aios_tools::ToolRegistry;
use aios_tools::builtin::ui_panel::UiPanelTool;

use crate::ui::channel_overlay::ChannelOverlay;
use crate::ui::chat_view::ChatView;
use crate::ui::first_boot::SetupConversation;
use crate::ui::main_window;
use crate::ui::panel_renderer::PanelRenderer;
use crate::ui::prompt_input::PromptInput;
use crate::ui::settings_dialog;

// ---------------------------------------------------------------------------
// AiosApp
// ---------------------------------------------------------------------------

/// Start a background voice listener thread that continuously captures audio,
/// detects speech via VAD, and transcribes using whisper-cpp (or sends raw
/// audio to the STT engine). Transcribed text is sent back to the GTK thread
/// via the provided glib sender.
fn start_voice_listener(
    stt_tx: std::sync::mpsc::Sender<String>,
    stt_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<()> {
    use aios_voice::audio::capture::AudioCapture;
    use aios_voice::audio::vad::{VadConfig, VoiceActivityDetector, DEFAULT_FRAME_SIZE};

    std::thread::spawn(move || {
        info!("Voice listener thread started");

        let mut capture = AudioCapture::new();
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.015,
            min_speech_frames: 4,
            min_silence_frames: 20, // ~600ms of silence to end utterance
        });

        // Buffer to accumulate speech audio
        let mut speech_buffer: Vec<f32> = Vec::new();
        let mut was_active = false;

        // Try to start recording — may fail in VMs where mic is not available
        if let Err(e) = capture.start_recording() {
            warn!("Voice listener: no microphone available: {e}");
            let _ = stt_tx.send(String::new()); // empty = no error shown
            // Check if we're in a VM (no mic through SPICE)
            let is_vm = std::path::Path::new("/sys/class/dmi/id/product_name")
                .read_dir().is_ok()
                || std::fs::read_to_string("/sys/class/dmi/id/chassis_type")
                    .map(|s| s.trim() == "1") // "1" = Other (VM)
                    .unwrap_or(false);
            if is_vm {
                info!("Voice listener: running in VM — microphone not available via SPICE. STT will work on real hardware.");
            }
            return;
        }
        info!("Voice listener: recording started, waiting for speech...");

        loop {
            // Check if STT is still enabled
            if !stt_enabled.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }

            // Read accumulated samples from the capture buffer
            std::thread::sleep(std::time::Duration::from_millis(30));

            // Access the capture buffer directly
            let samples = {
                if let Ok(mut buf) = capture.buffer().lock() {
                    let s = std::mem::take(&mut *buf);
                    s
                } else {
                    continue;
                }
            };

            if samples.is_empty() {
                continue;
            }

            // Process frames through VAD
            for chunk in samples.chunks(DEFAULT_FRAME_SIZE) {
                let is_active = vad.process_frame(chunk);

                if is_active {
                    speech_buffer.extend_from_slice(chunk);
                } else if was_active && !is_active {
                    // Speech just ended — transcribe the buffer
                    let audio_duration = speech_buffer.len() as f32 / 16000.0;

                    if audio_duration > 0.5 && audio_duration < 30.0 {
                        info!("Voice listener: speech detected ({audio_duration:.1}s), transcribing...");

                        // Try whisper-cpp-cli first
                        match transcribe_with_whisper(&speech_buffer) {
                            Ok(text) if !text.is_empty() => {
                                info!("Voice listener: transcribed: {}", &text[..text.len().min(50)]);
                                let _ = stt_tx.send(text);
                            }
                            Ok(_) => {
                                debug!("Voice listener: empty transcription, ignoring");
                            }
                            Err(e) => {
                                warn!("Voice listener: transcription failed: {e}");
                            }
                        }
                    }

                    speech_buffer.clear();
                    vad.reset();
                }
                was_active = is_active;
            }

            // Prevent unbounded buffer growth
            if speech_buffer.len() > 16000 * 30 {
                warn!("Voice listener: speech buffer too large, clearing");
                speech_buffer.clear();
                vad.reset();
            }
        }
    })
}

/// Transcribe audio samples using whisper-cpp-cli.
fn transcribe_with_whisper(samples: &[f32]) -> Result<String, String> {
    // Write samples as WAV to a temp file
    let tmp_path = "/tmp/aios-stt-input.wav";
    let sample_rate: u32 = 16000;
    let num_samples = samples.len() as u32;
    let data_size = num_samples * 2; // 16-bit samples

    let mut buf: Vec<u8> = Vec::with_capacity(44 + data_size as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &s in samples {
        let sample = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    std::fs::write(tmp_path, &buf).map_err(|e| format!("Failed to write WAV: {e}"))?;

    // Try whisper-cpp-cli
    let output = std::process::Command::new("whisper-cpp-cli")
        .args(["-m", "/home/aios/.aios/models/whisper/ggml-tiny.bin"])
        .args(["-f", tmp_path])
        .args(["--no-timestamps", "-nt"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("whisper-cpp-cli not available: {e}"))?;

    let _ = std::fs::remove_file(tmp_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("whisper failed: {stderr}"));
    }

    let text = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    Ok(text)
}

/// Stop any running TTS playback immediately.
fn stop_tts() {
    std::thread::spawn(|| {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "piper"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = std::process::Command::new("pkill")
            .args(["-f", "espeak-ng"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = std::process::Command::new("pkill")
            .args(["-f", "aplay.*raw"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// Speak text using espeak-ng if TTS is enabled.
///
/// Runs in a background thread so it doesn't block the GTK main loop.
fn speak_if_enabled(text: &str, config: &ConfigManager) {
    let tts_enabled = config.get_bool("voice.tts_enabled", true);
    if !tts_enabled {
        return;
    }
    let text = text.to_string();
    std::thread::spawn(move || {
        // Truncate very long responses for TTS (read first ~500 chars)
        let speak_text = if text.len() > 500 {
            format!("{}... and more.", &text[..text.floor_char_boundary(500)])
        } else {
            text
        };

        // Try Piper first (high-quality, natural sounding voice)
        let piper_model = "/home/aios/.aios/models/piper/en_US-amy-medium.onnx";
        if std::path::Path::new(piper_model).exists() {
            let mut child = match std::process::Command::new("piper")
                .args(["--model", piper_model, "--output_raw"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => {
                    // Piper not available, fall through to espeak
                    let _ = std::process::Command::new("espeak-ng")
                        .args(["-v", "en", "-s", "170"])
                        .arg(&speak_text)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    return;
                }
            };

            // Write text to piper's stdin
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(speak_text.as_bytes());
                drop(stdin); // Close stdin to signal EOF
            }

            // Pipe piper's raw audio output to aplay
            if let Some(stdout) = child.stdout.take() {
                let _ = std::process::Command::new("aplay")
                    .args(["-r", "22050", "-f", "S16_LE", "-t", "raw", "-c", "1"])
                    .stdin(stdout)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            let _ = child.wait();
            return;
        }

        // Fallback: espeak-ng (robotic but always available)
        let _ = std::process::Command::new("espeak-ng")
            .args(["-v", "en", "-s", "170"])
            .arg(&speak_text)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// Application-level state shared across signal handlers.
///
/// Wrapped in `Rc<RefCell<...>>` for the GTK main-thread parts, and
/// `Arc<tokio::sync::Mutex<...>>` for anything shared with the Tokio runtime.
pub struct AiosApp {
    config: ConfigManager,
    llm: Arc<tokio::sync::Mutex<LlmManager>>,
    #[allow(dead_code)]
    tools: ToolRegistry,
    conversation: Vec<aios_core::types::Message>,
    rt: tokio::runtime::Handle,
}

impl AiosApp {
    /// Called from `Application::connect_activate`. Builds the entire UI and
    /// wires up signals.
    pub fn activate(app: &adw::Application, rt: tokio::runtime::Handle) {
        // Check if the vault exists. If not, run the first-boot setup.
        let vault_path = ConfigManager::default_config_dir().join("vault.enc");
        let vault = Vault::new(vault_path);

        if !vault.exists() {
            info!("No vault found — launching first-boot setup conversation");
            Self::run_first_boot_setup(app, rt);
            return;
        }

        // Vault exists — proceed with normal startup.
        Self::activate_main(app, rt);
    }

    /// Run the first-boot setup as a conversation in the main chat view.
    ///
    /// Builds the normal main window (fullscreen) but starts a
    /// [`SetupConversation`] that drives setup cards through the chat view.
    /// Once setup completes, the vault is created, secrets are stored, the
    /// LLM manager is configured, and normal chat mode begins.
    ///
    /// **Channel restriction**: First-boot setup can only happen on the
    /// Desktop (GTK) or Web channel. Remote channels (Signal, Voice, etc.)
    /// are not started until `activate_main()`, which runs *after* the vault
    /// has been created. This means the setup wizard is never reachable from
    /// remote channels — by the time they are online, the vault already
    /// exists and `activate()` takes the normal startup path.
    fn run_first_boot_setup(app: &adw::Application, rt: tokio::runtime::Handle) {
        // Build the main UI — same window as normal mode.
        let chat_view = ChatView::new();
        let prompt_input = PromptInput::new();
        let channel_overlay = ChannelOverlay::new();

        let window = main_window::build_main_window(app, &chat_view, &prompt_input, &channel_overlay, &["Setup..."]);

        // Hide the prompt input during setup — it will be shown in transition_to_normal_mode.
        prompt_input.widget().set_visible(false);

        // Load config and create channel infrastructure for setup mode.
        let mut config = match ConfigManager::new() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to load config for first-boot, using defaults: {e}");
                match ConfigManager::with_path(std::path::PathBuf::from("/tmp/.aios/config.json")) {
                    Ok(c) => c,
                    Err(e2) => {
                        error!("Fallback config also failed: {e2}");
                        panic!("Cannot initialize configuration from any path");
                    }
                }
            }
        };

        // Apply assistant display name from config.
        let assistant_name = config.get_str("assistant.name", "Assistant");
        crate::ui::chat_view::set_assistant_display_name(&assistant_name);

        // Create the shared AppRuntime for multi-channel orchestration.
        let runtime = aios_core::channel::AppRuntime::new();

        // Register Desktop channel (always available).
        runtime.switcher.register_channel(
            aios_core::channel::ChannelKind::Desktop,
            aios_core::channel::ChannelContext::desktop(),
        );

        // Build boot status text for setup mode.
        let boot_status_text = {
            use aios_core::types::{BootStatus, StatusLine};
            let mut status = BootStatus::new();

            status.add(StatusLine::new("Desktop", true, "GTK4/libadwaita"));

            let web_enabled = config.get_bool("channels.web.enabled", true);
            let web_port = config.get_str("channels.web.port", "80");
            if web_enabled {
                status.add(StatusLine::new("Web Channel", true, format!("http://aios.local:{web_port}")));
            } else {
                status.add(StatusLine::new("Web Channel", false, "disabled (/channel web on)"));
            }

            // Check audio
            let has_piper = std::path::Path::new("/usr/bin/piper").exists();
            let has_espeak = std::path::Path::new("/usr/bin/espeak-ng").exists();
            let has_whisper = std::path::Path::new("/usr/bin/whisper-cpp-cli").exists();
            let tts_backend = if has_piper { "Piper" } else if has_espeak { "espeak-ng" } else { "none" };
            status.add(StatusLine::new("Audio Output (TTS)", has_piper || has_espeak, tts_backend));
            status.add(StatusLine::new("Audio Input (STT)", has_whisper, if has_whisper { "Whisper" } else { "not installed" }));

            // System info
            let kb = config.get_str("system.keyboard_layout", "us");
            let tz = std::fs::read_to_string("/etc/timezone")
                .unwrap_or_else(|_| "UTC".into()).trim().to_string();
            let boot_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            status.add(StatusLine::new("Keyboard", true, kb));
            status.add(StatusLine::new("Timezone", true, &tz));
            status.add(StatusLine::new("Boot time", true, boot_time));

            status.format()
        };

        // Start the web server so the setup wizard is also available via browser.
        // Enter the Tokio runtime context — server.start() uses tokio::spawn().
        let _guard = rt.enter();
        let _web_server = Self::start_web_server(
            &mut config,
            &runtime,
            Some(boot_status_text.clone()),
        );

        // Show boot status on Desktop.
        chat_view.add_level_message(aios_core::types::MessageLevel::Info, &boot_status_text);

        // Apply theme from config at startup.
        {
            let theme = config.get_str("ui.theme", "dark");
            Self::apply_theme(&theme);
        }

        // Check for hostname collision (set by aios-hostname-check.service at boot).
        if std::path::Path::new("/tmp/aios-name-conflict").exists() {
            if let Ok(conflicting) = std::fs::read_to_string("/tmp/aios-name-conflict") {
                let conflicting = conflicting.trim().to_string();
                let random_suffix = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() % 10000) as u16;
                let suggestion = format!("{conflicting}-{random_suffix}");

                chat_view.add_level_message(
                    aios_core::types::MessageLevel::Warning,
                    &format!("Hostname conflict: another machine on this network is already using '{conflicting}.local'"),
                );

                // Show rename card
                let input_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
                input_box.set_margin_top(8);

                let entry = gtk4::Entry::builder()
                    .placeholder_text("New hostname")
                    .text(&suggestion)
                    .hexpand(true)
                    .build();
                input_box.append(&entry);

                let hint = gtk4::Label::new(Some("Lowercase letters, numbers, and hyphens only. Will be reachable as <name>.local"));
                hint.add_css_class("dim-label");
                hint.set_halign(gtk4::Align::Start);
                hint.set_wrap(true);
                input_box.append(&hint);

                let error_label = gtk4::Label::new(None);
                error_label.add_css_class("error");
                error_label.set_visible(false);
                error_label.set_halign(gtk4::Align::Start);
                input_box.append(&error_label);

                let apply_btn = gtk4::Button::with_label("Apply");
                apply_btn.add_css_class("suggested-action");
                apply_btn.set_halign(gtk4::Align::Start);
                apply_btn.set_margin_top(4);
                input_box.append(&apply_btn);

                let entry_ref = entry.clone();
                let error_ref = error_label.clone();
                let chat_ref = chat_view.clone();
                apply_btn.connect_clicked(move |b| {
                    let name = entry_ref.text().to_string().trim().to_lowercase();

                    // Validate: lowercase alphanumeric + hyphens, 1-63 chars, no leading/trailing hyphens
                    let valid = !name.is_empty()
                        && name.len() <= 63
                        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                        && !name.starts_with('-')
                        && !name.ends_with('-');

                    if !valid {
                        error_ref.set_text("Invalid hostname: use lowercase letters, numbers, hyphens (1-63 chars)");
                        error_ref.set_visible(true);
                        return;
                    }

                    b.set_sensitive(false);
                    error_ref.set_visible(false);

                    // Apply hostname change
                    let _ = std::process::Command::new("sudo")
                        .args(["hostnamectl", "set-hostname", &name])
                        .status();
                    let _ = std::process::Command::new("sudo")
                        .args(["systemctl", "restart", "avahi-daemon"])
                        .status();

                    // Update config
                    if let Ok(mut cfg) = aios_core::config::ConfigManager::new() {
                        let _ = cfg.set("system.machine_name", serde_json::json!(name));
                    }

                    chat_ref.add_message("system", &format!("Hostname changed to '{name}'. Reachable as {name}.local"));
                    let _ = std::fs::remove_file("/tmp/aios-name-conflict");
                });

                chat_view.add_setup_card(
                    "network-server-symbolic",
                    "Rename Your Machine",
                    &format!("Another machine is using '{conflicting}' on this network.\nChoose a different name:"),
                    Some(input_box.upcast_ref()),
                );
            }
        }

        // Create the setup conversation with config for pre-filling API keys.
        let setup = SetupConversation::new(chat_view.clone(), Some(config));

        // On completion: create vault, store secrets, transition to normal mode.
        let app_ref = app.clone();
        let rt_ref = rt.clone();
        let chat_view_ref = chat_view.clone();
        let prompt_ref = prompt_input.clone();
        let window_ref = window.clone();
        setup.on_complete(move |result| {
            info!(
                "First-boot setup complete: {} provider(s) configured",
                result.providers.len()
            );

            // Create the vault and store the API keys.
            let vault_path = ConfigManager::default_config_dir().join("vault.enc");
            let mut vault = Vault::new(vault_path);

            if let Err(e) = vault.create(&result.master_password) {
                warn!("Failed to create vault: {e}");
                // Continue anyway — the user can set keys later.
            } else {
                for provider in &result.providers {
                    let key_name = format!("{}_api_key", provider.name);
                    let label = match provider.name.as_str() {
                        "claude" => "Claude API Key",
                        "openai" => "OpenAI API Key",
                        other => other,
                    };
                    let entry = SecretEntry {
                        kind: SecretKind::ApiKey,
                        value: provider.api_key.clone(),
                        label: label.to_string(),
                        created: chrono::Utc::now(),
                        last_accessed: None,
                    };
                    if let Err(e) = vault.set(&key_name, entry) {
                        warn!("Failed to store {key_name} in vault: {e}");
                    }
                }
                info!("Vault created with {} secret(s)", result.providers.len());
            }

            // Store provider/key info in the config so the LLM manager
            // can pick them up immediately.
            if let Ok(mut config) = ConfigManager::new() {
                if let Some(primary) = result.providers.first() {
                    let _ = config.set(
                        "llm.provider",
                        serde_json::json!(primary.name),
                    );
                }
                for p in &result.providers {
                    match p.name.as_str() {
                        "claude" => {
                            let _ = config.set(
                                "llm.claude_api_key",
                                serde_json::json!(p.api_key),
                            );
                        }
                        "openai" => {
                            let _ = config.set(
                                "llm.openai_api_key",
                                serde_json::json!(p.api_key),
                            );
                        }
                        _ => {}
                    }
                }

                // Save assistant identity
                let _ = config.set("assistant.name", serde_json::json!(result.assistant_name));
                let _ = config.set("voice.wake_word", serde_json::json!(result.wake_word));
                let _ = config.set("system.machine_name", serde_json::json!(result.machine_name));
            }

            // Update display name
            crate::ui::chat_view::set_assistant_display_name(&result.assistant_name);

            // Update system hostname
            let _ = std::process::Command::new("sudo")
                .args(["hostnamectl", "set-hostname", &result.machine_name])
                .status();
            let _ = std::process::Command::new("sudo")
                .args(["systemctl", "restart", "avahi-daemon"])
                .status();

            // Transition to normal mode: initialize LLM, wire up the real
            // prompt handler, and show the ready message.
            Self::transition_to_normal_mode(
                &app_ref,
                rt_ref.clone(),
                &chat_view_ref,
                &prompt_ref,
                &window_ref,
            );
        });

        // During setup, the prompt input feeds into the setup conversation
        // as voice-like text input (for steps that accept it).
        let setup_ref = setup.clone();
        prompt_input.on_submit(move |text| {
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }
            if setup_ref.is_active() {
                setup_ref.on_voice_input(&text);
            }
        });

        // Start the conversation.
        setup.start();

        window.present();
    }

    /// After first-boot setup completes, configure the app for normal chat mode.
    ///
    /// This initializes the LLM manager, creates the shared application state,
    /// and re-wires the prompt input for real chat.
    fn transition_to_normal_mode(
        _app: &adw::Application,
        rt: tokio::runtime::Handle,
        chat_view: &ChatView,
        prompt_input: &PromptInput,
        window: &adw::ApplicationWindow,
    ) {
        // Load configuration (now includes the keys we just stored).
        let config = match ConfigManager::new() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to load config, using defaults: {e}");
                match ConfigManager::with_path(std::path::PathBuf::from("/tmp/.aios/config.json")) {
                    Ok(c) => c,
                    Err(e2) => {
                        error!("Fallback config also failed: {e2}");
                        panic!("Cannot initialize configuration from any path");
                    }
                }
            }
        };

        // Initialize tool registry.
        let mut tools = ToolRegistry::new();
        tools.load_builtins();
        info!("Loaded {} built-in tools", tools.len());

        // Initialize LLM providers.
        let mut llm = LlmManager::new();
        Self::init_llm(&config, &mut llm);

        // Create a UiPanelTool with the GTK panel renderer callback.
        let ui_panel_tool = Self::create_ui_panel_tool(window);

        // Wire tool executor into LLM manager.
        let tool_registry = Arc::new(std::sync::Mutex::new(ToolRegistry::new()));
        {
            let mut tr = tool_registry.lock().unwrap();
            tr.load_builtins();
            // Replace the callback-less builtin with the GTK-wired one.
            let _ = tr.unregister("ui_panel");
            let _ = tr.register(Box::new(UiPanelToolWrapper(ui_panel_tool.clone())));
        }
        let tr_for_executor = tool_registry.clone();
        llm.set_tool_executor(Arc::new(move |name, args, channel| {
            let registry = tr_for_executor.lock().unwrap();
            let result = registry.execute_on_channel(&name, args, &channel);
            if result.success {
                result.output
            } else {
                format!("Tool error: {}", result.output)
            }
        }));

        // Collect configured providers before config is moved into the state.
        let configured_providers: Vec<&str> = {
            let mut providers = Vec::new();
            if !config.get_str("llm.claude_api_key", "").is_empty() {
                providers.push("Claude");
            }
            let openai_key = config.get_str("llm.openai_api_key", "");
            if !openai_key.is_empty() && openai_key != "your-api-key-here" {
                providers.push("ChatGPT");
            }
            providers
        };

        // Create shared application state.
        let state = Rc::new(RefCell::new(AiosApp {
            config,
            llm: Arc::new(tokio::sync::Mutex::new(llm)),
            tools,
            conversation: Vec::new(),
            rt,
        }));

        // Show the transition message.
        chat_view.add_message(
            "system",
            "Setup complete. How can I help?",
        );

        // Show the settings button (hidden during setup).
        main_window::set_settings_button_visible(window, true);

        // Show the prompt input (hidden during setup).
        prompt_input.widget().set_visible(true);

        // Update the provider dropdown to show only configured providers.
        main_window::update_provider_dropdown(window, &configured_providers);

        // --- Connect signals ---

        // Provider dropdown changed.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        main_window::connect_provider_dropdown(window, move |provider_name| {
            let mut s = state_ref.borrow_mut();
            let name = match provider_name {
                "ChatGPT" | "chatgpt" => "openai".to_string(),
                other => other.to_lowercase(),
            };
            let _ = s.config.set("llm.provider", serde_json::json!(name));
            if let Ok(mut llm) = s.llm.try_lock() {
                if llm.set_active(&name).is_err() {
                    chat_view_ref
                        .add_message("system", &format!("Provider '{name}' not available"));
                } else {
                    chat_view_ref.add_message("system", &format!("Switched to {name}"));
                }
            }
        });

        // Settings button.
        let state_ref = state.clone();
        let win_ref = window.clone();
        main_window::connect_settings_button(window, move || {
            let s = state_ref.borrow();
            settings_dialog::show_settings(&win_ref, &s.config);
        });

        // Mic toggle.
        let state_ref = state.clone();
        main_window::connect_mic_toggle(window, move |active| {
            let mut s = state_ref.borrow_mut();
            let _ = s.config.set("voice.stt_enabled", serde_json::json!(active));
            info!("Mic toggled: {active}");
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

        // Re-wire message submission for normal chat mode.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        let prompt_ref = prompt_input.clone();
        prompt_input.on_submit(move |text| {
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }

            // Check if it's a slash command.
            if text.starts_with('/') {
                Self::handle_command(&state_ref, &chat_view_ref, &text);
                return;
            }

            // Display the user message immediately.
            chat_view_ref.add_message("user", &text);

            // Force immediate redraw before the async LLM call begins.
            while gtk4::glib::MainContext::default().iteration(false) {}

            // Send to LLM asynchronously.
            Self::send_to_llm(&state_ref, &chat_view_ref, &prompt_ref, text);
        });
    }

    /// Normal application startup — builds the UI and wires up signals.
    fn activate_main(app: &adw::Application, rt: tokio::runtime::Handle) {
        // Load configuration.
        let mut config = match ConfigManager::new() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to load config, using defaults: {e}");
                match ConfigManager::with_path(std::path::PathBuf::from("/tmp/.aios/config.json")) {
                    Ok(c) => c,
                    Err(e2) => {
                        error!("Fallback config also failed: {e2}");
                        panic!("Cannot initialize configuration from any path");
                    }
                }
            }
        };

        // Initialize tool registry with built-in tools.
        let mut tools = ToolRegistry::new();
        tools.load_builtins();
        info!("Loaded {} built-in tools", tools.len());

        // Initialize LLM providers.
        let mut llm = LlmManager::new();
        Self::init_llm(&config, &mut llm);

        // Build the UI first so we can wire the UiPanelTool callback.
        let chat_view = ChatView::new();
        let prompt_input = PromptInput::new();
        let channel_overlay = ChannelOverlay::new();

        // Build provider list — only show providers with API keys configured.
        let mut available_providers = Vec::new();
        if !config.get_str("llm.claude_api_key", "").is_empty() {
            available_providers.push("Claude");
        }
        if !config.get_str("llm.openai_api_key", "").is_empty() {
            let k = config.get_str("llm.openai_api_key", "");
            if k != "your-api-key-here" {
                available_providers.push("ChatGPT");
            }
        }
        let provider_refs: Vec<&str> = available_providers.iter().map(|s| *s).collect();

        let window = main_window::build_main_window(
            app,
            &chat_view,
            &prompt_input,
            &channel_overlay,
            &provider_refs,
        );

        // Normal boot — show settings button (it starts hidden for setup flow).
        main_window::set_settings_button_visible(&window, true);

        // Create a UiPanelTool with the GTK panel renderer callback.
        let ui_panel_tool = Self::create_ui_panel_tool(&window);

        // Wire tool executor into LLM manager.
        // The LLM executor gets its own registry that includes the
        // ui_panel tool with the GTK callback already set.
        let tool_registry = Arc::new(std::sync::Mutex::new(ToolRegistry::new()));
        {
            let mut tr = tool_registry.lock().unwrap();
            tr.load_builtins();
            // The builtin UiPanelTool has no callback — replace it with
            // the one that has the GTK callback wired in.
            let _ = tr.unregister("ui_panel");
            let _ = tr.register(Box::new(UiPanelToolWrapper(ui_panel_tool.clone())));
        }
        let tr_for_executor = tool_registry.clone();
        llm.set_tool_executor(Arc::new(move |name, args, channel| {
            let registry = tr_for_executor.lock().unwrap();
            let result = registry.execute_on_channel(&name, args, &channel);
            if result.success {
                result.output
            } else {
                format!("Tool error: {}", result.output)
            }
        }));

        // Read channel config (used by both boot status and channel startup).
        let web_enabled = config.get_bool("channels.web.enabled", true);
        let web_port = config.get_str("channels.web.port", "80");
        let signal_enabled = config.get_bool("channels.signal.enabled", false);
        let signal_phone = config.get_str("channels.signal.phone", "");

        // Build boot status text (used by GTK + Web).
        let boot_status_text = {
            use aios_core::types::{BootStatus, StatusLine};

            let mut status = BootStatus::new();

            // -- Channels --
            status.add(StatusLine::new("Desktop", true, "GTK4/libadwaita"));
            if web_enabled {
                status.add(StatusLine::new("Web Channel", true, format!("http://aios.local:{web_port}")));
            } else {
                status.add(StatusLine::new("Web Channel", false, "disabled (/channel web on)"));
            }
            if signal_enabled && !signal_phone.is_empty() {
                status.add(StatusLine::new("Signal", true, &signal_phone));
            } else {
                status.add(StatusLine::new("Signal", false, "disabled (/channel signal on)"));
            }

            // -- LLM Provider + Model --
            let provider = config.get_str("llm.provider", "claude");
            let claude_key = config.get_str("llm.claude_api_key", "");
            let openai_key = config.get_str("llm.openai_api_key", "");
            let has_claude = !claude_key.is_empty();
            let has_openai = !openai_key.is_empty() && openai_key != "your-api-key-here";
            let has_key = match provider.as_str() {
                "claude" => has_claude,
                "openai" => has_openai,
                _ => false,
            };
            let model = match provider.as_str() {
                "claude" => config.get_str("llm.claude_model", "claude-sonnet-4-20250514"),
                "openai" => config.get_str("llm.openai_model", "gpt-4o"),
                _ => provider.clone(),
            };
            if has_key {
                status.add(StatusLine::new("LLM Provider", true, format!("{provider} ({model})")));
            } else {
                status.add(StatusLine::new("LLM Provider", false, format!("{provider} — **no API key** (use /key)")));
            }

            // Show backup provider if available
            if has_claude && provider != "claude" {
                status.add(StatusLine::new("Backup", true, "Claude available"));
            }
            if has_openai && provider != "openai" {
                status.add(StatusLine::new("Backup", true, "ChatGPT available"));
            }

            // -- Voice --
            let stt = config.get_bool("voice.stt_enabled", true);
            let tts = config.get_bool("voice.tts_enabled", true);
            let tts_voice = config.get_str("voice.tts_voice", "en_US-amy-medium");
            let has_piper = std::path::Path::new("/usr/bin/piper").exists();
            let has_whisper = std::path::Path::new("/usr/bin/whisper-cpp-cli").exists();
            let tts_backend = if has_piper { "Piper" } else { "espeak-ng" };
            let stt_backend = if has_whisper { "Whisper" } else { "not installed" };
            status.add(StatusLine::new("Audio Output (TTS)", tts,
                format!("{tts_backend} — voice: {tts_voice}")));
            status.add(StatusLine::new("Audio Input (STT)", stt && has_whisper,
                format!("{stt_backend}{}", if !stt { " — disabled" } else { "" })));

            // -- System --
            let kb_layout = config.get_str("system.keyboard_layout", "us");
            let timezone = std::fs::read_to_string("/etc/timezone")
                .unwrap_or_else(|_| "UTC".into())
                .trim().to_string();
            let boot_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            status.add(StatusLine::new("Keyboard", true, kb_layout));
            status.add(StatusLine::new("Timezone", true, &timezone));
            status.add(StatusLine::new("Boot time", true, boot_time));

            status.format()
        };

        // Show boot status on Desktop.
        chat_view.add_level_message(aios_core::types::MessageLevel::Info, &boot_status_text);

        // Apply theme from config at startup.
        {
            let theme = config.get_str("ui.theme", "dark");
            Self::apply_theme(&theme);
        }

        // Check for hostname collision (set by aios-hostname-check.service at boot).
        if std::path::Path::new("/tmp/aios-name-conflict").exists() {
            if let Ok(conflicting) = std::fs::read_to_string("/tmp/aios-name-conflict") {
                let conflicting = conflicting.trim().to_string();
                let random_suffix = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() % 10000) as u16;
                let suggestion = format!("{conflicting}-{random_suffix}");

                chat_view.add_level_message(
                    aios_core::types::MessageLevel::Warning,
                    &format!("Hostname conflict: another machine on this network is already using '{conflicting}.local'"),
                );

                // Show rename card
                let input_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
                input_box.set_margin_top(8);

                let entry = gtk4::Entry::builder()
                    .placeholder_text("New hostname")
                    .text(&suggestion)
                    .hexpand(true)
                    .build();
                input_box.append(&entry);

                let hint = gtk4::Label::new(Some("Lowercase letters, numbers, and hyphens only. Will be reachable as <name>.local"));
                hint.add_css_class("dim-label");
                hint.set_halign(gtk4::Align::Start);
                hint.set_wrap(true);
                input_box.append(&hint);

                let error_label = gtk4::Label::new(None);
                error_label.add_css_class("error");
                error_label.set_visible(false);
                error_label.set_halign(gtk4::Align::Start);
                input_box.append(&error_label);

                let apply_btn = gtk4::Button::with_label("Apply");
                apply_btn.add_css_class("suggested-action");
                apply_btn.set_halign(gtk4::Align::Start);
                apply_btn.set_margin_top(4);
                input_box.append(&apply_btn);

                let entry_ref = entry.clone();
                let error_ref = error_label.clone();
                let chat_ref = chat_view.clone();
                apply_btn.connect_clicked(move |b| {
                    let name = entry_ref.text().to_string().trim().to_lowercase();

                    // Validate: lowercase alphanumeric + hyphens, 1-63 chars, no leading/trailing hyphens
                    let valid = !name.is_empty()
                        && name.len() <= 63
                        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                        && !name.starts_with('-')
                        && !name.ends_with('-');

                    if !valid {
                        error_ref.set_text("Invalid hostname: use lowercase letters, numbers, hyphens (1-63 chars)");
                        error_ref.set_visible(true);
                        return;
                    }

                    b.set_sensitive(false);
                    error_ref.set_visible(false);

                    // Apply hostname change
                    let _ = std::process::Command::new("sudo")
                        .args(["hostnamectl", "set-hostname", &name])
                        .status();
                    let _ = std::process::Command::new("sudo")
                        .args(["systemctl", "restart", "avahi-daemon"])
                        .status();

                    // Update config
                    if let Ok(mut cfg) = aios_core::config::ConfigManager::new() {
                        let _ = cfg.set("system.machine_name", serde_json::json!(name));
                    }

                    chat_ref.add_message("system", &format!("Hostname changed to '{name}'. Reachable as {name}.local"));
                    let _ = std::fs::remove_file("/tmp/aios-name-conflict");
                });

                chat_view.add_setup_card(
                    "network-server-symbolic",
                    "Rename Your Machine",
                    &format!("Another machine is using '{conflicting}' on this network.\nChoose a different name:"),
                    Some(input_box.upcast_ref()),
                );
            }
        }

        chat_view.add_message("system", "Type a message or use /help to see available commands.");

        // Apply assistant display name from config.
        let assistant_name = config.get_str("assistant.name", "Assistant");
        crate::ui::chat_view::set_assistant_display_name(&assistant_name);

        // --- Channel infrastructure ---

        // Create the shared AppRuntime for multi-channel orchestration.
        let runtime = aios_core::channel::AppRuntime::new();

        // Register Desktop channel (always available).
        runtime.switcher.register_channel(
            aios_core::channel::ChannelKind::Desktop,
            aios_core::channel::ChannelContext::desktop(),
        );

        // Start Web server if enabled.
        let web_server = Self::start_web_server(
            &mut config,
            &runtime,
            Some(boot_status_text.clone()),
        );

        // Start Signal listener if enabled.
        let signal_sender = if signal_enabled && !signal_phone.is_empty() {
            let contacts_str = config.get_str("channels.signal.allowed_contacts", "");
            let allowed: Vec<String> = contacts_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let listener = aios_signal::SignalListener::new(
                signal_phone.clone(),
                allowed,
            );
            let sig_tx = runtime.message_sender();
            listener.start(sig_tx);
            runtime.switcher.register_channel(
                aios_core::channel::ChannelKind::Signal,
                aios_core::channel::ChannelContext::signal(),
            );
            info!("Signal channel started for {signal_phone}");
            Some(Arc::new(aios_signal::SignalSender::new(signal_phone)))
        } else {
            None
        };

        // Wire channel switcher to the overlay (item 5).
        // GTK widgets aren't Send, so we use a std::sync::mpsc channel to
        // bridge from the switcher callback (any thread) to the GTK thread.
        {
            let (overlay_tx, overlay_rx) =
                std::sync::mpsc::channel::<aios_core::channel::ChannelKind>();

            runtime.switcher.on_switch(Arc::new(move |_old, new| {
                let _ = overlay_tx.send(new);
            }));

            // GTK-side: poll the channel and update the overlay.
            let overlay_for_poll = channel_overlay.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                while let Ok(new_kind) = overlay_rx.try_recv() {
                    if new_kind == aios_core::channel::ChannelKind::Desktop {
                        overlay_for_poll.hide();
                    } else {
                        overlay_for_poll.show(new_kind);
                    }
                }
                glib::ControlFlow::Continue
            });

            // "Switch back here" button → switch to Desktop.
            let switcher_for_btn = runtime.switcher.clone();
            channel_overlay.on_switch_back(move || {
                let _ = switcher_for_btn.switch_to(aios_core::channel::ChannelKind::Desktop);
            });
        }

        // Warn if no API key is configured.
        {
            let provider = config.get_str("llm.provider", "claude");
            let has_key = match provider.as_str() {
                "claude" => !config.get_str("llm.claude_api_key", "").is_empty(),
                "openai" => !config.get_str("llm.openai_api_key", "").is_empty(),
                _ => false,
            };
            if !has_key {
                chat_view.add_level_message(
                    aios_core::types::MessageLevel::Warning,
                    "No API key configured. Use **/key claude sk-ant-...** or **/key openai sk-...** to set one.",
                );
            }
        }

        // Create shared application state.
        let llm_arc = Arc::new(tokio::sync::Mutex::new(llm));
        let state = Rc::new(RefCell::new(AiosApp {
            config,
            llm: llm_arc.clone(),
            tools,
            conversation: Vec::new(),
            rt: rt.clone(),
        }));

        // --- Unified message loop (item 3) ---
        // Poll incoming messages from ALL channels (Web, Signal, Desktop)
        // and route them through the LLM.
        {
            let msg_rx_holder = runtime.clone();
            let state_for_loop = state.clone();
            let chat_for_loop = chat_view.clone();
            let switcher_for_loop = runtime.switcher.clone();
            let llm_for_loop = llm_arc.clone();
            let rt_for_loop = rt.clone();
            let web_tx = web_server.clone();
            let sig_sender = signal_sender.clone();

            // We take the receiver on the Tokio side and poll it from GTK.
            let (bridge_tx, bridge_rx) =
                std::sync::mpsc::channel::<aios_core::channel::IncomingMessage>();

            // Tokio task: drain AppRuntime's message channel into the std bridge.
            rt.spawn(async move {
                if let Some(mut rx) = msg_rx_holder.take_message_rx().await {
                    while let Some(msg) = rx.recv().await {
                        if bridge_tx.send(msg).is_err() {
                            break;
                        }
                    }
                }
            });

            // GTK poll: check for incoming messages from remote channels.
            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                while let Ok(msg) = bridge_rx.try_recv() {
                    // Switch channel if needed.
                    if switcher_for_loop.active_kind() != msg.channel {
                        let _ = switcher_for_loop.switch_to(msg.channel);
                    }

                    // Always show in Desktop chat (conversation continuity).
                    chat_for_loop.add_message("user", &msg.text);

                    // Handle slash commands from remote channels (don't add to LLM history).
                    if msg.text.starts_with('/') {
                        Self::handle_command(&state_for_loop, &chat_for_loop, &msg.text);
                        continue;
                    }

                    // Record in conversation history (after slash command check).
                    {
                        let mut s = state_for_loop.borrow_mut();
                        s.conversation.push(aios_core::types::Message::user(&msg.text));
                    }

                    // Send to LLM and route response to active channel (item 4).
                    let llm = llm_for_loop.clone();
                    let history = state_for_loop.borrow().conversation.clone();
                    let text = msg.text.clone();
                    let sender_id = msg.sender_id.clone();
                    let active_channel = msg.channel;
                    let (resp_tx, resp_rx) = std::sync::mpsc::channel::<LlmResult>();
                    let web_tx_inner = web_tx.clone();
                    let sig_inner = sig_sender.clone();

                    rt_for_loop.spawn(async move {
                        let mut llm_guard = llm.lock().await;
                        let mut history = history;
                        if history.last().is_some_and(|m| m.role == aios_core::types::Role::User) {
                            history.pop();
                        }
                        let result = llm_guard.chat(&text, Some(&mut history), &[], None).await;
                        drop(llm_guard);

                        match result {
                            Ok(response) => {
                                let content = response.content.unwrap_or_else(|| "I received your message but have no response text.".to_string());
                                // Route to active channel.
                                match active_channel {
                                    aios_core::channel::ChannelKind::Web => {
                                        if let Some(ref tx) = web_tx_inner {
                                            let msg = aios_web::protocol::ServerMessage::Message {
                                                role: "assistant".into(),
                                                content: content.clone(),
                                                level: None,
                                            };
                                            if let Ok(json) = serde_json::to_string(&msg) {
                                                let _ = tx.send(json);
                                            }
                                        }
                                    }
                                    aios_core::channel::ChannelKind::Signal => {
                                        if let Some(ref sender) = sig_inner {
                                            if let Some(ref recipient) = sender_id {
                                                let _ = sender.send_text(recipient, &content).await;
                                            }
                                        }
                                    }
                                    _ => {} // Desktop is handled below via resp_tx.
                                }
                                let _ = resp_tx.send(LlmResult::Success {
                                    content,
                                    updated_history: history,
                                });
                            }
                            Err(e) => {
                                let _ = resp_tx.send(LlmResult::Error(format!("{e}")));
                            }
                        }
                    });

                    // Poll for this response too (displays on Desktop).
                    let chat_for_resp = chat_for_loop.clone();
                    let state_for_resp = state_for_loop.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                        match resp_rx.try_recv() {
                            Ok(LlmResult::Success { content, updated_history }) => {
                                chat_for_resp.add_message("assistant", &content);
                                speak_if_enabled(&content, &state_for_resp.borrow().config);
                                let mut s = state_for_resp.borrow_mut();
                                s.conversation = updated_history;
                                s.conversation.push(aios_core::types::Message::assistant(&content));
                                glib::ControlFlow::Break
                            }
                            Ok(LlmResult::Error(err)) => {
                                chat_for_resp.add_message("system", &format!("Error: {err}"));
                                glib::ControlFlow::Break
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                        }
                    });
                }
                glib::ControlFlow::Continue
            });
        }

        // --- Connect GTK signals ---

        // Provider dropdown changed.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        main_window::connect_provider_dropdown(&window, move |provider_name| {
            let mut s = state_ref.borrow_mut();
            let name = match provider_name {
                "ChatGPT" | "chatgpt" => "openai".to_string(),
                other => other.to_lowercase(),
            };
            let _ = s.config.set("llm.provider", serde_json::json!(name));
            if let Ok(mut llm) = s.llm.try_lock() {
                if llm.set_active(&name).is_err() {
                    chat_view_ref
                        .add_message("system", &format!("Provider '{name}' not available"));
                } else {
                    chat_view_ref.add_message("system", &format!("Switched to {name}"));
                }
            }
        });

        // Settings button.
        let state_ref = state.clone();
        let win_ref = window.clone();
        main_window::connect_settings_button(&window, move || {
            let s = state_ref.borrow();
            settings_dialog::show_settings(&win_ref, &s.config);
        });

        // Mic toggle + voice listener.
        let stt_enabled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            state.borrow().config.get_bool("voice.stt_enabled", true),
        ));
        let stt_flag = stt_enabled.clone();
        let state_ref = state.clone();
        main_window::connect_mic_toggle(&window, move |active| {
            let mut s = state_ref.borrow_mut();
            let _ = s.config.set("voice.stt_enabled", serde_json::json!(active));
            stt_flag.store(active, std::sync::atomic::Ordering::Relaxed);
            info!("Mic toggled: {active}");
        });

        // Start voice listener thread — receives transcribed text via mpsc channel.
        let (stt_tx, stt_rx) = std::sync::mpsc::channel::<String>();
        let _voice_handle = start_voice_listener(stt_tx, stt_enabled);

        // Poll for transcribed text from the voice listener (GTK main thread).
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        let prompt_ref = prompt_input.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            while let Ok(text) = stt_rx.try_recv() {
                let text = text.trim().to_string();
                if text.is_empty() {
                    continue;
                }

                info!("STT transcription received: {}", &text[..text.len().min(50)]);
                chat_view_ref.add_message("user", &format!("\u{1f3a4} {text}"));

                // Send to LLM
                Self::send_to_llm(&state_ref, &chat_view_ref, &prompt_ref, text);
            }
            glib::ControlFlow::Continue
        });

        // Speaker toggle.
        let state_ref = state.clone();
        main_window::connect_speaker_toggle(&window, move |active| {
            let mut s = state_ref.borrow_mut();
            let _ = s.config.set("voice.tts_enabled", serde_json::json!(active));
            if !active {
                stop_tts();
            }
            info!("Speaker toggled: {active}");
        });

        // Message submission (Enter key or send button) — Desktop channel.
        let state_ref = state.clone();
        let chat_view_ref = chat_view.clone();
        let prompt_ref = prompt_input.clone();
        prompt_input.on_submit(move |text| {
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }

            // Check if it's a slash command.
            if text.starts_with('/') {
                Self::handle_command(&state_ref, &chat_view_ref, &text);
                return;
            }

            // Display the user message immediately.
            chat_view_ref.add_message("user", &text);

            // Force immediate redraw before the async LLM call begins.
            while gtk4::glib::MainContext::default().iteration(false) {}

            // Send to LLM asynchronously.
            Self::send_to_llm(&state_ref, &chat_view_ref, &prompt_ref, text);
        });

        window.present();
    }

    /// Start the web server if enabled in config.
    ///
    /// Reads web-channel settings from `config`, generates or loads an auth
    /// token, creates a [`aios_web::server::WebServer`], starts it, and
    /// registers the Web channel on the given `runtime`.
    ///
    /// Returns `Some(broadcast::Sender)` for routing AI responses to web
    /// clients, or `None` if the web channel is disabled.
    fn start_web_server(
        config: &mut ConfigManager,
        runtime: &aios_core::channel::AppRuntime,
        welcome_message: Option<String>,
    ) -> Option<tokio::sync::broadcast::Sender<String>> {
        let web_enabled = config.get_bool("channels.web.enabled", true);
        if !web_enabled {
            return None;
        }

        let web_port = config.get_str("channels.web.port", "80");
        let port: u16 = web_port.parse().unwrap_or(80);
        let web_tx = runtime.message_sender();

        // Generate or load auth token for web access.
        let web_token = config.get_str("channels.web.token", "");
        let web_token = if web_token.is_empty() {
            let token = uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_string();
            let _ = config.set("channels.web.token", serde_json::json!(token));
            info!("Generated web auth token: {token}");
            Some(token)
        } else {
            Some(web_token)
        };

        let server = aios_web::server::WebServer::new(
            port,
            web_tx,
            web_token,
            Some(runtime.switcher.clone()),
            welcome_message,
        );
        let response_tx = server.response_tx.clone();
        server.start();
        runtime.switcher.register_channel(
            aios_core::channel::ChannelKind::Web,
            aios_core::channel::ChannelContext::web(),
        );
        info!("Web channel started on port {port}");
        Some(response_tx)
    }

    /// Initialize LLM providers from config.
    fn init_llm(config: &ConfigManager, llm: &mut LlmManager) {
        let claude_key = config.get_str("llm.claude_api_key", "");
        let claude_model = config.get_str("llm.claude_model", "claude-sonnet-4-20250514");
        llm.register_provider(Box::new(ClaudeProvider::new(
            claude_key,
            Some(claude_model),
            None,
        )));

        let openai_key = config.get_str("llm.openai_api_key", "");
        let openai_model = config.get_str("llm.openai_model", "gpt-4o");
        llm.register_provider(Box::new(OpenAIProvider::new(
            openai_key,
            Some(openai_model),
            None,
        )));

        let active = config.get_str("llm.provider", "claude");
        if let Err(e) = llm.set_active(&active) {
            warn!("Failed to set active provider to '{active}': {e}");
        }
        info!("LLM providers initialized, active: {active}");
    }

    /// Create a [`UiPanelTool`] with its callback wired to the GTK
    /// [`PanelRenderer`].
    ///
    /// The returned tool can be used from any thread.  When `execute()` is
    /// called, it marshals the panel display to the GTK main thread via
    /// `glib::idle_add_local_once` and blocks until the user responds.
    ///
    /// GTK widgets are not `Send`/`Sync`, but the tool callback must be.
    /// We use a `std::sync::mpsc` channel protected by a mutex to route
    /// panel requests from the worker thread to the GTK main thread:
    ///
    /// 1. Worker thread sends `(PanelRequest, reply_tx)` into a shared queue.
    /// 2. A periodic `glib::timeout_add_local` polls the queue on the GTK
    ///    main thread and calls `PanelRenderer::build_and_show_panel`.
    /// 3. Worker thread blocks on `reply_rx.recv()`.
    fn create_ui_panel_tool(parent_window: &adw::ApplicationWindow) -> Arc<UiPanelTool> {
        use aios_tools::builtin::ui_panel::{PanelRequest, PanelResponse};

        type PanelMsg = (
            PanelRequest,
            std::sync::mpsc::Sender<Option<PanelResponse>>,
        );

        // Shared queue: worker pushes, GTK main thread pops.
        let queue: Arc<std::sync::Mutex<Vec<PanelMsg>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        // Poll the queue from the GTK main thread.
        let queue_for_gtk = queue.clone();
        let window: adw::ApplicationWindow = parent_window.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            let msg = {
                let mut q = queue_for_gtk.lock().unwrap();
                if q.is_empty() {
                    None
                } else {
                    Some(q.remove(0))
                }
            };
            if let Some((request, reply_tx)) = msg {
                let parent: gtk4::Window = window.clone().upcast();
                PanelRenderer::build_and_show_panel(request, &parent, reply_tx);
            }
            glib::ControlFlow::Continue
        });

        // The tool callback: Send-safe, captures only the Arc<Mutex<Vec>>.
        let queue_for_tool = queue.clone();
        let tool = Arc::new(UiPanelTool::new());
        tool.set_panel_callback(move |request, _channel| {
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            {
                let mut q = queue_for_tool.lock().unwrap();
                q.push((request, reply_tx));
            }
            // Block the worker thread until the GTK main thread responds.
            reply_rx.recv().ok().flatten()
        });

        tool
    }

    /// Handle a slash command.
    fn handle_command(
        state: &Rc<RefCell<AiosApp>>,
        chat_view: &ChatView,
        input: &str,
    ) {
        let mut s = state.borrow_mut();
        let mut handler = CommandHandler::new(&mut s.config);
        let result = handler.execute(input);

        match result {
            CommandResult::Response(text) => {
                chat_view.add_message("system", &text);
            }
            CommandResult::Clear => {
                chat_view.clear();
                s.conversation.clear();
                chat_view.add_message("system", "Chat history cleared.");
            }
            CommandResult::Configure => {
                chat_view.add_message(
                    "system",
                    "Use the settings button (gear icon) to configure AiOS.",
                );
            }
            CommandResult::SysInfo => {
                drop(s);
                let info = aios_core::system_monitor::SystemInfo::gather();
                chat_view.add_level_message(
                    aios_core::types::MessageLevel::Info,
                    &info.format_text(),
                );
                return;
            }
            CommandResult::ClosePanel => {
                drop(s);
                let closed = Self::close_topmost_dialog();
                if closed {
                    chat_view.add_message("system", "Panel closed.");
                } else {
                    chat_view.add_message("system", "No open panel or dialog to close.");
                }
                return;
            }
            CommandResult::SelfTest(filter) => {
                drop(s);
                Self::run_selftest(state, chat_view, &filter);
                return;
            }
            CommandResult::Update(url) => {
                let url = url.trim().to_string();
                if url.is_empty() {
                    chat_view.add_message("system",
                        "Usage: /update <url>\n\
                         Example: /update https://example.com/aios\n\n\
                         Or from your dev machine:\n\
                         ./deploy.sh aios.local");
                } else {
                    chat_view.add_level_message(
                        aios_core::types::MessageLevel::Warning,
                        &format!("Updating AiOS from: **{url}**\nThis will restart the app..."),
                    );
                    // Run aios-update in the background.
                    let rt = s.rt.clone();
                    drop(s);
                    rt.spawn(async move {
                        let output = tokio::process::Command::new("aios-update")
                            .arg(&url)
                            .output()
                            .await;
                        match output {
                            Ok(o) => {
                                let stdout = String::from_utf8_lossy(&o.stdout);
                                let stderr = String::from_utf8_lossy(&o.stderr);
                                tracing::info!("aios-update: {stdout}{stderr}");
                            }
                            Err(e) => {
                                tracing::error!("aios-update failed: {e}");
                            }
                        }
                    });
                    return;
                }
            }
            CommandResult::Panel { title, description, fields, config_key } => {
                // Render the panel as an interactive card in the chat view.
                let input_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
                input_box.set_margin_top(8);

                if !description.is_empty() {
                    let desc = gtk4::Label::new(Some(&description));
                    desc.set_halign(gtk4::Align::Start);
                    desc.set_opacity(0.7);
                    input_box.append(&desc);
                }

                for field in &fields {
                    match &field.kind {
                        PanelFieldKind::Dropdown { options, selected } => {
                            let label = gtk4::Label::new(Some(&field.label));
                            label.set_halign(gtk4::Align::Start);
                            label.add_css_class("heading");
                            input_box.append(&label);

                            let string_list = gtk4::StringList::new(
                                &options.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                            );
                            let dropdown = gtk4::DropDown::new(
                                Some(string_list),
                                gtk4::Expression::NONE,
                            );

                            // Set the currently selected value.
                            if let Some(sel) = selected {
                                if let Some(idx) = options.iter().position(|o| o == sel) {
                                    dropdown.set_selected(idx as u32);
                                }
                            }
                            input_box.append(&dropdown);

                            // Apply button.
                            let apply_btn = gtk4::Button::with_label("Apply");
                            apply_btn.add_css_class("suggested-action");
                            apply_btn.set_halign(gtk4::Align::Start);
                            apply_btn.set_margin_top(4);

                            let options_clone = options.clone();
                            let config_key_clone = config_key.clone();
                            let state_for_panel = state.clone();
                            let chat_for_panel = chat_view.clone();
                            let dd_ref = dropdown.clone();
                            let field_id = field.id.clone();
                            apply_btn.connect_clicked(move |b| {
                                b.set_sensitive(false);
                                let idx = dd_ref.selected() as usize;
                                if let Some(value) = options_clone.get(idx) {
                                    let mut s = state_for_panel.borrow_mut();
                                    let _ = s.config.set(
                                        &config_key_clone,
                                        serde_json::json!(value),
                                    );

                                    // Apply theme change immediately via libadwaita.
                                    if config_key_clone == "ui.theme" {
                                        Self::apply_theme(value);
                                    }

                                    // Apply resolution change.
                                    if field_id == "resolution" {
                                        aios_core::config::commands::CommandHandler::apply_resolution(value);
                                    }

                                    chat_for_panel.add_message(
                                        "system",
                                        &format!("Set to: {value}"),
                                    );
                                }
                            });
                            input_box.append(&apply_btn);
                        }
                    }
                }

                drop(s);
                chat_view.add_setup_card(
                    "preferences-system-symbolic",
                    &title,
                    "",
                    Some(input_box.upcast_ref()),
                );
                return;
            }
            CommandResult::Unknown(cmd) => {
                chat_view.add_message(
                    "system",
                    &format!("Unknown command: {cmd}\nType /help for available commands."),
                );
            }
        }

        // Re-apply any provider/key changes to the LLM manager.
        let provider = s.config.get_str("llm.provider", "claude");
        if let Ok(mut llm) = s.llm.try_lock() {
            let _ = llm.set_active(&provider);

            let claude_key = s.config.get_str("llm.claude_api_key", "");
            if !claude_key.is_empty() {
                let _ = llm.set_api_key("claude", claude_key);
            }
            let openai_key = s.config.get_str("llm.openai_api_key", "");
            if !openai_key.is_empty() {
                let _ = llm.set_api_key("openai", openai_key);
            }
        }

        // Re-apply theme from config (handles /theme dark, /theme light, etc.).
        let theme = s.config.get_str("ui.theme", "dark");
        Self::apply_theme(&theme);
    }

    /// Run the self-test suite on the current channel.
    fn run_selftest(
        _state: &Rc<RefCell<AiosApp>>,
        chat_view: &ChatView,
        filter: &str,
    ) {
        use aios_core::selftest::SelfTestRunner;
        use aios_core::selftest::runner::TestContext;

        let chat = chat_view.clone();
        chat.add_message("system", "Starting AiOS self-test...");

        let runner = SelfTestRunner::new();

        // Build context factory — each test gets a fresh context.
        let chat_for_ctx = chat_view.clone();
        let ctx_factory = move || -> TestContext {
            let c = chat_for_ctx.clone();
            TestContext {
                display: Box::new(move |role, content| {
                    c.add_message(role, content);
                }),
                // Panel support: not wired yet (would need the UiPanelTool callback).
                // For now, interactive tests will show "skipped".
                show_panel: None,
                channel_kind: aios_core::channel::ChannelKind::Desktop,
            }
        };

        let results = match filter.trim() {
            "" => runner.run_all(ctx_factory),
            "quick" => runner.run_quick(ctx_factory),
            tag => runner.run_tagged(tag, ctx_factory),
        };

        let report = SelfTestRunner::format_report(&results);
        chat.add_message("system", &report);
    }

    /// Close the topmost modal/transient dialog window.
    ///
    /// Iterates all windows registered with the GTK application and looks
    /// for visible windows that are not the main `ApplicationWindow`.
    /// Closes the last one found (topmost) and returns `true` if a window
    /// was closed.
    fn close_topmost_dialog() -> bool {
        // Get the running GtkApplication via gio::Application::default().
        let gio_app = match gtk4::gio::Application::default() {
            Some(a) => a,
            None => return false,
        };
        let gtk_app = match gio_app.downcast::<gtk4::Application>() {
            Ok(a) => a,
            Err(_) => return false,
        };

        // Iterate windows registered with the application.
        // The list is ordered; the last matching window is the topmost.
        let mut candidate: Option<gtk4::Window> = None;

        for win in gtk_app.windows() {
            // Skip the main application window.
            if win.downcast_ref::<adw::ApplicationWindow>().is_some() {
                continue;
            }
            if win.is_visible() {
                candidate = Some(win);
            }
        }

        if let Some(win) = candidate {
            win.close();
            true
        } else {
            false
        }
    }

    /// Apply a theme setting via libadwaita's StyleManager.
    fn apply_theme(theme: &str) {
        let style_manager = adw::StyleManager::default();
        match theme {
            "dark" => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
            "light" => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
            "auto" | "system" => style_manager.set_color_scheme(adw::ColorScheme::Default),
            _ => {
                warn!("Unknown theme: {theme}, defaulting to dark");
                style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
            }
        }
        info!("Theme applied: {theme}");
    }

    /// Send a user message to the LLM on the Tokio runtime.
    fn send_to_llm(
        state: &Rc<RefCell<AiosApp>>,
        chat_view: &ChatView,
        _prompt: &PromptInput,
        text: String,
    ) {
        // Stop any active TTS when the user sends a new message
        stop_tts();

        let s = state.borrow();
        let llm = s.llm.clone();
        let rt = s.rt.clone();

        // Record user message in conversation history.
        drop(s);
        state
            .borrow_mut()
            .conversation
            .push(aios_core::types::Message::user(&text));

        // Use mpsc channel: tokio task sends result, GTK polls via idle_add.
        let (tx, rx) = std::sync::mpsc::channel::<LlmResult>();
        let history = state.borrow().conversation.clone();

        // Spawn LLM call on Tokio (no GTK types captured — all Send-safe).
        rt.spawn(async move {
            let mut llm_guard = llm.lock().await;
            let mut history = history;

            if history
                .last()
                .is_some_and(|m| m.role == aios_core::types::Role::User)
            {
                history.pop();
            }

            let result = llm_guard
                .chat(&text, Some(&mut history), &[], None)
                .await;
            drop(llm_guard);

            match result {
                Ok(response) => {
                    let content = response.content.unwrap_or_else(|| {
                        "I received your message but have no response text.".to_string()
                    });
                    let _ = tx.send(LlmResult::Success {
                        content,
                        updated_history: history,
                    });
                }
                Err(e) => {
                    let _ = tx.send(LlmResult::Error(format!("{e}")));
                }
            }
        });

        // Poll the channel from the GTK main thread.
        let chat_view_ref = chat_view.clone();
        let state_ref = state.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            match rx.try_recv() {
                Ok(LlmResult::Success { content, updated_history }) => {
                    chat_view_ref.add_message("assistant", &content);
                    speak_if_enabled(&content, &state_ref.borrow().config);
                    let mut s = state_ref.borrow_mut();
                    s.conversation = updated_history;
                    s.conversation
                        .push(aios_core::types::Message::assistant(&content));
                    glib::ControlFlow::Break
                }
                Ok(LlmResult::Error(err)) => {
                    chat_view_ref.add_message("system", &format!("Error: {err}"));
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    }
}

/// Internal result type for async LLM communication.
enum LlmResult {
    Success {
        content: String,
        updated_history: Vec<aios_core::types::Message>,
    },
    Error(String),
}

// ---------------------------------------------------------------------------
// UiPanelToolWrapper
// ---------------------------------------------------------------------------

/// Wraps an `Arc<UiPanelTool>` as a `Tool` so it can be registered in the
/// [`ToolRegistry`].
///
/// This allows us to register a pre-configured `UiPanelTool` (with the GTK
/// panel callback already set) into the registry used by the LLM executor.
struct UiPanelToolWrapper(Arc<UiPanelTool>);

impl aios_tools::Tool for UiPanelToolWrapper {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn description(&self) -> &str {
        self.0.description()
    }

    fn parameters(&self) -> serde_json::Value {
        self.0.parameters()
    }

    fn execute(&self, args: serde_json::Value) -> aios_core::types::ToolResult {
        self.0.execute(args)
    }

    fn execute_on_channel(
        &self,
        args: serde_json::Value,
        channel: &aios_core::channel::ChannelContext,
    ) -> aios_core::types::ToolResult {
        self.0.execute_on_channel(args, channel)
    }
}
