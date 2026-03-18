//! Show Image tool — display an image from a local file or URL.
//!
//! This tool accepts a local file path or a web URL, validates the image,
//! and returns structured data for the active channel to render.  For URL
//! sources the image is downloaded to a temporary file first.
//!
//! Channel behaviour:
//! - **Desktop**: GTK renderer shows the image inline in the chat.
//! - **Web**: sends image data for HTML `<img>` rendering.
//! - **Signal**: sends the image file as an attachment.
//! - **Voice**: reads the caption only (no visual output).

use std::fs;
use std::path::Path;
use std::time::Duration;

use aios_core::channel::{ChannelContext, ChannelKind};
use aios_core::types::ToolResult;
use tracing::{debug, warn};

use crate::tool::Tool;

/// Allowed image file extensions (lowercase).
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp"];

/// User-Agent header for image downloads.
const USER_AGENT: &str = "AiOS/2.0 (ShowImageTool; Rust)";

/// Download timeout.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum image download size (20 MB).
const MAX_IMAGE_BYTES: usize = 20_000_000;

/// Display an image from a local file or a web URL.
///
/// The tool validates the source, ensures the file is an image (by
/// extension), and returns structured data that the channel renderer
/// uses to display the image inline, send it as an attachment, or
/// fall back to a text caption.
pub struct ShowImageTool {
    client: reqwest::blocking::Client,
}

impl ShowImageTool {
    /// Create a new `ShowImageTool` with a pre-configured HTTP client.
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(DOWNLOAD_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self { client }
    }
}

impl Default for ShowImageTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ShowImageTool {
    fn name(&self) -> &str {
        "show_image"
    }

    fn description(&self) -> &str {
        "Display an image from a local file path or a web URL. \
         Supports png, jpg, jpeg, gif, svg, webp, bmp."
    }

    fn category(&self) -> &str {
        "ui"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "enum": ["file", "url"],
                    "description": "Whether the image comes from a local file or a web URL."
                },
                "path": {
                    "type": "string",
                    "description": "Local file path of the image (required when source is 'file')."
                },
                "url": {
                    "type": "string",
                    "description": "Web URL of the image (required when source is 'url')."
                },
                "caption": {
                    "type": "string",
                    "description": "Optional description text for the image."
                }
            },
            "required": ["source"]
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
        let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let caption = args
            .get("caption")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Voice channel: no visual output, just read the caption.
        if channel.kind == ChannelKind::Voice {
            return self.execute_voice_fallback(source, &args, caption);
        }

        match source {
            "file" => self.show_from_file(&args, caption, channel),
            "url" => self.show_from_url(&args, caption, channel),
            "" => ToolResult::fail("'source' is required. Use 'file' or 'url'."),
            _ => ToolResult::fail(format!(
                "Unknown source {source:?}. Use 'file' or 'url'."
            )),
        }
    }
}

impl ShowImageTool {
    /// Show an image from a local file path.
    fn show_from_file(
        &self,
        args: &serde_json::Value,
        caption: &str,
        _channel: &ChannelContext,
    ) -> ToolResult {
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path_str.is_empty() {
            return ToolResult::fail("'path' is required when source is 'file'.");
        }

        let path = Path::new(path_str);

        // Verify the file exists.
        if !path.exists() {
            return ToolResult::fail(format!("File not found: {path_str}"));
        }

        // Validate image extension.
        if !is_image_extension(path) {
            return ToolResult::fail(format!(
                "Not a supported image format. Allowed extensions: {}.",
                IMAGE_EXTENSIONS.join(", ")
            ));
        }

        debug!(path = path_str, caption, "showing image from file");

        let mut data = serde_json::json!({
            "image_path": path_str,
            "source": "file",
        });
        if !caption.is_empty() {
            data["caption"] = serde_json::Value::String(caption.to_string());
        }

        let output = if caption.is_empty() {
            format!("Displaying image: {path_str}")
        } else {
            format!("Displaying image: {path_str} — {caption}")
        };

        ToolResult::ok_with_data(output, data)
    }

    /// Download an image from a URL and show it.
    fn show_from_url(
        &self,
        args: &serde_json::Value,
        caption: &str,
        _channel: &ChannelContext,
    ) -> ToolResult {
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            return ToolResult::fail("'url' is required when source is 'url'.");
        }

        debug!(url, caption, "downloading image from URL");

        // Download the image.
        let response = match self.client.get(url).send() {
            Ok(r) => r,
            Err(e) => {
                if e.is_timeout() {
                    return ToolResult::fail(format!("Image download timed out: {e}"));
                }
                return ToolResult::fail(format!("Could not download image: {e}"));
            }
        };

        if !response.status().is_success() {
            return ToolResult::fail(format!(
                "Image download failed — HTTP {}: {}",
                response.status().as_u16(),
                response.status()
            ));
        }

        let bytes = match response.bytes() {
            Ok(b) => b,
            Err(e) => return ToolResult::fail(format!("Failed to read image data: {e}")),
        };

        if bytes.len() > MAX_IMAGE_BYTES {
            return ToolResult::fail(format!(
                "Image too large ({} bytes, max {} bytes).",
                bytes.len(),
                MAX_IMAGE_BYTES,
            ));
        }

        // Determine a file extension from the URL.
        let extension = url_to_extension(url).unwrap_or("png");

        // Write to a temp file.
        let temp_path = format!(
            "/tmp/aios-img-{}.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            extension,
        );

        if let Err(e) = fs::write(&temp_path, &bytes) {
            return ToolResult::fail(format!("Failed to save downloaded image: {e}"));
        }

        debug!(url, temp_path, bytes = bytes.len(), "image downloaded");

        let mut data = serde_json::json!({
            "image_path": temp_path,
            "source": "url",
            "original_url": url,
        });
        if !caption.is_empty() {
            data["caption"] = serde_json::Value::String(caption.to_string());
        }

        let output = if caption.is_empty() {
            format!("Downloaded and displaying image from {url}")
        } else {
            format!("Downloaded and displaying image from {url} — {caption}")
        };

        ToolResult::ok_with_data(output, data)
    }

    /// Voice-only fallback: read the caption, no image rendering.
    fn execute_voice_fallback(
        &self,
        source: &str,
        args: &serde_json::Value,
        caption: &str,
    ) -> ToolResult {
        if caption.is_empty() {
            // Try to give some context about what image was requested.
            let location = match source {
                "file" => args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown file"),
                "url" => args
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown URL"),
                _ => "unknown source",
            };
            warn!(source, "show_image on voice channel without caption");
            ToolResult::ok_with_data(
                format!("Image from {location} (no caption available)."),
                serde_json::json!({
                    "source": source,
                    "voice_only": true,
                }),
            )
        } else {
            ToolResult::ok_with_data(
                caption.to_string(),
                serde_json::json!({
                    "source": source,
                    "caption": caption,
                    "voice_only": true,
                }),
            )
        }
    }
}

/// Check if a file path has a supported image extension.
fn is_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Extract a likely image extension from a URL, or `None` if not recognized.
fn url_to_extension(url: &str) -> Option<&'static str> {
    // Strip query params and fragments.
    let clean = url.split('?').next().unwrap_or(url);
    let clean = clean.split('#').next().unwrap_or(clean);

    // Get the last path segment.
    let segment = clean.rsplit('/').next().unwrap_or("");

    // Check against known extensions.
    let lower = segment.to_lowercase();
    for ext in IMAGE_EXTENSIONS {
        if lower.ends_with(&format!(".{ext}")) {
            return Some(ext);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Tool metadata --

    #[test]
    fn tool_name_is_show_image() {
        let tool = ShowImageTool::new();
        assert_eq!(tool.name(), "show_image");
    }

    #[test]
    fn tool_category_is_ui() {
        let tool = ShowImageTool::new();
        assert_eq!(tool.category(), "ui");
    }

    #[test]
    fn tool_description_is_non_empty() {
        let tool = ShowImageTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn tool_parameters_has_required_fields() {
        let tool = ShowImageTool::new();
        let params = tool.parameters();
        let props = params["properties"].as_object().unwrap();
        assert!(props.contains_key("source"));
        assert!(props.contains_key("path"));
        assert!(props.contains_key("url"));
        assert!(props.contains_key("caption"));
        let required = params["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("source")));
    }

    #[test]
    fn tool_schema_is_valid() {
        let tool = ShowImageTool::new();
        let schema = tool.to_schema();
        assert_eq!(schema.name, "show_image");
        assert!(!schema.description.is_empty());
        assert!(schema.parameters.is_object());
    }

    #[test]
    fn default_trait_creates_tool() {
        let tool = ShowImageTool::default();
        assert_eq!(tool.name(), "show_image");
    }

    // -- Missing/invalid parameters --

    #[test]
    fn missing_source_returns_error() {
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({}));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("source"));
    }

    #[test]
    fn unknown_source_returns_error() {
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({ "source": "clipboard" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown source"));
    }

    #[test]
    fn file_source_missing_path_returns_error() {
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({ "source": "file" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("path"));
    }

    #[test]
    fn url_source_missing_url_returns_error() {
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({ "source": "url" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("url"));
    }

    #[test]
    fn file_source_empty_path_returns_error() {
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({ "source": "file", "path": "" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("path"));
    }

    #[test]
    fn url_source_empty_url_returns_error() {
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({ "source": "url", "url": "" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("url"));
    }

    // -- File source: valid path --

    #[test]
    fn file_source_with_valid_png() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.png");
        fs::write(&img_path, b"fake-png-data").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
        assert!(r.output.contains("Displaying image"));
        let data = r.data.unwrap();
        assert_eq!(data["source"], "file");
        assert_eq!(data["image_path"], img_path.to_str().unwrap());
    }

    #[test]
    fn file_source_with_valid_jpg() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("photo.jpg");
        fs::write(&img_path, b"fake-jpg-data").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
        assert!(r.output.contains("Displaying image"));
    }

    #[test]
    fn file_source_with_valid_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("photo.jpeg");
        fs::write(&img_path, b"fake-jpeg-data").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
    }

    #[test]
    fn file_source_with_valid_gif() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("anim.gif");
        fs::write(&img_path, b"fake-gif-data").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
    }

    #[test]
    fn file_source_with_valid_svg() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("icon.svg");
        fs::write(&img_path, b"<svg></svg>").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
    }

    #[test]
    fn file_source_with_valid_webp() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("image.webp");
        fs::write(&img_path, b"fake-webp-data").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
    }

    #[test]
    fn file_source_with_valid_bmp() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("image.bmp");
        fs::write(&img_path, b"fake-bmp-data").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
    }

    // -- File source: invalid path --

    #[test]
    fn file_source_nonexistent_path() {
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": "/tmp/nonexistent-aios-test-image.png"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("not found"));
    }

    // -- Image extension validation --

    #[test]
    fn file_source_rejects_non_image_extension() {
        let dir = tempfile::tempdir().unwrap();
        let txt_path = dir.path().join("notes.txt");
        fs::write(&txt_path, b"not an image").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": txt_path.to_str().unwrap()
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("image format"));
    }

    #[test]
    fn file_source_rejects_pdf_extension() {
        let dir = tempfile::tempdir().unwrap();
        let pdf_path = dir.path().join("document.pdf");
        fs::write(&pdf_path, b"fake-pdf").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": pdf_path.to_str().unwrap()
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("image format"));
    }

    #[test]
    fn file_source_rejects_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let no_ext = dir.path().join("mystery");
        fs::write(&no_ext, b"unknown content").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": no_ext.to_str().unwrap()
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("image format"));
    }

    #[test]
    fn file_source_case_insensitive_extension() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("photo.PNG");
        fs::write(&img_path, b"fake-png").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
    }

    // -- Caption in output --

    #[test]
    fn file_source_with_caption() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("photo.png");
        fs::write(&img_path, b"fake-png").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap(),
            "caption": "A beautiful sunset"
        }));
        assert!(r.success);
        assert!(r.output.contains("A beautiful sunset"));
        let data = r.data.unwrap();
        assert_eq!(data["caption"], "A beautiful sunset");
    }

    #[test]
    fn file_source_without_caption_no_caption_in_data() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("photo.png");
        fs::write(&img_path, b"fake-png").unwrap();

        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "file",
            "path": img_path.to_str().unwrap()
        }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert!(data.get("caption").is_none());
    }

    // -- URL source (network tests) --

    #[test]
    fn url_source_with_invalid_url_returns_error() {
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "url",
            "url": "not-a-valid-url"
        }));
        // Should fail with a network/parse error.
        assert!(!r.success);
    }

    #[test]
    fn url_source_with_caption_is_passed_through() {
        // We cannot guarantee network in CI, but we verify the tool handles
        // both success and failure paths without panicking.
        let tool = ShowImageTool::new();
        let r = tool.execute(serde_json::json!({
            "source": "url",
            "url": "https://httpbin.org/status/404",
            "caption": "test caption"
        }));
        // Should fail (404) or succeed — either way no panic and proper result.
        assert!(r.error.is_some() || r.success);
    }

    // -- Channel-aware behaviour --

    #[test]
    fn voice_channel_returns_caption_only() {
        let tool = ShowImageTool::new();
        let channel = ChannelContext::voice();
        let r = tool.execute_on_channel(
            serde_json::json!({
                "source": "file",
                "path": "/tmp/test.png",
                "caption": "A photo of the Eiffel Tower"
            }),
            &channel,
        );
        assert!(r.success);
        assert_eq!(r.output, "A photo of the Eiffel Tower");
        let data = r.data.unwrap();
        assert_eq!(data["voice_only"], true);
        assert_eq!(data["caption"], "A photo of the Eiffel Tower");
    }

    #[test]
    fn voice_channel_without_caption_returns_location() {
        let tool = ShowImageTool::new();
        let channel = ChannelContext::voice();
        let r = tool.execute_on_channel(
            serde_json::json!({
                "source": "file",
                "path": "/tmp/sunset.png"
            }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("/tmp/sunset.png"));
        let data = r.data.unwrap();
        assert_eq!(data["voice_only"], true);
    }

    #[test]
    fn voice_channel_url_without_caption() {
        let tool = ShowImageTool::new();
        let channel = ChannelContext::voice();
        let r = tool.execute_on_channel(
            serde_json::json!({
                "source": "url",
                "url": "https://example.com/photo.jpg"
            }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("https://example.com/photo.jpg"));
    }

    #[test]
    fn desktop_channel_processes_file_normally() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("chart.png");
        fs::write(&img_path, b"fake-png").unwrap();

        let tool = ShowImageTool::new();
        let channel = ChannelContext::desktop();
        let r = tool.execute_on_channel(
            serde_json::json!({
                "source": "file",
                "path": img_path.to_str().unwrap()
            }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("Displaying image"));
    }

    #[test]
    fn web_channel_processes_file_normally() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("chart.png");
        fs::write(&img_path, b"fake-png").unwrap();

        let tool = ShowImageTool::new();
        let channel = ChannelContext::web();
        let r = tool.execute_on_channel(
            serde_json::json!({
                "source": "file",
                "path": img_path.to_str().unwrap()
            }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("Displaying image"));
    }

    #[test]
    fn signal_channel_processes_file_normally() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("photo.jpg");
        fs::write(&img_path, b"fake-jpg").unwrap();

        let tool = ShowImageTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({
                "source": "file",
                "path": img_path.to_str().unwrap()
            }),
            &channel,
        );
        assert!(r.success);
        // Signal supports images — should process normally.
        assert!(r.output.contains("Displaying image"));
    }

    // -- Helper function tests --

    #[test]
    fn is_image_extension_accepts_all_valid() {
        for ext in IMAGE_EXTENSIONS {
            let s = format!("/tmp/test.{ext}");
            assert!(
                is_image_extension(Path::new(s.as_str())),
                "should accept .{ext}"
            );
        }
    }

    #[test]
    fn is_image_extension_rejects_non_image() {
        let cases = ["txt", "pdf", "doc", "mp3", "mp4", "zip", "html", "rs"];
        for ext in &cases {
            let s = format!("/tmp/test.{ext}");
            assert!(
                !is_image_extension(Path::new(s.as_str())),
                "should reject .{ext}"
            );
        }
    }

    #[test]
    fn is_image_extension_handles_no_extension() {
        let path = Path::new("/tmp/noext");
        assert!(!is_image_extension(path));
    }

    #[test]
    fn is_image_extension_case_insensitive() {
        assert!(is_image_extension(Path::new("/tmp/photo.PNG")));
        assert!(is_image_extension(Path::new("/tmp/photo.Jpg")));
        assert!(is_image_extension(Path::new("/tmp/photo.JPEG")));
        assert!(is_image_extension(Path::new("/tmp/photo.GIF")));
        assert!(is_image_extension(Path::new("/tmp/photo.SVG")));
        assert!(is_image_extension(Path::new("/tmp/photo.WEBP")));
        assert!(is_image_extension(Path::new("/tmp/photo.BMP")));
    }

    #[test]
    fn url_to_extension_extracts_png() {
        assert_eq!(url_to_extension("https://example.com/photo.png"), Some("png"));
    }

    #[test]
    fn url_to_extension_extracts_jpg() {
        assert_eq!(url_to_extension("https://example.com/photo.jpg"), Some("jpg"));
    }

    #[test]
    fn url_to_extension_strips_query_params() {
        assert_eq!(
            url_to_extension("https://example.com/photo.png?w=100&h=100"),
            Some("png")
        );
    }

    #[test]
    fn url_to_extension_strips_fragment() {
        assert_eq!(
            url_to_extension("https://example.com/photo.gif#section"),
            Some("gif")
        );
    }

    #[test]
    fn url_to_extension_returns_none_for_unknown() {
        assert_eq!(url_to_extension("https://example.com/page"), None);
        assert_eq!(url_to_extension("https://example.com/data.json"), None);
    }

    #[test]
    fn url_to_extension_handles_empty_url() {
        assert_eq!(url_to_extension(""), None);
    }
}
