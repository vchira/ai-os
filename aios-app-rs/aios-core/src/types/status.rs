//! Message severity levels and boot status reporting.
//!
//! Messages in the AiOS chat can carry a severity level (INFO, WARNING,
//! IMPORTANT, ERROR, SUCCESS) which frontends render with appropriate
//! icons and colors.

use std::fmt;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// MessageLevel
// ---------------------------------------------------------------------------

/// Severity level for a chat message.
///
/// Frontends render these with distinct icons and colors:
/// - **Info**: blue, `(i)` icon
/// - **Success**: green, checkmark icon
/// - **Warning**: yellow, triangle icon
/// - **Important**: orange, exclamation icon
/// - **Error**: red, X icon
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageLevel {
    Info,
    Success,
    Warning,
    Important,
    Error,
}

impl MessageLevel {
    /// The icon prefix for this level (UTF-8 symbols).
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Info => "\u{2139}\u{fe0f}",       // ℹ️
            Self::Success => "\u{2705}",              // ✅
            Self::Warning => "\u{26a0}\u{fe0f}",     // ⚠️
            Self::Important => "\u{2757}",            // ❗
            Self::Error => "\u{274c}",                // ❌
        }
    }

    /// The label text for this level.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Success => "SUCCESS",
            Self::Warning => "WARNING",
            Self::Important => "IMPORTANT",
            Self::Error => "ERROR",
        }
    }

    /// CSS class name for this level.
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Info => "msg-info",
            Self::Success => "msg-success",
            Self::Warning => "msg-warning",
            Self::Important => "msg-important",
            Self::Error => "msg-error",
        }
    }

    /// HTML color for this level.
    pub fn color(&self) -> &'static str {
        match self {
            Self::Info => "#4a9eff",
            Self::Success => "#2ed573",
            Self::Warning => "#ffa502",
            Self::Important => "#ff6348",
            Self::Error => "#ff4757",
        }
    }

    /// Format a message with the level prefix: `[ICON LABEL] content`.
    pub fn format(&self, content: &str) -> String {
        format!("{} [{}] {}", self.icon(), self.label(), content)
    }
}

impl fmt::Display for MessageLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// StatusLine
// ---------------------------------------------------------------------------

/// A single line in a boot status report.
#[derive(Debug, Clone)]
pub struct StatusLine {
    /// The component name (e.g. "Web Channel", "Signal", "LLM Provider").
    pub component: String,
    /// Whether the component is available / running.
    pub available: bool,
    /// Extra detail (e.g. URL, phone number, provider name).
    pub detail: String,
}

impl StatusLine {
    /// Create a new status line.
    pub fn new(component: impl Into<String>, available: bool, detail: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            available,
            detail: detail.into(),
        }
    }

    /// Format as a single line with green/red indicator.
    pub fn format(&self) -> String {
        let icon = if self.available { "\u{2705}" } else { "\u{274c}" }; // ✅ / ❌
        let status = if self.available { "available" } else { "unavailable" };
        if self.detail.is_empty() {
            format!("  {icon} {}: {status}", self.component)
        } else {
            format!("  {icon} {}: {status} — {}", self.component, self.detail)
        }
    }

    /// Format as HTML with green/red color.
    pub fn format_html(&self) -> String {
        let color = if self.available { "#2ed573" } else { "#ff4757" };
        let status = if self.available { "available" } else { "unavailable" };
        if self.detail.is_empty() {
            format!(
                r#"  <span style="color:{color}">\u25CF</span> <b>{}</b>: {status}"#,
                self.component
            )
        } else {
            format!(
                r#"  <span style="color:{color}">\u25CF</span> <b>{}</b>: {status} — {}"#,
                self.component, self.detail
            )
        }
    }
}

// ---------------------------------------------------------------------------
// BootStatus
// ---------------------------------------------------------------------------

/// Collects system status at boot time and formats a report.
pub struct BootStatus {
    lines: Vec<StatusLine>,
}

impl BootStatus {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Add a status line.
    pub fn add(&mut self, line: StatusLine) {
        self.lines.push(line);
    }

    /// Format the boot status as a plain-text report.
    pub fn format(&self) -> String {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let mut parts = vec![
            format!("{} [{}] AiOS System Status", MessageLevel::Info.icon(), MessageLevel::Info.label()),
            format!("  Boot time: {now}"),
            String::new(),
        ];

        for line in &self.lines {
            parts.push(line.format());
        }

        parts.join("\n")
    }

    /// Format as HTML (for the web client).
    pub fn format_html(&self) -> String {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let mut parts = vec![
            format!(
                r#"<div style="color:#4a9eff;font-weight:bold">{} [INFO] AiOS System Status</div>"#,
                MessageLevel::Info.icon()
            ),
            format!(r#"<div style="color:#8892a4">  Boot time: {now}</div>"#),
            String::new(),
        ];

        for line in &self.lines {
            parts.push(line.format_html());
        }

        parts.join("\n")
    }
}

impl Default for BootStatus {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_level_format() {
        let msg = MessageLevel::Info.format("System ready");
        assert!(msg.contains("[INFO]"));
        assert!(msg.contains("System ready"));
    }

    #[test]
    fn message_level_success_format() {
        let msg = MessageLevel::Success.format("Web available");
        assert!(msg.contains("[SUCCESS]"));
        assert!(msg.contains("\u{2705}"));
    }

    #[test]
    fn message_level_css_class() {
        assert_eq!(MessageLevel::Error.css_class(), "msg-error");
        assert_eq!(MessageLevel::Success.css_class(), "msg-success");
    }

    #[test]
    fn status_line_available() {
        let line = StatusLine::new("Web Channel", true, "http://aios.local");
        let formatted = line.format();
        assert!(formatted.contains("\u{2705}"));
        assert!(formatted.contains("available"));
        assert!(formatted.contains("http://aios.local"));
    }

    #[test]
    fn status_line_unavailable() {
        let line = StatusLine::new("Signal", false, "not configured");
        let formatted = line.format();
        assert!(formatted.contains("\u{274c}"));
        assert!(formatted.contains("unavailable"));
    }

    #[test]
    fn boot_status_report() {
        let mut status = BootStatus::new();
        status.add(StatusLine::new("Desktop", true, "GTK4"));
        status.add(StatusLine::new("Web Channel", true, "http://aios.local:80"));
        status.add(StatusLine::new("Signal", false, "not configured"));
        status.add(StatusLine::new("LLM Provider", true, "Claude"));

        let report = status.format();
        assert!(report.contains("[INFO] AiOS System Status"));
        assert!(report.contains("Desktop: available"));
        assert!(report.contains("Web Channel: available"));
        assert!(report.contains("http://aios.local:80"));
        assert!(report.contains("Signal: unavailable"));
        assert!(report.contains("LLM Provider: available"));
    }

    #[test]
    fn message_level_serializes() {
        let json = serde_json::to_string(&MessageLevel::Warning).unwrap();
        assert_eq!(json, "\"warning\"");
        let back: MessageLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, MessageLevel::Warning);
    }
}
