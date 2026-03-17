//! Slash-command parser and handler.
//!
//! Ported from the Python `aios/config/commands.py` module. Because the Rust
//! handler cannot hold mutable references to external managers the way Python
//! does, actions that need external state (e.g. setting an API key on the LLM
//! manager) are expressed as [`CommandResult`] variants that the caller can
//! match on and forward to the appropriate subsystem.

use serde_json::json;

use super::ConfigManager;
use crate::i18n::{t, t_fmt};

// ---------------------------------------------------------------------------
// CommandInfo — metadata for autocomplete
// ---------------------------------------------------------------------------

/// Command metadata for autocomplete menus.
pub struct CommandInfo {
    pub command: &'static str,
    pub description: String,
}

/// Return all available commands with short descriptions.
pub fn command_list() -> Vec<CommandInfo> {
    vec![
        CommandInfo { command: "/help", description: t("cmd.help.desc") },
        CommandInfo { command: "/key", description: t("cmd.key.desc") },
        CommandInfo { command: "/provider", description: t("cmd.provider.desc") },
        CommandInfo { command: "/model", description: t("cmd.model.desc") },
        CommandInfo { command: "/effort", description: t("cmd.effort.desc") },
        CommandInfo { command: "/mode", description: t("cmd.mode.desc") },
        CommandInfo { command: "/keyboard", description: t("cmd.keyboard.desc") },
        CommandInfo { command: "/resolution", description: t("cmd.resolution.desc") },
        CommandInfo { command: "/theme", description: t("cmd.theme.desc") },
        CommandInfo { command: "/voice", description: t("cmd.voice.desc") },
        CommandInfo { command: "/mic", description: t("cmd.mic.desc") },
        CommandInfo { command: "/speaker", description: t("cmd.speaker.desc") },
        CommandInfo { command: "/language", description: t("cmd.language.desc") },
        CommandInfo { command: "/tools", description: t("cmd.tools.desc") },
        CommandInfo { command: "/channel", description: t("cmd.channel.desc") },
        CommandInfo { command: "/selftest", description: t("cmd.selftest.desc") },
        CommandInfo { command: "/info", description: t("cmd.info.desc") },
        CommandInfo { command: "/sysinfo", description: t("cmd.sysinfo.desc") },
        CommandInfo { command: "/close", description: t("cmd.close.desc") },
        CommandInfo { command: "/update", description: t("cmd.update.desc") },
        CommandInfo { command: "/configure", description: t("cmd.configure.desc") },
        CommandInfo { command: "/wake", description: t("cmd.wake.desc") },
        CommandInfo { command: "/clear", description: t("cmd.clear.desc") },
    ]
}

// ---------------------------------------------------------------------------
// CommandResult
// ---------------------------------------------------------------------------

/// A field in a command-generated settings panel.
#[derive(Debug, Clone)]
pub struct PanelField {
    /// Unique identifier for this field.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// The type of input widget.
    pub kind: PanelFieldKind,
}

/// Input widget types for command panels.
#[derive(Debug, Clone)]
pub enum PanelFieldKind {
    /// A dropdown / combo box with a list of options.
    Dropdown {
        options: Vec<String>,
        selected: Option<String>,
    },
}

/// Outcome of executing a slash command.
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// A plain text response to display to the user.
    Response(String),
    /// The user asked to clear the chat history.
    Clear,
    /// The user asked to open the settings dialog.
    Configure,
    /// The user asked to run the self-test.
    /// The string argument is the optional filter (e.g. "quick", "channel", "interactive").
    SelfTest(String),
    /// The user asked to see the system monitor (`/sysinfo`).
    SysInfo,
    /// The user asked to close the topmost panel/dialog (`/close`).
    ClosePanel,
    /// The user requested a self-update (`/update <url>`).
    Update(String),
    /// A settings panel with interactive fields (dropdown, toggle, etc.).
    ///
    /// The GTK handler should render this as a card in the chat view with
    /// the appropriate input widgets. The `config_key` is the config path
    /// to update when the user makes a selection.
    Panel {
        title: String,
        description: String,
        fields: Vec<PanelField>,
        /// The config key prefix to update when the user selects a value.
        config_key: String,
    },
    /// The command was not recognised.
    Unknown(String),
}

// ---------------------------------------------------------------------------
// CommandHandler
// ---------------------------------------------------------------------------

/// Handles `/commands` typed in the prompt.
///
/// The handler owns a *mutable* reference to the [`ConfigManager`] for
/// commands that change settings. Commands that need to talk to subsystems
/// the handler does not own (LLM manager, voice controller, tool registry)
/// return structured [`CommandResult`] variants so the caller can act.
pub struct CommandHandler<'a> {
    config: &'a mut ConfigManager,
}

impl<'a> CommandHandler<'a> {
    /// Create a new command handler backed by the given config manager.
    pub fn new(config: &'a mut ConfigManager) -> Self {
        Self { config }
    }

    /// Parse raw input and return `Some((command, args))` if it starts with
    /// `/`, or `None` if it is not a slash command.
    pub fn parse(input: &str) -> Option<(String, String)> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let cmd = parts.next()?.to_lowercase();
        let args = parts.next().unwrap_or("").to_string();
        Some((cmd, args))
    }

    /// Execute a slash command string and return the result.
    pub fn execute(&mut self, input: &str) -> CommandResult {
        let Some((cmd, args)) = Self::parse(input) else {
            return CommandResult::Unknown(input.to_string());
        };

        match cmd.as_str() {
            "/help" => self.cmd_help(),
            "/key" => self.cmd_key(&args),
            "/provider" => self.cmd_provider(&args),
            "/model" => self.cmd_model(&args),
            "/keyboard" => self.cmd_keyboard(&args),
            "/resolution" => self.cmd_resolution(&args),
            "/theme" => self.cmd_theme(&args),
            "/voice" => self.cmd_voice(&args),
            "/mic" => self.cmd_mic(&args),
            "/speaker" => self.cmd_speaker(&args),
            "/language" => self.cmd_language(&args),
            "/tools" => self.cmd_tools(),
            "/effort" => self.cmd_effort(&args),
            "/mode" => self.cmd_mode(&args),
            "/wake" => self.cmd_wake(&args),
            "/channel" => self.cmd_channel(&args),
            "/selftest" => CommandResult::SelfTest(args),
            "/sysinfo" => CommandResult::SysInfo,
            "/close" => CommandResult::ClosePanel,
            "/update" => CommandResult::Update(args),
            "/clear" => CommandResult::Clear,
            "/configure" => CommandResult::Configure,
            "/info" => self.cmd_info(),
            _ => CommandResult::Unknown(cmd),
        }
    }

    // -- individual commands ------------------------------------------------

    fn cmd_help(&self) -> CommandResult {
        let lines = [
            t("cmd.help.title"),
            String::new(),
            t("cmd.help.key"),
            t("cmd.help.provider"),
            t("cmd.help.model"),
            t("cmd.help.keyboard"),
            t("cmd.help.resolution"),
            t("cmd.help.theme"),
            t("cmd.help.voice"),
            t("cmd.help.mic"),
            t("cmd.help.speaker"),
            t("cmd.help.language"),
            t("cmd.help.wake"),
            t("cmd.help.tools"),
            t("cmd.help.effort"),
            t("cmd.help.mode"),
            t("cmd.help.channel"),
            t("cmd.help.selftest"),
            t("cmd.help.sysinfo"),
            t("cmd.help.info"),
            t("cmd.help.close"),
            t("cmd.help.update"),
            t("cmd.help.configure"),
            t("cmd.help.clear"),
            t("cmd.help.help"),
        ];
        CommandResult::Response(lines.join("\n"))
    }

    fn cmd_key(&mut self, args: &str) -> CommandResult {
        let parts: Vec<&str> = args.trim().splitn(2, char::is_whitespace).collect();
        if parts.len() != 2 {
            return CommandResult::Response(t("cmd.key.usage"));
        }

        let provider = parts[0].to_lowercase();
        let key = parts[1];

        if key.is_empty() {
            return CommandResult::Response(t("cmd.key.empty"));
        }

        match provider.as_str() {
            "claude" => {
                let _ = self.config.set("llm.claude_api_key", json!(key));
                let preview = &key[..key.len().min(12)];
                CommandResult::Response(t_fmt("cmd.key.claude_set", &[("preview", preview)]))
            }
            "openai" => {
                let _ = self.config.set("llm.openai_api_key", json!(key));
                let preview = &key[..key.len().min(12)];
                CommandResult::Response(t_fmt("cmd.key.openai_set", &[("preview", preview)]))
            }
            _ => CommandResult::Response(
                t_fmt("cmd.key.unknown_provider", &[("provider", &provider)]),
            ),
        }
    }

    fn cmd_provider(&mut self, args: &str) -> CommandResult {
        let name = args.trim().to_lowercase();
        if name != "claude" && name != "openai" {
            return CommandResult::Response(t("cmd.provider.usage"));
        }
        let _ = self.config.set("llm.provider", json!(name));
        CommandResult::Response(t_fmt("cmd.provider.switched", &[("name", &name)]))
    }

    fn cmd_model(&mut self, args: &str) -> CommandResult {
        let model = args.trim();
        if model.is_empty() {
            return CommandResult::Response(t("cmd.model.usage"));
        }
        let provider = self.config.get_str("llm.provider", "claude");
        let key = format!("llm.{provider}_model");
        let _ = self.config.set(&key, json!(model));
        CommandResult::Response(t_fmt("cmd.model.set", &[("model", model), ("provider", &provider)]))
    }

    fn cmd_keyboard(&mut self, args: &str) -> CommandResult {
        let parts: Vec<&str> = args.trim().split_whitespace().collect();
        if parts.is_empty() {
            let current = self.config.get_str("system.keyboard_layout", "us");
            return CommandResult::Panel {
                title: "Keyboard Layout".into(),
                description: "Select your keyboard layout".into(),
                fields: vec![PanelField {
                    id: "layout".into(),
                    label: "Layout".into(),
                    kind: PanelFieldKind::Dropdown {
                        options: vec![
                            "us".into(), "de".into(), "fr".into(), "es".into(),
                            "it".into(), "pt".into(), "gb".into(), "ro".into(),
                            "ru".into(), "jp".into(), "kr".into(), "br".into(),
                        ],
                        selected: Some(current),
                    },
                }],
                config_key: "system.keyboard_layout".into(),
            };
        }
        let layout = parts[0];
        let variant = parts.get(1).copied().unwrap_or("");
        let _ = self.config.set("system.keyboard_layout", json!(layout));
        let _ = self.config.set("system.keyboard_variant", json!(variant));
        let suffix = if variant.is_empty() {
            String::new()
        } else {
            format!(" ({variant})")
        };
        CommandResult::Response(t_fmt("cmd.keyboard.set", &[("layout", layout), ("suffix", &suffix)]))
    }

    fn cmd_resolution(&self, args: &str) -> CommandResult {
        let res = args.trim();
        if res.is_empty() {
            // Detect available resolutions via wlr-randr.
            let available = Self::detect_resolutions();
            if available.is_empty() {
                return CommandResult::Response(t("cmd.resolution.no_detect"));
            }
            let current = Self::detect_current_resolution();
            return CommandResult::Panel {
                title: "Screen Resolution".into(),
                description: format!(
                    "Current: {}",
                    current.as_deref().unwrap_or("unknown"),
                ),
                fields: vec![PanelField {
                    id: "resolution".into(),
                    label: "Resolution".into(),
                    kind: PanelFieldKind::Dropdown {
                        options: available,
                        selected: current,
                    },
                }],
                config_key: "system.resolution".into(),
            };
        }
        if !res.to_lowercase().contains('x') {
            return CommandResult::Response(t("cmd.resolution.invalid"));
        }
        // Apply resolution via wlr-randr.
        Self::apply_resolution(res);
        CommandResult::Response(t_fmt("cmd.resolution.set", &[("resolution", res)]))
    }

    /// Detect available resolutions using wlr-randr.
    fn detect_resolutions() -> Vec<String> {
        let output = std::process::Command::new("wlr-randr")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout).ok()
                } else {
                    None
                }
            });

        let Some(text) = output else {
            return Vec::new();
        };

        // Parse lines like "    1920x1080 px, 60.000000 Hz (preferred, current)"
        let mut resolutions = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            // Resolution lines start with a digit and contain "px"
            if trimmed.starts_with(|c: char| c.is_ascii_digit()) && trimmed.contains("px") {
                if let Some(res) = trimmed.split_whitespace().next() {
                    let res = res.trim_end_matches(',');
                    if !resolutions.contains(&res.to_string()) {
                        resolutions.push(res.to_string());
                    }
                }
            }
        }
        resolutions
    }

    /// Detect the current resolution from wlr-randr output.
    fn detect_current_resolution() -> Option<String> {
        let output = std::process::Command::new("wlr-randr")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())?;

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.contains("current") && trimmed.contains("px") {
                if let Some(res) = trimmed.split_whitespace().next() {
                    return Some(res.trim_end_matches(',').to_string());
                }
            }
        }
        None
    }

    /// Apply a resolution using wlr-randr.
    pub fn apply_resolution(resolution: &str) {
        // Find the output name first.
        let output_name = std::process::Command::new("wlr-randr")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|text| {
                // First non-indented line with a name like "HDMI-A-1" or "Virtual-1"
                text.lines()
                    .find(|l| !l.starts_with(' ') && !l.is_empty())
                    .and_then(|l| l.split_whitespace().next())
                    .map(|s| s.to_string())
            });

        if let Some(name) = output_name {
            let _ = std::process::Command::new("wlr-randr")
                .args(["--output", &name, "--mode", resolution])
                .status();
        }
    }

    fn cmd_theme(&mut self, args: &str) -> CommandResult {
        let theme = args.trim().to_lowercase();
        if theme.is_empty() {
            let current = self.config.get_str("ui.theme", "dark");
            return CommandResult::Panel {
                title: "UI Theme".into(),
                description: "Choose the appearance theme".into(),
                fields: vec![PanelField {
                    id: "theme".into(),
                    label: "Theme".into(),
                    kind: PanelFieldKind::Dropdown {
                        options: vec!["dark".into(), "light".into(), "auto".into()],
                        selected: Some(current),
                    },
                }],
                config_key: "ui.theme".into(),
            };
        }
        if !matches!(theme.as_str(), "dark" | "light" | "auto") {
            return CommandResult::Response(t("cmd.theme.usage"));
        }
        let _ = self.config.set("ui.theme", json!(theme));
        CommandResult::Response(t_fmt("cmd.theme.set", &[("theme", &theme)]))
    }

    fn cmd_voice(&mut self, args: &str) -> CommandResult {
        let voice_id = args.trim();
        if voice_id.is_empty() {
            return CommandResult::Response(t("cmd.voice.usage"));
        }
        let _ = self.config.set("voice.tts_voice", json!(voice_id));
        CommandResult::Response(t_fmt("cmd.voice.set", &[("voice", voice_id)]))
    }

    fn cmd_mic(&mut self, args: &str) -> CommandResult {
        let state = args.trim().to_lowercase();
        if state != "on" && state != "off" {
            return CommandResult::Response(t("cmd.mic.usage"));
        }
        let enabled = state == "on";
        let _ = self.config.set("voice.stt_enabled", json!(enabled));
        let key = if enabled { "cmd.mic.enabled" } else { "cmd.mic.disabled" };
        CommandResult::Response(t(key))
    }

    fn cmd_speaker(&mut self, args: &str) -> CommandResult {
        let state = args.trim().to_lowercase();
        if state != "on" && state != "off" {
            return CommandResult::Response(t("cmd.speaker.usage"));
        }
        let enabled = state == "on";
        let _ = self.config.set("voice.tts_enabled", json!(enabled));
        let key = if enabled { "cmd.speaker.enabled" } else { "cmd.speaker.disabled" };
        CommandResult::Response(t(key))
    }

    fn cmd_language(&mut self, args: &str) -> CommandResult {
        let lang = args.trim();
        let _ = self.config.set("voice.stt_language", json!(lang));
        if lang.is_empty() {
            CommandResult::Response(t("cmd.language.auto"))
        } else {
            CommandResult::Response(t_fmt("cmd.language.set", &[("lang", lang)]))
        }
    }

    fn cmd_wake(&mut self, args: &str) -> CommandResult {
        let phrase = args.trim();
        if phrase.is_empty() {
            let current = self.config.get_str("voice.wake_word", "Assistant");
            let enabled = self.config.get_bool("voice.wake_enabled", true);
            let status = if enabled { "enabled" } else { "disabled" };
            return CommandResult::Response(
                t_fmt("cmd.wake.status", &[("current", &current), ("status", status)]),
            );
        }

        match phrase.to_lowercase().as_str() {
            "off" => {
                let _ = self.config.set("voice.wake_enabled", json!(false));
                CommandResult::Response(t("cmd.wake.disabled"))
            }
            "on" => {
                let _ = self.config.set("voice.wake_enabled", json!(true));
                let current = self.config.get_str("voice.wake_word", "Assistant");
                CommandResult::Response(
                    t_fmt("cmd.wake.enabled", &[("current", &current)]),
                )
            }
            _ => {
                let _ = self.config.set("voice.wake_word", json!(phrase));
                let _ = self.config.set("voice.wake_enabled", json!(true));
                CommandResult::Response(t_fmt("cmd.wake.set", &[("phrase", phrase)]))
            }
        }
    }

    fn cmd_tools(&self) -> CommandResult {
        // Actual tool listing requires the tool registry which lives outside
        // aios-core.  Return a placeholder the caller can enrich.
        CommandResult::Response(t("cmd.tools.placeholder"))
    }

    fn cmd_effort(&mut self, args: &str) -> CommandResult {
        let level = args.trim().to_lowercase();
        match level.as_str() {
            "low" | "medium" | "high" | "auto" => {
                let _ = self.config.set("llm.effort", json!(level));
                CommandResult::Response(t_fmt("cmd.effort.set", &[("level", &level)]))
            }
            "" => {
                let current = self.config.get_str("llm.effort", "auto");
                CommandResult::Response(
                    t_fmt("cmd.effort.current", &[("current", &current)]),
                )
            }
            _ => CommandResult::Response(t("cmd.effort.usage")),
        }
    }

    fn cmd_mode(&mut self, args: &str) -> CommandResult {
        let mode = args.trim().to_lowercase();
        match mode.as_str() {
            "saver" | "balanced" | "thorough" => {
                let _ = self.config.set("llm.quality_mode", json!(mode));
                CommandResult::Response(t_fmt("cmd.mode.set", &[("mode", &mode)]))
            }
            "" => {
                let current = self.config.get_str("llm.quality_mode", "balanced");
                CommandResult::Response(
                    t_fmt("cmd.mode.current", &[("current", &current)]),
                )
            }
            _ => CommandResult::Response(t("cmd.mode.usage")),
        }
    }

    fn cmd_channel(&mut self, args: &str) -> CommandResult {
        let args = args.trim();

        if args.is_empty() {
            // Show current channel settings.
            let web_enabled = self.config.get_bool("channels.web.enabled", false);
            let web_port = self.config.get_str("channels.web.port", "80");
            let signal_enabled = self.config.get_bool("channels.signal.enabled", false);
            let signal_phone = self.config.get_str("channels.signal.phone", "(not set)");

            let title = t("cmd.channel.title");
            let web_status = if web_enabled { "enabled" } else { "disabled" };
            let signal_status = if signal_enabled { "enabled" } else { "disabled" };
            let info = format!(
                "\
{title}
========================================
Web:    {web_status} (port {web_port})
Signal: {signal_status} (phone: {signal_phone})

Usage:
  /channel web on|off       Enable/disable web channel
  /channel signal on|off    Enable/disable Signal channel
  /channel web port <N>     Set web server port
  /channel signal phone <N> Set Signal phone number",
            );
            return CommandResult::Response(info);
        }

        let mut parts = args.splitn(3, char::is_whitespace);
        let channel = parts.next().unwrap_or("");
        let action = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("").trim();

        match (channel, action) {
            ("web", "on") => {
                let _ = self.config.set("channels.web.enabled", json!(true));
                CommandResult::Response(t("cmd.channel.web_enabled"))
            }
            ("web", "off") => {
                let _ = self.config.set("channels.web.enabled", json!(false));
                CommandResult::Response(t("cmd.channel.web_disabled"))
            }
            ("web", "port") if !value.is_empty() => {
                match value.parse::<u16>() {
                    Ok(port) => {
                        let _ = self.config.set("channels.web.port", json!(port));
                        let port_str = port.to_string();
                        CommandResult::Response(t_fmt("cmd.channel.web_port_set", &[("port", &port_str)]))
                    }
                    Err(_) => CommandResult::Response(
                        t_fmt("cmd.channel.web_port_invalid", &[("value", value)]),
                    ),
                }
            }
            ("signal", "on") => {
                let _ = self.config.set("channels.signal.enabled", json!(true));
                CommandResult::Response(t("cmd.channel.signal_enabled"))
            }
            ("signal", "off") => {
                let _ = self.config.set("channels.signal.enabled", json!(false));
                CommandResult::Response(t("cmd.channel.signal_disabled"))
            }
            ("signal", "phone") if !value.is_empty() => {
                let _ = self.config.set("channels.signal.phone", json!(value));
                CommandResult::Response(t_fmt("cmd.channel.signal_phone_set", &[("value", value)]))
            }
            _ => CommandResult::Response(t("cmd.channel.usage")),
        }
    }

    fn cmd_info(&self) -> CommandResult {
        let provider = self.config.get_str("llm.provider", "claude");
        let stt = if self.config.get_bool("voice.stt_enabled", true) {
            "enabled"
        } else {
            "disabled"
        };
        let tts = if self.config.get_bool("voice.tts_enabled", true) {
            "enabled"
        } else {
            "disabled"
        };
        let voice = self.config.get_str("voice.tts_voice", "default");
        let theme = self.config.get_str("ui.theme", "dark");
        let keyboard = self.config.get_str("system.keyboard_layout", "us");

        let title = t("cmd.info.title");
        let info = format!(
            "\
{title}
========================================
AiOS Version: 2.0.0
Provider: {provider}
STT: {stt}
TTS: {tts}
Voice: {voice}
Theme: {theme}
Keyboard: {keyboard}"
        );
        CommandResult::Response(info)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a ConfigManager backed by a temp dir.
    fn temp_config() -> (tempfile::TempDir, ConfigManager) {
        crate::i18n::init();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mgr = ConfigManager::with_path(path).unwrap();
        (dir, mgr)
    }

    #[test]
    fn parse_non_command() {
        assert!(CommandHandler::parse("hello world").is_none());
    }

    #[test]
    fn parse_simple_command() {
        let (cmd, args) = CommandHandler::parse("/help").unwrap();
        assert_eq!(cmd, "/help");
        assert!(args.is_empty());
    }

    #[test]
    fn parse_command_with_args() {
        let (cmd, args) = CommandHandler::parse("/key claude sk-ant-123").unwrap();
        assert_eq!(cmd, "/key");
        assert_eq!(args, "claude sk-ant-123");
    }

    #[test]
    fn execute_help() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/help");
        match result {
            CommandResult::Response(text) => assert!(text.contains("/key")),
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn execute_clear() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        assert!(matches!(handler.execute("/clear"), CommandResult::Clear));
    }

    #[test]
    fn execute_configure() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        assert!(matches!(
            handler.execute("/configure"),
            CommandResult::Configure
        ));
    }

    #[test]
    fn execute_unknown() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        assert!(matches!(
            handler.execute("/notacommand"),
            CommandResult::Unknown(_)
        ));
    }

    #[test]
    fn execute_theme() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/theme light");
        match result {
            CommandResult::Response(text) => assert!(text.contains("light")),
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("ui.theme", ""), "light");
    }

    #[test]
    fn execute_provider() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/provider openai");
        match result {
            CommandResult::Response(text) => assert!(text.contains("openai")),
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("llm.provider", ""), "openai");
    }

    #[test]
    fn execute_key_claude() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/key claude sk-ant-test123456");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Claude API key set"));
                assert!(text.contains("sk-ant-test1"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(
            cfg.get_str("llm.claude_api_key", ""),
            "sk-ant-test123456"
        );
    }

    #[test]
    fn execute_mic_on_off() {
        let (_dir, mut cfg) = temp_config();
        {
            let mut handler = CommandHandler::new(&mut cfg);
            handler.execute("/mic off");
        }
        assert!(!cfg.get_bool("voice.stt_enabled", true));
        {
            let mut handler = CommandHandler::new(&mut cfg);
            handler.execute("/mic on");
        }
        assert!(cfg.get_bool("voice.stt_enabled", false));
    }

    #[test]
    fn execute_keyboard_with_variant() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/keyboard us intl");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("us"));
                assert!(text.contains("intl"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("system.keyboard_layout", ""), "us");
        assert_eq!(cfg.get_str("system.keyboard_variant", ""), "intl");
    }

    #[test]
    fn execute_effort_set_low() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/effort low");
        match result {
            CommandResult::Response(text) => assert!(text.contains("low")),
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("llm.effort", ""), "low");
    }

    #[test]
    fn execute_effort_set_auto() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/effort auto");
        match result {
            CommandResult::Response(text) => assert!(text.contains("auto")),
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("llm.effort", ""), "auto");
    }

    #[test]
    fn execute_effort_show_current() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/effort");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Current effort level"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn execute_effort_invalid() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/effort extreme");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Usage"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn execute_info() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/info");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("AiOS"));
                assert!(text.contains("2.0.0"));
                assert!(text.contains("claude"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    // -- /mode command tests --------------------------------------------------

    #[test]
    fn execute_mode_set_saver() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/mode saver");
        match result {
            CommandResult::Response(text) => assert!(text.contains("saver")),
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("llm.quality_mode", ""), "saver");
    }

    #[test]
    fn execute_mode_set_balanced() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/mode balanced");
        match result {
            CommandResult::Response(text) => assert!(text.contains("balanced")),
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("llm.quality_mode", ""), "balanced");
    }

    #[test]
    fn execute_mode_set_thorough() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/mode thorough");
        match result {
            CommandResult::Response(text) => assert!(text.contains("thorough")),
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("llm.quality_mode", ""), "thorough");
    }

    #[test]
    fn execute_mode_show_current() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/mode");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Current quality mode"));
                assert!(text.contains("balanced"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn execute_mode_invalid() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/mode turbo");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Usage"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    // -- Comprehensive command tests --

    #[test]
    fn help_contains_all_command_names() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/help");
        match result {
            CommandResult::Response(text) => {
                let expected = [
                    "/key", "/provider", "/model", "/keyboard", "/resolution",
                    "/theme", "/voice", "/mic", "/speaker", "/language", "/wake",
                    "/tools", "/effort", "/mode", "/channel", "/selftest",
                    "/sysinfo", "/info", "/close", "/update", "/configure", "/clear",
                    "/help",
                ];
                for cmd in &expected {
                    assert!(text.contains(cmd), "help text missing {cmd}");
                }
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn key_with_empty_args_returns_usage() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/key");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Usage"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn key_with_valid_provider_stores_it() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/key openai sk-test-key-12345");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("OpenAI API key set"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("llm.openai_api_key", ""), "sk-test-key-12345");
    }

    #[test]
    fn key_with_empty_key_returns_usage() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/key claude");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Usage"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn provider_with_unknown_name() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/provider gemini");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Usage"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn channel_shows_status() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/channel");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Channel Settings"));
                assert!(text.contains("Web:"));
                assert!(text.contains("Signal:"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn channel_web_on_enables() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/channel web on");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("enabled"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
        assert!(cfg.get_bool("channels.web.enabled", false));
    }

    #[test]
    fn channel_web_off_disables() {
        let (_dir, mut cfg) = temp_config();
        {
            let mut handler = CommandHandler::new(&mut cfg);
            handler.execute("/channel web on");
        }
        assert!(cfg.get_bool("channels.web.enabled", false));
        {
            let mut handler = CommandHandler::new(&mut cfg);
            let result = handler.execute("/channel web off");
            match result {
                CommandResult::Response(text) => {
                    assert!(text.contains("disabled"));
                }
                other => panic!("expected Response, got {other:?}"),
            }
        }
        assert!(!cfg.get_bool("channels.web.enabled", true));
    }

    #[test]
    fn channel_signal_phone_sets_phone() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/channel signal phone +1234567890");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("+1234567890"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("channels.signal.phone", ""), "+1234567890");
    }

    #[test]
    fn wake_shows_current_wake_word() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/wake");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Wake word:"));
                assert!(text.contains("Assistant"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn wake_sets_new_wake_word() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/wake ok computer");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("ok computer"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("voice.wake_word", ""), "ok computer");
    }

    #[test]
    fn wake_off_disables() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/wake off");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("disabled"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
        assert!(!cfg.get_bool("voice.wake_enabled", true));
    }

    #[test]
    fn wake_on_enables() {
        let (_dir, mut cfg) = temp_config();
        {
            let mut handler = CommandHandler::new(&mut cfg);
            handler.execute("/wake off");
        }
        {
            let mut handler = CommandHandler::new(&mut cfg);
            let result = handler.execute("/wake on");
            match result {
                CommandResult::Response(text) => {
                    assert!(text.contains("enabled"));
                }
                other => panic!("expected Response, got {other:?}"),
            }
        }
        assert!(cfg.get_bool("voice.wake_enabled", false));
    }

    #[test]
    fn selftest_returns_selftest_variant() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/selftest") {
            CommandResult::SelfTest(filter) => assert!(filter.is_empty()),
            other => panic!("expected SelfTest, got {other:?}"),
        }
    }

    #[test]
    fn selftest_with_filter() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/selftest quick") {
            CommandResult::SelfTest(filter) => assert_eq!(filter, "quick"),
            other => panic!("expected SelfTest, got {other:?}"),
        }
    }

    #[test]
    fn sysinfo_returns_sysinfo_variant() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        assert!(matches!(handler.execute("/sysinfo"), CommandResult::SysInfo));
    }

    #[test]
    fn close_returns_closepanel_variant() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        assert!(matches!(handler.execute("/close"), CommandResult::ClosePanel));
    }

    #[test]
    fn update_returns_update_variant() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/update") {
            CommandResult::Update(url) => assert!(url.is_empty()),
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn update_with_url() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/update https://example.com/aios.bin") {
            CommandResult::Update(url) => assert_eq!(url, "https://example.com/aios.bin"),
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn unknown_command_returns_unknown() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/foobar") {
            CommandResult::Unknown(cmd) => assert_eq!(cmd, "/foobar"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn parse_command_case_insensitive() {
        let (cmd, _) = CommandHandler::parse("/HELP").unwrap();
        assert_eq!(cmd, "/help");
    }

    #[test]
    fn parse_command_with_leading_space_not_command() {
        let result = CommandHandler::parse("/ help");
        assert!(result.is_some());
        let (cmd, _) = result.unwrap();
        assert_eq!(cmd, "/");
    }

    #[test]
    fn empty_input_is_not_command() {
        assert!(CommandHandler::parse("").is_none());
        assert!(CommandHandler::parse("   ").is_none());
        assert!(CommandHandler::parse("hello").is_none());
    }

    #[test]
    fn execute_speaker_on_off() {
        let (_dir, mut cfg) = temp_config();
        {
            let mut handler = CommandHandler::new(&mut cfg);
            handler.execute("/speaker off");
        }
        assert!(!cfg.get_bool("voice.tts_enabled", true));
        {
            let mut handler = CommandHandler::new(&mut cfg);
            handler.execute("/speaker on");
        }
        assert!(cfg.get_bool("voice.tts_enabled", false));
    }

    #[test]
    fn execute_language_set_and_auto() {
        let (_dir, mut cfg) = temp_config();
        {
            let mut handler = CommandHandler::new(&mut cfg);
            let result = handler.execute("/language ro");
            match result {
                CommandResult::Response(text) => assert!(text.contains("ro")),
                other => panic!("expected Response, got {other:?}"),
            }
        }
        assert_eq!(cfg.get_str("voice.stt_language", ""), "ro");

        {
            let mut handler = CommandHandler::new(&mut cfg);
            let result = handler.execute("/language");
            match result {
                CommandResult::Response(text) => assert!(text.contains("auto-detect")),
                other => panic!("expected Response, got {other:?}"),
            }
        }
    }

    #[test]
    fn execute_model_set() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/model gpt-4o-mini");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("gpt-4o-mini"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn execute_model_empty_args() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/model");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Usage"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn execute_voice_set() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/voice en_US-ryan-medium");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("en_US-ryan-medium"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
        assert_eq!(cfg.get_str("voice.tts_voice", ""), "en_US-ryan-medium");
    }

    #[test]
    fn execute_resolution_valid() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/resolution 1920x1080");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("1920x1080"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn execute_resolution_invalid() {
        let (_dir, mut cfg) = temp_config();
        let mut handler = CommandHandler::new(&mut cfg);
        let result = handler.execute("/resolution blah");
        match result {
            CommandResult::Response(text) => {
                assert!(text.contains("Usage"));
            }
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn command_list_returns_all_commands() {
        let list = command_list();
        assert!(list.len() >= 20, "expected at least 20 commands, got {}", list.len());
        let names: Vec<&str> = list.iter().map(|c| c.command).collect();
        assert!(names.contains(&"/help"));
        assert!(names.contains(&"/key"));
        assert!(names.contains(&"/channel"));
        assert!(names.contains(&"/selftest"));
        assert!(names.contains(&"/wake"));
    }
}
