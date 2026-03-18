//! Display tool — render images, notifications, and markdown in the UI.
//!
//! The display tool does not directly manipulate GUI widgets.  Instead it
//! invokes registered *callback* functions that the UI layer provides at
//! startup.  This keeps the tool layer decoupled from any particular frontend.
//!
//! Ported from `aios-app/aios/tools/builtin/display.py`.

use std::sync::Arc;

use aios_core::channel::{ChannelContext, ChannelKind};
use aios_core::types::ToolResult;
use tracing::warn;

use crate::tool::Tool;

/// Callback type for showing an image (receives `path_or_url`).
pub type ShowImageCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Callback type for showing a desktop notification (receives `title`, `message`).
pub type ShowNotificationCallback = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Callback type for rendering markdown text.
pub type ShowMarkdownCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Display content to the user: show an image, send a desktop notification,
/// or render markdown text.
///
/// Callbacks are set by the UI layer at startup via the `set_*` methods.
/// If no callback is registered for an action, the tool returns a text
/// description instead of invoking the UI.
pub struct DisplayTool {
    show_image_cb: Option<ShowImageCallback>,
    show_notification_cb: Option<ShowNotificationCallback>,
    show_markdown_cb: Option<ShowMarkdownCallback>,
}

impl DisplayTool {
    /// Create a new `DisplayTool` with no callbacks registered.
    pub fn new() -> Self {
        Self {
            show_image_cb: None,
            show_notification_cb: None,
            show_markdown_cb: None,
        }
    }

    /// Register the callback for showing images.
    pub fn set_show_image_callback(&mut self, cb: ShowImageCallback) {
        self.show_image_cb = Some(cb);
    }

    /// Register the callback for showing desktop notifications.
    pub fn set_show_notification_callback(&mut self, cb: ShowNotificationCallback) {
        self.show_notification_cb = Some(cb);
    }

    /// Register the callback for rendering markdown.
    pub fn set_show_markdown_callback(&mut self, cb: ShowMarkdownCallback) {
        self.show_markdown_cb = Some(cb);
    }
}

impl Default for DisplayTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for DisplayTool {
    fn name(&self) -> &str {
        "display"
    }

    fn description(&self) -> &str {
        "Display content to the user: show an image, send a desktop \
         notification, or render markdown text."
    }

    fn category(&self) -> &str {
        "ui"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["show_image", "show_notification", "show_markdown"],
                    "description": "Display action to perform."
                },
                "path": {
                    "type": "string",
                    "description": "File path or URL of the image (for show_image)."
                },
                "title": {
                    "type": "string",
                    "description": "Notification title (for show_notification)."
                },
                "message": {
                    "type": "string",
                    "description": "Notification body text (for show_notification)."
                },
                "text": {
                    "type": "string",
                    "description": "Markdown text to render (for show_markdown)."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        self.execute_on_channel(args, &ChannelContext::default())
    }

    fn execute_on_channel(
        &self,
        args: serde_json::Value,
        channel: &ChannelContext,
    ) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        // On channels without rich UI (Signal, Voice), return text-only
        // fallbacks instead of trying to invoke GUI callbacks.
        match channel.kind {
            ChannelKind::Signal | ChannelKind::Voice => {
                return self.execute_text_fallback(action, &args);
            }
            // Desktop and Web use the registered callbacks.
            ChannelKind::Desktop | ChannelKind::Web => {}
            // System channel has no rendering surface.
            ChannelKind::System => {
                return self.execute_text_fallback(action, &args);
            }
        }

        match action {
            "show_image" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                self.show_image(path)
            }
            "show_notification" => {
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("AiOS");
                let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
                self.show_notification(title, message)
            }
            "show_markdown" => {
                let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
                self.show_markdown(text)
            }
            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. Use: show_image, show_notification, show_markdown."
            )),
        }
    }
}

impl DisplayTool {
    /// Text-only fallback for channels without rich UI (Signal, Voice).
    fn execute_text_fallback(&self, action: &str, args: &serde_json::Value) -> ToolResult {
        match action {
            "show_image" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                if path.is_empty() {
                    return ToolResult::fail("'path' is required for show_image.");
                }
                ToolResult::ok_with_data(
                    format!("Image ready: {path}"),
                    serde_json::json!({ "type": "image", "path": path }),
                )
            }
            "show_notification" => {
                let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("AiOS");
                let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
                if message.is_empty() {
                    return ToolResult::fail("'message' is required for show_notification.");
                }
                ToolResult::ok_with_data(
                    format!("[{title}] {message}"),
                    serde_json::json!({
                        "type": "notification",
                        "title": title,
                        "message": message,
                    }),
                )
            }
            "show_markdown" => {
                let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if text.is_empty() {
                    return ToolResult::fail("'text' is required for show_markdown.");
                }
                // Strip markdown for text-only channels.
                ToolResult::ok_with_data(
                    text.to_string(),
                    serde_json::json!({ "type": "markdown", "text": text }),
                )
            }
            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. Use: show_image, show_notification, show_markdown."
            )),
        }
    }

    /// Show an image via the registered callback, or return a text fallback.
    fn show_image(&self, path_or_url: &str) -> ToolResult {
        if path_or_url.is_empty() {
            return ToolResult::fail("'path' is required for show_image.");
        }

        match &self.show_image_cb {
            Some(cb) => {
                // Invoke the UI callback.  If it panics we catch it below.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cb(path_or_url);
                }));
                match result {
                    Ok(()) => ToolResult::ok_with_data(
                        format!("Displayed image: {path_or_url}"),
                        serde_json::json!({ "type": "image", "path": path_or_url }),
                    ),
                    Err(_) => ToolResult::fail("Failed to display image: callback panicked."),
                }
            }
            None => {
                warn!("show_image callback not registered; returning path only");
                ToolResult::ok_with_data(
                    format!("Image ready: {path_or_url}"),
                    serde_json::json!({ "type": "image", "path": path_or_url }),
                )
            }
        }
    }

    /// Show a desktop notification via the registered callback.
    fn show_notification(&self, title: &str, message: &str) -> ToolResult {
        if message.is_empty() {
            return ToolResult::fail("'message' is required for show_notification.");
        }

        match &self.show_notification_cb {
            Some(cb) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cb(title, message);
                }));
                match result {
                    Ok(()) => ToolResult::ok_with_data(
                        format!("Notification sent: [{title}] {message}"),
                        serde_json::json!({
                            "type": "notification",
                            "title": title,
                            "message": message,
                        }),
                    ),
                    Err(_) => {
                        ToolResult::fail("Failed to show notification: callback panicked.")
                    }
                }
            }
            None => {
                warn!("show_notification callback not registered; logging only");
                ToolResult::ok_with_data(
                    format!("Notification: [{title}] {message}"),
                    serde_json::json!({
                        "type": "notification",
                        "title": title,
                        "message": message,
                    }),
                )
            }
        }
    }

    /// Render markdown text via the registered callback.
    fn show_markdown(&self, text: &str) -> ToolResult {
        if text.is_empty() {
            return ToolResult::fail("'text' is required for show_markdown.");
        }

        match &self.show_markdown_cb {
            Some(cb) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cb(text);
                }));
                match result {
                    Ok(()) => ToolResult::ok_with_data(
                        "Markdown rendered.".to_string(),
                        serde_json::json!({ "type": "markdown", "text": text }),
                    ),
                    Err(_) => ToolResult::fail("Failed to render markdown: callback panicked."),
                }
            }
            None => {
                warn!("show_markdown callback not registered; returning raw text");
                ToolResult::ok_with_data(
                    text.to_string(),
                    serde_json::json!({ "type": "markdown", "text": text }),
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_image_no_callback() {
        let tool = DisplayTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "show_image",
            "path": "/tmp/test.png"
        }));
        assert!(r.success);
        assert!(r.output.contains("Image ready"));
    }

    #[test]
    fn show_image_with_callback() {
        let mut tool = DisplayTool::new();
        let called = Arc::new(std::sync::Mutex::new(false));
        let called_clone = called.clone();
        tool.set_show_image_callback(Arc::new(move |_path| {
            *called_clone.lock().unwrap() = true;
        }));

        let r = tool.execute(serde_json::json!({
            "action": "show_image",
            "path": "/tmp/test.png"
        }));
        assert!(r.success);
        assert!(r.output.contains("Displayed image"));
        assert!(*called.lock().unwrap());
    }

    #[test]
    fn show_image_requires_path() {
        let tool = DisplayTool::new();
        let r = tool.execute(serde_json::json!({ "action": "show_image" }));
        assert!(!r.success);
    }

    #[test]
    fn show_notification_no_callback() {
        let tool = DisplayTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "show_notification",
            "title": "Test",
            "message": "Hello world"
        }));
        assert!(r.success);
        assert!(r.output.contains("Notification"));
    }

    #[test]
    fn show_notification_requires_message() {
        let tool = DisplayTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "show_notification",
            "title": "Test"
        }));
        assert!(!r.success);
    }

    #[test]
    fn show_notification_default_title() {
        let tool = DisplayTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "show_notification",
            "message": "hello"
        }));
        assert!(r.success);
        assert!(r.output.contains("[AiOS]"));
    }

    #[test]
    fn show_markdown_no_callback() {
        let tool = DisplayTool::new();
        let r = tool.execute(serde_json::json!({
            "action": "show_markdown",
            "text": "# Hello\n\nWorld"
        }));
        assert!(r.success);
        assert!(r.output.contains("# Hello"));
    }

    #[test]
    fn show_markdown_requires_text() {
        let tool = DisplayTool::new();
        let r = tool.execute(serde_json::json!({ "action": "show_markdown" }));
        assert!(!r.success);
    }

    #[test]
    fn unknown_action() {
        let tool = DisplayTool::new();
        let r = tool.execute(serde_json::json!({ "action": "explode" }));
        assert!(!r.success);
    }
}
