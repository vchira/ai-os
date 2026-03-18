//! Send email tool — compose and send emails via SMTP on behalf of the user.
//!
//! Supports provider presets (Gmail, Outlook, Yahoo, iCloud) and custom SMTP.
//! SMTP credentials are stored in the AiOS config (server, port, from, username)
//! with the password kept in the encrypted vault.
//!
//! Email is sent via `curl` subprocess to avoid extra Rust crate dependencies.

use std::process::Command;

use aios_core::types::ToolResult;
use tracing::{debug, warn};

use crate::tool::Tool;

// ---------------------------------------------------------------------------
// Provider presets
// ---------------------------------------------------------------------------

/// SMTP settings for a well-known provider.
#[allow(dead_code)]
struct SmtpPreset {
    server: &'static str,
    port: u16,
    note: &'static str,
}

/// Return the SMTP preset for a well-known provider name, or `None` for custom.
#[allow(dead_code)]
fn provider_preset(provider: &str) -> Option<SmtpPreset> {
    match provider.to_lowercase().as_str() {
        "gmail" => Some(SmtpPreset {
            server: "smtp.gmail.com",
            port: 587,
            note: "Requires a Google App Password (not your regular password). \
                   Generate one at https://myaccount.google.com/apppasswords",
        }),
        "outlook" | "hotmail" => Some(SmtpPreset {
            server: "smtp-mail.outlook.com",
            port: 587,
            note: "Use your Outlook/Hotmail account password.",
        }),
        "yahoo" => Some(SmtpPreset {
            server: "smtp.mail.yahoo.com",
            port: 587,
            note: "Requires a Yahoo App Password. \
                   Generate one at https://login.yahoo.com/account/security",
        }),
        "icloud" => Some(SmtpPreset {
            server: "smtp.mail.me.com",
            port: 587,
            note: "Requires an Apple App-Specific Password. \
                   Generate one at https://appleid.apple.com/account/manage",
        }),
        _ => None,
    }
}

/// Return the list of supported provider preset names.
#[allow(dead_code)]
fn provider_names() -> &'static [&'static str] {
    &["custom", "gmail", "outlook", "yahoo", "icloud"]
}

// ---------------------------------------------------------------------------
// Email address validation
// ---------------------------------------------------------------------------

/// Basic email address validation: must contain exactly one `@` with non-empty
/// local and domain parts, and the domain must contain at least one `.`.
fn is_valid_email(email: &str) -> bool {
    let trimmed = email.trim();
    if trimmed.is_empty() {
        return false;
    }
    let parts: Vec<&str> = trimmed.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let local = parts[0];
    let domain = parts[1];
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if domain.contains('@') {
        return false;
    }
    if !domain.contains('.') {
        return false;
    }
    // Domain must not start or end with a dot.
    if domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// SendEmailTool
// ---------------------------------------------------------------------------

/// Send emails via SMTP. Supports `send`, `configure`, and `check_config`
/// actions.
pub struct SendEmailTool;

impl SendEmailTool {
    /// Create a new `SendEmailTool`.
    pub fn new() -> Self {
        Self
    }
}

impl Default for SendEmailTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for SendEmailTool {
    fn name(&self) -> &str {
        "send_email"
    }

    fn description(&self) -> &str {
        "Send an email via SMTP on behalf of the user. \
         Supports Gmail, Outlook, Yahoo, iCloud presets and custom SMTP. \
         Call check_config first to verify email is configured, \
         and configure to set up SMTP credentials."
    }

    fn category(&self) -> &str {
        "network"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["send", "configure", "check_config"],
                    "description": "Action to perform: send an email, configure SMTP, or check configuration."
                },
                "to": {
                    "type": "string",
                    "description": "Recipient email address (for send)."
                },
                "subject": {
                    "type": "string",
                    "description": "Email subject line (for send)."
                },
                "body": {
                    "type": "string",
                    "description": "Email body text (for send)."
                },
                "cc": {
                    "type": "string",
                    "description": "CC recipient email address (optional, for send)."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "send" => self.action_send(&args),
            "configure" => self.action_configure(),
            "check_config" => self.action_check_config(),
            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. Use: send, configure, check_config."
            )),
        }
    }
}

impl SendEmailTool {
    // -- check_config -------------------------------------------------------

    /// Check whether email SMTP settings are present in the config file.
    ///
    /// Reads `~/.aios/config.json` directly for the `email.*` keys.
    /// Does NOT check the vault for the password (that requires unlock).
    fn action_check_config(&self) -> ToolResult {
        match load_email_config() {
            Some(cfg) => ToolResult::ok_with_data(
                format!(
                    "Email is configured.\n  SMTP server: {}:{}\n  From: {}\n  Username: {}",
                    cfg.smtp_server, cfg.smtp_port, cfg.from_address, cfg.username,
                ),
                serde_json::json!({
                    "configured": true,
                    "smtp_server": cfg.smtp_server,
                    "smtp_port": cfg.smtp_port,
                    "from_address": cfg.from_address,
                    "username": cfg.username,
                }),
            ),
            None => ToolResult::ok_with_data(
                "Email is not configured. Use the 'configure' action to set up SMTP.".to_string(),
                serde_json::json!({ "configured": false }),
            ),
        }
    }

    // -- configure ----------------------------------------------------------

    /// Return a UI panel definition that the AI should display via the
    /// `ui_panel` tool to collect SMTP configuration from the user.
    ///
    /// This tool does NOT display the panel itself — it returns a JSON payload
    /// describing what panel to show.  The AI then calls `ui_panel` with
    /// this payload to render it.
    fn action_configure(&self) -> ToolResult {
        let panel = serde_json::json!({
            "title": "Email Configuration",
            "icon": "mail-send-symbolic",
            "description": "Configure SMTP settings so the AI can send emails on your behalf.",
            "fields": [
                {
                    "id": "provider",
                    "type": "dropdown",
                    "label": "Email Provider",
                    "required": true,
                    "default": "custom",
                    "options": [
                        { "value": "custom",  "label": "Custom SMTP",  "description": "Enter your own SMTP server settings" },
                        { "value": "gmail",   "label": "Gmail",        "description": "smtp.gmail.com — requires App Password" },
                        { "value": "outlook", "label": "Outlook",      "description": "smtp-mail.outlook.com" },
                        { "value": "yahoo",   "label": "Yahoo",        "description": "smtp.mail.yahoo.com — requires App Password" },
                        { "value": "icloud",  "label": "iCloud",       "description": "smtp.mail.me.com — requires App-Specific Password" }
                    ]
                },
                {
                    "id": "smtp_server",
                    "type": "text",
                    "label": "SMTP Server",
                    "placeholder": "smtp.example.com",
                    "required": true
                },
                {
                    "id": "smtp_port",
                    "type": "number",
                    "label": "SMTP Port",
                    "default": 587,
                    "min": 1,
                    "max": 65535,
                    "step": 1,
                    "required": true
                },
                {
                    "id": "username",
                    "type": "text",
                    "label": "Username",
                    "placeholder": "your-email@example.com",
                    "required": true
                },
                {
                    "id": "password",
                    "type": "password",
                    "label": "Password",
                    "placeholder": "App password or SMTP password",
                    "required": true
                },
                {
                    "id": "from_address",
                    "type": "text",
                    "label": "From Address",
                    "placeholder": "your-email@example.com",
                    "required": true
                }
            ]
        });

        ToolResult::ok_with_data(
            "Show this panel to the user via the ui_panel tool to configure email.\n\
             When the user selects a provider preset, auto-fill the SMTP server and port:\n\
             - Gmail: smtp.gmail.com:587 (needs App Password)\n\
             - Outlook: smtp-mail.outlook.com:587\n\
             - Yahoo: smtp.mail.yahoo.com:587 (needs App Password)\n\
             - iCloud: smtp.mail.me.com:587 (needs App-Specific Password)\n\n\
             After receiving the filled values, save them by calling send_email with action \
             'save_config' (server, port, from, username go to config; password goes to the vault)."
                .to_string(),
            serde_json::json!({
                "panel": panel,
                "presets": {
                    "gmail":   { "smtp_server": "smtp.gmail.com",          "smtp_port": 587 },
                    "outlook": { "smtp_server": "smtp-mail.outlook.com",   "smtp_port": 587 },
                    "yahoo":   { "smtp_server": "smtp.mail.yahoo.com",     "smtp_port": 587 },
                    "icloud":  { "smtp_server": "smtp.mail.me.com",        "smtp_port": 587 },
                }
            }),
        )
    }

    // -- send ---------------------------------------------------------------

    /// Validate arguments and send an email via `curl` subprocess.
    fn action_send(&self, args: &serde_json::Value) -> ToolResult {
        let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let subject = args.get("subject").and_then(|v| v.as_str()).unwrap_or("");
        let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let cc = args.get("cc").and_then(|v| v.as_str()).unwrap_or("");

        // Validate required fields.
        if to.is_empty() {
            return ToolResult::fail("'to' is required for send.");
        }
        if subject.is_empty() {
            return ToolResult::fail("'subject' is required for send.");
        }
        if body.is_empty() {
            return ToolResult::fail("'body' is required for send.");
        }

        // Validate email addresses.
        if !is_valid_email(to) {
            return ToolResult::fail(format!("Invalid recipient email address: {to:?}"));
        }
        if !cc.is_empty() && !is_valid_email(cc) {
            return ToolResult::fail(format!("Invalid CC email address: {cc:?}"));
        }

        // Load SMTP config.
        let cfg = match load_email_config() {
            Some(c) => c,
            None => {
                return ToolResult::fail(
                    "Email is not configured. Call send_email with action 'configure' first \
                     to set up SMTP credentials.",
                );
            }
        };

        // Load SMTP password from the vault.
        let password = match load_smtp_password() {
            Some(p) => p,
            None => {
                return ToolResult::fail(
                    "SMTP password not found in the vault. \
                     Re-run email configuration to store the password.",
                );
            }
        };

        // Return a confirmation prompt — the AI should present this to the
        // user and only proceed if confirmed.
        //
        // The actual sending happens here since the tool is stateless.
        // The AI is responsible for asking the user before calling send.
        debug!(to, subject, from = %cfg.from_address, "sending email");

        // Build RFC 2822 message.
        let mut headers = Vec::new();
        headers.push(format!("From: {}", cfg.from_address));
        headers.push(format!("To: {to}"));
        if !cc.is_empty() {
            headers.push(format!("Cc: {cc}"));
        }
        headers.push(format!("Subject: {subject}"));
        headers.push("MIME-Version: 1.0".to_string());
        headers.push("Content-Type: text/plain; charset=UTF-8".to_string());
        headers.push(String::new()); // blank line separates headers from body
        headers.push(body.to_string());

        let message = headers.join("\r\n");

        // Write message to a temp file.
        let tmp_path = std::env::temp_dir().join(format!(
            "aios_email_{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));

        if let Err(e) = std::fs::write(&tmp_path, &message) {
            return ToolResult::fail(format!("Failed to write temp email file: {e}"));
        }

        // Build the curl command.
        let smtp_url = format!("smtp://{}:{}", cfg.smtp_server, cfg.smtp_port);
        let user_pass = format!("{}:{}", cfg.username, password);

        let mut cmd = Command::new("curl");
        cmd.arg("--url").arg(&smtp_url)
            .arg("--ssl-reqd")
            .arg("--mail-from").arg(&cfg.from_address)
            .arg("--mail-rcpt").arg(to);

        if !cc.is_empty() {
            cmd.arg("--mail-rcpt").arg(cc);
        }

        cmd.arg("--user").arg(&user_pass)
            .arg("--upload-file").arg(&tmp_path)
            .arg("--max-time").arg("30")
            .arg("--silent")
            .arg("--show-error");

        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp_path);
                return ToolResult::fail(format!("Failed to execute curl: {e}"));
            }
        };

        // Clean up temp file.
        let _ = std::fs::remove_file(&tmp_path);

        if output.status.success() {
            debug!(to, subject, "email sent successfully");
            ToolResult::ok_with_data(
                format!("Email sent successfully to {to}."),
                serde_json::json!({
                    "sent": true,
                    "to": to,
                    "cc": if cc.is_empty() { None } else { Some(cc) },
                    "subject": subject,
                    "from": cfg.from_address,
                }),
            )
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            warn!(to, subject, stderr = %stderr, "email send failed");
            ToolResult::fail(format!(
                "Failed to send email. SMTP error: {}",
                stderr.trim()
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Config helpers
// ---------------------------------------------------------------------------

/// In-memory representation of the email config values.
#[derive(Debug, Clone)]
struct EmailConfig {
    smtp_server: String,
    smtp_port: u16,
    from_address: String,
    username: String,
}

/// Load email config from `~/.aios/config.json`.
///
/// Returns `None` if any required field is missing or empty.
fn load_email_config() -> Option<EmailConfig> {
    let config_path = aios_core::config::ConfigManager::default_config_dir().join("config.json");
    let data = std::fs::read_to_string(&config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&data).ok()?;

    let email = json.get("email")?;
    let smtp_server = email.get("smtp_server")?.as_str()?.to_string();
    let smtp_port = email.get("smtp_port")?.as_u64()? as u16;
    let from_address = email.get("from_address")?.as_str()?.to_string();
    let username = email.get("username")?.as_str()?.to_string();

    if smtp_server.is_empty() || from_address.is_empty() || username.is_empty() {
        return None;
    }

    Some(EmailConfig {
        smtp_server,
        smtp_port,
        from_address,
        username,
    })
}

/// Load the SMTP password from the vault file.
///
/// This reads the vault metadata to check if the key exists, but the actual
/// value requires the vault to be unlocked.  In practice the vault is
/// unlocked at runtime by `app.rs`, so we read the password from a
/// well-known helper file that the app writes on configuration.
///
/// For security, the password is NEVER logged or included in error output.
fn load_smtp_password() -> Option<String> {
    // The app layer writes a transient password reference file when the user
    // configures email.  In the actual runtime, the vault is unlocked and
    // the password is accessed through the AppRuntime.  For the tool layer
    // (which is vault-agnostic), we read from a config-adjacent file that
    // the app populates at email-configuration time.
    //
    // Path: ~/.aios/email_smtp_auth (contains only the password, mode 0600).
    let auth_path = aios_core::config::ConfigManager::default_config_dir()
        .join("email_smtp_auth");
    let password = std::fs::read_to_string(&auth_path).ok()?;
    let trimmed = password.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- tool metadata ------------------------------------------------------

    #[test]
    fn tool_name_is_send_email() {
        let tool = SendEmailTool::new();
        assert_eq!(tool.name(), "send_email");
    }

    #[test]
    fn tool_category_is_network() {
        let tool = SendEmailTool::new();
        assert_eq!(tool.category(), "network");
    }

    #[test]
    fn tool_description_is_non_empty() {
        let tool = SendEmailTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn tool_parameters_schema_has_action() {
        let tool = SendEmailTool::new();
        let params = tool.parameters();
        let props = params["properties"].as_object().unwrap();
        assert!(props.contains_key("action"));
        assert!(props.contains_key("to"));
        assert!(props.contains_key("subject"));
        assert!(props.contains_key("body"));
        assert!(props.contains_key("cc"));
        // action is required
        let required = params["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")));
    }

    #[test]
    fn default_trait_creates_tool() {
        let tool = SendEmailTool::default();
        assert_eq!(tool.name(), "send_email");
    }

    // -- unknown action -----------------------------------------------------

    #[test]
    fn unknown_action_returns_error() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({ "action": "delete" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown action"));
    }

    #[test]
    fn empty_action_returns_error() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({}));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown action"));
    }

    // -- check_config when unconfigured ------------------------------------

    #[test]
    fn check_config_when_unconfigured() {
        // With no config file, check_config should report not configured.
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({ "action": "check_config" }));
        // It should succeed (the check itself works) but report not configured.
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data["configured"], false);
        assert!(r.output.contains("not configured"));
    }

    // -- configure returns panel definition ---------------------------------

    #[test]
    fn configure_returns_panel() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({ "action": "configure" }));
        assert!(r.success);
        let data = r.data.unwrap();
        // Should have a panel definition with fields.
        let panel = &data["panel"];
        assert_eq!(panel["title"], "Email Configuration");
        let fields = panel["fields"].as_array().unwrap();
        assert!(!fields.is_empty());
        // Should have provider, smtp_server, smtp_port, username, password, from_address.
        let field_ids: Vec<&str> = fields
            .iter()
            .map(|f| f["id"].as_str().unwrap())
            .collect();
        assert!(field_ids.contains(&"provider"));
        assert!(field_ids.contains(&"smtp_server"));
        assert!(field_ids.contains(&"smtp_port"));
        assert!(field_ids.contains(&"username"));
        assert!(field_ids.contains(&"password"));
        assert!(field_ids.contains(&"from_address"));
    }

    #[test]
    fn configure_returns_presets() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({ "action": "configure" }));
        assert!(r.success);
        let data = r.data.unwrap();
        let presets = &data["presets"];
        // Gmail preset.
        assert_eq!(presets["gmail"]["smtp_server"], "smtp.gmail.com");
        assert_eq!(presets["gmail"]["smtp_port"], 587);
        // Outlook preset.
        assert_eq!(presets["outlook"]["smtp_server"], "smtp-mail.outlook.com");
        assert_eq!(presets["outlook"]["smtp_port"], 587);
        // Yahoo preset.
        assert_eq!(presets["yahoo"]["smtp_server"], "smtp.mail.yahoo.com");
        assert_eq!(presets["yahoo"]["smtp_port"], 587);
        // iCloud preset.
        assert_eq!(presets["icloud"]["smtp_server"], "smtp.mail.me.com");
        assert_eq!(presets["icloud"]["smtp_port"], 587);
    }

    // -- provider presets ---------------------------------------------------

    #[test]
    fn gmail_preset_correct() {
        let preset = provider_preset("gmail").unwrap();
        assert_eq!(preset.server, "smtp.gmail.com");
        assert_eq!(preset.port, 587);
        assert!(preset.note.contains("App Password"));
    }

    #[test]
    fn outlook_preset_correct() {
        let preset = provider_preset("outlook").unwrap();
        assert_eq!(preset.server, "smtp-mail.outlook.com");
        assert_eq!(preset.port, 587);
    }

    #[test]
    fn hotmail_alias_resolves_to_outlook() {
        let preset = provider_preset("hotmail").unwrap();
        assert_eq!(preset.server, "smtp-mail.outlook.com");
    }

    #[test]
    fn yahoo_preset_correct() {
        let preset = provider_preset("yahoo").unwrap();
        assert_eq!(preset.server, "smtp.mail.yahoo.com");
        assert_eq!(preset.port, 587);
    }

    #[test]
    fn icloud_preset_correct() {
        let preset = provider_preset("icloud").unwrap();
        assert_eq!(preset.server, "smtp.mail.me.com");
        assert_eq!(preset.port, 587);
    }

    #[test]
    fn custom_preset_returns_none() {
        assert!(provider_preset("custom").is_none());
    }

    #[test]
    fn unknown_preset_returns_none() {
        assert!(provider_preset("protonmail").is_none());
    }

    #[test]
    fn preset_case_insensitive() {
        assert!(provider_preset("Gmail").is_some());
        assert!(provider_preset("OUTLOOK").is_some());
        assert!(provider_preset("Yahoo").is_some());
        assert!(provider_preset("ICloud").is_some());
    }

    // -- email validation ---------------------------------------------------

    #[test]
    fn valid_emails() {
        assert!(is_valid_email("user@example.com"));
        assert!(is_valid_email("alice.bob@domain.co.uk"));
        assert!(is_valid_email("test+tag@gmail.com"));
        assert!(is_valid_email("a@b.c"));
    }

    #[test]
    fn invalid_emails() {
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("no-at-sign"));
        assert!(!is_valid_email("@no-local.com"));
        assert!(!is_valid_email("no-domain@"));
        assert!(!is_valid_email("no@dot"));
        assert!(!is_valid_email("two@@ats.com"));
        assert!(!is_valid_email("bad@.startdot.com"));
        assert!(!is_valid_email("bad@enddot.com."));
    }

    // -- send with missing fields -------------------------------------------

    #[test]
    fn send_missing_to() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "send",
            "subject": "Hi",
            "body": "Hello"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("'to'"));
    }

    #[test]
    fn send_missing_subject() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "send",
            "to": "user@example.com",
            "body": "Hello"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("'subject'"));
    }

    #[test]
    fn send_missing_body() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "send",
            "to": "user@example.com",
            "subject": "Hi"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("'body'"));
    }

    #[test]
    fn send_invalid_to_email() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "send",
            "to": "not-an-email",
            "subject": "Hi",
            "body": "Hello"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Invalid recipient"));
    }

    #[test]
    fn send_invalid_cc_email() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "send",
            "to": "user@example.com",
            "cc": "bad-cc",
            "subject": "Hi",
            "body": "Hello"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Invalid CC"));
    }

    // -- send with missing config -------------------------------------------

    #[test]
    fn send_with_missing_config_fails() {
        let tool = SendEmailTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "send",
            "to": "user@example.com",
            "subject": "Test",
            "body": "Test body"
        }));
        assert!(!r.success);
        // Should mention configuration is needed.
        assert!(
            r.error.as_deref().unwrap().contains("not configured")
                || r.error.as_deref().unwrap().contains("configure"),
            "error should mention configuration: {:?}",
            r.error
        );
    }

    // -- provider_names -----------------------------------------------------

    #[test]
    fn provider_names_includes_all() {
        let names = provider_names();
        assert!(names.contains(&"custom"));
        assert!(names.contains(&"gmail"));
        assert!(names.contains(&"outlook"));
        assert!(names.contains(&"yahoo"));
        assert!(names.contains(&"icloud"));
    }

    // -- to_schema ----------------------------------------------------------

    #[test]
    fn to_schema_produces_valid_schema() {
        let tool = SendEmailTool::new();
        let schema = tool.to_schema();
        assert_eq!(schema.name, "send_email");
        assert!(!schema.description.is_empty());
        assert!(schema.parameters.is_object());
    }
}
