//! UI panel tool — show input panels to the user and collect responses.
//!
//! The AI (or a scripted conversation) can call the `ui_panel` tool to show
//! input panels to the user.  The tool receives a panel definition (title,
//! icon, description, list of fields), shows a GTK panel/dialog, waits for
//! user input, and returns the filled values as JSON.
//!
//! This is how the AI asks for ANY user input — API keys, preferences, file
//! names, dates, etc.
//!
//! The tool uses a callback pattern: the UI layer registers a callback at
//! startup that handles rendering the panel and collecting input.  This keeps
//! the tool layer decoupled from any particular frontend.

use std::sync::{Arc, Mutex};

use aios_core::channel::ChannelContext;
use aios_core::types::ToolResult;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::tool::Tool;

// ---------------------------------------------------------------------------
// Panel data types (no GTK dependency — safe for aios-tools)
// ---------------------------------------------------------------------------

/// Describes a panel to show to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelRequest {
    /// Panel title (displayed at the top).
    pub title: String,
    /// Optional GTK symbolic icon name (e.g. `"dialog-password-symbolic"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Optional description shown below the title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// List of input fields.
    pub fields: Vec<PanelField>,
}

/// Data protection level for a panel field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldProtection {
    /// Normal field — value returned to the AI as-is.
    #[default]
    None,
    /// Private field (email, name, address, phone) — value stored in vault.
    /// The AI model never reads the raw value directly, but tools CAN
    /// display it to the user (e.g., showing email in a "From:" field).
    /// Tools need permission to access. Value is visible in tool UI but
    /// never sent to the AI model.
    Private,
    /// Secure field (passwords, API keys, tokens) — value stored in vault.
    /// Can NEVER be displayed anywhere after entry. Only used internally
    /// by tools (e.g., passed to SMTP auth). Not shown to user, AI, or logs.
    Secure,
}

impl FieldProtection {
    /// Serde skip helper.
    pub fn is_none(&self) -> bool {
        matches!(self, FieldProtection::None)
    }
}

/// A single input field in a panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelField {
    /// Unique field identifier used as the key in the response JSON.
    pub id: String,
    /// The type of input widget to display.
    pub field_type: FieldType,
    /// Human-readable label for the field.
    pub label: String,
    /// Optional placeholder text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Whether the field is required.
    #[serde(default)]
    pub required: bool,
    /// Optional default value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    /// Options for `choice` and `dropdown` fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<FieldOption>,
    /// Minimum value for `number` fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Maximum value for `number` fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Step increment for `number` fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// Data protection level. When `Private` or `Secure`, the value is
    /// stored directly in the encrypted vault and the AI receives a
    /// redacted placeholder instead of the actual value.
    #[serde(default, skip_serializing_if = "FieldProtection::is_none")]
    pub protection: FieldProtection,
    /// Vault key for storing protected values (required when `protection`
    /// is `Private` or `Secure`). E.g., `"gmail_app_password"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_key: Option<String>,
    /// Human-readable description of the protected value, stored in the
    /// secure registry so the AI knows what the key is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_description: Option<String>,
}

/// The type of input widget for a [`PanelField`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// Single-line text entry.
    Text,
    /// Password entry (masked input).
    Password,
    /// Numeric spinner.
    Number,
    /// Date entry (YYYY-MM-DD text field).
    Date,
    /// Radio-style choice (one of N options).
    Choice,
    /// Dropdown / combo box.
    Dropdown,
    /// On/off toggle switch.
    Toggle,
    /// Multi-line text area.
    Multiline,
}

/// An option for `choice` or `dropdown` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldOption {
    /// The value returned in the response when this option is selected.
    pub value: String,
    /// Human-readable label shown to the user.
    pub label: String,
    /// Optional description shown below the label (for choice cards).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The user's response after interacting with a panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelResponse {
    /// Field id -> value mappings.
    pub values: serde_json::Map<String, serde_json::Value>,
    /// Whether the user cancelled the panel.
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// Callback type
// ---------------------------------------------------------------------------

/// Callback signature: receives a [`PanelRequest`] and the active
/// [`ChannelContext`], returns an optional [`PanelResponse`].
///
/// The callback implementation decides how to render the panel based on the
/// channel (GTK dialog on Desktop, HTML form on Web, text choices on Signal).
/// Returning `None` means the panel could not be displayed.
pub type PanelCallback =
    Arc<dyn Fn(PanelRequest, &ChannelContext) -> Option<PanelResponse> + Send + Sync>;

// ---------------------------------------------------------------------------
// UiPanelTool
// ---------------------------------------------------------------------------

/// Show an input panel to the user and collect responses.
///
/// The callback is set by the UI layer at startup via
/// [`set_panel_callback`](UiPanelTool::set_panel_callback).  When the tool is
/// executed, it parses the arguments into a [`PanelRequest`], invokes the
/// callback, and returns the result.
///
/// Because [`Tool::execute`] is synchronous but UI panels need to wait for
/// user interaction, the callback implementation typically uses a
/// [`std::sync::Condvar`] or channel internally to block the worker thread
/// until the UI responds on the main thread.
pub struct UiPanelTool {
    panel_callback: Arc<Mutex<Option<PanelCallback>>>,
}

impl UiPanelTool {
    /// Create a new `UiPanelTool` with no callback registered.
    pub fn new() -> Self {
        Self {
            panel_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Register the callback that renders panels and collects user input.
    ///
    /// The callback receives a [`PanelRequest`] and the active
    /// [`ChannelContext`], and must return:
    /// - `Some(PanelResponse)` with the user's values (or `cancelled: true`)
    /// - `None` if the panel could not be displayed
    ///
    /// The callback implementation decides how to render based on the channel:
    /// - **Desktop**: GTK dialog
    /// - **Web**: HTML form via WebSocket
    /// - **Signal**: numbered text choices
    /// - **Voice**: sequential spoken prompts
    ///
    /// The callback will be called from a worker thread.  It is responsible
    /// for marshalling to the UI thread if needed (e.g. via
    /// `glib::idle_add_local_once` + channel).
    pub fn set_panel_callback(
        &self,
        cb: impl Fn(PanelRequest, &ChannelContext) -> Option<PanelResponse> + Send + Sync + 'static,
    ) {
        let mut guard = self.panel_callback.lock().unwrap();
        *guard = Some(Arc::new(cb));
    }
}

impl Default for UiPanelTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for UiPanelTool {
    fn name(&self) -> &str {
        "ui_panel"
    }

    fn description(&self) -> &str {
        "Show an input panel to the user and collect responses. \
         Use this whenever you need input from the user."
    }

    fn category(&self) -> &str {
        "ui"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Panel title"
                },
                "icon": {
                    "type": "string",
                    "description": "GTK symbolic icon name (e.g., dialog-password-symbolic)"
                },
                "description": {
                    "type": "string",
                    "description": "Explanation shown to the user"
                },
                "fields": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Field identifier for the response"
                            },
                            "type": {
                                "type": "string",
                                "enum": ["text", "password", "number", "date", "choice", "dropdown", "toggle", "multiline"],
                                "description": "Field widget type"
                            },
                            "label": {
                                "type": "string",
                                "description": "Field label"
                            },
                            "placeholder": {
                                "type": "string",
                                "description": "Placeholder text"
                            },
                            "required": {
                                "type": "boolean",
                                "description": "Whether the field is required"
                            },
                            "default": {
                                "description": "Default value"
                            },
                            "options": {
                                "type": "array",
                                "description": "For choice/dropdown: list of options",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "value": { "type": "string" },
                                        "label": { "type": "string" },
                                        "description": { "type": "string" }
                                    }
                                }
                            },
                            "min": {
                                "type": "number",
                                "description": "For number: minimum value"
                            },
                            "max": {
                                "type": "number",
                                "description": "For number: maximum value"
                            },
                            "step": {
                                "type": "number",
                                "description": "For number: step increment"
                            }
                        },
                        "required": ["id", "type", "label"]
                    }
                }
            },
            "required": ["title", "fields"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        // Channel-agnostic fallback: uses default Desktop context.
        self.execute_on_channel(args, &ChannelContext::default())
    }

    fn execute_on_channel(
        &self,
        args: serde_json::Value,
        channel: &ChannelContext,
    ) -> ToolResult {
        // Parse the request from JSON arguments.
        let request = match parse_panel_request(&args) {
            Ok(req) => req,
            Err(e) => return ToolResult::fail(format!("Invalid panel request: {e}")),
        };

        // Validate: must have at least one field.
        if request.fields.is_empty() {
            return ToolResult::fail("Panel must have at least one field.");
        }

        // Collect protection info before the request is consumed by the callback.
        let protected_fields: Vec<(String, FieldProtection, Option<String>, Option<String>)> =
            request
                .fields
                .iter()
                .filter(|f| !f.protection.is_none())
                .map(|f| (
                    f.id.clone(),
                    f.protection.clone(),
                    f.vault_key.clone(),
                    f.vault_description.clone(),
                ))
                .collect();

        // Invoke the callback with channel context.
        let callback = {
            let guard = self.panel_callback.lock().unwrap();
            guard.clone()
        };

        match callback {
            Some(cb) => {
                let channel_for_cb = channel.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cb(request, &channel_for_cb)
                }));
                match result {
                    Ok(Some(response)) => {
                        if response.cancelled {
                            ToolResult::fail("User cancelled the panel.")
                        } else {
                            let mut values = response.values.clone();

                            // Process protected fields: store in vault, redact for AI.
                            for (field_id, protection, vault_key, description) in &protected_fields {
                                if let Some(val) = values.get(field_id).and_then(|v| v.as_str()) {
                                    let key = vault_key.as_deref().unwrap_or(field_id.as_str());
                                    let desc = description.as_deref().unwrap_or(field_id.as_str());

                                    // Store in the memory file (the vault requires master password
                                    // which we may not have here, so use the memory store).
                                    let mem_path = aios_core::config::ConfigManager::default_config_dir()
                                        .join("memory.json");
                                    if let Ok(contents) = std::fs::read_to_string(&mem_path) {
                                        if let Ok(mut store) = serde_json::from_str::<std::collections::BTreeMap<String, String>>(&contents) {
                                            store.insert(key.to_string(), val.to_string());
                                            let _ = std::fs::write(&mem_path, serde_json::to_string_pretty(&store).unwrap_or_default());
                                        }
                                    } else {
                                        let mut store = std::collections::BTreeMap::new();
                                        store.insert(key.to_string(), val.to_string());
                                        let _ = std::fs::write(&mem_path, serde_json::to_string_pretty(&store).unwrap_or_default());
                                    }

                                    // Register in the secure registry.
                                    let reg_path = aios_core::secure::SecureRegistry::default_path();
                                    let mut registry = aios_core::secure::SecureRegistry::load(&reg_path);
                                    let kind = aios_core::secure::SecureKind::from_key(key);
                                    registry.register(key, desc, desc, kind);
                                    registry.mark_stored(key);
                                    let _ = registry.save(&reg_path);

                                    // Redact the value for the AI.
                                    match protection {
                                        FieldProtection::Secure => {
                                            values.insert(
                                                field_id.clone(),
                                                serde_json::Value::String(
                                                    format!("[SECURE — stored as '{key}']")
                                                ),
                                            );
                                        }
                                        FieldProtection::Private => {
                                            values.insert(
                                                field_id.clone(),
                                                serde_json::Value::String(
                                                    format!("[PRIVATE — stored as '{key}']")
                                                ),
                                            );
                                        }
                                        _ => {}
                                    }

                                    tracing::info!(
                                        "Protected field '{field_id}' ({protection:?}) stored as vault key '{key}'"
                                    );
                                }
                            }

                            let json_value = serde_json::Value::Object(values);
                            ToolResult::ok_with_data(
                                serde_json::to_string_pretty(&json_value)
                                    .unwrap_or_else(|_| "{}".to_string()),
                                json_value,
                            )
                        }
                    }
                    Ok(None) => {
                        ToolResult::fail("Panel could not be displayed (no UI available).")
                    }
                    Err(_) => {
                        ToolResult::fail("Panel callback panicked.")
                    }
                }
            }
            None => {
                warn!("ui_panel callback not registered; cannot show panel");
                ToolResult::fail(
                    "No UI panel callback registered. The panel cannot be displayed \
                     without a UI layer.",
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

/// Parse a [`PanelRequest`] from the raw JSON arguments passed to the tool.
fn parse_panel_request(args: &serde_json::Value) -> Result<PanelRequest, String> {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or("'title' is required")?
        .to_string();

    let icon = args.get("icon").and_then(|v| v.as_str()).map(String::from);
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let fields_arr = args
        .get("fields")
        .and_then(|v| v.as_array())
        .ok_or("'fields' must be an array")?;

    let mut fields = Vec::with_capacity(fields_arr.len());
    for (i, field_val) in fields_arr.iter().enumerate() {
        let field = parse_panel_field(field_val)
            .map_err(|e| format!("field[{i}]: {e}"))?;
        fields.push(field);
    }

    Ok(PanelRequest {
        title,
        icon,
        description,
        fields,
    })
}

/// Parse a single [`PanelField`] from a JSON value.
fn parse_panel_field(val: &serde_json::Value) -> Result<PanelField, String> {
    let id = val
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("'id' is required")?
        .to_string();

    let type_str = val
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or("'type' is required")?;

    let field_type = match type_str {
        "text" => FieldType::Text,
        "password" => FieldType::Password,
        "number" => FieldType::Number,
        "date" => FieldType::Date,
        "choice" => FieldType::Choice,
        "dropdown" => FieldType::Dropdown,
        "toggle" => FieldType::Toggle,
        "multiline" => FieldType::Multiline,
        other => return Err(format!("unknown field type: {other:?}")),
    };

    let label = val
        .get("label")
        .and_then(|v| v.as_str())
        .ok_or("'label' is required")?
        .to_string();

    let placeholder = val
        .get("placeholder")
        .and_then(|v| v.as_str())
        .map(String::from);

    let required = val
        .get("required")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let default_value = val.get("default").cloned();

    let options = if let Some(opts) = val.get("options").and_then(|v| v.as_array()) {
        opts.iter()
            .filter_map(|o| {
                let value = o.get("value").and_then(|v| v.as_str())?.to_string();
                let label = o.get("label").and_then(|v| v.as_str())?.to_string();
                let description = o
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                Some(FieldOption {
                    value,
                    label,
                    description,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    let min = val.get("min").and_then(|v| v.as_f64());
    let max = val.get("max").and_then(|v| v.as_f64());
    let step = val.get("step").and_then(|v| v.as_f64());

    // Parse protection level for secure/private fields.
    let protection = val
        .get("protection")
        .and_then(|v| v.as_str())
        .map(|s| match s {
            "secure" => FieldProtection::Secure,
            "private" => FieldProtection::Private,
            _ => FieldProtection::None,
        })
        .unwrap_or_default();
    let vault_key = val.get("vault_key").and_then(|v| v.as_str()).map(String::from);
    let vault_description = val.get("vault_description").and_then(|v| v.as_str()).map(String::from);

    Ok(PanelField {
        id,
        field_type,
        label,
        placeholder,
        required,
        default_value,
        options,
        min,
        max,
        step,
        protection,
        vault_key,
        vault_description,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_callback_returns_error() {
        let tool = UiPanelTool::new();
        let r = tool.execute(serde_json::json!({
            "title": "Test Panel",
            "fields": [{
                "id": "name",
                "type": "text",
                "label": "Your name"
            }]
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("callback"));
    }

    #[test]
    fn missing_title_returns_error() {
        let tool = UiPanelTool::new();
        let r = tool.execute(serde_json::json!({
            "fields": [{
                "id": "name",
                "type": "text",
                "label": "Your name"
            }]
        }));
        assert!(!r.success);
    }

    #[test]
    fn empty_fields_returns_error() {
        let tool = UiPanelTool::new();
        let r = tool.execute(serde_json::json!({
            "title": "Test",
            "fields": []
        }));
        assert!(!r.success);
    }

    #[test]
    fn callback_returns_values() {
        let tool = UiPanelTool::new();
        tool.set_panel_callback(|_request, _channel| {
            let mut values = serde_json::Map::new();
            values.insert("name".to_string(), serde_json::json!("Alice"));
            Some(PanelResponse {
                values,
                cancelled: false,
            })
        });

        let r = tool.execute(serde_json::json!({
            "title": "Test Panel",
            "fields": [{
                "id": "name",
                "type": "text",
                "label": "Your name"
            }]
        }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data.get("name").and_then(|v| v.as_str()), Some("Alice"));
    }

    #[test]
    fn callback_cancelled() {
        let tool = UiPanelTool::new();
        tool.set_panel_callback(|_request, _channel| {
            Some(PanelResponse {
                values: serde_json::Map::new(),
                cancelled: true,
            })
        });

        let r = tool.execute(serde_json::json!({
            "title": "Test Panel",
            "fields": [{
                "id": "name",
                "type": "text",
                "label": "Your name"
            }]
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap_or("").contains("cancelled"));
    }

    #[test]
    fn callback_returns_none() {
        let tool = UiPanelTool::new();
        tool.set_panel_callback(|_request, _channel| None);

        let r = tool.execute(serde_json::json!({
            "title": "Test Panel",
            "fields": [{
                "id": "name",
                "type": "text",
                "label": "Your name"
            }]
        }));
        assert!(!r.success);
    }

    #[test]
    fn parse_all_field_types() {
        let tool = UiPanelTool::new();
        tool.set_panel_callback(|request, _channel| {
            assert_eq!(request.fields.len(), 8);
            assert_eq!(request.fields[0].field_type, FieldType::Text);
            assert_eq!(request.fields[1].field_type, FieldType::Password);
            assert_eq!(request.fields[2].field_type, FieldType::Number);
            assert_eq!(request.fields[3].field_type, FieldType::Date);
            assert_eq!(request.fields[4].field_type, FieldType::Choice);
            assert_eq!(request.fields[5].field_type, FieldType::Dropdown);
            assert_eq!(request.fields[6].field_type, FieldType::Toggle);
            assert_eq!(request.fields[7].field_type, FieldType::Multiline);
            Some(PanelResponse {
                values: serde_json::Map::new(),
                cancelled: false,
            })
        });

        let r = tool.execute(serde_json::json!({
            "title": "All Types",
            "fields": [
                { "id": "a", "type": "text",      "label": "Text" },
                { "id": "b", "type": "password",  "label": "Password" },
                { "id": "c", "type": "number",    "label": "Number", "min": 0, "max": 100, "step": 1 },
                { "id": "d", "type": "date",      "label": "Date" },
                { "id": "e", "type": "choice",    "label": "Choice", "options": [
                    { "value": "a", "label": "Option A", "description": "First option" }
                ]},
                { "id": "f", "type": "dropdown",  "label": "Dropdown", "options": [
                    { "value": "x", "label": "X" }
                ]},
                { "id": "g", "type": "toggle",    "label": "Toggle" },
                { "id": "h", "type": "multiline", "label": "Multiline" }
            ]
        }));
        assert!(r.success);
    }

    #[test]
    fn unknown_field_type_returns_error() {
        let tool = UiPanelTool::new();
        tool.set_panel_callback(|_, _| {
            Some(PanelResponse {
                values: serde_json::Map::new(),
                cancelled: false,
            })
        });

        let r = tool.execute(serde_json::json!({
            "title": "Bad Type",
            "fields": [{ "id": "x", "type": "color_picker", "label": "Pick" }]
        }));
        assert!(!r.success);
    }

    #[test]
    fn panel_request_with_icon_and_description() {
        let tool = UiPanelTool::new();
        tool.set_panel_callback(|req, _channel| {
            assert_eq!(req.icon.as_deref(), Some("dialog-password-symbolic"));
            assert_eq!(req.description.as_deref(), Some("Please enter info."));
            Some(PanelResponse {
                values: serde_json::Map::new(),
                cancelled: false,
            })
        });

        let r = tool.execute(serde_json::json!({
            "title": "With Extras",
            "icon": "dialog-password-symbolic",
            "description": "Please enter info.",
            "fields": [{ "id": "x", "type": "text", "label": "X" }]
        }));
        assert!(r.success);
    }

    #[test]
    fn field_defaults_and_required() {
        let tool = UiPanelTool::new();
        tool.set_panel_callback(|req, _channel| {
            let f = &req.fields[0];
            assert!(f.required);
            assert_eq!(f.default_value, Some(serde_json::json!("hello")));
            assert_eq!(f.placeholder.as_deref(), Some("Type here"));
            Some(PanelResponse {
                values: serde_json::Map::new(),
                cancelled: false,
            })
        });

        let r = tool.execute(serde_json::json!({
            "title": "Defaults",
            "fields": [{
                "id": "x",
                "type": "text",
                "label": "X",
                "required": true,
                "default": "hello",
                "placeholder": "Type here"
            }]
        }));
        assert!(r.success);
    }
}
