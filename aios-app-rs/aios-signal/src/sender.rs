//! Signal message sender — sends messages via signal-cli.

use tracing::{error, info};

/// Sends messages and attachments via signal-cli.
pub struct SignalSender {
    /// The phone number registered with signal-cli.
    phone: String,
}

impl SignalSender {
    /// Create a new sender for the given phone number.
    pub fn new(phone: String) -> Self {
        Self { phone }
    }

    /// Send a text message to a recipient.
    pub async fn send_text(&self, recipient: &str, text: &str) -> Result<(), String> {
        info!("Sending Signal message to {recipient}");

        let output = tokio::process::Command::new("signal-cli")
            .args(["-a", &self.phone, "send", "-m", text, recipient])
            .output()
            .await
            .map_err(|e| format!("Failed to run signal-cli: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("signal-cli send failed: {stderr}");
            return Err(format!("signal-cli send failed: {stderr}"));
        }

        Ok(())
    }

    /// Send an image attachment to a recipient.
    pub async fn send_image(
        &self,
        recipient: &str,
        path: &str,
        caption: &str,
    ) -> Result<(), String> {
        info!("Sending Signal image to {recipient}: {path}");

        let mut args = vec!["-a", &self.phone, "send", "-a", path];
        if !caption.is_empty() {
            args.extend(["-m", caption]);
        }
        args.push(recipient);

        let output = tokio::process::Command::new("signal-cli")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("Failed to run signal-cli: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("signal-cli send image failed: {stderr}");
            return Err(format!("signal-cli send image failed: {stderr}"));
        }

        Ok(())
    }

    /// Send a "channel moved" notification.
    pub async fn send_channel_moved(&self, recipient: &str) -> Result<(), String> {
        self.send_text(recipient, "Conversation moved back to desktop.")
            .await
    }
}
