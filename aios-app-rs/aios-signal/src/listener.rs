//! Signal message listener — reads incoming messages from signal-cli.
//!
//! Spawns `signal-cli daemon --json` as a child process and reads JSON
//! lines from stdout.  Each incoming message is parsed and forwarded to
//! the unified message channel.

use std::sync::Arc;

use aios_core::channel::{ChannelKind, IncomingMessage};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Listens for incoming Signal messages via signal-cli.
pub struct SignalListener {
    /// The phone number registered with signal-cli.
    phone: String,
    /// Only accept messages from these phone numbers.
    allowed_contacts: Vec<String>,
}

impl SignalListener {
    /// Create a new listener for the given phone number.
    pub fn new(phone: String, allowed_contacts: Vec<String>) -> Self {
        Self {
            phone,
            allowed_contacts,
        }
    }

    /// Start listening in the background.
    ///
    /// Spawns a Tokio task that runs `signal-cli daemon --json` and
    /// forwards incoming text messages to `tx`.
    pub fn start(
        &self,
        tx: mpsc::UnboundedSender<IncomingMessage>,
    ) -> tokio::task::JoinHandle<()> {
        let phone = self.phone.clone();
        let allowed = Arc::new(self.allowed_contacts.clone());

        tokio::spawn(async move {
            info!("Starting signal-cli daemon for {phone}");

            let result = Command::new("signal-cli")
                .args(["-a", &phone, "daemon", "--json"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            let mut child = match result {
                Ok(child) => child,
                Err(e) => {
                    error!("Failed to start signal-cli: {e}");
                    return;
                }
            };

            let stdout = match child.stdout.take() {
                Some(stdout) => stdout,
                None => {
                    error!("signal-cli stdout not available");
                    return;
                }
            };

            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }

                // Parse the JSON envelope from signal-cli.
                let parsed: serde_json::Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("Failed to parse signal-cli JSON: {e}");
                        continue;
                    }
                };

                // Extract sender and message text from the envelope.
                let envelope = parsed.get("envelope").unwrap_or(&parsed);
                let sender = envelope
                    .get("source")
                    .or_else(|| envelope.get("sourceNumber"))
                    .and_then(|v| v.as_str());

                let text = envelope
                    .get("dataMessage")
                    .and_then(|dm| dm.get("message"))
                    .and_then(|v| v.as_str());

                if let (Some(sender), Some(text)) = (sender, text) {
                    // Check allow list.
                    if !allowed.is_empty() && !allowed.iter().any(|a| a == sender) {
                        warn!("Ignoring message from unknown sender: {sender}");
                        continue;
                    }

                    info!("Signal message from {sender}: {}", &text[..text.len().min(50)]);

                    let msg = IncomingMessage {
                        channel: ChannelKind::Signal,
                        text: text.to_string(),
                        sender_id: Some(sender.to_string()),
                    };

                    if tx.send(msg).is_err() {
                        warn!("Message channel closed, stopping Signal listener");
                        break;
                    }
                }
            }

            info!("Signal listener stopped");
        })
    }
}
