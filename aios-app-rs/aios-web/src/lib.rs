//! Web channel for AiOS — HTTP server with WebSocket chat.
//!
//! Provides a full web-based chat interface accessible at `http://aios.local`
//! (or `http://<ip>:<port>`).  The web UI mirrors the Desktop experience:
//! chat, panels, images, settings — everything the GTK app can do.
//!
//! # Architecture
//!
//! - `axum` HTTP server serves the static web app and WebSocket endpoint
//! - WebSocket carries JSON messages for chat, panel requests, images, etc.
//! - The static client is a single-page HTML/CSS/JS app (no build system)
//! - Incoming messages are forwarded to the unified message channel
//! - AI responses are sent back over WebSocket

pub mod protocol;
pub mod server;
