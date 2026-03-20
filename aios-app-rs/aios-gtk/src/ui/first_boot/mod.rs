//! Conversational first-boot setup flow.
//!
//! Instead of a separate wizard dialog, [`SetupConversation`] drives a
//! state machine through the main [`ChatView`], presenting styled "setup
//! cards" with inline input widgets. The user progresses by clicking
//! buttons or (for future voice support) by speaking responses.

mod complete;
mod identity;
mod provider;
mod sentinel;
mod system;
mod welcome;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::{self as gtk};
use tracing::info;

use aios_core::config::ConfigManager;
use aios_core::i18n::t;

use super::chat_view::ChatView;

// ---------------------------------------------------------------------------
// Pretrained wake word catalog
// ---------------------------------------------------------------------------

/// Pretrained wake word models bundled with AiOS.
/// Each entry is `(model_id, display_name)`.
pub(crate) const PRETRAINED_WAKE_WORDS: &[(&str, &str)] = &[
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
    /// Selected Sentinel model (e.g., "llama3.2:3b"). Required.
    pub sentinel_model: String,
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
    SentinelModel,
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
    /// Custom wake word (if use_same_name is false and no pretrained selected).
    wake_word_custom: String,
    /// Pretrained wake word model ID selected from the dropdown.
    /// Empty string means the user wants a custom/trained wake word.
    wake_word_pretrained_id: String,
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
    /// Selected Sentinel model (e.g., "llama3.2:3b").
    sentinel_model: String,
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
            wake_word_custom: String::new(),
            wake_word_pretrained_id: PRETRAINED_WAKE_WORDS[0].0.to_string(),
            machine_name_custom: t("setup.name.default_lowercase"),
            country: None,
            install_to_drive: false,
            target_drive: None,
            partition_plan: None,
            sentinel_model: String::new(),
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
                    &aios_core::i18n::t_fmt("setup.audio_input.voice_works", &[("text", text)]));
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
                // Wake word defaults to the first pretrained option when set via voice.
                let name = text.trim().to_string();
                if !name.is_empty() {
                    let machine = name.to_lowercase().replace(' ', "-");
                    {
                        let mut s = self.state.borrow_mut();
                        s.assistant_name = name.clone();
                        s.wake_word_pretrained_id = PRETRAINED_WAKE_WORDS[0].0.to_string();
                        s.wake_word_custom = String::new();
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
            SetupStep::SentinelModel => {
                // Sentinel model selection uses dropdown — ignore voice.
                info!("Voice input ignored for Sentinel model selection step");
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
            SetupStep::SentinelModel => self.show_sentinel_model(),
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
            let home_dir = std::env::var("HOME").unwrap_or_else(|_| String::from("/home/aios"));
            let piper_model = format!("{home_dir}/.aios/models/piper/en_US-amy-medium.onnx");
            if std::path::Path::new(&piper_model).exists() {
                if let Ok(mut child) = std::process::Command::new("piper")
                    .args(["--model", &piper_model, "--output_raw"])
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
                if let Ok(wav) = welcome::generate_test_beep() {
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
