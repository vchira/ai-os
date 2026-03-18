//! Browse URL tool — open a webpage for the user to view.
//!
//! On Desktop/Web channels, the tool signals the UI layer to open or embed
//! the URL.  On Signal/Voice channels (no browser), it returns the URL as
//! plain text so the AI can relay it.
//!
//! This is a channel-aware tool: it overrides [`execute_on_channel`] to
//! adapt behaviour per channel.

use std::sync::Arc;

use aios_core::channel::{ChannelContext, ChannelKind};
use aios_core::types::ToolResult;
use tracing::warn;

use crate::tool::Tool;

/// Callback type for opening a URL in the user's browser or embedded view.
///
/// The UI layer (GTK or Web) registers this at startup. Receives the URL
/// to open.
pub type BrowseUrlCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Open a webpage from a URL so the user can view it.
///
/// On Desktop, this launches the URL via `gtk::UriLauncher` or `open::that()`.
/// On Web, it sends metadata so the client can render an iframe or clickable
/// link.  On Signal/Voice, it returns the URL as text.
pub struct BrowseUrlTool {
    browse_cb: Option<BrowseUrlCallback>,
}

impl BrowseUrlTool {
    /// Create a new `BrowseUrlTool` with no callback registered.
    pub fn new() -> Self {
        Self { browse_cb: None }
    }

    /// Register the callback for opening URLs.
    pub fn set_browse_callback(&mut self, cb: BrowseUrlCallback) {
        self.browse_cb = Some(cb);
    }
}

impl Default for BrowseUrlTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for BrowseUrlTool {
    fn name(&self) -> &str {
        "browse_url"
    }

    fn description(&self) -> &str {
        "Open a webpage from a URL so the user can view it in their browser \
         or embedded viewer."
    }

    fn category(&self) -> &str {
        "ui"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL of the webpage to open (must start with http:// or https://)."
                }
            },
            "required": ["url"]
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
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        // --- Validation ---
        if url.is_empty() {
            return ToolResult::fail("'url' is required for browse_url.");
        }

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult::fail(format!(
                "Invalid URL: {url:?}. URL must start with http:// or https://."
            ));
        }

        // On channels without a browser (Signal, Voice, System), return the
        // URL as plain text so the AI can relay it.
        match channel.kind {
            ChannelKind::Signal | ChannelKind::Voice | ChannelKind::System => {
                return ToolResult::ok_with_data(
                    format!("Here is the link: {url}"),
                    serde_json::json!({
                        "type": "browse_url",
                        "url": url,
                    }),
                );
            }
            ChannelKind::Desktop | ChannelKind::Web => {}
        }

        // --- Desktop / Web ---
        match channel.kind {
            ChannelKind::Web => {
                // For the web channel, return metadata that the web client
                // can use to render an iframe or open a new tab.
                ToolResult::ok_with_data(
                    format!("Opening {url} in web view."),
                    serde_json::json!({
                        "type": "browse_url",
                        "url": url,
                    }),
                )
            }
            ChannelKind::Desktop => {
                // On desktop, invoke the registered callback (which should
                // use gtk::UriLauncher or open::that).
                match &self.browse_cb {
                    Some(cb) => {
                        let result =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                cb(url);
                            }));
                        match result {
                            Ok(()) => ToolResult::ok_with_data(
                                format!("Opened {url} in the browser."),
                                serde_json::json!({
                                    "type": "browse_url",
                                    "url": url,
                                }),
                            ),
                            Err(_) => ToolResult::fail(
                                "Failed to open URL: browser callback panicked.",
                            ),
                        }
                    }
                    None => {
                        warn!("browse_url callback not registered; returning URL only");
                        ToolResult::ok_with_data(
                            format!("URL ready: {url}"),
                            serde_json::json!({
                                "type": "browse_url",
                                "url": url,
                            }),
                        )
                    }
                }
            }
            // Already handled above, but satisfy the compiler.
            _ => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Metadata tests --

    #[test]
    fn tool_name_is_browse_url() {
        let tool = BrowseUrlTool::new();
        assert_eq!(tool.name(), "browse_url");
    }

    #[test]
    fn tool_category_is_ui() {
        let tool = BrowseUrlTool::new();
        assert_eq!(tool.category(), "ui");
    }

    #[test]
    fn tool_description_is_non_empty() {
        let tool = BrowseUrlTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn tool_parameters_has_url() {
        let tool = BrowseUrlTool::new();
        let params = tool.parameters();
        let props = params["properties"].as_object().unwrap();
        assert!(props.contains_key("url"));
        let required = params["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("url")));
    }

    #[test]
    fn default_trait_creates_tool() {
        let tool = BrowseUrlTool::default();
        assert_eq!(tool.name(), "browse_url");
    }

    #[test]
    fn to_schema_produces_valid_schema() {
        let tool = BrowseUrlTool::new();
        let schema = tool.to_schema();
        assert_eq!(schema.name, "browse_url");
        assert!(!schema.description.is_empty());
        assert!(schema.parameters.is_object());
    }

    // -- Valid URL tests --

    #[test]
    fn valid_https_url_succeeds() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({
            "url": "https://example.com"
        }));
        assert!(r.success);
        assert!(r.output.contains("example.com"));
    }

    #[test]
    fn valid_http_url_succeeds() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({
            "url": "http://example.com"
        }));
        assert!(r.success);
        assert!(r.output.contains("example.com"));
    }

    #[test]
    fn valid_url_returns_data_with_type_and_url() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({
            "url": "https://example.com/page"
        }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data["type"].as_str().unwrap(), "browse_url");
        assert_eq!(data["url"].as_str().unwrap(), "https://example.com/page");
    }

    // -- Empty / missing URL tests --

    #[test]
    fn empty_url_fails() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({ "url": "" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("url"));
    }

    #[test]
    fn missing_url_fails() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({}));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("url"));
    }

    // -- Malformed URL tests --

    #[test]
    fn malformed_url_no_scheme_fails() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({
            "url": "example.com"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("http"));
    }

    #[test]
    fn malformed_url_ftp_scheme_fails() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({
            "url": "ftp://example.com"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("http"));
    }

    #[test]
    fn malformed_url_random_string_fails() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({
            "url": "not-a-url"
        }));
        assert!(!r.success);
    }

    #[test]
    fn whitespace_only_url_fails() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({
            "url": "   "
        }));
        assert!(!r.success);
    }

    // -- Channel-aware tests --

    #[test]
    fn desktop_channel_no_callback_returns_url_ready() {
        let tool = BrowseUrlTool::new();
        let channel = ChannelContext::desktop();
        let r = tool.execute_on_channel(
            serde_json::json!({ "url": "https://example.com" }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("URL ready"));
    }

    #[test]
    fn desktop_channel_with_callback_opens_url() {
        let mut tool = BrowseUrlTool::new();
        let called = Arc::new(std::sync::Mutex::new(String::new()));
        let called_clone = called.clone();
        tool.set_browse_callback(Arc::new(move |url| {
            *called_clone.lock().unwrap() = url.to_string();
        }));

        let channel = ChannelContext::desktop();
        let r = tool.execute_on_channel(
            serde_json::json!({ "url": "https://example.com" }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("Opened"));
        assert_eq!(*called.lock().unwrap(), "https://example.com");
    }

    #[test]
    fn web_channel_returns_browse_metadata() {
        let tool = BrowseUrlTool::new();
        let channel = ChannelContext::web();
        let r = tool.execute_on_channel(
            serde_json::json!({ "url": "https://example.com" }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("Opening"));
        let data = r.data.unwrap();
        assert_eq!(data["type"].as_str().unwrap(), "browse_url");
        assert_eq!(data["url"].as_str().unwrap(), "https://example.com");
    }

    #[test]
    fn signal_channel_returns_text_link() {
        let tool = BrowseUrlTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "url": "https://example.com" }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("https://example.com"));
        assert!(r.output.contains("link"));
    }

    #[test]
    fn voice_channel_returns_text_link() {
        let tool = BrowseUrlTool::new();
        let channel = ChannelContext::voice();
        let r = tool.execute_on_channel(
            serde_json::json!({ "url": "https://example.com" }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("https://example.com"));
    }

    // -- Callback panics --

    #[test]
    fn desktop_callback_panic_returns_failure() {
        let mut tool = BrowseUrlTool::new();
        tool.set_browse_callback(Arc::new(|_url| {
            panic!("simulated callback panic");
        }));

        let channel = ChannelContext::desktop();
        let r = tool.execute_on_channel(
            serde_json::json!({ "url": "https://example.com" }),
            &channel,
        );
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("panicked"));
    }

    // -- URL trimming --

    #[test]
    fn url_with_leading_trailing_whitespace_is_trimmed() {
        let tool = BrowseUrlTool::new();
        let r = tool.execute(serde_json::json!({
            "url": "  https://example.com  "
        }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data["url"].as_str().unwrap(), "https://example.com");
    }
}
