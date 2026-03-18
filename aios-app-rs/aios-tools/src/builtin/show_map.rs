//! Show map tool — display a location on an OpenStreetMap map.
//!
//! The AI uses this tool to show a location to the user.  On Desktop/Web
//! channels the tool opens the map URL in the browser.  On Signal/Voice
//! channels it returns a text description with the URL.
//!
//! Accepts either an `address` string **or** `latitude`+`longitude` numbers.
//! Uses OpenStreetMap (no API key needed).

use aios_core::channel::{ChannelContext, ChannelKind};
use aios_core::types::ToolResult;

use crate::tool::Tool;

/// Display a location on an OpenStreetMap map.
///
/// On Desktop/Web channels the map URL is opened via `xdg-open`.
/// On Signal/Voice channels the tool returns the URL as text.
pub struct ShowMapTool;

impl ShowMapTool {
    /// Create a new `ShowMapTool`.
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShowMapTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Percent-encode a string for use in a URL query parameter.
///
/// Encodes all characters except unreserved characters defined by RFC 3986:
/// `A-Z a-z 0-9 - _ . ~`
fn url_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
        }
    }
    encoded
}

/// Build an OpenStreetMap URL for the given parameters.
///
/// Returns `Ok(url)` on success or `Err(message)` on validation failure.
fn build_map_url(args: &serde_json::Value) -> Result<(String, String), String> {
    let address = args.get("address").and_then(|v| v.as_str());
    let latitude = args.get("latitude").and_then(|v| v.as_f64());
    let longitude = args.get("longitude").and_then(|v| v.as_f64());

    if let Some(addr) = address {
        let addr = addr.trim();
        if addr.is_empty() {
            return Err("'address' must not be empty.".to_string());
        }
        let encoded = url_encode(addr);
        let url = format!("https://www.openstreetmap.org/search?query={encoded}");
        let description = format!("Map location: {addr}");
        Ok((url, description))
    } else if let (Some(lat), Some(lon)) = (latitude, longitude) {
        // Validate latitude range: -90..=90
        if !(-90.0..=90.0).contains(&lat) {
            return Err(format!(
                "Invalid latitude {lat}: must be between -90 and 90."
            ));
        }
        // Validate longitude range: -180..=180
        if !(-180.0..=180.0).contains(&lon) {
            return Err(format!(
                "Invalid longitude {lon}: must be between -180 and 180."
            ));
        }
        let url = format!("https://www.openstreetmap.org/#map=15/{lat}/{lon}");
        let description = format!("Map location: {lat}, {lon}");
        Ok((url, description))
    } else {
        Err(
            "Provide either 'address' (string) or both 'latitude' and 'longitude' (numbers)."
                .to_string(),
        )
    }
}

impl Tool for ShowMapTool {
    fn name(&self) -> &str {
        "show_map"
    }

    fn description(&self) -> &str {
        "Display a location on an OpenStreetMap map. Accepts an address or latitude/longitude coordinates."
    }

    fn category(&self) -> &str {
        "ui"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "Street address or place name to show on the map."
                },
                "latitude": {
                    "type": "number",
                    "description": "Latitude coordinate (-90 to 90). Use with longitude."
                },
                "longitude": {
                    "type": "number",
                    "description": "Longitude coordinate (-180 to 180). Use with latitude."
                }
            }
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
        let (url, description) = match build_map_url(&args) {
            Ok(result) => result,
            Err(msg) => return ToolResult::fail(msg),
        };

        match channel.kind {
            // On Desktop/Web: open the URL in the default browser.
            ChannelKind::Desktop | ChannelKind::Web => {
                // Use xdg-open on Linux to open the map in the browser.
                let open_result = std::process::Command::new("xdg-open")
                    .arg(&url)
                    .spawn();

                match open_result {
                    Ok(_) => ToolResult::ok_with_data(
                        format!("Opened map: {description}\n{url}"),
                        serde_json::json!({
                            "type": "map",
                            "url": url,
                            "description": description,
                        }),
                    ),
                    Err(e) => {
                        // If xdg-open fails, still return the URL as text.
                        tracing::warn!("xdg-open failed: {e}; returning URL as text");
                        ToolResult::ok_with_data(
                            format!("{description}\n{url}"),
                            serde_json::json!({
                                "type": "map",
                                "url": url,
                                "description": description,
                            }),
                        )
                    }
                }
            }
            // On Signal/Voice/System: return a text description with the URL.
            ChannelKind::Signal | ChannelKind::Voice | ChannelKind::System => {
                ToolResult::ok_with_data(
                    format!("{description}\n{url}"),
                    serde_json::json!({
                        "type": "map",
                        "url": url,
                        "description": description,
                    }),
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

    // -- Tool metadata --------------------------------------------------------

    #[test]
    fn tool_name() {
        let tool = ShowMapTool::new();
        assert_eq!(tool.name(), "show_map");
    }

    #[test]
    fn tool_category_is_ui() {
        let tool = ShowMapTool::new();
        assert_eq!(tool.category(), "ui");
    }

    #[test]
    fn tool_description_is_non_empty() {
        let tool = ShowMapTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn tool_parameters_is_json_object() {
        let tool = ShowMapTool::new();
        let params = tool.parameters();
        assert!(params.is_object());
        let props = params.get("properties").unwrap();
        assert!(props.get("address").is_some());
        assert!(props.get("latitude").is_some());
        assert!(props.get("longitude").is_some());
    }

    #[test]
    fn tool_schema_roundtrips() {
        let tool = ShowMapTool::new();
        let schema = tool.to_schema();
        assert_eq!(schema.name, "show_map");
        assert!(!schema.description.is_empty());
        assert!(schema.parameters.is_object());
    }

    // -- Address lookup -------------------------------------------------------

    #[test]
    fn address_lookup_success() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal(); // avoid xdg-open in tests
        let r = tool.execute_on_channel(
            serde_json::json!({ "address": "Berlin, Germany" }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("Berlin, Germany"));
        assert!(r.output.contains("openstreetmap.org"));

        // Check structured data.
        let data = r.data.unwrap();
        assert_eq!(data["type"], "map");
        assert!(data["url"].as_str().unwrap().contains("openstreetmap.org/search?query="));
        assert!(data["url"].as_str().unwrap().contains("Berlin"));
    }

    #[test]
    fn address_lookup_url_format() {
        let (url, _desc) =
            build_map_url(&serde_json::json!({ "address": "Paris, France" })).unwrap();
        assert_eq!(
            url,
            "https://www.openstreetmap.org/search?query=Paris%2C%20France"
        );
    }

    #[test]
    fn empty_address_fails() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "address": "" }),
            &channel,
        );
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("empty"));
    }

    #[test]
    fn whitespace_only_address_fails() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "address": "   " }),
            &channel,
        );
        assert!(!r.success);
    }

    // -- Coordinate lookup ----------------------------------------------------

    #[test]
    fn coordinate_lookup_success() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "latitude": 48.8566, "longitude": 2.3522 }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("48.8566"));
        assert!(r.output.contains("2.3522"));
        assert!(r.output.contains("openstreetmap.org"));

        let data = r.data.unwrap();
        assert_eq!(data["type"], "map");
        let url = data["url"].as_str().unwrap();
        assert!(url.contains("#map=15/48.8566/2.3522"));
    }

    #[test]
    fn coordinate_lookup_url_format() {
        let (url, _desc) = build_map_url(&serde_json::json!({
            "latitude": 51.5074,
            "longitude": -0.1278
        }))
        .unwrap();
        assert_eq!(
            url,
            "https://www.openstreetmap.org/#map=15/51.5074/-0.1278"
        );
    }

    #[test]
    fn negative_coordinates() {
        let (url, _desc) = build_map_url(&serde_json::json!({
            "latitude": -33.8688,
            "longitude": 151.2093
        }))
        .unwrap();
        assert!(url.contains("-33.8688"));
        assert!(url.contains("151.2093"));
    }

    #[test]
    fn boundary_coordinates() {
        // Exact boundary values should work.
        let r = build_map_url(&serde_json::json!({
            "latitude": 90.0,
            "longitude": 180.0
        }));
        assert!(r.is_ok());

        let r = build_map_url(&serde_json::json!({
            "latitude": -90.0,
            "longitude": -180.0
        }));
        assert!(r.is_ok());
    }

    #[test]
    fn invalid_latitude_fails() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "latitude": 91.0, "longitude": 0.0 }),
            &channel,
        );
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("latitude"));
    }

    #[test]
    fn invalid_longitude_fails() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "latitude": 0.0, "longitude": 181.0 }),
            &channel,
        );
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("longitude"));
    }

    // -- Missing parameters ---------------------------------------------------

    #[test]
    fn missing_all_params_fails() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(serde_json::json!({}), &channel);
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("address"));
    }

    #[test]
    fn latitude_without_longitude_fails() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "latitude": 48.0 }),
            &channel,
        );
        assert!(!r.success);
    }

    #[test]
    fn longitude_without_latitude_fails() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "longitude": 2.0 }),
            &channel,
        );
        assert!(!r.success);
    }

    // -- URL encoding ---------------------------------------------------------

    #[test]
    fn url_encode_simple() {
        assert_eq!(url_encode("hello"), "hello");
    }

    #[test]
    fn url_encode_spaces() {
        assert_eq!(url_encode("hello world"), "hello%20world");
    }

    #[test]
    fn url_encode_special_characters() {
        assert_eq!(url_encode("Berlin, Germany"), "Berlin%2C%20Germany");
    }

    #[test]
    fn url_encode_unicode() {
        // "Munchen" with umlaut: u-umlaut is 0xC3 0xBC in UTF-8
        let encoded = url_encode("M\u{00fc}nchen");
        assert_eq!(encoded, "M%C3%BCnchen");
    }

    #[test]
    fn url_encode_preserves_unreserved() {
        assert_eq!(url_encode("a-z_0.9~"), "a-z_0.9~");
    }

    #[test]
    fn url_encode_ampersand_and_equals() {
        assert_eq!(url_encode("a=1&b=2"), "a%3D1%26b%3D2");
    }

    #[test]
    fn url_encode_slash_and_hash() {
        assert_eq!(url_encode("path/to#anchor"), "path%2Fto%23anchor");
    }

    #[test]
    fn url_encode_empty_string() {
        assert_eq!(url_encode(""), "");
    }

    // -- Address with special characters in URL -------------------------------

    #[test]
    fn address_with_special_characters() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "address": "Straße des 17. Juni, Berlin" }),
            &channel,
        );
        assert!(r.success);
        let data = r.data.unwrap();
        let url = data["url"].as_str().unwrap();
        // Verify the URL is properly encoded — no raw spaces or special chars.
        assert!(!url.contains(' '));
        assert!(url.contains("Stra%C3%9Fe"));
    }

    // -- Channel behaviour ----------------------------------------------------

    #[test]
    fn signal_channel_returns_text_with_url() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "address": "Tokyo" }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("Tokyo"));
        assert!(r.output.contains("openstreetmap.org"));
    }

    #[test]
    fn voice_channel_returns_text_with_url() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::voice();
        let r = tool.execute_on_channel(
            serde_json::json!({ "address": "Tokyo" }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("Tokyo"));
        assert!(r.output.contains("openstreetmap.org"));
    }

    #[test]
    fn system_channel_returns_text_with_url() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::new(ChannelKind::System);
        let r = tool.execute_on_channel(
            serde_json::json!({ "latitude": 40.7128, "longitude": -74.006 }),
            &channel,
        );
        assert!(r.success);
        assert!(r.output.contains("40.7128"));
    }

    // -- Address takes priority over coordinates when both are provided --------

    #[test]
    fn address_takes_priority_over_coordinates() {
        let (url, desc) = build_map_url(&serde_json::json!({
            "address": "London",
            "latitude": 51.5074,
            "longitude": -0.1278
        }))
        .unwrap();
        // Address wins — URL should be a search, not coordinate-based.
        assert!(url.contains("search?query=London"));
        assert!(desc.contains("London"));
    }

    // -- Default trait --------------------------------------------------------

    #[test]
    fn default_creates_tool() {
        let tool = ShowMapTool::default();
        assert_eq!(tool.name(), "show_map");
    }

    // -- Structured data always present on success ----------------------------

    #[test]
    fn structured_data_has_type_url_description() {
        let tool = ShowMapTool::new();
        let channel = ChannelContext::signal();
        let r = tool.execute_on_channel(
            serde_json::json!({ "address": "Rome, Italy" }),
            &channel,
        );
        assert!(r.success);
        let data = r.data.expect("data should be present");
        assert_eq!(data["type"], "map");
        assert!(data["url"].is_string());
        assert!(data["description"].is_string());
    }
}
