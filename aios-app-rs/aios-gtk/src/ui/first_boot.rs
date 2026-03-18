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
    /// Country-derived locale settings (if country was selected).
    pub country: Option<aios_core::installer::locale::CountryDefaults>,
    /// Whether installation to hard drive was performed.
    pub installed_to_drive: bool,
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
    Country,
    InstallDecision,
    DriveSelection,
    PartitionPlan,
    InstallConfirm,
    InstallProgress,
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
    /// Selected country defaults (from Country step).
    country: Option<aios_core::installer::locale::CountryDefaults>,
    /// Whether the user chose to install to hard drive.
    install_to_drive: bool,
    /// Selected target drive (from DriveSelection step).
    target_drive: Option<aios_core::installer::drives::DriveInfo>,
    /// Generated partition plan (from PartitionPlan step).
    partition_plan: Option<aios_core::installer::partition::PartitionPlan>,
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
            country: None,
            install_to_drive: false,
            target_drive: None,
            partition_plan: None,
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
                self.advance(SetupStep::Country);
            }
            SetupStep::Country => {
                // Dropdown interaction — ignore voice.
                info!("Voice input ignored for country selection step");
            }
            SetupStep::InstallDecision => {
                if lower.contains("yes") || lower.contains("install") {
                    self.state.borrow_mut().install_to_drive = true;
                    self.chat_view.add_message("user", &t("setup.install_decision.user_yes"));
                    self.advance(SetupStep::DriveSelection);
                } else if lower.contains("no") || lower.contains("skip") || lower.contains("usb") {
                    self.state.borrow_mut().install_to_drive = false;
                    self.chat_view.add_message("user", &t("setup.install_decision.user_no"));
                    self.advance(SetupStep::NameAssistant);
                }
            }
            SetupStep::DriveSelection => {
                // Dropdown interaction — ignore voice.
                info!("Voice input ignored for drive selection step");
            }
            SetupStep::PartitionPlan => {
                if lower.contains("yes") || lower.contains("continue") || lower.contains("confirm") {
                    self.advance(SetupStep::NameAssistant);
                } else if lower.contains("no") || lower.contains("cancel") || lower.contains("back") {
                    self.state.borrow_mut().install_to_drive = false;
                    self.advance(SetupStep::NameAssistant);
                }
            }
            SetupStep::InstallConfirm => {
                if lower.contains("install") || lower.contains("yes") || lower.contains("confirm") {
                    self.advance(SetupStep::InstallProgress);
                } else if lower.contains("cancel") || lower.contains("no") {
                    self.state.borrow_mut().install_to_drive = false;
                    self.finish();
                }
            }
            SetupStep::InstallProgress => {
                // Installation in progress — ignore voice.
                info!("Voice input ignored during installation");
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
            SetupStep::Country => self.show_country(),
            SetupStep::InstallDecision => self.show_install_decision(),
            SetupStep::DriveSelection => self.show_drive_selection(),
            SetupStep::PartitionPlan => self.show_partition_plan(),
            SetupStep::InstallConfirm => self.show_install_confirm(),
            SetupStep::InstallProgress => self.show_install_progress(),
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
            this.advance(SetupStep::Country);
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
            this.advance(SetupStep::Country);
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
            this.advance(SetupStep::Country);
        });

        self.speak(&t("setup.audio_input.tts"));
    }

    // -- Step: Country Selection --------------------------------------------

    fn show_country(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let countries = aios_core::installer::locale::all_countries();

        // Build a StringList model for the dropdown.
        let names: Vec<&str> = countries.iter().map(|c| c.country_name).collect();
        let model = gtk::StringList::new(&names);

        let dropdown = gtk::DropDown::new(Some(model), gtk::Expression::NONE);
        dropdown.set_hexpand(true);
        input_box.append(&dropdown);

        // Summary label showing derived settings.
        let summary_label = gtk::Label::new(None);
        summary_label.set_halign(Align::Start);
        summary_label.set_wrap(true);
        summary_label.add_css_class("dim-label");
        summary_label.set_margin_top(4);
        input_box.append(&summary_label);

        // Update summary whenever the dropdown selection changes.
        {
            let label_ref = summary_label.clone();
            dropdown.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                let all = aios_core::installer::locale::all_countries();
                if idx < all.len() {
                    let c = &all[idx];
                    let time_fmt = if c.time_format_24h { "24h" } else { "12h" };
                    label_ref.set_text(&format!(
                        "Language: {} | Keyboard: {} | Timezone: {} | Time: {}",
                        c.language, c.keyboard, c.timezone, time_fmt
                    ));
                }
            });
        }

        // Trigger initial summary update.
        dropdown.notify("selected");

        let next_btn = gtk::Button::with_label(&t("setup.country.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(8);
        input_box.append(&next_btn);

        let handle = self.chat_view.add_setup_card(
            "preferences-desktop-locale-symbolic",
            &t("setup.country.title"),
            &t("setup.country.description"),
            Some(input_box.upcast_ref()),
        );

        // Wire the Next button.
        let this = self.clone();
        let dd_ref = dropdown.clone();
        next_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let idx = dd_ref.selected() as usize;
            let all = aios_core::installer::locale::all_countries();
            if idx < all.len() {
                let country = all[idx].clone();
                let name = country.country_name.to_string();
                {
                    let mut s = this.state.borrow_mut();
                    s.country = Some(country);
                }
                this.chat_view.add_message("user", &name);
                if let Some(ref h) = handle {
                    h.dismiss(&name);
                }

                // If running from live ISO, show install decision; otherwise skip to NameAssistant.
                if aios_core::installer::is_live_iso() {
                    this.advance(SetupStep::InstallDecision);
                } else {
                    this.advance(SetupStep::NameAssistant);
                }
            }
        });

        // Background country auto-detection via IP geolocation.
        // We cannot move the DropDown into a background thread (it's not Send),
        // so we send the detected index via a channel and poll from GTK.
        let (detect_tx, detect_rx) = std::sync::mpsc::channel::<u32>();
        std::thread::spawn(move || {
            // Use the blocking detect_country function from the locale module.
            // It internally uses reqwest::blocking with a 5s timeout.
            if let Some(detected) = aios_core::installer::locale::detect_country_sync_pub() {
                let all = aios_core::installer::locale::all_countries();
                if let Some(idx) = all.iter().position(|c| c.country_code == detected.country_code) {
                    let _ = detect_tx.send(idx as u32);
                }
            }
        });

        let dd_for_detect = dropdown.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            match detect_rx.try_recv() {
                Ok(idx) => {
                    dd_for_detect.set_selected(idx);
                    gtk::glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    gtk::glib::ControlFlow::Continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    gtk::glib::ControlFlow::Break
                }
            }
        });

        self.speak(&t("setup.country.tts"));
    }

    // -- Step: Install Decision ---------------------------------------------

    fn show_install_decision(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);

        let install_btn = gtk::Button::with_label(&t("setup.install_decision.install"));
        install_btn.add_css_class("suggested-action");
        btn_box.append(&install_btn);

        let usb_btn = gtk::Button::with_label(&t("setup.install_decision.usb"));
        btn_box.append(&usb_btn);

        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "drive-harddisk-symbolic",
            &t("setup.install_decision.title"),
            &t("setup.install_decision.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        install_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.state.borrow_mut().install_to_drive = true;
            this.chat_view.add_message("user", &t("setup.install_decision.user_yes"));
            if let Some(ref h) = h { h.dismiss(&t("setup.install_decision.user_yes")); }
            this.advance(SetupStep::DriveSelection);
        });

        let this = self.clone();
        usb_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.state.borrow_mut().install_to_drive = false;
            this.chat_view.add_message("user", &t("setup.install_decision.user_no"));
            if let Some(ref h) = handle { h.dismiss(&t("setup.install_decision.user_no")); }
            this.advance(SetupStep::NameAssistant);
        });

        self.speak(&t("setup.install_decision.tts"));
    }

    // -- Step: Drive Selection ----------------------------------------------

    fn show_drive_selection(&self) {
        let drives = aios_core::installer::drives::list_drives();

        // Filter out live media.
        let eligible: Vec<_> = drives.iter()
            .filter(|d| !d.is_live_media)
            .collect();

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        if eligible.is_empty() {
            let error_label = gtk::Label::new(Some(&t("setup.drive_selection.no_drives")));
            error_label.add_css_class("error");
            error_label.set_halign(Align::Start);
            error_label.set_wrap(true);
            input_box.append(&error_label);

            let back_btn = gtk::Button::with_label(&t("setup.drive_selection.back"));
            back_btn.set_halign(Align::Start);
            back_btn.set_margin_top(4);
            input_box.append(&back_btn);

            self.chat_view.add_setup_card(
                "drive-harddisk-symbolic",
                &t("setup.drive_selection.title"),
                &t("setup.drive_selection.no_drives"),
                Some(input_box.upcast_ref()),
            );

            let this = self.clone();
            back_btn.connect_clicked(move |_| {
                this.state.borrow_mut().install_to_drive = false;
                this.advance(SetupStep::NameAssistant);
            });
            return;
        }

        // Build dropdown with drive display names.
        let display_names: Vec<String> = eligible.iter()
            .map(|d| aios_core::installer::drives::drive_display_name(d))
            .collect();
        let name_refs: Vec<&str> = display_names.iter().map(|s| s.as_str()).collect();
        let model = gtk::StringList::new(&name_refs);
        let dropdown = gtk::DropDown::new(Some(model), gtk::Expression::NONE);
        dropdown.set_hexpand(true);
        input_box.append(&dropdown);

        // Drive details label.
        let detail_label = gtk::Label::new(None);
        detail_label.set_halign(Align::Start);
        detail_label.set_wrap(true);
        detail_label.add_css_class("dim-label");
        detail_label.set_margin_top(4);
        input_box.append(&detail_label);

        // Update details when selection changes.
        let eligible_clone: Vec<aios_core::installer::drives::DriveInfo> =
            eligible.iter().map(|d| (*d).clone()).collect();
        {
            let drives_for_notify = eligible_clone.clone();
            let label_ref = detail_label.clone();
            dropdown.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                if idx < drives_for_notify.len() {
                    let d = &drives_for_notify[idx];
                    let mut parts = Vec::new();
                    for p in &d.partitions {
                        let label = p.label.as_deref().unwrap_or("");
                        let mount = p.mount_point.as_deref().unwrap_or("");
                        parts.push(format!("  {} {} {} {}", p.device, p.size_human, p.fstype, if !label.is_empty() { label } else { mount }));
                    }
                    if parts.is_empty() {
                        label_ref.set_text(&t("setup.drive_selection.no_partitions"));
                    } else {
                        label_ref.set_text(&parts.join("\n"));
                    }
                }
            });
        }
        dropdown.notify("selected");

        let next_btn = gtk::Button::with_label(&t("setup.drive_selection.next"));
        next_btn.add_css_class("suggested-action");
        next_btn.set_halign(Align::Start);
        next_btn.set_margin_top(8);
        input_box.append(&next_btn);

        let handle = self.chat_view.add_setup_card(
            "drive-harddisk-symbolic",
            &t("setup.drive_selection.title"),
            &t("setup.drive_selection.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let dd_ref = dropdown.clone();
        let drives_for_click = eligible_clone;
        next_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let idx = dd_ref.selected() as usize;
            if idx < drives_for_click.len() {
                let drive = drives_for_click[idx].clone();
                let display = aios_core::installer::drives::drive_display_name(&drive);
                {
                    let mut s = this.state.borrow_mut();
                    s.target_drive = Some(drive);
                }
                this.chat_view.add_message("user", &display);
                if let Some(ref h) = handle { h.dismiss(&display); }
                this.advance(SetupStep::PartitionPlan);
            }
        });

        self.speak(&t("setup.drive_selection.tts"));
    }

    // -- Step: Partition Plan -----------------------------------------------

    fn show_partition_plan(&self) {
        let drive = self.state.borrow().target_drive.clone();
        let drive = match drive {
            Some(d) => d,
            None => {
                self.advance(SetupStep::NameAssistant);
                return;
            }
        };

        let plan = match aios_core::installer::partition::plan_partitions(&drive.device, drive.size_bytes) {
            Ok(p) => p,
            Err(e) => {
                self.chat_view.add_message("system", &format!("Partition planning failed: {e}"));
                self.state.borrow_mut().install_to_drive = false;
                self.advance(SetupStep::NameAssistant);
                return;
            }
        };

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Show planned partitions.
        for p in &plan.partitions {
            let role_label = gtk::Label::new(Some(&format!(
                "{}: {} ({})", p.role, p.size_human, p.filesystem
            )));
            role_label.set_halign(Align::Start);
            input_box.append(&role_label);
        }

        // Warning text.
        let warning = gtk::Label::new(Some(&t_fmt(
            "setup.partition_plan.warning",
            &[("drive", if drive.model.is_empty() { &drive.device } else { &drive.model })],
        )));
        warning.add_css_class("error");
        warning.set_halign(Align::Start);
        warning.set_wrap(true);
        warning.set_margin_top(8);
        input_box.append(&warning);

        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_margin_top(8);

        let continue_btn = gtk::Button::with_label(&t("setup.partition_plan.continue"));
        continue_btn.add_css_class("destructive-action");
        btn_box.append(&continue_btn);

        let cancel_btn = gtk::Button::with_label(&t("setup.partition_plan.cancel"));
        btn_box.append(&cancel_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "drive-harddisk-symbolic",
            &t("setup.partition_plan.title"),
            &t("setup.partition_plan.description"),
            Some(input_box.upcast_ref()),
        );

        // Store the plan in state.
        self.state.borrow_mut().partition_plan = Some(plan);

        let this = self.clone();
        let h = handle.clone();
        continue_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.chat_view.add_message("user", &t("setup.partition_plan.user_continue"));
            if let Some(ref h) = h { h.dismiss(&t("setup.partition_plan.user_continue")); }
            this.advance(SetupStep::NameAssistant);
        });

        let this = self.clone();
        cancel_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.state.borrow_mut().install_to_drive = false;
            this.state.borrow_mut().target_drive = None;
            this.state.borrow_mut().partition_plan = None;
            this.chat_view.add_message("user", &t("setup.partition_plan.user_cancel"));
            if let Some(ref h) = handle { h.dismiss(&t("setup.partition_plan.user_cancel")); }
            this.advance(SetupStep::NameAssistant);
        });
    }

    // -- Step: Install Confirmation -----------------------------------------

    fn show_install_confirm(&self) {
        use aios_core::types::{BootStatus, StatusLine, MessageLevel};

        let s = self.state.borrow();
        let mut status = BootStatus::new();

        if let Some(ref country) = s.country {
            status.add(StatusLine::new(&t("setup.install_confirm.country"), true, country.country_name));
            let locale_info = format!(
                "{} | Keyboard: {} | Timezone: {}",
                country.language, country.keyboard, country.timezone
            );
            status.add(StatusLine::new(&t("setup.install_confirm.locale"), true, &locale_info));
        }

        if let Some(ref drive) = s.target_drive {
            let display = aios_core::installer::drives::drive_display_name(drive);
            status.add(StatusLine::new(&t("setup.install_confirm.target"), true, &display));
        }

        if let Some(ref plan) = s.partition_plan {
            let parts: Vec<String> = plan.partitions.iter()
                .map(|p| format!("{} {}", p.role, p.size_human))
                .collect();
            status.add(StatusLine::new(&t("setup.install_confirm.partitions"), true, &parts.join(" + ")));
        }

        status.add(StatusLine::new(&t("setup.complete.assistant_name"), true, &s.assistant_name));
        status.add(StatusLine::new(&t("setup.complete.master_password"), true, &t("setup.complete.master_password_set")));

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
            status.add(StatusLine::new(&role, true, &display));
        }

        drop(s);

        let summary_text = status.format();
        self.chat_view.add_level_message(
            MessageLevel::Info,
            &format!("{}\n\n{}", t("setup.install_confirm.title"), summary_text),
        );

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);

        let install_btn = gtk::Button::with_label(&t("setup.install_confirm.install"));
        install_btn.add_css_class("destructive-action");
        btn_box.append(&install_btn);

        let cancel_btn = gtk::Button::with_label(&t("setup.install_confirm.cancel"));
        btn_box.append(&cancel_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "emblem-important-symbolic",
            &t("setup.install_confirm.card_title"),
            &t("setup.install_confirm.card_description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        install_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.chat_view.add_message("user", &t("setup.install_confirm.user_install"));
            if let Some(ref h) = h { h.dismiss(&t("setup.install_confirm.user_install")); }
            this.advance(SetupStep::InstallProgress);
        });

        let this = self.clone();
        cancel_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.state.borrow_mut().install_to_drive = false;
            this.chat_view.add_message("user", &t("setup.install_confirm.user_cancel"));
            if let Some(ref h) = handle { h.dismiss(&t("setup.install_confirm.user_cancel")); }
            this.finish();
        });
    }

    // -- Step: Install Progress ---------------------------------------------

    fn show_install_progress(&self) {
        let install_config = self.build_install_config();

        self.chat_view.add_message("system", &t("setup.install_progress.starting"));

        // Use std::sync::mpsc for thread-safe communication, then poll from GTK.
        let (tx, rx) = std::sync::mpsc::channel::<(String, String, bool)>();

        // Background thread runs the installation.
        let config_clone = install_config.clone();
        std::thread::spawn(move || {
            let result = aios_core::installer::run_install(&config_clone, |phase, msg| {
                let _ = tx.send((phase.description().to_string(), msg.to_string(), false));
            });
            match result {
                Ok(()) => {
                    let _ = tx.send(("complete".to_string(), String::new(), true));
                }
                Err(e) => {
                    let _ = tx.send(("error".to_string(), e, true));
                }
            }
        });

        // Poll for progress messages from the GTK main thread.
        let chat = self.chat_view.clone();
        let this = self.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            loop {
                match rx.try_recv() {
                    Ok((desc, msg, is_final)) => {
                        if is_final {
                            if msg.is_empty() {
                                // Success!
                                this.state.borrow_mut().install_to_drive = true;
                                this.show_reboot_dialog();
                            } else {
                                // Error — show message and recovery options.
                                chat.add_message("system",
                                    &format!("{} {msg}", t("setup.install_progress.failed")));
                                this.show_install_error_dialog(&msg);
                            }
                            return gtk::glib::ControlFlow::Break;
                        }
                        if msg.is_empty() {
                            chat.add_message("system", &desc);
                        } else {
                            chat.add_message("system", &format!("{desc} {msg}"));
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        return gtk::glib::ControlFlow::Continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return gtk::glib::ControlFlow::Break;
                    }
                }
            }
        });
    }

    /// Assemble an `InstallConfig` from the current `SetupState`.
    fn build_install_config(&self) -> aios_core::installer::InstallConfig {
        let s = self.state.borrow();
        let hostname = if s.use_same_name {
            s.assistant_name.to_lowercase().replace(' ', "-")
        } else {
            s.machine_name_custom.clone()
        };
        let wake_word = if s.use_same_name {
            s.assistant_name.clone()
        } else {
            s.wake_word_custom.clone()
        };

        aios_core::installer::InstallConfig {
            target_device: s.target_drive.as_ref().map(|d| d.device.clone()).unwrap_or_default(),
            target_size_bytes: s.target_drive.as_ref().map(|d| d.size_bytes).unwrap_or(0),
            country: s.country.clone().unwrap_or_else(|| {
                aios_core::installer::locale::country_by_code("US").unwrap().clone()
            }),
            hostname,
            master_password: s.master_password.clone(),
            assistant_name: s.assistant_name.clone(),
            wake_word,
            providers: s.providers.iter().map(|p| aios_core::installer::ProviderConfig {
                name: p.name.clone(),
                api_key: p.api_key.clone(),
            }).collect(),
        }
    }

    /// Show the modal reboot dialog after successful installation.
    fn show_install_error_dialog(&self, error: &str) {
        use libadwaita as adw;
        use adw::prelude::*;

        let widget = self.chat_view.widget();
        let window = widget.root()
            .and_then(|r| r.downcast::<adw::ApplicationWindow>().ok());
        let win_ref: Option<&gtk::Window> = window.as_ref().map(|w| w.upcast_ref::<gtk::Window>());

        let dialog = adw::MessageDialog::new(
            win_ref,
            Some("Installation Failed"),
            Some(error),
        );
        dialog.add_response("retry", "Retry Installation");
        dialog.add_response("continue", "Continue to Chat");
        dialog.add_response("reboot", "Reboot");
        dialog.set_default_response(Some("retry"));
        dialog.set_close_response("continue");

        let this = self.clone();
        dialog.connect_response(None, move |_, response| {
            match response {
                "retry" => {
                    this.show_install_progress();
                }
                "reboot" => {
                    let _ = std::process::Command::new("sudo").args(["reboot"]).status();
                }
                _ => {
                    // Continue to chat — skip installation, proceed with setup
                    this.advance(SetupStep::NameAssistant);
                }
            }
        });
        dialog.present();
    }

    fn show_reboot_dialog(&self) {
        use libadwaita as adw;
        use adw::prelude::*;

        // Find the top-level window.
        let widget = self.chat_view.widget();
        let window = widget.root()
            .and_then(|r| r.downcast::<adw::ApplicationWindow>().ok());

        let win_ref: Option<&gtk::Window> = window.as_ref().map(|w| w.upcast_ref::<gtk::Window>());

        let dialog = adw::MessageDialog::new(
            win_ref,
            Some(&t("setup.reboot.title")),
            Some(&t("setup.reboot.body")),
        );
        dialog.add_response("reboot", &t("setup.reboot.button"));
        dialog.set_default_response(Some("reboot"));
        dialog.set_close_response("reboot"); // Cannot dismiss without rebooting.
        dialog.connect_response(None, |_, _| {
            let _ = std::process::Command::new("sudo").args(["reboot"]).status();
        });
        dialog.present();
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
            let installing = self.state.borrow().install_to_drive;
            if installing {
                self.advance_with_choice(SetupStep::InstallConfirm, Some(&t("setup.backup.dismiss_none")));
            } else {
                self.advance_with_choice(SetupStep::Complete, Some(&t("setup.backup.dismiss_none")));
            }
        }
    }

    // -- Step 8: Provider Order --------------------------------------------

    fn show_provider_order(&self) {
        let providers = self.state.borrow().providers.clone();
        if providers.len() < 2 {
            // Only one provider — skip ordering.
            self.advance_to_complete_or_install();
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

        self.advance_to_complete_or_install();
    }

    /// Advance to `InstallConfirm` if the user chose to install, otherwise `Complete`.
    fn advance_to_complete_or_install(&self) {
        let installing = self.state.borrow().install_to_drive;
        if installing {
            self.advance(SetupStep::InstallConfirm);
        } else {
            self.advance(SetupStep::Complete);
        }
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
            country: s.country.clone(),
            installed_to_drive: s.install_to_drive,
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
