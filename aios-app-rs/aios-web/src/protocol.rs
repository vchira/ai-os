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
}
