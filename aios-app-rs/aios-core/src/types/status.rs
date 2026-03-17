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

    /// Format as a single line with green/red indicator using Pango markup.
    pub fn format(&self) -> String {
        let (icon, color) = if self.available {
            ("\u{2713}", "#2ed573") // ✓ green
        } else {
            ("\u{2717}", "#ff4757") // ✗ red
        };
        let status = if self.available { "available" } else { "unavailable" };
        let detail_part = if self.detail.is_empty() {
            String::new()
        } else {
            let escaped = self.detail
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!(" <span color='#888888'>\u{2014} {escaped}</span>")
        };
        format!(
            "  <span color='{color}'>{icon}</span> <b>{}</b>: <span color='#888888'>{status}</span>{detail_part}",
            self.component
        )
    }

    /// Format as HTML with green/red color.
    pub fn format_html(&self) -> String {
        let color = if self.available { "#2ed573" } else { "#ff4757" };
        let status = if self.available { "available" } else { "unavailable" };
        if self.detail.is_empty() {
            format!(
                "  <span style=\"color:{color}\">\u{25CF}</span> <b>{}</b>: {status}",
                self.component
            )
        } else {
            format!(
                "  <span style=\"color:{color}\">\u{25CF}</span> <b>{}</b>: {status} \u{2014} {}",
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

    /// Format the boot status as a Pango markup report.
    pub fn format(&self) -> String {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let mut parts = vec![
            format!("<b>AiOS System Status</b>"),
            format!("  <span color='#888888'>Boot time: {now}</span>"),
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

    // -- Additional edge-case tests --

    #[test]
    fn all_five_levels_have_distinct_icons() {
        let levels = [
            MessageLevel::Info,
            MessageLevel::Success,
            MessageLevel::Warning,
            MessageLevel::Important,
            MessageLevel::Error,
        ];
        let icons: Vec<&str> = levels.iter().map(|l| l.icon()).collect();
        // All icons must be distinct.
        let mut unique = icons.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "expected 5 distinct icons, got {:?}", icons);
    }

    #[test]
    fn all_five_levels_have_distinct_colors() {
        let levels = [
            MessageLevel::Info,
            MessageLevel::Success,
            MessageLevel::Warning,
            MessageLevel::Important,
            MessageLevel::Error,
        ];
        let colors: Vec<&str> = levels.iter().map(|l| l.color()).collect();
        let mut unique = colors.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "expected 5 distinct colors, got {:?}", colors);
    }

    #[test]
    fn all_five_levels_have_distinct_css_classes() {
        let levels = [
            MessageLevel::Info,
            MessageLevel::Success,
            MessageLevel::Warning,
            MessageLevel::Important,
            MessageLevel::Error,
        ];
        let classes: Vec<&str> = levels.iter().map(|l| l.css_class()).collect();
        let mut unique = classes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "expected 5 distinct CSS classes, got {:?}", classes);
    }

    #[test]
    fn boot_status_with_zero_lines() {
        let status = BootStatus::new();
        let report = status.format();
        assert!(report.contains("[INFO] AiOS System Status"));
        assert!(report.contains("Boot time:"));
    }

    #[test]
    fn boot_status_with_ten_lines() {
        let mut status = BootStatus::new();
        for i in 0..10 {
            status.add(StatusLine::new(
                format!("Component-{i}"),
                i % 2 == 0,
                format!("detail-{i}"),
            ));
        }
        let report = status.format();
        for i in 0..10 {
            assert!(report.contains(&format!("Component-{i}")));
            assert!(report.contains(&format!("detail-{i}")));
        }
        // Even-numbered components are available, odd are unavailable.
        assert!(report.contains("Component-0: available"));
        assert!(report.contains("Component-1: unavailable"));
    }

    #[test]
    fn status_line_with_empty_detail() {
        let line = StatusLine::new("TestComp", true, "");
        let formatted = line.format();
        assert!(formatted.contains("TestComp: available"));
        // With empty detail, there should be no dash separator.
        assert!(!formatted.contains("\u{2014}")); // em-dash
    }

    #[test]
    fn status_line_format_contains_component_name() {
        let line = StatusLine::new("LLM Provider", true, "Claude");
        let formatted = line.format();
        assert!(formatted.contains("LLM Provider"));
        assert!(formatted.contains("Claude"));
    }

    #[test]
    fn message_level_display_trait() {
        assert_eq!(format!("{}", MessageLevel::Info), "INFO");
        assert_eq!(format!("{}", MessageLevel::Success), "SUCCESS");
        assert_eq!(format!("{}", MessageLevel::Warning), "WARNING");
        assert_eq!(format!("{}", MessageLevel::Important), "IMPORTANT");
        assert_eq!(format!("{}", MessageLevel::Error), "ERROR");
    }

    #[test]
    fn message_level_format_includes_icon_and_label() {
        let formatted = MessageLevel::Error.format("something broke");
        assert!(formatted.contains("[ERROR]"));
        assert!(formatted.contains("something broke"));
        assert!(formatted.contains(MessageLevel::Error.icon()));
    }

    #[test]
    fn status_line_html_format() {
        let line = StatusLine::new("Web", true, "localhost:80");
        let html = line.format_html();
        assert!(html.contains("Web"));
        assert!(html.contains("available"));
        assert!(html.contains("localhost:80"));
        assert!(html.contains("#2ed573")); // green color for available
    }

    #[test]
    fn status_line_html_unavailable() {
        let line = StatusLine::new("Signal", false, "");
        let html = line.format_html();
        assert!(html.contains("unavailable"));
        assert!(html.contains("#ff4757")); // red color for unavailable
    }

    #[test]
    fn boot_status_html_format() {
        let mut status = BootStatus::new();
        status.add(StatusLine::new("Desktop", true, "GTK4"));
        let html = status.format_html();
        assert!(html.contains("[INFO] AiOS System Status"));
        assert!(html.contains("Desktop"));
        assert!(html.contains("GTK4"));
    }

    #[test]
    fn message_level_all_serialize_roundtrip() {
        let levels = [
            MessageLevel::Info,
            MessageLevel::Success,
            MessageLevel::Warning,
            MessageLevel::Important,
            MessageLevel::Error,
        ];
        for level in &levels {
            let json = serde_json::to_string(level).unwrap();
            let back: MessageLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(*level, back);
        }
    }
}
