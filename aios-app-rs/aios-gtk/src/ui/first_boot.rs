//! Conversational first-boot setup flow.
//!
//! Instead of a separate wizard dialog, [`SetupConversation`] drives a
//! state machine through the main [`ChatView`], presenting styled "setup
//! cards" with inline input widgets. The user progresses by clicking
//! buttons or (for future voice support) by speaking responses.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};
use tracing::info;

use aios_core::config::ConfigManager;
use aios_core::i18n::{t, t_fmt};

use super::chat_view::ChatView;

// ---------------------------------------------------------------------------
// Public result types (unchanged — used by app.rs)
// ---------------------------------------------------------------------------

/// Configuration for a single LLM provider collected during setup.
#[derive(Debug, Clone)]
pub struct ProviderSetup {
    /// Provider identifier: `"claude"` or `"openai"`.
    pub name: String,
    /// The API key entered by the user.
    pub api_key: String,
}

/// The complete result of a successful first-boot setup.
#[derive(Debug, Clone)]
pub struct SetupResult {
    /// Configured providers, ordered by preference (first = primary).
    pub providers: Vec<ProviderSetup>,
    /// The master password chosen by the user.
    pub master_password: String,
    /// The display name for the AI assistant.
    pub assistant_name: String,
    /// The wake word used to activate the assistant by voice.
    pub wake_word: String,
    /// The network hostname (reachable as `<name>.local`).
    pub machine_name: String,
}

// ---------------------------------------------------------------------------
// Setup steps
// ---------------------------------------------------------------------------

/// Identifies the current step in the conversational setup flow.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SetupStep {
    Welcome,
    TestAudioOutput,
    TestAudioInput,
    NameAssistant,
    ChooseProvider,
    EnterApiKey { provider: String },
    CreatePassword,
    ConfirmPassword,
    AddBackup,
    EnterBackupKey { provider: String },
    ProviderOrder,
    Complete,
}

// ---------------------------------------------------------------------------
// Internal mutable state
// ---------------------------------------------------------------------------

struct SetupState {
    step: SetupStep,
    /// Monotonic counter bumped on every step change — used to detect stale callbacks.
    step_version: u32,
    /// Providers configured so far.
    providers: Vec<ProviderSetup>,
    /// The master password (set during CreatePassword).
    master_password: String,
    /// Temporarily holds the password from CreatePassword before confirmation.
    pending_password: String,
    /// The primary provider chosen in ChooseProvider.
    primary_provider: String,
    /// The display name for the AI assistant (shown in chat).
    assistant_name: String,
    /// Whether to use the same name for wake word and hostname.
    use_same_name: bool,
    /// Custom wake word (if use_same_name is false).
    wake_word_custom: String,
    /// Custom machine/hostname (if use_same_name is false).
    machine_name_custom: String,
}

impl Default for SetupState {
    fn default() -> Self {
        Self {
            step: SetupStep::Welcome,
            step_version: 0,
            providers: Vec::new(),
            master_password: String::new(),
            pending_password: String::new(),
            primary_provider: String::new(),
            assistant_name: t("setup.name.default"),
            use_same_name: true,
            wake_word_custom: t("setup.name.default"),
            machine_name_custom: t("setup.name.default_lowercase"),
        }
    }
}

// ---------------------------------------------------------------------------
// SetupConversation
// ---------------------------------------------------------------------------

/// Drives the first-boot setup as a conversation in the main chat view.
///
/// Usage:
/// ```ignore
/// let setup = SetupConversation::new(chat_view.clone());
/// setup.on_complete(move |result| { /* create vault, store keys */ });
/// setup.start();
/// ```
#[derive(Clone)]
pub struct SetupConversation {
    state: Rc<RefCell<SetupState>>,
    chat_view: ChatView,
    config: Rc<RefCell<Option<ConfigManager>>>,
    on_complete_cb: Rc<RefCell<Option<Box<dyn Fn(SetupResult)>>>>,
}

impl SetupConversation {
    /// Create a new setup conversation that will display in the given chat view.
    ///
    /// If `config` is provided, API keys from the system config are used to
    /// pre-fill entry fields and auto-select providers when only one has a key.
    pub fn new(chat_view: ChatView, config: Option<ConfigManager>) -> Self {
        Self {
            state: Rc::new(RefCell::new(SetupState::default())),
            chat_view,
            config: Rc::new(RefCell::new(config)),
            on_complete_cb: Rc::new(RefCell::new(None)),
        }
    }

    /// Register a callback invoked when the setup finishes successfully.
    pub fn on_complete(&self, callback: impl Fn(SetupResult) + 'static) {
        *self.on_complete_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Begin the conversational setup flow by showing the Welcome card.
    pub fn start(&self) {
        self.show_step(SetupStep::Welcome);
    }

    /// Returns `true` if the setup conversation is still active (not complete).
    pub fn is_active(&self) -> bool {
        self.state.borrow().step != SetupStep::Complete
    }

    /// Handle voice input during setup. Checks the current step for matching
    /// keywords and auto-advances if appropriate.
    pub fn on_voice_input(&self, text: &str) {
        let step = self.state.borrow().step.clone();
        let lower = text.to_lowercase();

        match step {
            SetupStep::Welcome => {
                // Any voice input advances past welcome.
                self.advance(SetupStep::TestAudioOutput);
            }
            SetupStep::TestAudioOutput => {
                // Voice input during audio test = audio works!
                self.advance(SetupStep::TestAudioInput);
            }
            SetupStep::TestAudioInput => {
                // Any voice input means the mic works.
                self.chat_view.add_message("system",
                    &t_fmt("setup.audio_input.voice_works", &[("text", text)]));
                self.advance(SetupStep::NameAssistant);
            }
            SetupStep::NameAssistant => {
                // Any voice input sets the assistant name and advances.
                let name = text.trim().to_string();
                if !name.is_empty() {
                    let machine = name.to_lowercase().replace(' ', "-");
                    {
                        let mut s = self.state.borrow_mut();
                        s.assistant_name = name.clone();
                        s.wake_word_custom = name.clone();
                        s.machine_name_custom = machine;
                        s.use_same_name = true;
                    }
                    self.chat_view.add_message("user", &name);
                    self.advance(SetupStep::ChooseProvider);
                }
            }
            SetupStep::ChooseProvider => {
                if lower.contains("claude") || lower.contains("anthropic") {
                    self.select_provider("claude");
                } else if lower.contains("openai") || lower.contains("gpt") || lower.contains("chatgpt") {
                    self.select_provider("openai");
                }
                // Ignore unrecognized voice input.
            }
            SetupStep::EnterApiKey { .. } | SetupStep::EnterBackupKey { .. } => {
                // API keys cannot be entered via voice.
                info!("Voice input ignored for API key entry step");
            }
            SetupStep::CreatePassword | SetupStep::ConfirmPassword => {
                // Passwords cannot be entered via voice.
                info!("Voice input ignored for password entry step");
            }
            SetupStep::AddBackup => {
                if lower.contains("yes") || lower.contains("add") || lower.contains("backup") {
                    self.handle_add_backup(true);
                } else if lower.contains("no")
                    || lower.contains("skip")
                    || lower.contains("done")
                {
                    self.handle_add_backup(false);
                }
            }
            SetupStep::ProviderOrder => {
                if lower.contains("claude") || lower.contains("anthropic") {
                    self.set_primary_order("claude");
                } else if lower.contains("openai") || lower.contains("gpt") || lower.contains("chatgpt") {
                    self.set_primary_order("openai");
                }
            }
            SetupStep::Complete => {
                // Any voice input after completion closes setup.
                self.finish();
            }
        }
    }

    // -----------------------------------------------------------------------
    // Step rendering
    // -----------------------------------------------------------------------

    /// Transition to and render a new step.
    fn show_step(&self, step: SetupStep) {
        self.state.borrow_mut().step = step.clone();

        match step {
            SetupStep::Welcome => self.show_welcome(),
            SetupStep::TestAudioOutput => self.show_test_audio_output(),
            SetupStep::TestAudioInput => self.show_test_audio_input(),
            SetupStep::NameAssistant => self.show_name_assistant(),
            SetupStep::ChooseProvider => self.show_choose_provider(),
            SetupStep::EnterApiKey { ref provider } => {
                self.show_enter_api_key(provider.clone(), false);
            }
            SetupStep::CreatePassword => self.show_create_password(),
            SetupStep::ConfirmPassword => self.show_confirm_password(),
            SetupStep::AddBackup => self.show_add_backup(),
            SetupStep::EnterBackupKey { ref provider } => {
                self.show_enter_api_key(provider.clone(), true);
            }
            SetupStep::ProviderOrder => self.show_provider_order(),
            SetupStep::Complete => self.show_complete(),
        }
    }

    /// Advance to the next step.
    ///
    /// Dismisses the interactive widgets from the previous step's card
    /// so the user can't click old buttons.
    fn advance(&self, next: SetupStep) {
        self.stop_speaking();
        self.advance_with_choice(next, None);
    }

    /// Stop any active TTS playback immediately.
    fn stop_speaking(&self) {
        std::thread::spawn(|| {
            // Kill any running TTS processes
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
                .args(["-f", "aplay"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    }

    /// Advance to the next step, showing what the user chose in the previous card.
    fn advance_with_choice(&self, next: SetupStep, choice: Option<&str>) {
        // Prevent double-click race — only advance if the step actually changes.
        {
            let mut s = self.state.borrow_mut();
            if s.step == next {
                return; // Already on this step (duplicate click)
            }
            // Bump version so stale closures from the old step are ignored.
            let new_ver = s.step_version.wrapping_add(1);
            s.step_version = new_ver;
        }
        self.chat_view.dismiss_last_card_input_with_choice(choice);
        self.show_step(next);
    }

    // -- Step 1: Welcome ----------------------------------------------------

    fn show_welcome(&self) {
        let btn = gtk::Button::with_label(&t("setup.welcome.button"));
        btn.add_css_class("suggested-action");
        btn.add_css_class("pill");
        btn.set_halign(Align::Start);
        btn.set_margin_top(8);

        let handle = self.chat_view.add_setup_card(
            "starred-symbolic",
            &t("setup.welcome.title"),
            &t("setup.welcome.description"),
            Some(btn.upcast_ref()),
        );

        let this = self.clone();
        btn.connect_clicked(move |_| {
            if let Some(ref h) = handle { h.dismiss(&t("setup.welcome.dismiss")); }
            this.advance(SetupStep::TestAudioOutput);
        });

        self.speak(&t("setup.welcome.tts"));
    }

    // -- Step 1b: Test Audio Output -----------------------------------------

    fn show_test_audio_output(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // "Replay" button — speaks the test phrase again.
        let replay_btn = gtk::Button::with_label(&t("setup.audio_output.replay"));
        replay_btn.set_halign(Align::Start);

        let this_for_replay = self.clone();
        replay_btn.connect_clicked(move |_| {
            this_for_replay.speak(&t("setup.audio_output.tts"));
        });
        input_box.append(&replay_btn);

        // Row of yes/no buttons.
        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_margin_top(4);

        let yes_btn = gtk::Button::with_label(&t("setup.audio_output.yes"));
        yes_btn.add_css_class("suggested-action");
        btn_box.append(&yes_btn);

        let no_btn = gtk::Button::with_label(&t("setup.audio_output.no"));
        btn_box.append(&no_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "audio-speakers-symbolic",
            &t("setup.audio_output.title"),
            &t("setup.audio_output.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        yes_btn.connect_clicked(move |_| {
            if let Some(ref h) = h { h.dismiss(&t("setup.audio_output.dismiss_works")); }
            this.chat_view.add_message("user", &t("setup.audio_output.user_yes"));
            this.advance(SetupStep::TestAudioInput);
        });

        let this = self.clone();
        no_btn.connect_clicked(move |_| {
            if let Some(ref h) = handle { h.dismiss(&t("setup.audio_output.dismiss_skipped")); }
            this.chat_view.add_message("user", &t("setup.audio_output.user_skip"));
            this.chat_view.add_message("system",
                &t("setup.audio_output.skipped"));
            this.advance(SetupStep::NameAssistant);
        });

        // Speak the test phrase.
        self.speak(&t("setup.audio_output.tts"));
    }

    // -- Step 1c: Test Audio Input ------------------------------------------

    fn show_test_audio_input(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let status_label = gtk::Label::new(Some(&t("setup.audio_input.speak_now")));
        status_label.set_halign(Align::Start);
        input_box.append(&status_label);

        // VU meter — real-time audio level display
        let level_bar = gtk::LevelBar::for_interval(0.0, 1.0);
        level_bar.set_value(0.0);
        level_bar.set_hexpand(true);
        level_bar.set_margin_top(4);
        level_bar.set_margin_bottom(4);
        // Add color thresholds: green (low), yellow (mid), red (high)
        level_bar.remove_offset_value(Some("low"));
        level_bar.remove_offset_value(Some("high"));
        level_bar.remove_offset_value(Some("full"));
        level_bar.add_offset_value("low", 0.3);
        level_bar.add_offset_value("high", 0.6);
        level_bar.add_offset_value("full", 0.9);
        level_bar.set_size_request(-1, 20);
        input_box.append(&level_bar);

        let level_label = gtk::Label::new(Some(&t("setup.audio_input.no_audio")));
        level_label.set_halign(Align::Start);
        level_label.add_css_class("dim-label");
        input_box.append(&level_label);

        // Buttons row
        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_margin_top(8);

        let works_btn = gtk::Button::with_label(&t("setup.audio_input.works"));
        works_btn.add_css_class("suggested-action");
        works_btn.set_sensitive(false); // enabled once audio is detected
        btn_box.append(&works_btn);

        let skip_btn = gtk::Button::with_label(&t("setup.audio_input.skip"));
        btn_box.append(&skip_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "audio-input-microphone-symbolic",
            &t("setup.audio_input.title"),
            &t("setup.audio_input.description"),
            Some(input_box.upcast_ref()),
        );

        // Start audio capture for the VU meter
        let mic_active = std::rc::Rc::new(std::cell::Cell::new(true));
        let mic_buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));

        // Background thread: capture mic audio
        let buf_writer = mic_buffer.clone();
        let active_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let flag_for_thread = active_flag.clone();
        std::thread::spawn(move || {
            use aios_voice::audio::capture::AudioCapture;
            let mut capture = AudioCapture::new();
            if capture.start_recording().is_err() {
                return; // no mic available
            }
            while flag_for_thread.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if let Ok(mut buf) = capture.buffer().lock() {
                    if !buf.is_empty() {
                        let samples: Vec<f32> = std::mem::take(&mut *buf);
                        if let Ok(mut target) = buf_writer.lock() {
                            *target = samples;
                        }
                    }
                }
            }
            capture.stop_recording();
        });

        // GTK timer: update the VU meter every 60ms
        let buf_reader = mic_buffer.clone();
        let mic_active_for_timer = mic_active.clone();
        let level_bar_ref = level_bar.clone();
        let level_label_ref = level_label.clone();
        let works_btn_ref = works_btn.clone();
        let mut peak_seen = false;
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
            if !mic_active_for_timer.get() {
                level_bar_ref.set_value(0.0);
                return gtk::glib::ControlFlow::Break;
            }

            let rms = if let Ok(samples) = buf_reader.lock() {
                if samples.is_empty() {
                    0.0
                } else {
                    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
                    (sum_sq / samples.len() as f32).sqrt()
                }
            } else {
                0.0
            };

            // Scale RMS to 0..1 range (typical speech is 0.01-0.15)
            let level = (rms * 8.0).min(1.0);
            level_bar_ref.set_value(level as f64);

            if level > 0.05 && !peak_seen {
                peak_seen = true;
                level_label_ref.set_text(&t("setup.audio_input.detected"));
                works_btn_ref.set_sensitive(true);
            } else if level > 0.05 {
                // Keep updating with current level
                let bars = (level * 20.0) as usize;
                let bar_str: String = "\u{2588}".repeat(bars);
                level_label_ref.set_text(&format!("\u{1f3a4} {bar_str}"));
            } else if !peak_seen {
                level_label_ref.set_text(&t("setup.audio_input.waiting"));
            }

            gtk::glib::ControlFlow::Continue
        });

        // "Mic works" button
        let this = self.clone();
        let h = handle.clone();
        let mic_active_for_works = mic_active.clone();
        let flag_for_works = active_flag.clone();
        works_btn.connect_clicked(move |_| {
            mic_active_for_works.set(false);
            flag_for_works.store(false, std::sync::atomic::Ordering::Relaxed);
            if let Some(ref h) = h { h.dismiss(&t("setup.audio_input.dismiss_works")); }
            this.chat_view.add_message("user", &t("setup.audio_input.user_works"));
            this.advance(SetupStep::NameAssistant);
        });

        // "Skip" button
        let this = self.clone();
        let mic_active_for_skip = mic_active.clone();
        let flag_for_skip = active_flag.clone();
        skip_btn.connect_clicked(move |_| {
            mic_active_for_skip.set(false);
            flag_for_skip.store(false, std::sync::atomic::Ordering::Relaxed);
            if let Some(ref h) = handle { h.dismiss(&t("setup.audio_input.dismiss_skipped")); }
            this.chat_view.add_message("user", &t("setup.audio_input.user_skip"));
            this.chat_view.add_message("system",
                &t("setup.audio_input.skipped"));
            this.advance(SetupStep::NameAssistant);
        });

        self.speak(&t("setup.audio_input.tts"));
    }

    // -- Step 1d: Name Your Assistant ----------------------------------------

    fn show_name_assistant(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Assistant name
        let name_label = gtk::Label::new(Some(&t("setup.name.label")));
        name_label.set_halign(Align::Start);
        input_box.append(&name_label);

        let name_entry = gtk::Entry::builder()
            .text(&t("setup.name.default"))
            .hexpand(true)
            .build();
        name_entry.add_css_class("setup-input");
        input_box.append(&name_entry);

        // Same-for-all toggle
        let same_check = gtk::CheckButton::with_label(
            &t("setup.name.same_toggle"),
        );
        same_check.set_active(true);
        same_check.set_margin_top(8);
        input_box.append(&same_check);

        // Extra fields (hidden by default)
        let extra_box = gtk::Box::new(Orientation::Vertical, 6);
        extra_box.set_visible(false);
        extra_box.set_margin_top(8);

        let wake_label = gtk::Label::new(Some(&t("setup.name.wake_label")));
        wake_label.set_halign(Align::Start);
        extra_box.append(&wake_label);

        let wake_entry = gtk::Entry::builder()
            .text(&t("setup.name.default"))
            .hexpand(true)
            .build();
        wake_entry.add_css_class("setup-input");
        extra_box.append(&wake_entry);

        let host_label = gtk::Label::new(Some(&t("setup.name.host_label")));
        host_label.set_halign(Align::Start);
        extra_box.append(&host_label);

        let host_entry = gtk::Entry::builder()
            .text(&t("setup.name.default_lowercase"))
            .hexpand(true)
            .build();
        host_entry.add_css_class("setup-input");
        extra_box.append(&host_entry);

        input_box.append(&extra_box);

        // Toggle visibility of extra fields
        {
            let extra_ref = extra_box.clone();
            same_check.connect_toggled(move |t| {
                extra_ref.set_visible(!t.is_active());
            });
        }

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        input_box.append(&error_label);

        let next_btn = gtk::Button::with_label(&t("setup.name.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(4);
        input_box.append(&next_btn);

        let handle = self.chat_view.add_setup_card(
            "avatar-default-symbolic",
            &t("setup.name.title"),
            &t("setup.name.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let name_ref = name_entry.clone();
        let same_ref = same_check.clone();
        let wake_ref = wake_entry.clone();
        let host_ref = host_entry.clone();
        let error_ref = error_label.clone();
        next_btn.connect_clicked(move |b| {
            let name = name_ref.text().to_string().trim().to_string();
            if name.is_empty() {
                error_ref.set_text(&t("setup.name.error_empty"));
                error_ref.set_visible(true);
                return;
            }

            let use_same = same_ref.is_active();
            let wake = if use_same {
                name.clone()
            } else {
                wake_ref.text().to_string().trim().to_string()
            };
            let machine = if use_same {
                name.to_lowercase().replace(' ', "-")
            } else {
                host_ref.text().to_string().trim().to_lowercase()
            };

            // Validate machine name
            let machine_valid = !machine.is_empty()
                && machine.len() <= 63
                && machine.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !machine.starts_with('-')
                && !machine.ends_with('-');

            if !machine_valid {
                error_ref.set_text(&t("setup.name.error_hostname"));
                error_ref.set_visible(true);
                return;
            }

            b.set_sensitive(false);
            error_ref.set_visible(false);

            {
                let mut s = this.state.borrow_mut();
                s.assistant_name = name.clone();
                s.wake_word_custom = wake;
                s.machine_name_custom = machine;
                s.use_same_name = use_same;
            }

            this.chat_view.add_message("user", &name);
            if let Some(ref h) = handle {
                h.dismiss(&name);
            }
            this.advance(SetupStep::ChooseProvider);
        });

        self.speak(&t("setup.name.tts"));
    }

    // -- Step 2: Choose Provider --------------------------------------------

    fn show_choose_provider(&self) {
        // If only one provider has a key in config, auto-select it.
        if let Some(ref config) = *self.config.borrow() {
            let has_claude = !config.get_str("llm.claude_api_key", "").is_empty();
            let has_openai = {
                let k = config.get_str("llm.openai_api_key", "");
                !k.is_empty() && k != "your-api-key-here"
            };
            if has_claude && !has_openai {
                self.chat_view.add_message("system",
                    &t("setup.provider.auto_claude"));
                self.select_provider("claude");
                return;
            }
            if has_openai && !has_claude {
                self.chat_view.add_message("system",
                    &t("setup.provider.auto_chatgpt"));
                self.select_provider("openai");
                return;
            }
        }

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Claude button.
        let claude_btn = Self::make_provider_button(
            &t("setup.provider.claude_name"),
            &t("setup.provider.claude_desc"),
        );

        input_box.append(&claude_btn);

        // ChatGPT button.
        let openai_btn = Self::make_provider_button(
            &t("setup.provider.chatgpt_name"),
            &t("setup.provider.chatgpt_desc"),
        );
        input_box.append(&openai_btn);

        let handle = self.chat_view.add_setup_card(
            "network-server-symbolic",
            &t("setup.provider.title"),
            &t("setup.provider.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        claude_btn.connect_clicked(move |_| {
            if let Some(ref h) = h { h.dismiss(&t("setup.provider.claude_name")); }
            this.select_provider("claude");
        });

        let this = self.clone();
        openai_btn.connect_clicked(move |_| {
            if let Some(ref h) = handle { h.dismiss(&t("setup.provider.chatgpt_name")); }
            this.select_provider("openai");
        });

        self.speak(&t("setup.provider.tts"));
    }

    /// Create a styled provider selection button.
    fn make_provider_button(name: &str, description: &str) -> gtk::Button {
        let content = gtk::Box::new(Orientation::Vertical, 2);
        content.set_margin_top(4);
        content.set_margin_bottom(4);
        content.set_margin_start(8);
        content.set_margin_end(8);

        let name_label = gtk::Label::new(Some(name));
        name_label.add_css_class("heading");
        name_label.set_halign(Align::Start);
        content.append(&name_label);

        let desc_label = gtk::Label::new(Some(description));
        desc_label.add_css_class("dim-label");
        desc_label.set_halign(Align::Start);
        desc_label.set_wrap(true);
        content.append(&desc_label);

        let btn = gtk::Button::new();
        btn.set_child(Some(&content));
        btn.add_css_class("setup-provider-button");
        btn
    }

    /// Handle provider selection (from button click or voice).
    fn select_provider(&self, provider: &str) {
        {
            let mut s = self.state.borrow_mut();
            s.primary_provider = provider.to_owned();
        }

        // Show a user-style confirmation message.
        let display = match provider {
            "claude" => t("setup.provider.claude_name"),
            "openai" => t("setup.provider.chatgpt_name"),
            other => other.to_string(),
        };
        self.chat_view.add_message("user", &display);

        let next = SetupStep::EnterApiKey {
            provider: provider.to_owned(),
        };
        self.advance_with_choice(next, Some(&display));
    }

    // -- Step 3 / Step 7: Enter API Key ------------------------------------

    fn show_enter_api_key(&self, provider: String, is_backup: bool) {
        let (title, tutorial) = match provider.as_str() {
            "claude" => (
                t("setup.api_key.title_claude"),
                t("setup.api_key.tutorial_claude"),
            ),
            "openai" => (
                t("setup.api_key.title_chatgpt"),
                t("setup.api_key.tutorial_chatgpt"),
            ),
            _ => (
                t_fmt("setup.api_key.title_generic", &[("provider", &provider)]),
                t("setup.api_key.tutorial_generic"),
            ),
        };

        let input_box = gtk::Box::new(Orientation::Vertical, 6);
        input_box.set_margin_top(8);

        let entry = gtk::PasswordEntry::builder()
            .placeholder_text(&t("setup.api_key.placeholder"))
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        entry.add_css_class("setup-input");

        // Pre-fill from config if an API key already exists.
        if let Some(ref config) = *self.config.borrow() {
            let config_key = match provider.as_str() {
                "claude" => "llm.claude_api_key",
                "openai" => "llm.openai_api_key",
                _ => "",
            };
            if !config_key.is_empty() {
                let existing = config.get_str(config_key, "");
                if !existing.is_empty() && existing != "your-api-key-here" {
                    entry.set_text(&existing);
                }
            }
        }

        input_box.append(&entry);

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        input_box.append(&error_label);

        let next_btn = gtk::Button::with_label(&t("setup.api_key.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(4);
        input_box.append(&next_btn);

        let this = self.clone();
        let entry_ref = entry.clone();
        let error_ref = error_label.clone();
        let provider_clone = provider.clone();
        next_btn.connect_clicked(move |b| {
            let api_key = entry_ref.text().to_string().trim().to_owned();
            if api_key.is_empty() {
                error_ref.set_text(&t("setup.api_key.error_empty"));
                error_ref.set_visible(true);
                return;
            }
            b.set_sensitive(false);
            error_ref.set_visible(false);

            this.store_api_key(&provider_clone, api_key, is_backup);
        });

        // Allow Enter key to submit.
        let this = self.clone();
        let entry_ref2 = entry.clone();
        let error_ref2 = error_label.clone();
        let provider_clone2 = provider.clone();
        entry.connect_activate(move |_| {
            let api_key = entry_ref2.text().to_string().trim().to_owned();
            if api_key.is_empty() {
                error_ref2.set_text(&t("setup.api_key.error_empty"));
                error_ref2.set_visible(true);
                return;
            }
            error_ref2.set_visible(false);

            this.store_api_key(&provider_clone2, api_key, is_backup);
        });

        self.chat_view.add_setup_card(
            "dialog-password-symbolic",
            &title,
            &tutorial,
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.api_key.tts"));

        // Focus the entry after a brief delay so the card is rendered.
        let entry_focus = entry.clone();
        gtk4::glib::idle_add_local_once(move || {
            entry_focus.grab_focus();
        });
    }

    /// Store an API key and advance to the next step.
    fn store_api_key(&self, provider: &str, api_key: String, is_backup: bool) {
        // Show masked key as user response.
        let masked = if api_key.len() > 8 {
            format!(
                "{}...{}",
                &api_key[..4],
                &api_key[api_key.len() - 4..]
            )
        } else {
            t("setup.api_key.masked_fallback")
        };
        self.chat_view
            .add_message("user", &t_fmt("setup.api_key.user_masked", &[("masked", &masked)]));

        {
            let mut s = self.state.borrow_mut();
            s.providers.push(ProviderSetup {
                name: provider.to_owned(),
                api_key,
            });
        }

        if is_backup {
            // Backup key entered — go to provider order.
            self.advance(SetupStep::ProviderOrder);
        } else {
            // Primary key entered — go to create password.
            self.advance(SetupStep::CreatePassword);
        }
    }

    // -- Step 4: Create Password -------------------------------------------

    fn show_create_password(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 6);
        input_box.set_margin_top(8);

        let entry = gtk::PasswordEntry::builder()
            .placeholder_text(&t("setup.password.placeholder"))
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        entry.add_css_class("setup-input");
        input_box.append(&entry);

        // Strength indicator.
        let strength_box = gtk::Box::new(Orientation::Horizontal, 6);
        strength_box.set_margin_top(2);
        let strength_label = gtk::Label::new(Some(&t("setup.password.strength")));
        strength_label.add_css_class("dim-label");
        strength_box.append(&strength_label);

        let strength_bar = gtk::LevelBar::builder()
            .min_value(0.0)
            .max_value(4.0)
            .value(0.0)
            .hexpand(true)
            .build();
        strength_box.append(&strength_bar);
        input_box.append(&strength_box);

        // Update strength as user types.
        {
            let bar = strength_bar.clone();
            entry.connect_changed(move |e| {
                let text = e.text();
                let score = password_strength_score(&text);
                bar.set_value(score);
            });
        }

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        input_box.append(&error_label);

        let next_btn = gtk::Button::with_label(&t("setup.password.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(4);
        input_box.append(&next_btn);

        {
            let this = self.clone();
            let entry_ref = entry.clone();
            let error_ref = error_label.clone();
            next_btn.connect_clicked(move |b| {
                let password = entry_ref.text().to_string();
                if password.len() < 8 {
                    error_ref.set_text(&t("setup.password.error_short"));
                    error_ref.set_visible(true);
                    return;
                }
                b.set_sensitive(false);
                error_ref.set_visible(false);

                {
                    let mut s = this.state.borrow_mut();
                    s.pending_password = password;
                }

                this.chat_view.add_message("user", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
                this.advance(SetupStep::ConfirmPassword);
            });
        }

        {
            let this = self.clone();
            let entry_ref = entry.clone();
            let error_ref = error_label.clone();
            entry.connect_activate(move |_| {
                let password = entry_ref.text().to_string();
                if password.len() < 8 {
                    error_ref.set_text(&t("setup.password.error_short"));
                    error_ref.set_visible(true);
                    return;
                }
                error_ref.set_visible(false);

                {
                    let mut s = this.state.borrow_mut();
                    s.pending_password = password;
                }

                this.chat_view.add_message("user", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
                this.advance(SetupStep::ConfirmPassword);
            });
        }

        self.chat_view.add_setup_card(
            "channel-secure-symbolic",
            &t("setup.password.title"),
            &t("setup.password.description"),
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.password.tts"));

        let entry_focus = entry.clone();
        gtk4::glib::idle_add_local_once(move || {
            entry_focus.grab_focus();
        });
    }

    // -- Step 5: Confirm Password ------------------------------------------

    fn show_confirm_password(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 6);
        input_box.set_margin_top(8);

        let entry = gtk::PasswordEntry::builder()
            .placeholder_text(&t("setup.confirm_password.placeholder"))
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        entry.add_css_class("setup-input");
        input_box.append(&entry);

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        error_label.set_halign(Align::Start);
        input_box.append(&error_label);

        let next_btn = gtk::Button::with_label(&t("setup.confirm_password.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(4);
        input_box.append(&next_btn);

        {
            let this = self.clone();
            let entry_ref = entry.clone();
            let error_ref = error_label.clone();
            next_btn.connect_clicked(move |b| {
                let confirm = entry_ref.text().to_string();
                let pending = this.state.borrow().pending_password.clone();

                if confirm != pending {
                    error_ref.set_text(&t("setup.confirm_password.error_mismatch"));
                    error_ref.set_visible(true);
                    return;
                }
                b.set_sensitive(false);
                error_ref.set_visible(false);

                {
                    let mut s = this.state.borrow_mut();
                    s.master_password = s.pending_password.clone();
                    s.pending_password.clear();
                }

                this.chat_view.add_message("user", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
                this.advance(SetupStep::AddBackup);
            });
        }

        {
            let this = self.clone();
            let entry_ref = entry.clone();
            let error_ref = error_label.clone();
            entry.connect_activate(move |_| {
                let confirm = entry_ref.text().to_string();
                let pending = this.state.borrow().pending_password.clone();

                if confirm != pending {
                    error_ref.set_text(&t("setup.confirm_password.error_mismatch"));
                    error_ref.set_visible(true);
                    return;
                }
                error_ref.set_visible(false);

                {
                    let mut s = this.state.borrow_mut();
                    s.master_password = s.pending_password.clone();
                    s.pending_password.clear();
                }

                this.chat_view.add_message("user", "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
                this.advance(SetupStep::AddBackup);
            });
        }

        self.chat_view.add_setup_card(
            "emblem-ok-symbolic",
            &t("setup.confirm_password.title"),
            &t("setup.confirm_password.description"),
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.confirm_password.tts"));

        let entry_focus = entry.clone();
        gtk4::glib::idle_add_local_once(move || {
            entry_focus.grab_focus();
        });
    }

    // -- Step 6: Add Backup Provider? --------------------------------------

    fn show_add_backup(&self) {
        let primary = self.state.borrow().primary_provider.clone();
        let other_name = if primary == "claude" {
            t("setup.provider.chatgpt_name")
        } else {
            t("setup.provider.claude_name")
        };

        // Always show the backup provider question — the user can enter
        // the key manually if one isn't pre-configured.

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let yes_btn = gtk::Button::with_label(&t_fmt("setup.backup.yes", &[("provider", &other_name)]));
        yes_btn.add_css_class("suggested-action");
        yes_btn.set_halign(Align::Start);

        let this = self.clone();
        yes_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.handle_add_backup(true);
        });
        input_box.append(&yes_btn);

        let no_btn = gtk::Button::with_label(&t("setup.backup.no"));
        no_btn.set_halign(Align::Start);

        let this = self.clone();
        no_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.handle_add_backup(false);
        });
        input_box.append(&no_btn);

        self.chat_view.add_setup_card(
            "list-add-symbolic",
            &t("setup.backup.title"),
            &t("setup.backup.description"),
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.backup.tts"));
    }

    /// Handle the add-backup decision (from button or voice).
    fn handle_add_backup(&self, add: bool) {
        if add {
            let primary = self.state.borrow().primary_provider.clone();
            let backup = if primary == "claude" {
                "openai"
            } else {
                "claude"
            };

            let display = if backup == "claude" {
                t("setup.provider.claude_name")
            } else {
                t("setup.provider.chatgpt_name")
            };
            self.chat_view.add_message("user",
                &t_fmt("setup.backup.user_yes", &[("provider", &display)]));

            let dismiss = t_fmt("setup.backup.dismiss_add", &[("provider", &display)]);
            self.advance_with_choice(SetupStep::EnterBackupKey {
                provider: backup.to_owned(),
            }, Some(&dismiss));
        } else {
            self.chat_view.add_message("user", &t("setup.backup.user_no"));
            self.advance_with_choice(SetupStep::Complete, Some(&t("setup.backup.dismiss_none")));
        }
    }

    // -- Step 8: Provider Order --------------------------------------------

    fn show_provider_order(&self) {
        let providers = self.state.borrow().providers.clone();
        if providers.len() < 2 {
            // Only one provider — skip ordering.
            self.advance(SetupStep::Complete);
            return;
        }

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        for p in &providers {
            let display = match p.name.as_str() {
                "claude" => t("setup.provider.claude_name"),
                "openai" => t("setup.provider.chatgpt_name"),
                other => other.to_string(),
            };

            let btn = gtk::Button::with_label(&t_fmt("setup.order.primary", &[("provider", &display)]));
            btn.add_css_class("setup-provider-button");
            btn.set_halign(Align::Start);

            let this = self.clone();
            let name = p.name.clone();
            btn.connect_clicked(move |b| {
                b.set_sensitive(false);
                this.set_primary_order(&name);
            });
            input_box.append(&btn);
        }

        self.chat_view.add_setup_card(
            "view-sort-descending-symbolic",
            &t("setup.order.title"),
            &t("setup.order.description"),
            Some(input_box.upcast_ref()),
        );

        self.speak(&t("setup.order.tts"));
    }

    /// Set the primary provider order and advance.
    fn set_primary_order(&self, primary: &str) {
        {
            let mut s = self.state.borrow_mut();
            s.providers.sort_by(|a, _b| {
                if a.name == primary {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            });
        }

        let display = match primary {
            "claude" => t("setup.provider.claude_name"),
            "openai" => t("setup.provider.chatgpt_name"),
            other => other.to_string(),
        };
        self.chat_view
            .add_message("user", &t_fmt("setup.order.primary", &[("provider", &display)]));

        self.advance(SetupStep::Complete);
    }

    // -- Step: Complete -----------------------------------------------------

    fn show_complete(&self) {
        use aios_core::types::{BootStatus, StatusLine, MessageLevel};

        let s = self.state.borrow();
        let mut status = BootStatus::new();

        for (i, p) in s.providers.iter().enumerate() {
            let role = if i == 0 {
                t("setup.complete.primary_provider")
            } else {
                t("setup.complete.backup_provider")
            };
            let display = match p.name.as_str() {
                "claude" => "Claude".to_string(),
                "openai" => "ChatGPT".to_string(),
                other => other.to_string(),
            };
            status.add(StatusLine::new(
                &role,
                true,
                t_fmt("setup.complete.api_key_stored", &[("provider", &display)]),
            ));
        }
        status.add(StatusLine::new(&t("setup.complete.assistant_name"), true, &s.assistant_name));
        status.add(StatusLine::new(&t("setup.complete.wake_word"), true, &s.wake_word_custom));
        status.add(StatusLine::new(&t("setup.complete.network"), true, t_fmt("setup.complete.network_local", &[("name", &s.machine_name_custom)])));
        status.add(StatusLine::new(&t("setup.complete.master_password"), true, &t("setup.complete.master_password_set")));
        status.add(StatusLine::new(&t("setup.complete.vault"), true, &t("setup.complete.vault_created")));
        status.add(StatusLine::new(&t("setup.complete.web_channel"), true, &t("setup.complete.web_channel_url")));

        drop(s);

        // Show the INFO summary.
        self.chat_view.add_level_message(
            MessageLevel::Info,
            &format!("{}\n\n{}", t("setup.complete.title"), status.format()),
        );

        // "Start Chatting" button via a setup card (no description needed).
        let btn = gtk::Button::with_label(&t("setup.complete.button"));
        btn.add_css_class("suggested-action");
        btn.add_css_class("pill");
        btn.set_halign(Align::Start);
        btn.set_margin_top(8);

        let this = self.clone();
        btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.finish();
        });

        self.chat_view.add_setup_card(
            "emblem-default-symbolic",
            &t("setup.complete.all_set_title"),
            &t("setup.complete.all_set_description"),
            Some(btn.upcast_ref()),
        );

        self.speak(&t("setup.complete.tts"));
    }

    /// Invoke the completion callback with the accumulated setup result.
    fn finish(&self) {
        self.chat_view.dismiss_last_card_input();
        let s = self.state.borrow();
        let wake = if s.use_same_name {
            s.assistant_name.clone()
        } else {
            s.wake_word_custom.clone()
        };
        let machine = if s.use_same_name {
            s.assistant_name.to_lowercase().replace(' ', "-")
        } else {
            s.machine_name_custom.clone()
        };
        let result = SetupResult {
            providers: s.providers.clone(),
            master_password: s.master_password.clone(),
            assistant_name: s.assistant_name.clone(),
            wake_word: wake,
            machine_name: machine,
        };
        drop(s);

        info!(
            "Setup conversation complete: {} provider(s)",
            result.providers.len()
        );

        if let Some(ref cb) = *self.on_complete_cb.borrow() {
            cb(result);
        }
    }

    // -----------------------------------------------------------------------
    // TTS helper
    // -----------------------------------------------------------------------

    /// Attempt to speak text via TTS. Currently just logs — real TTS integration
    /// will be wired in via the voice controller.
    fn speak(&self, text: &str) {
        // Use Piper (natural voice) or espeak-ng (fallback) for TTS.
        let text = text.to_string();
        std::thread::spawn(move || {
            // Small delay to let any preceding pkill (from stop_speaking) finish
            // before we start a new TTS process — avoids a race where the new
            // process is immediately killed by a still-running pkill.
            std::thread::sleep(std::time::Duration::from_millis(200));

            info!(tts = %text, "setup TTS");

            // Try Piper first
            let piper_model = "/home/aios/.aios/models/piper/en_US-amy-medium.onnx";
            if std::path::Path::new(piper_model).exists() {
                if let Ok(mut child) = std::process::Command::new("piper")
                    .args(["--model", piper_model, "--output_raw"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        use std::io::Write;
                        let _ = stdin.write_all(text.as_bytes());
                        drop(stdin);
                    }
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
            }

            // Fallback: espeak-ng
            let result = std::process::Command::new("espeak-ng")
                .args(["-v", "en", "-s", "160", "-p", "50"])
                .arg(&text)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();

            if let Err(e) = result {
                info!("TTS failed ({e}), falling back to test beep");
                if let Ok(wav) = generate_test_beep() {
                    let tmp = "/tmp/aios-test-beep.wav";
                    if std::fs::write(tmp, &wav).is_ok() {
                        let _ = std::process::Command::new("aplay")
                            .arg(tmp)
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                        let _ = std::fs::remove_file(tmp);
                    }
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a 1-second 440Hz sine wave as a WAV file in memory.
fn generate_test_beep() -> Result<Vec<u8>, std::io::Error> {
    let sample_rate: u32 = 44100;
    let duration_secs: f32 = 1.0;
    let frequency: f32 = 440.0;
    let num_samples = (sample_rate as f32 * duration_secs) as u32;
    let bits_per_sample: u16 = 16;
    let num_channels: u16 = 1;
    let byte_rate = sample_rate * u32::from(num_channels) * u32::from(bits_per_sample) / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = num_samples * u32::from(block_align);

    let mut buf: Vec<u8> = Vec::with_capacity(44 + data_size as usize);

    // WAV header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    // Generate sine wave samples with fade-in/out to avoid clicks
    let fade_samples = (sample_rate as f32 * 0.05) as u32; // 50ms fade
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let mut amplitude = (2.0 * std::f32::consts::PI * frequency * t).sin();

        // Fade in
        if i < fade_samples {
            amplitude *= i as f32 / fade_samples as f32;
        }
        // Fade out
        if i > num_samples - fade_samples {
            amplitude *= (num_samples - i) as f32 / fade_samples as f32;
        }

        let sample = (amplitude * 0.5 * i16::MAX as f32) as i16;
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    Ok(buf)
}

/// Simple password strength score (0.0 to 4.0) based on length and
/// character class diversity.
fn password_strength_score(password: &str) -> f64 {
    if password.is_empty() {
        return 0.0;
    }

    let len = password.len();
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| !c.is_alphanumeric());

    let classes = [has_lower, has_upper, has_digit, has_special]
        .iter()
        .filter(|&&b| b)
        .count();

    let length_score = if len >= 16 {
        2.0
    } else if len >= 12 {
        1.5
    } else if len >= 8 {
        1.0
    } else {
        0.5
    };

    let class_score = classes as f64 * 0.5;

    (length_score + class_score).min(4.0)
}
