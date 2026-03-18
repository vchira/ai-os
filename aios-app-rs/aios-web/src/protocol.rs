//! WebSocket protocol — JSON message types exchanged between the web
//! client and the AiOS server.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Client → Server messages
// ---------------------------------------------------------------------------

/// A message sent from the web client to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// A chat message from the user.
    Message {
        text: String,
    },
    /// User responded to a panel request.
    PanelResponse {
        id: String,
        values: serde_json::Map<String, serde_json::Value>,
        cancelled: bool,
    },
    /// User wants to switch the channel back to another surface.
    SwitchChannel {
        target: String,
    },
}

// ---------------------------------------------------------------------------
// Server → Client messages
// ---------------------------------------------------------------------------

/// A message sent from the server to the web client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// A chat message (user, assistant, system, or tool).
    Message {
        role: String,
        content: String,
        /// Optional severity level (info, success, warning, important, error).
        #[serde(skip_serializing_if = "Option::is_none")]
        level: Option<String>,
    },
    /// A panel request for the web client to render as an HTML form.
    PanelRequest {
        id: String,
        request: serde_json::Value,
    },
    /// An image to display.
    Image {
        path: String,
    },
    /// A notification.
    Notification {
        title: String,
        message: String,
    },
    /// Channel status update.
    ChannelStatus {
        active: String,
    },
    /// System info or error.
    System {
        content: String,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_roundtrip() {
        let msg = ClientMessage::Message {
            text: "Hello AI".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"message\""));
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::Message { text } => assert_eq!(text, "Hello AI"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_roundtrip() {
        let msg = ServerMessage::Message {
            role: "assistant".to_string(),
            content: "Hello!".to_string(),
            level: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"message\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Message { role, content, .. } => {
                assert_eq!(role, "assistant");
                assert_eq!(content, "Hello!");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn panel_response_roundtrip() {
        let mut values = serde_json::Map::new();
        values.insert("name".to_string(), serde_json::json!("Alice"));
        let msg = ClientMessage::PanelResponse {
            id: "panel-1".to_string(),
            values,
            cancelled: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::PanelResponse { id, values, cancelled } => {
                assert_eq!(id, "panel-1");
                assert!(!cancelled);
                assert_eq!(values.get("name").unwrap(), "Alice");
            }
            _ => panic!("wrong variant"),
        }
    }

    // -- Additional roundtrip tests --

    #[test]
    fn client_switch_channel_roundtrip() {
        let msg = ClientMessage::SwitchChannel {
            target: "signal".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"switch_channel\""));
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SwitchChannel { target } => assert_eq!(target, "signal"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_panel_request_roundtrip() {
        let msg = ServerMessage::PanelRequest {
            id: "req-42".to_string(),
            request: serde_json::json!({
                "title": "Enter API Key",
                "fields": [{ "id": "key", "type": "password" }]
            }),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"panel_request\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::PanelRequest { id, request } => {
                assert_eq!(id, "req-42");
                assert_eq!(request.get("title").unwrap(), "Enter API Key");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_notification_roundtrip() {
        let msg = ServerMessage::Notification {
            title: "Update".to_string(),
            message: "New version available".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"notification\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Notification { title, message } => {
                assert_eq!(title, "Update");
                assert_eq!(message, "New version available");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_channel_status_roundtrip() {
        let msg = ServerMessage::ChannelStatus {
            active: "desktop".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"channel_status\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::ChannelStatus { active } => {
                assert_eq!(active, "desktop");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_system_roundtrip() {
        let msg = ServerMessage::System {
            content: "System shutting down".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"system\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::System { content } => {
                assert_eq!(content, "System shutting down");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_image_roundtrip() {
        let msg = ServerMessage::Image {
            path: "/tmp/screenshot.png".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"image\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Image { path } => {
                assert_eq!(path, "/tmp/screenshot.png");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_with_level_field() {
        let msg = ServerMessage::Message {
            role: "system".to_string(),
            content: "Warning issued".to_string(),
            level: Some("warning".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"level\":\"warning\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Message { role, content, level } => {
                assert_eq!(role, "system");
                assert_eq!(content, "Warning issued");
                assert_eq!(level, Some("warning".to_string()));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_without_level_field_absent_in_json() {
        let msg = ServerMessage::Message {
            role: "assistant".to_string(),
            content: "Hello".to_string(),
            level: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        // level is None and skip_serializing_if ensures it's absent.
        assert!(!json.contains("\"level\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Message { level, .. } => {
                assert!(level.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    // -----------------------------------------------------------------------
    // ServerMessage::Chat (Message variant) serializes correctly
    // -----------------------------------------------------------------------

    #[test]
    fn server_message_chat_serializes_with_type_field() {
        let msg = ServerMessage::Message {
            role: "assistant".to_string(),
            content: "Hello, I am AiOS.".to_string(),
            level: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        // Must contain the tagged type.
        assert!(json.contains("\"type\":\"message\""));
        // Must contain the role and content.
        assert!(json.contains("\"role\":\"assistant\""));
        assert!(json.contains("\"content\":\"Hello, I am AiOS.\""));
        // level should be absent when None.
        assert!(!json.contains("\"level\""));
    }

    #[test]
    fn server_message_chat_with_level_serializes_correctly() {
        let msg = ServerMessage::Message {
            role: "system".to_string(),
            content: "Boot complete.".to_string(),
            level: Some("info".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"message\""));
        assert!(json.contains("\"role\":\"system\""));
        assert!(json.contains("\"level\":\"info\""));
    }

    // -----------------------------------------------------------------------
    // ServerMessage::System serializes correctly
    // -----------------------------------------------------------------------

    #[test]
    fn server_system_serializes_with_type_and_content() {
        let msg = ServerMessage::System {
            content: "Server restarting...".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"system\""));
        assert!(json.contains("\"content\":\"Server restarting...\""));
    }

    #[test]
    fn server_system_deserializes_back() {
        let json = r#"{"type":"system","content":"All systems operational"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::System { content } => {
                assert_eq!(content, "All systems operational");
            }
            _ => panic!("expected System variant"),
        }
    }

    // -----------------------------------------------------------------------
    // ClientMessage::Chat (Message variant) deserializes correctly
    // -----------------------------------------------------------------------

    #[test]
    fn client_message_chat_deserializes_from_json() {
        let json = r#"{"type":"message","text":"What time is it?"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Message { text } => {
                assert_eq!(text, "What time is it?");
            }
            _ => panic!("expected Message variant"),
        }
    }

    #[test]
    fn client_message_chat_deserializes_empty_text() {
        let json = r#"{"type":"message","text":""}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Message { text } => {
                assert_eq!(text, "");
            }
            _ => panic!("expected Message variant"),
        }
    }

    #[test]
    fn client_message_chat_deserializes_unicode() {
        let json = r#"{"type":"message","text":"Salut, cum ești?"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Message { text } => {
                assert_eq!(text, "Salut, cum ești?");
            }
            _ => panic!("expected Message variant"),
        }
    }

    // -----------------------------------------------------------------------
    // ClientMessage::PanelResponse deserializes correctly
    // -----------------------------------------------------------------------

    #[test]
    fn client_panel_response_deserializes_from_json() {
        let json = r#"{
            "type": "panel_response",
            "id": "panel-42",
            "values": {"api_key": "sk-test-123", "provider": "claude"},
            "cancelled": false
        }"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::PanelResponse { id, values, cancelled } => {
                assert_eq!(id, "panel-42");
                assert!(!cancelled);
                assert_eq!(values.get("api_key").unwrap(), "sk-test-123");
                assert_eq!(values.get("provider").unwrap(), "claude");
            }
            _ => panic!("expected PanelResponse variant"),
        }
    }

    #[test]
    fn client_panel_response_cancelled_deserializes() {
        let json = r#"{
            "type": "panel_response",
            "id": "panel-99",
            "values": {},
            "cancelled": true
        }"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::PanelResponse { id, cancelled, values } => {
                assert_eq!(id, "panel-99");
                assert!(cancelled);
                assert!(values.is_empty());
            }
            _ => panic!("expected PanelResponse variant"),
        }
    }

    // -----------------------------------------------------------------------
    // Invalid JSON returns error
    // -----------------------------------------------------------------------

    #[test]
    fn invalid_json_returns_error_for_client_message() {
        let result = serde_json::from_str::<ClientMessage>("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_json_returns_error_for_server_message() {
        let result = serde_json::from_str::<ServerMessage>("{{{bad");
        assert!(result.is_err());
    }

    #[test]
    fn empty_string_returns_error() {
        let result = serde_json::from_str::<ClientMessage>("");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_json_number_returns_error() {
        let result = serde_json::from_str::<ClientMessage>("42");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Missing fields return error
    // -----------------------------------------------------------------------

    #[test]
    fn missing_type_field_returns_error() {
        // JSON object without the "type" tag.
        let json = r#"{"text":"hello"}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_text_field_on_message_returns_error() {
        let json = r#"{"type":"message"}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_id_field_on_panel_response_returns_error() {
        let json = r#"{"type":"panel_response","values":{},"cancelled":false}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_values_field_on_panel_response_returns_error() {
        let json = r#"{"type":"panel_response","id":"p1","cancelled":false}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_content_field_on_server_system_returns_error() {
        let json = r#"{"type":"system"}"#;
        let result = serde_json::from_str::<ServerMessage>(json);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_type_tag_returns_error() {
        let json = r#"{"type":"unknown_variant","foo":"bar"}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }
}
