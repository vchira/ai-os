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
/tools                      List available tools
/info                       Show system information
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

    fn cmd_tools(&self) -> CommandResult {
        // Actual tool listing requires the tool registry which lives outside
        // aios-core.  Return a placeholder the caller can enrich.
        CommandResult::Response("Tool listing requires the tool registry.".into())
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
}
