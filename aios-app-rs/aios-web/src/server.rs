//! Axum HTTP server with WebSocket endpoint for the AiOS web channel.
//!
//! The server:
//! - Serves the static web client at `GET /`
//! - Upgrades to WebSocket at `GET /ws`
//! - Forwards incoming WebSocket messages to the unified message channel
//! - Sends AI responses back over WebSocket

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tracing::{error, info, warn};

use aios_core::channel::{ChannelKind, ChannelSwitcher, IncomingMessage};

use crate::protocol::{ClientMessage, ServerMessage};

// ---------------------------------------------------------------------------
// Embedded static client
// ---------------------------------------------------------------------------

/// The embedded single-page web client (HTML + CSS + JS).
const INDEX_HTML: &str = include_str!("static/index.html");

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Response from a panel interaction, sent by the web client.
#[derive(Debug, Clone)]
pub struct PanelResponse {
    /// The panel ID this response belongs to.
    pub id: String,
    /// Field values submitted by the user.
    pub values: serde_json::Map<String, serde_json::Value>,
    /// Whether the user cancelled the panel.
    pub cancelled: bool,
}

/// Shared server state available to all WebSocket handlers.
pub struct WebServerState {
    /// Channel to forward incoming user messages to the main message loop.
    pub message_tx: mpsc::UnboundedSender<IncomingMessage>,
    /// Broadcast channel for sending AI responses to all connected clients.
    pub response_tx: broadcast::Sender<String>,
    /// Track the currently connected WebSocket (only one active at a time).
    pub active_ws: Mutex<bool>,
    /// Pending panel requests waiting for a response from the web client.
    pub pending_panels: Arc<Mutex<HashMap<String, oneshot::Sender<PanelResponse>>>>,
    /// Optional auth token. If set, requests must include `?token=<value>`.
    pub auth_token: Option<String>,
    /// Channel switcher for handling SwitchChannel requests.
    pub switcher: Option<ChannelSwitcher>,
    /// Welcome/boot status message sent to each new WebSocket client.
    pub welcome_message: Option<String>,
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
    /// - `auth_token`: Optional authentication token. If set, requests must
    ///   include `?token=<value>` to access the UI.
    pub fn new(
        port: u16,
        message_tx: mpsc::UnboundedSender<IncomingMessage>,
        auth_token: Option<String>,
        switcher: Option<ChannelSwitcher>,
        welcome_message: Option<String>,
    ) -> Self {
        let (response_tx, _) = broadcast::channel(256);
        let state = Arc::new(WebServerState {
            message_tx,
            response_tx: response_tx.clone(),
            active_ws: Mutex::new(false),
            pending_panels: Arc::new(Mutex::new(HashMap::new())),
            auth_token,
            switcher,
            welcome_message,
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

    /// Send a panel request to the connected web client and wait for the response.
    ///
    /// Inserts a `oneshot::Sender` into `pending_panels`, sends the
    /// `PanelRequest` JSON over the broadcast channel, and awaits the
    /// client's `PanelResponse`.
    ///
    /// Returns `None` if there is no connected client, the client never
    /// responds, or an internal error occurs.
    ///
    /// **Note**: This is an async method intended to be called from a Tokio
    /// context. Full end-to-end wiring from the `ui_panel` tool callback
    /// (which runs on the GTK thread for Desktop, or a worker thread for
    /// Web) is not yet implemented — that requires changes in `app.rs`'s
    /// panel callback routing.
    pub async fn request_panel(
        state: &Arc<WebServerState>,
        id: String,
        request_json: serde_json::Value,
    ) -> Option<PanelResponse> {
        let (tx, rx) = oneshot::channel();

        // Register the pending panel.
        {
            let mut panels = state.pending_panels.lock().await;
            panels.insert(id.clone(), tx);
        }

        // Send the PanelRequest to the web client.
        let msg = ServerMessage::PanelRequest {
            id: id.clone(),
            request: request_json,
        };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = state.response_tx.send(json);
        }

        // Wait for the client's response.
        match rx.await {
            Ok(response) => Some(response),
            Err(_) => {
                // Sender was dropped (e.g. client disconnected) — clean up.
                let mut panels = state.pending_panels.lock().await;
                panels.remove(&id);
                None
            }
        }
    }

    /// Start the server in the background.
    ///
    /// Returns a `JoinHandle` for the server task.
    ///
    /// If binding to the configured port fails (e.g. port 80 without
    /// capabilities), automatically falls back to port 8080.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        let primary_addr = format!("{}:{}", self.bind_addr, self.port);
        let fallback_port = if self.port == 80 { 8080 } else { self.port + 1 };
        let fallback_addr = format!("{}:{}", self.bind_addr, fallback_port);
        let state = self.state.clone();

        tokio::spawn(async move {
            let app = Router::new()
                .route("/", get(serve_index))
                .route("/ws", get(ws_upgrade))
                .with_state(state);

            info!("Web server starting on {primary_addr}");

            let listener = match tokio::net::TcpListener::bind(&primary_addr).await {
                Ok(l) => {
                    info!("Web server bound to {primary_addr}");
                    l
                }
                Err(e) => {
                    warn!("Failed to bind on {primary_addr}: {e} — trying port {fallback_port}");
                    match tokio::net::TcpListener::bind(&fallback_addr).await {
                        Ok(l) => {
                            info!("Web server bound to {fallback_addr} (fallback)");
                            l
                        }
                        Err(e2) => {
                            error!("Failed to bind web server on {fallback_addr}: {e2}");
                            return;
                        }
                    }
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

/// Query parameters for token auth.
#[derive(serde::Deserialize, Default)]
struct AuthQuery {
    #[serde(default)]
    token: Option<String>,
}

/// Check if the request is authorized.
fn check_auth(state: &WebServerState, query: &AuthQuery) -> bool {
    match &state.auth_token {
        None => true, // No token configured — open access
        Some(expected) => query.token.as_deref() == Some(expected.as_str()),
    }
}

/// Serve the static web client (with optional token auth).
async fn serve_index(
    State(state): State<Arc<WebServerState>>,
    axum::extract::Query(query): axum::extract::Query<AuthQuery>,
) -> impl IntoResponse {
    if !check_auth(&state, &query) {
        return Html("<h1>401 Unauthorized</h1><p>Add ?token=YOUR_TOKEN to the URL.</p>".to_string());
    }
    Html(INDEX_HTML.to_string())
}

/// Handle WebSocket upgrade (with optional token auth).
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebServerState>>,
    axum::extract::Query(query): axum::extract::Query<AuthQuery>,
) -> axum::response::Response {
    if !check_auth(&state, &query) {
        return (axum::http::StatusCode::UNAUTHORIZED, "Invalid token").into_response();
    }
    ws.on_upgrade(|socket| handle_ws(socket, state)).into_response()
}

/// Handle an active WebSocket connection.
async fn handle_ws(socket: WebSocket, state: Arc<WebServerState>) {
    info!("WebSocket client connected");

    {
        let mut active = state.active_ws.lock().await;
        *active = true;
    }

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Send welcome/boot status message on connect.
    if let Some(ref welcome) = state.welcome_message {
        let msg = ServerMessage::Message {
            role: "system".into(),
            content: welcome.clone(),
            level: Some("info".into()),
        };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = ws_tx.send(Message::Text(json.into())).await;
        }
    }

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
                    Ok(ClientMessage::PanelResponse { id, values, cancelled }) => {
                        // Route panel response to the pending oneshot sender.
                        // When the AI calls `ui_panel` on the Web channel,
                        // `request_panel()` inserts a oneshot sender here.
                        // Completing it unblocks the tool callback that is
                        // waiting for the user's response.
                        let response = PanelResponse {
                            id: id.clone(),
                            values,
                            cancelled,
                        };
                        let mut panels = state.pending_panels.lock().await;
                        if let Some(sender) = panels.remove(&id) {
                            if sender.send(response).is_err() {
                                warn!("Panel response receiver dropped for id={id}");
                            } else {
                                info!("Routed panel response for id={id}");
                            }
                        } else {
                            warn!("No pending panel found for id={id}");
                        }
                    }
                    Ok(ClientMessage::SwitchChannel { target }) => {
                        info!("Web client requests channel switch to: {target}");
                        if let Some(ref switcher) = state.switcher {
                            if let Some(kind) = aios_core::channel::ChannelKind::from_str_opt(&target) {
                                if let Err(e) = switcher.switch_to(kind) {
                                    warn!("Channel switch failed: {e}");
                                }
                            }
                        }
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
