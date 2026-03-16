//! Axum HTTP server with WebSocket endpoint for the AiOS web channel.
//!
//! The server:
//! - Serves the static web client at `GET /`
//! - Upgrades to WebSocket at `GET /ws`
//! - Forwards incoming WebSocket messages to the unified message channel
//! - Sends AI responses back over WebSocket

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{error, info, warn};

use aios_core::channel::{ChannelKind, IncomingMessage};

use crate::protocol::{ClientMessage, ServerMessage};

// ---------------------------------------------------------------------------
// Embedded static client
// ---------------------------------------------------------------------------

/// The embedded single-page web client (HTML + CSS + JS).
const INDEX_HTML: &str = include_str!("static/index.html");

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Shared server state available to all WebSocket handlers.
pub struct WebServerState {
    /// Channel to forward incoming user messages to the main message loop.
    pub message_tx: mpsc::UnboundedSender<IncomingMessage>,
    /// Broadcast channel for sending AI responses to all connected clients.
    pub response_tx: broadcast::Sender<String>,
    /// Track the currently connected WebSocket (only one active at a time).
    pub active_ws: Mutex<bool>,
}

// ---------------------------------------------------------------------------
// WebServer
// ---------------------------------------------------------------------------

/// The AiOS web server.
pub struct WebServer {
    /// Port to bind on.
    port: u16,
    /// Bind address (0.0.0.0 for LAN access).
    bind_addr: String,
    /// Shared state.
    state: Arc<WebServerState>,
    /// Broadcast sender for AI responses (clone this to send).
    pub response_tx: broadcast::Sender<String>,
}

impl WebServer {
    /// Create a new web server.
    ///
    /// - `port`: Port to bind on (default 80 for `aios.local`).
    /// - `message_tx`: Channel to forward user messages to the AI.
    pub fn new(port: u16, message_tx: mpsc::UnboundedSender<IncomingMessage>) -> Self {
        let (response_tx, _) = broadcast::channel(256);
        let state = Arc::new(WebServerState {
            message_tx,
            response_tx: response_tx.clone(),
            active_ws: Mutex::new(false),
        });

        Self {
            port,
            bind_addr: "0.0.0.0".to_string(),
            state,
            response_tx,
        }
    }

    /// Send a message to all connected WebSocket clients.
    pub fn send_to_clients(&self, msg: &ServerMessage) {
        if let Ok(json) = serde_json::to_string(msg) {
            let _ = self.response_tx.send(json);
        }
    }

    /// Start the server in the background.
    ///
    /// Returns a `JoinHandle` for the server task.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        let addr = format!("{}:{}", self.bind_addr, self.port);
        let state = self.state.clone();

        tokio::spawn(async move {
            let app = Router::new()
                .route("/", get(serve_index))
                .route("/ws", get(ws_upgrade))
                .with_state(state);

            info!("Web server starting on {addr}");

            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!("Failed to bind web server on {addr}: {e}");
                    return;
                }
            };

            if let Err(e) = axum::serve(listener, app).await {
                error!("Web server error: {e}");
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// Serve the static web client.
async fn serve_index() -> impl IntoResponse {
    Html(INDEX_HTML)
}

/// Handle WebSocket upgrade.
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebServerState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

/// Handle an active WebSocket connection.
async fn handle_ws(socket: WebSocket, state: Arc<WebServerState>) {
    info!("WebSocket client connected");

    {
        let mut active = state.active_ws.lock().await;
        *active = true;
    }

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Subscribe to broadcast responses.
    let mut response_rx = state.response_tx.subscribe();

    // Task: forward AI responses to the WebSocket client.
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = response_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Main: read messages from the WebSocket client.
    let message_tx = state.message_tx.clone();
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                let text_str: &str = &text;
                match serde_json::from_str::<ClientMessage>(text_str) {
                    Ok(ClientMessage::Message { text }) => {
                        let incoming = IncomingMessage {
                            channel: ChannelKind::Web,
                            text,
                            sender_id: None,
                        };
                        if message_tx.send(incoming).is_err() {
                            warn!("Message channel closed");
                            break;
                        }
                    }
                    Ok(ClientMessage::PanelResponse { .. }) => {
                        // TODO: Route panel responses to the pending panel callback.
                        info!("Received panel response from web client");
                    }
                    Ok(ClientMessage::SwitchChannel { target }) => {
                        info!("Web client requests channel switch to: {target}");
                        // TODO: Wire to ChannelSwitcher.
                    }
                    Err(e) => {
                        warn!("Invalid WebSocket message: {e}");
                    }
                }
            }
            Message::Close(_) => {
                info!("WebSocket client disconnected");
                break;
            }
            _ => {} // Ignore ping/pong/binary
        }
    }

    // Cleanup.
    send_task.abort();
    {
        let mut active = state.active_ws.lock().await;
        *active = false;
    }
    info!("WebSocket connection closed");
}
