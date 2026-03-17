//! Panel rendering for the Signal channel.
//!
//! Formats `PanelRequest` JSON (from `aios-tools/src/builtin/ui_panel.rs`) as
//! numbered text choices suitable for Signal, and parses user text replies
//! back into panel response values.

/// Format a `PanelRequest` as text suitable for Signal.
///
/// Returns a text prompt that can be sent as a message. Each field type
/// is rendered as human-readable instructions:
///
/// - **choice / dropdown**: numbered options ("Reply 1 for X, 2 for Y, ...")
/// - **text**: "Reply with your answer"
/// - **password**: "Reply with your password"
/// - **toggle**: "Reply yes or no"
/// - **number**: "Reply with a number"
pub fn format_panel_as_text(request: &serde_json::Value) -> String {
    let mut lines = Vec::new();

    // Title.
    if let Some(title) = request.get("title").and_then(|v| v.as_str()) {
        lines.push(format!("--- {title} ---"));
    }

    // Description.
    if let Some(desc) = request.get("description").and_then(|v| v.as_str()) {
        lines.push(desc.to_string());
    }

    if !lines.is_empty() {
        lines.push(String::new());
    }

    // Fields.
    let fields = request
        .get("fields")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for field in &fields {
        let label = field
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("Input");
        let field_type = field
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        match field_type {
            "choice" | "dropdown" => {
                lines.push(format!("{label}:"));
                if let Some(options) = field.get("options").and_then(|v| v.as_array()) {
                    for (i, opt) in options.iter().enumerate() {
                        let opt_label = opt
                            .get("label")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        let desc = opt
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if desc.is_empty() {
                            lines.push(format!("  {} — {}", i + 1, opt_label));
                        } else {
                            lines.push(format!("  {} — {} ({})", i + 1, opt_label, desc));
                        }
                    }
                    lines.push(format!("Reply with a number (1-{}).", options.len()));
                }
            }
            "password" => {
                lines.push(format!("{label}: Reply with your password."));
            }
            "toggle" => {
                lines.push(format!("{label}: Reply yes or no."));
            }
            "number" => {
                lines.push(format!("{label}: Reply with a number."));
            }
            // "text" and any unknown type
            _ => {
                let placeholder = field
                    .get("placeholder")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if placeholder.is_empty() {
                    lines.push(format!("{label}: Reply with your answer."));
                } else {
                    lines.push(format!("{label}: Reply with your answer (e.g. {placeholder})."));
                }
            }
        }
    }

    lines.join("\n")
}

/// Parse a user's text reply back into panel response values.
///
/// Takes the original request and the user's reply text. Returns a JSON
/// object mapping field IDs to their parsed values:
///
/// - **choice / dropdown**: "1" maps to the first option's value, "2" to the second, etc.
/// - **toggle**: "yes" / "y" / "true" map to `true`; anything else to `false`.
/// - **number**: parsed as a JSON number; falls back to the raw string if parsing fails.
/// - **text / password**: the reply text as-is.
///
/// For panels with a single field, the entire reply is used as the value.
/// For panels with multiple fields, only the first field is filled from the
/// reply (multi-turn collection would need a separate conversation flow).
pub fn parse_reply(request: &serde_json::Value, reply: &str) -> serde_json::Value {
    let reply = reply.trim();
    let mut result = serde_json::Map::new();

    let fields = request
        .get("fields")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // For simplicity, map the reply to the first field.
    // A multi-field panel over Signal would need a multi-turn flow which
    // is out of scope for this module.
    if let Some(field) = fields.first() {
        let id = field
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("value");
        let field_type = field
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        let value = match field_type {
            "choice" | "dropdown" => {
                // "1" -> first option value, "2" -> second, etc.
                if let Some(idx) = reply.parse::<usize>().ok().filter(|&n| n >= 1) {
                    field
                        .get("options")
                        .and_then(|v| v.as_array())
                        .and_then(|opts| opts.get(idx - 1))
                        .and_then(|opt| opt.get("value"))
                        .cloned()
                        .unwrap_or_else(|| serde_json::Value::String(reply.to_string()))
                } else {
                    serde_json::Value::String(reply.to_string())
                }
            }
            "toggle" => {
                let lower = reply.to_lowercase();
                let is_true = matches!(lower.as_str(), "yes" | "y" | "true" | "1" | "on");
                serde_json::Value::Bool(is_true)
            }
            "number" => {
                if let Ok(n) = reply.parse::<f64>() {
                    serde_json::json!(n)
                } else {
                    serde_json::Value::String(reply.to_string())
                }
            }
            // text, password, and any unknown type
            _ => serde_json::Value::String(reply.to_string()),
        };

        result.insert(id.to_string(), value);
    }

    serde_json::Value::Object(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_choice_panel() {
        let req = serde_json::json!({
            "title": "Pick a Color",
            "description": "Choose your favorite.",
            "fields": [{
                "id": "color",
                "type": "choice",
                "label": "Favorite color",
                "options": [
                    { "value": "red", "label": "Red", "description": "The color of fire" },
                    { "value": "blue", "label": "Blue" },
                    { "value": "green", "label": "Green", "description": "The color of nature" }
                ]
            }]
        });

        let text = format_panel_as_text(&req);
        assert!(text.contains("--- Pick a Color ---"));
        assert!(text.contains("Choose your favorite."));
        assert!(text.contains("1 — Red (The color of fire)"));
        assert!(text.contains("2 — Blue"));
        assert!(text.contains("3 — Green (The color of nature)"));
        assert!(text.contains("Reply with a number (1-3)."));
    }

    #[test]
    fn format_text_panel() {
        let req = serde_json::json!({
            "title": "Enter Name",
            "fields": [{
                "id": "name",
                "type": "text",
                "label": "Your name",
                "placeholder": "John Doe"
            }]
        });

        let text = format_panel_as_text(&req);
        assert!(text.contains("--- Enter Name ---"));
        assert!(text.contains("Your name: Reply with your answer (e.g. John Doe)."));
    }

    #[test]
    fn format_toggle_panel() {
        let req = serde_json::json!({
            "fields": [{
                "id": "enabled",
                "type": "toggle",
                "label": "Enable feature"
            }]
        });

        let text = format_panel_as_text(&req);
        assert!(text.contains("Enable feature: Reply yes or no."));
    }

    #[test]
    fn format_password_panel() {
        let req = serde_json::json!({
            "fields": [{
                "id": "pw",
                "type": "password",
                "label": "Master password"
            }]
        });

        let text = format_panel_as_text(&req);
        assert!(text.contains("Master password: Reply with your password."));
    }

    #[test]
    fn parse_choice_reply() {
        let req = serde_json::json!({
            "fields": [{
                "id": "color",
                "type": "choice",
                "options": [
                    { "value": "red", "label": "Red" },
                    { "value": "blue", "label": "Blue" },
                    { "value": "green", "label": "Green" }
                ]
            }]
        });

        let result = parse_reply(&req, "2");
        assert_eq!(result.get("color").unwrap(), "blue");
    }

    #[test]
    fn parse_choice_out_of_range() {
        let req = serde_json::json!({
            "fields": [{
                "id": "color",
                "type": "choice",
                "options": [
                    { "value": "red", "label": "Red" }
                ]
            }]
        });

        // Out of range falls back to raw text.
        let result = parse_reply(&req, "5");
        assert_eq!(result.get("color").unwrap(), "5");
    }

    #[test]
    fn parse_toggle_yes() {
        let req = serde_json::json!({
            "fields": [{ "id": "ok", "type": "toggle", "label": "Proceed?" }]
        });

        assert_eq!(parse_reply(&req, "yes").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "Y").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "true").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "no").get("ok").unwrap(), false);
        assert_eq!(parse_reply(&req, "nah").get("ok").unwrap(), false);
    }

    #[test]
    fn parse_text_reply() {
        let req = serde_json::json!({
            "fields": [{ "id": "name", "type": "text", "label": "Name" }]
        });

        let result = parse_reply(&req, "Alice");
        assert_eq!(result.get("name").unwrap(), "Alice");
    }

    #[test]
    fn parse_number_reply() {
        let req = serde_json::json!({
            "fields": [{ "id": "port", "type": "number", "label": "Port" }]
        });

        let result = parse_reply(&req, "8080");
        assert_eq!(result.get("port").unwrap(), 8080.0);
    }

    #[test]
    fn parse_number_invalid() {
        let req = serde_json::json!({
            "fields": [{ "id": "port", "type": "number", "label": "Port" }]
        });

        let result = parse_reply(&req, "abc");
        assert_eq!(result.get("port").unwrap(), "abc");
    }

    #[test]
    fn empty_fields_returns_empty_object() {
        let req = serde_json::json!({ "fields": [] });
        let result = parse_reply(&req, "hello");
        assert!(result.as_object().unwrap().is_empty());
    }

    // -- Additional edge-case tests --

    #[test]
    fn format_multiple_fields_combined() {
        let req = serde_json::json!({
            "title": "Settings",
            "fields": [
                {
                    "id": "provider",
                    "type": "choice",
                    "label": "Provider",
                    "options": [
                        { "value": "claude", "label": "Claude" },
                        { "value": "openai", "label": "OpenAI" }
                    ]
                },
                {
                    "id": "name",
                    "type": "text",
                    "label": "Username"
                },
                {
                    "id": "notifications",
                    "type": "toggle",
                    "label": "Enable notifications"
                }
            ]
        });

        let text = format_panel_as_text(&req);
        assert!(text.contains("--- Settings ---"));
        assert!(text.contains("Provider:"));
        assert!(text.contains("1 — Claude"));
        assert!(text.contains("2 — OpenAI"));
        assert!(text.contains("Username: Reply with your answer."));
        assert!(text.contains("Enable notifications: Reply yes or no."));
    }

    #[test]
    fn parse_reply_toggle_various_yes_no() {
        let req = serde_json::json!({
            "fields": [{ "id": "ok", "type": "toggle", "label": "OK?" }]
        });

        // Yes variants.
        assert_eq!(parse_reply(&req, "yes").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "y").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "true").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "1").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "on").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "YES").get("ok").unwrap(), true);
        assert_eq!(parse_reply(&req, "True").get("ok").unwrap(), true);

        // No variants.
        assert_eq!(parse_reply(&req, "no").get("ok").unwrap(), false);
        assert_eq!(parse_reply(&req, "false").get("ok").unwrap(), false);
        assert_eq!(parse_reply(&req, "0").get("ok").unwrap(), false);
        assert_eq!(parse_reply(&req, "off").get("ok").unwrap(), false);
        assert_eq!(parse_reply(&req, "nah").get("ok").unwrap(), false);
    }

    #[test]
    fn parse_reply_number_42() {
        let req = serde_json::json!({
            "fields": [{ "id": "count", "type": "number", "label": "Count" }]
        });

        let result = parse_reply(&req, "42");
        assert_eq!(result.get("count").unwrap(), 42.0);
    }

    #[test]
    fn parse_reply_text_returns_raw_text() {
        let req = serde_json::json!({
            "fields": [{ "id": "msg", "type": "text", "label": "Message" }]
        });

        let result = parse_reply(&req, "Hello, world!");
        assert_eq!(result.get("msg").unwrap(), "Hello, world!");
    }

    #[test]
    fn format_panel_empty_fields() {
        let req = serde_json::json!({
            "title": "Empty Panel",
            "fields": []
        });

        let text = format_panel_as_text(&req);
        assert!(text.contains("--- Empty Panel ---"));
        // No field instructions should appear.
        assert!(!text.contains("Reply"));
    }

    #[test]
    fn format_panel_with_nested_option_descriptions() {
        let req = serde_json::json!({
            "fields": [{
                "id": "model",
                "type": "dropdown",
                "label": "Model",
                "options": [
                    { "value": "haiku", "label": "Haiku", "description": "Fast and cheap" },
                    { "value": "sonnet", "label": "Sonnet", "description": "Balanced" },
                    { "value": "opus", "label": "Opus", "description": "Most capable" }
                ]
            }]
        });

        let text = format_panel_as_text(&req);
        assert!(text.contains("1 — Haiku (Fast and cheap)"));
        assert!(text.contains("2 — Sonnet (Balanced)"));
        assert!(text.contains("3 — Opus (Most capable)"));
        assert!(text.contains("Reply with a number (1-3)."));
    }

    #[test]
    fn parse_reply_out_of_range_choice_number() {
        let req = serde_json::json!({
            "fields": [{
                "id": "color",
                "type": "choice",
                "options": [
                    { "value": "red", "label": "Red" },
                    { "value": "blue", "label": "Blue" }
                ]
            }]
        });

        // 99 is way out of range — falls back to raw text.
        let result = parse_reply(&req, "99");
        assert_eq!(result.get("color").unwrap(), "99");
    }

    #[test]
    fn parse_reply_choice_zero_is_out_of_range() {
        let req = serde_json::json!({
            "fields": [{
                "id": "x",
                "type": "choice",
                "options": [{ "value": "a", "label": "A" }]
            }]
        });

        // 0 is not >= 1, so it falls back to raw text.
        let result = parse_reply(&req, "0");
        assert_eq!(result.get("x").unwrap(), "0");
    }

    #[test]
    fn parse_reply_password_field() {
        let req = serde_json::json!({
            "fields": [{ "id": "pw", "type": "password", "label": "Password" }]
        });

        let result = parse_reply(&req, "s3cret!");
        assert_eq!(result.get("pw").unwrap(), "s3cret!");
    }

    #[test]
    fn format_number_field() {
        let req = serde_json::json!({
            "fields": [{ "id": "port", "type": "number", "label": "Port number" }]
        });
        let text = format_panel_as_text(&req);
        assert!(text.contains("Port number: Reply with a number."));
    }

    #[test]
    fn parse_reply_trims_whitespace() {
        let req = serde_json::json!({
            "fields": [{ "id": "name", "type": "text", "label": "Name" }]
        });

        let result = parse_reply(&req, "  Alice  ");
        assert_eq!(result.get("name").unwrap(), "Alice");
    }

    #[test]
    fn format_panel_no_title_no_description() {
        let req = serde_json::json!({
            "fields": [{ "id": "x", "type": "text", "label": "Input" }]
        });
        let text = format_panel_as_text(&req);
        assert!(!text.contains("---"));
        assert!(text.contains("Input: Reply with your answer."));
    }
}
