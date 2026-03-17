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
    on_complete_cb: Rc<RefCell<Option<Box<dyn Fn(SetupResult)>>>>,
}

impl SetupConversation {
    /// Create a new setup conversation that will display in the given chat view.
    pub fn new(chat_view: ChatView) -> Self {
        Self {
            state: Rc::new(RefCell::new(SetupState::default())),
            chat_view,
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
                    &format!("\u{2705} Mic works! I heard: \"{text}\""));
                self.advance(SetupStep::ChooseProvider);
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
        let btn = gtk::Button::with_label("Get Started \u{2192}");
        btn.add_css_class("suggested-action");
        btn.add_css_class("pill");
        btn.set_halign(Align::Start);
        btn.set_margin_top(8);

        let handle = self.chat_view.add_setup_card(
            "starred-symbolic",
            "Welcome to AiOS!",
            "I'm your AI assistant. Let's set up your system together.\n\
             First, let's check your audio.",
            Some(btn.upcast_ref()),
        );

        let this = self.clone();
        btn.connect_clicked(move |_| {
            if let Some(ref h) = handle { h.dismiss("Let's go!"); }
            this.advance(SetupStep::TestAudioOutput);
        });

        self.speak(
            "Welcome to AiOS! I'm your AI assistant. \
             Let's set up your system together.",
        );
    }

    // -- Step 1b: Test Audio Output -----------------------------------------

    fn show_test_audio_output(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // "Replay" button — speaks the test phrase again.
        let replay_btn = gtk::Button::with_label("\u{1f50a} Replay Audio");
        replay_btn.set_halign(Align::Start);

        let this_for_replay = self.clone();
        replay_btn.connect_clicked(move |_| {
            this_for_replay.speak("Can you hear me? This is AiOS speaking.");
        });
        input_box.append(&replay_btn);

        // Row of yes/no buttons.
        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_margin_top(4);

        let yes_btn = gtk::Button::with_label("\u{2705} Yes, I can hear");
        yes_btn.add_css_class("suggested-action");
        btn_box.append(&yes_btn);

        let no_btn = gtk::Button::with_label("\u{274c} No audio / Skip");
        btn_box.append(&no_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "audio-speakers-symbolic",
            "Test Audio Output",
            "Let's check if you can hear me.\n\
             I'll play a test message. Click Replay if you need to hear it again.",
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        yes_btn.connect_clicked(move |_| {
            if let Some(ref h) = h { h.dismiss("Audio works"); }
            this.chat_view.add_message("user", "Yes, I can hear the audio");
            this.advance(SetupStep::TestAudioInput);
        });

        let this = self.clone();
        no_btn.connect_clicked(move |_| {
            if let Some(ref h) = handle { h.dismiss("Skipped audio test"); }
            this.chat_view.add_message("user", "Skip audio test");
            this.chat_view.add_message("system",
                "Audio output skipped. You can configure it later in Settings.");
            this.advance(SetupStep::ChooseProvider);
        });

        // Speak the test phrase.
        self.speak("Can you hear me? This is AiOS speaking.");
    }

    // -- Step 1c: Test Audio Input ------------------------------------------

    fn show_test_audio_input(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let status_label = gtk::Label::new(Some(
            "\u{1f3a4} Speak now \u{2014} the meter should move:",
        ));
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

        let level_label = gtk::Label::new(Some("No audio detected"));
        level_label.set_halign(Align::Start);
        level_label.add_css_class("dim-label");
        input_box.append(&level_label);

        // Buttons row
        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_margin_top(8);

        let works_btn = gtk::Button::with_label("\u{2705} Mic works!");
        works_btn.add_css_class("suggested-action");
        works_btn.set_sensitive(false); // enabled once audio is detected
        btn_box.append(&works_btn);

        let skip_btn = gtk::Button::with_label("No mic / Skip \u{2192}");
        btn_box.append(&skip_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "audio-input-microphone-symbolic",
            "Test Microphone",
            "Speak into your microphone.\n\
             The meter below shows your audio level in real time.",
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
                level_label_ref.set_text("\u{2705} Audio detected! Your microphone works.");
                works_btn_ref.set_sensitive(true);
            } else if level > 0.05 {
                // Keep updating with current level
                let bars = (level * 20.0) as usize;
                let bar_str: String = "\u{2588}".repeat(bars);
                level_label_ref.set_text(&format!("\u{1f3a4} {bar_str}"));
            } else if !peak_seen {
                level_label_ref.set_text("Waiting for audio...");
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
            if let Some(ref h) = h { h.dismiss("Microphone works"); }
            this.chat_view.add_message("user", "Microphone works!");
            this.advance(SetupStep::ChooseProvider);
        });

        // "Skip" button
        let this = self.clone();
        let mic_active_for_skip = mic_active.clone();
        let flag_for_skip = active_flag.clone();
        skip_btn.connect_clicked(move |_| {
            mic_active_for_skip.set(false);
            flag_for_skip.store(false, std::sync::atomic::Ordering::Relaxed);
            if let Some(ref h) = handle { h.dismiss("Skipped mic test"); }
            this.chat_view.add_message("user", "Skip mic test");
            this.chat_view.add_message("system",
                "Mic test skipped. You can configure voice input later in Settings.");
            this.advance(SetupStep::ChooseProvider);
        });

        self.speak(
            "Now let's test your microphone. Please say something.",
        );
    }

    // -- Step 2: Choose Provider --------------------------------------------

    fn show_choose_provider(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Claude button.
        let claude_btn = Self::make_provider_button(
            "Claude (Anthropic)",
            "Advanced reasoning and analysis, strong at coding tasks",
        );

        input_box.append(&claude_btn);

        // ChatGPT button.
        let openai_btn = Self::make_provider_button(
            "ChatGPT (OpenAI)",
            "GPT-4o with broad general knowledge and tool use",
        );
        input_box.append(&openai_btn);

        let handle = self.chat_view.add_setup_card(
            "network-server-symbolic",
            "Choose Your AI Provider",
            "Which AI would you like to use as your primary assistant?",
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        claude_btn.connect_clicked(move |_| {
            if let Some(ref h) = h { h.dismiss("Claude (Anthropic)"); }
            this.select_provider("claude");
        });

        let this = self.clone();
        openai_btn.connect_clicked(move |_| {
            if let Some(ref h) = handle { h.dismiss("ChatGPT (OpenAI)"); }
            this.select_provider("openai");
        });

        self.speak(
            "Which AI provider would you like to use? \
             You can say Claude or ChatGPT.",
        );
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
            "claude" => "Claude (Anthropic)",
            "openai" => "ChatGPT (OpenAI)",
            other => other,
        };
        self.chat_view.add_message("user", display);

        let next = SetupStep::EnterApiKey {
            provider: provider.to_owned(),
        };
        self.advance_with_choice(next, Some(display));
    }

    // -- Step 3 / Step 7: Enter API Key ------------------------------------

    fn show_enter_api_key(&self, provider: String, is_backup: bool) {
        let (title, tutorial) = match provider.as_str() {
            "claude" => (
                "Enter Your Claude API Key".to_string(),
                "How to get your key:\n\
                 1. Go to console.anthropic.com\n\
                 2. Sign in or create an account\n\
                 3. Go to Settings \u{2192} API Keys\n\
                 4. Click \"Create Key\" and copy it\n\
                 \n\
                 The key starts with sk-ant-...".to_string(),
            ),
            "openai" => (
                "Enter Your ChatGPT API Key".to_string(),
                "How to get your key:\n\
                 1. Go to platform.openai.com\n\
                 2. Sign in or create an account\n\
                 3. Go to API Keys in the sidebar\n\
                 4. Click \"Create new secret key\" and copy it\n\
                 \n\
                 The key starts with sk-...".to_string(),
            ),
            _ => (
                format!("Enter Your {} API Key", provider),
                "Visit the provider's developer dashboard to create an API key.".to_string(),
            ),
        };

        let input_box = gtk::Box::new(Orientation::Vertical, 6);
        input_box.set_margin_top(8);

        let entry = gtk::PasswordEntry::builder()
            .placeholder_text("Paste your API key here")
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

        let next_btn = gtk::Button::with_label("Next \u{2192}");
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
                error_ref.set_text("Please enter an API key");
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
                error_ref2.set_text("Please enter an API key");
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

        self.speak("Please type or paste your API key.");

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
            "****".to_string()
        };
        self.chat_view
            .add_message("user", &format!("API Key: {masked}"));

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
            .placeholder_text("Master password (min. 8 characters)")
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        entry.add_css_class("setup-input");
        input_box.append(&entry);

        // Strength indicator.
        let strength_box = gtk::Box::new(Orientation::Horizontal, 6);
        strength_box.set_margin_top(2);
        let strength_label = gtk::Label::new(Some("Strength:"));
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

        let next_btn = gtk::Button::with_label("Next \u{2192}");
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
                    error_ref.set_text("Password must be at least 8 characters");
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
                    error_ref.set_text("Password must be at least 8 characters");
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
            "Secure Your Data",
            "Create a master password to protect your API keys and personal data.\n\
             Minimum 8 characters.",
            Some(input_box.upcast_ref()),
        );

        self.speak("Now let's secure your data. Please type a master password.");

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
            .placeholder_text("Confirm your password")
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

        let next_btn = gtk::Button::with_label("Next \u{2192}");
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
                    error_ref.set_text("Passwords do not match");
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
                    error_ref.set_text("Passwords do not match");
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
            "Confirm Password",
            "Type your password again to confirm.",
            Some(input_box.upcast_ref()),
        );

        self.speak("Please type your password again to confirm.");

        let entry_focus = entry.clone();
        gtk4::glib::idle_add_local_once(move || {
            entry_focus.grab_focus();
        });
    }

    // -- Step 6: Add Backup Provider? --------------------------------------

    fn show_add_backup(&self) {
        let primary = self.state.borrow().primary_provider.clone();
        let other_name = if primary == "claude" {
            "ChatGPT (OpenAI)"
        } else {
            "Claude"
        };

        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let yes_btn = gtk::Button::with_label(&format!("Yes, add {other_name}"));
        yes_btn.add_css_class("suggested-action");
        yes_btn.set_halign(Align::Start);

        let this = self.clone();
        yes_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.handle_add_backup(true);
        });
        input_box.append(&yes_btn);

        let no_btn = gtk::Button::with_label("No, I'm good");
        no_btn.set_halign(Align::Start);

        let this = self.clone();
        no_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            this.handle_add_backup(false);
        });
        input_box.append(&no_btn);

        self.chat_view.add_setup_card(
            "list-add-symbolic",
            "Add a Backup Provider?",
            "If your primary AI is unavailable, a backup can take over automatically.",
            Some(input_box.upcast_ref()),
        );

        self.speak("Would you like to add a backup AI provider?");
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

            self.chat_view.add_message("user", &format!(
                "Yes, add {}",
                if backup == "claude" { "Claude" } else { "ChatGPT (OpenAI)" }
            ));

            let display = if backup == "claude" { "Claude" } else { "ChatGPT (OpenAI)" };
            self.advance_with_choice(SetupStep::EnterBackupKey {
                provider: backup.to_owned(),
            }, Some(&format!("Add {display}")));
        } else {
            self.chat_view.add_message("user", "No, I'm good");
            self.advance_with_choice(SetupStep::Complete, Some("No backup provider"));
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
                "claude" => "Claude (Anthropic)",
                "openai" => "ChatGPT (OpenAI)",
                other => other,
            };

            let btn = gtk::Button::with_label(&format!("{display} as primary"));
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
            "Choose Primary Provider",
            "Which provider should be your primary AI? The other will be used as fallback.",
            Some(input_box.upcast_ref()),
        );

        self.speak(
            "Which provider should be your primary AI? \
             Say Claude or ChatGPT.",
        );
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
            "claude" => "Claude (Anthropic)",
            "openai" => "ChatGPT (OpenAI)",
            other => other,
        };
        self.chat_view
            .add_message("user", &format!("{display} as primary"));

        self.advance(SetupStep::Complete);
    }

    // -- Step: Complete -----------------------------------------------------

    fn show_complete(&self) {
        let s = self.state.borrow();

        let mut summary_lines = Vec::new();
        for (i, p) in s.providers.iter().enumerate() {
            let role = if i == 0 { "Primary" } else { "Backup" };
            let display = match p.name.as_str() {
                "claude" => "Claude (Anthropic)",
                "openai" => "ChatGPT (OpenAI)",
                other => other,
            };
            summary_lines.push(format!("\u{2713} {display} as {}", role.to_lowercase()));
        }
        summary_lines.push("\u{2713} Secure vault created".to_string());

        let description = format!(
            "Your AI is configured and your data is secured.\n{}",
            summary_lines.join("\n")
        );
        drop(s);

        let btn = gtk::Button::with_label("Start Chatting \u{2192}");
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
            "You're All Set!",
            &description,
            Some(btn.upcast_ref()),
        );

        self.speak(
            "You're all set! Your AI assistant is ready. \
             Just start talking or typing.",
        );
    }

    /// Invoke the completion callback with the accumulated setup result.
    fn finish(&self) {
        self.chat_view.dismiss_last_card_input();
        let s = self.state.borrow();
        let result = SetupResult {
            providers: s.providers.clone(),
            master_password: s.master_password.clone(),
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
