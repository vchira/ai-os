//! Slash-command parser and handler.
//!
//! Ported from the Python `aios/config/commands.py` module. Because the Rust
//! handler cannot hold mutable references to external managers the way Python
//! does, actions that need external state (e.g. setting an API key on the LLM
//! manager) are expressed as [`CommandResult`] variants that the caller can
//! match on and forward to the appropriate subsystem.

use serde_json::json;

use super::ConfigManager;

// ---------------------------------------------------------------------------
// CommandInfo — metadata for autocomplete
// ---------------------------------------------------------------------------

/// Command metadata for autocomplete menus.
pub struct CommandInfo {
    pub command: &'static str,
    pub description: &'static str,
}

/// Return all available commands with short descriptions.
pub fn command_list() -> Vec<CommandInfo> {
    vec![
        CommandInfo { command: "/help", description: "Show available commands" },
        CommandInfo { command: "/key", description: "Set API key for a provider" },
        CommandInfo { command: "/provider", description: "Switch LLM provider" },
        CommandInfo { command: "/model", description: "Set model for current provider" },
        CommandInfo { command: "/effort", description: "Set AI effort level" },
        CommandInfo { command: "/mode", description: "Set quality/cost mode" },
        CommandInfo { command: "/keyboard", description: "Set keyboard layout" },
        CommandInfo { command: "/resolution", description: "Set screen resolution" },
        CommandInfo { command: "/theme", description: "Set UI theme" },
        CommandInfo { command: "/voice", description: "Set TTS voice" },
        CommandInfo { command: "/mic", description: "Toggle voice input" },
        CommandInfo { command: "/speaker", description: "Toggle voice output" },
        CommandInfo { command: "/language", description: "Set STT language" },
        CommandInfo { command: "/tools", description: "List available tools" },
        CommandInfo { command: "/channel", description: "Channel settings" },
        CommandInfo { command: "/selftest", description: "Run self-tests" },
        CommandInfo { command: "/info", description: "Show system information" },
        CommandInfo { command: "/sysinfo", description: "Show system monitor" },
        CommandInfo { command: "/close", description: "Close topmost panel/dialog" },
        CommandInfo { command: "/update", description: "Self-update from URL" },
        CommandInfo { command: "/configure", description: "Open settings dialog" },
        CommandInfo { command: "/wake", description: "Set wake word phrase" },
        CommandInfo { command: "/clear", description: "Clear chat history" },
    ]
}

// ---------------------------------------------------------------------------
// CommandResult
// ---------------------------------------------------------------------------

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
        CommandResult::Response(
            "\
Available commands:

/key <provider> <api-key>   Set API key (e.g., /key claude sk-ant-...)
/provider <name>            Switch LLM provider (claude, openai)
/model <name>               Set the model for current provider
/keyboard <layout> [var]    Set keyboard layout (e.g., /keyboard de)
/resolution <WxH>           Set screen resolution (e.g., /resolution 1920x1080)
/theme <dark|light|auto>    Set UI theme
/voice <voice-id>           Set TTS voice (e.g., /voice en_US-ryan-medium)
/mic <on|off>               Enable/disable voice input
/speaker <on|off>           Enable/disable voice output
/language <code>            Set STT language (blank=auto)
/wake <phrase>              Set wake word (e.g., /wake hey aios)
/tools                      List available tools
/effort <level>             Set AI effort (low, medium, high, auto)
/mode <mode>                Set quality mode (saver, balanced, thorough)
/channel                    Show active channel / channel settings
/selftest [filter]          Run self-tests (quick, channel, tools, interactive)
/sysinfo                    Show system monitor (CPU, memory, disk, processes)
/info                       Show system information
/close                      Close the topmost panel or dialog
/update <url>               Self-update AiOS binary from URL
/configure                  Open settings dialog
/clear                      Clear chat history
/help                       Show this help"
                .to_string(),
        )
    }

    fn cmd_key(&mut self, args: &str) -> CommandResult {
        let parts: Vec<&str> = args.trim().splitn(2, char::is_whitespace).collect();
        if parts.len() != 2 {
            return CommandResult::Response(
                "Usage: /key <provider> <api-key>\nExample: /key claude sk-ant-...".into(),
            );
        }

        let provider = parts[0].to_lowercase();
        let key = parts[1];

        if key.is_empty() {
            return CommandResult::Response("API key cannot be empty.".to_string());
        }

        match provider.as_str() {
            "claude" => {
                let _ = self.config.set("llm.claude_api_key", json!(key));
                let preview = &key[..key.len().min(12)];
                CommandResult::Response(format!("Claude API key set ({preview}...)"))
            }
            "openai" => {
                let _ = self.config.set("llm.openai_api_key", json!(key));
                let preview = &key[..key.len().min(12)];
                CommandResult::Response(format!("OpenAI API key set ({preview}...)"))
            }
            _ => CommandResult::Response(format!(
                "Unknown provider: {provider}. Supported: claude, openai"
            )),
        }
    }

    fn cmd_provider(&mut self, args: &str) -> CommandResult {
        let name = args.trim().to_lowercase();
        if name != "claude" && name != "openai" {
            return CommandResult::Response("Usage: /provider <claude|openai>".into());
        }
        let _ = self.config.set("llm.provider", json!(name));
        CommandResult::Response(format!("Switched to {name}"))
    }

    fn cmd_model(&mut self, args: &str) -> CommandResult {
        let model = args.trim();
        if model.is_empty() {
            return CommandResult::Response(
                "Usage: /model <model-name>\nExamples: claude-sonnet-4-20250514, gpt-4o".into(),
            );
        }
        let provider = self.config.get_str("llm.provider", "claude");
        let key = format!("llm.{provider}_model");
        let _ = self.config.set(&key, json!(model));
        CommandResult::Response(format!("Model set to {model} for {provider}"))
    }

    fn cmd_keyboard(&mut self, args: &str) -> CommandResult {
        let parts: Vec<&str> = args.trim().split_whitespace().collect();
        if parts.is_empty() {
            return CommandResult::Response(
                "Usage: /keyboard <layout> [variant]\nExamples: /keyboard de, /keyboard us intl"
                    .into(),
            );
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
        CommandResult::Response(format!("Keyboard layout set to {layout}{suffix}"))
    }

    fn cmd_resolution(&self, args: &str) -> CommandResult {
        let res = args.trim();
        if res.is_empty() || !res.to_lowercase().contains('x') {
            return CommandResult::Response(
                "Usage: /resolution <WxH>\nExample: /resolution 1920x1080".into(),
            );
        }
        // Actual resolution change is handled by the GTK layer; we just
        // validate and return the intent.
        CommandResult::Response(format!("Resolution request: {res}"))
    }

    fn cmd_theme(&mut self, args: &str) -> CommandResult {
        let theme = args.trim().to_lowercase();
        if !matches!(theme.as_str(), "dark" | "light" | "auto") {
            return CommandResult::Response("Usage: /theme <dark|light|auto>".into());
        }
        let _ = self.config.set("ui.theme", json!(theme));
        CommandResult::Response(format!("Theme set to {theme}"))
    }

    fn cmd_voice(&mut self, args: &str) -> CommandResult {
        let voice_id = args.trim();
        if voice_id.is_empty() {
            return CommandResult::Response(
                "Usage: /voice <voice-id>\nExample: /voice en_US-ryan-medium".into(),
            );
        }
        let _ = self.config.set("voice.tts_voice", json!(voice_id));
        CommandResult::Response(format!("Voice set to {voice_id}"))
    }

    fn cmd_mic(&mut self, args: &str) -> CommandResult {
        let state = args.trim().to_lowercase();
        if state != "on" && state != "off" {
            return CommandResult::Response("Usage: /mic <on|off>".into());
        }
        let enabled = state == "on";
        let _ = self.config.set("voice.stt_enabled", json!(enabled));
        let label = if enabled { "enabled" } else { "disabled" };
        CommandResult::Response(format!("Voice input {label}"))
    }

    fn cmd_speaker(&mut self, args: &str) -> CommandResult {
        let state = args.trim().to_lowercase();
        if state != "on" && state != "off" {
            return CommandResult::Response("Usage: /speaker <on|off>".into());
        }
        let enabled = state == "on";
        let _ = self.config.set("voice.tts_enabled", json!(enabled));
        let label = if enabled { "enabled" } else { "disabled" };
        CommandResult::Response(format!("Voice output {label}"))
    }

    fn cmd_language(&mut self, args: &str) -> CommandResult {
        let lang = args.trim();
        let _ = self.config.set("voice.stt_language", json!(lang));
        if lang.is_empty() {
            CommandResult::Response("STT language set to auto-detect".into())
        } else {
            CommandResult::Response(format!("STT language set to {lang}"))
        }
    }

    fn cmd_wake(&mut self, args: &str) -> CommandResult {
        let phrase = args.trim();
        if phrase.is_empty() {
            let current = self.config.get_str("voice.wake_word", "Assistant");
            let enabled = self.config.get_bool("voice.wake_enabled", true);
            let status = if enabled { "enabled" } else { "disabled" };
            return CommandResult::Response(format!(
                "Wake word: \"{current}\" ({status})\n\
                 Usage: /wake <phrase>   Set wake word\n\
                 Usage: /wake off        Disable wake word detection\n\
                 Usage: /wake on         Enable wake word detection"
            ));
        }

        match phrase.to_lowercase().as_str() {
            "off" => {
                let _ = self.config.set("voice.wake_enabled", json!(false));
                CommandResult::Response("Wake word detection disabled".into())
            }
            "on" => {
                let _ = self.config.set("voice.wake_enabled", json!(true));
                let current = self.config.get_str("voice.wake_word", "Assistant");
                CommandResult::Response(format!(
                    "Wake word detection enabled (phrase: \"{current}\")"
                ))
            }
            _ => {
                let _ = self.config.set("voice.wake_word", json!(phrase));
                let _ = self.config.set("voice.wake_enabled", json!(true));
                CommandResult::Response(format!("Wake word set to \"{phrase}\""))
            }
        }
    }

    fn cmd_tools(&self) -> CommandResult {
        // Actual tool listing requires the tool registry which lives outside
        // aios-core.  Return a placeholder the caller can enrich.
        CommandResult::Response("Tool listing requires the tool registry.".into())
    }

    fn cmd_effort(&mut self, args: &str) -> CommandResult {
        let level = args.trim().to_lowercase();
        match level.as_str() {
            "low" | "medium" | "high" | "auto" => {
                let _ = self.config.set("llm.effort", json!(level));
                CommandResult::Response(format!("Effort level set to {level}"))
            }
            "" => {
                let current = self.config.get_str("llm.effort", "auto");
                CommandResult::Response(format!(
                    "Current effort level: {current}\n\
                     Usage: /effort <low|medium|high|auto>"
                ))
            }
            _ => CommandResult::Response(
                "Usage: /effort <low|medium|high|auto>\n\
                 - low: fast mode (smaller model, shorter output)\n\
                 - medium: balanced (default)\n\
                 - high: thorough mode (extended thinking)\n\
                 - auto: let the system decide based on message complexity"
                    .into(),
            ),
        }
    }

    fn cmd_mode(&mut self, args: &str) -> CommandResult {
        let mode = args.trim().to_lowercase();
        match mode.as_str() {
            "saver" | "balanced" | "thorough" => {
                let _ = self.config.set("llm.quality_mode", json!(mode));
                CommandResult::Response(format!("Quality mode set to {mode}"))
            }
            "" => {
                let current = self.config.get_str("llm.quality_mode", "balanced");
                CommandResult::Response(format!(
                    "Current quality mode: {current}\n\
                     Usage: /mode <saver|balanced|thorough>\n\
                     - saver: cheapest model, heuristic escalation\n\
                     - balanced: auto-detected effort, reliable escalation only\n\
                     - thorough: best model always, extended thinking"
                ))
            }
            _ => CommandResult::Response(
                "Usage: /mode <saver|balanced|thorough>\n\
                 - saver: cheapest model, heuristic escalation (saves money)\n\
                 - balanced: auto-detected effort, reliable escalation only (default)\n\
                 - thorough: best model always, extended thinking (best quality)"
                    .into(),
            ),
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

            let info = format!(
                "\
Channel Settings
========================================
Web:    {} (port {})
Signal: {} (phone: {})

Usage:
  /channel web on|off       Enable/disable web channel
  /channel signal on|off    Enable/disable Signal channel
  /channel web port <N>     Set web server port
  /channel signal phone <N> Set Signal phone number",
                if web_enabled { "enabled" } else { "disabled" },
                web_port,
                if signal_enabled { "enabled" } else { "disabled" },
                signal_phone,
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
                CommandResult::Response("Web channel enabled. Restart required.".to_string())
            }
            ("web", "off") => {
                let _ = self.config.set("channels.web.enabled", json!(false));
                CommandResult::Response("Web channel disabled. Restart required.".to_string())
            }
            ("web", "port") if !value.is_empty() => {
                match value.parse::<u16>() {
                    Ok(port) => {
                        let _ = self.config.set("channels.web.port", json!(port));
                        CommandResult::Response(format!("Web port set to {port}. Restart required."))
                    }
                    Err(_) => CommandResult::Response(format!("Invalid port number: {value}")),
                }
            }
            ("signal", "on") => {
                let _ = self.config.set("channels.signal.enabled", json!(true));
                CommandResult::Response("Signal channel enabled. Restart required.".to_string())
            }
            ("signal", "off") => {
                let _ = self.config.set("channels.signal.enabled", json!(false));
                CommandResult::Response("Signal channel disabled. Restart required.".to_string())
            }
            ("signal", "phone") if !value.is_empty() => {
                let _ = self.config.set("channels.signal.phone", json!(value));
                CommandResult::Response(format!("Signal phone set to {value}."))
            }
            _ => CommandResult::Response(
                "Usage: /channel web|signal on|off|port|phone [value]".to_string(),
            ),
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

        let info = format!(
            "\
AiOS System Information
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
