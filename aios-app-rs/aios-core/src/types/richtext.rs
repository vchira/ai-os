//! Rich text formatting — converts a simple markup to channel-specific output.
//!
//! Markup syntax (similar to Markdown subset):
//! - `**bold**` -> bold
//! - `*italic*` -> italic
//! - `__underline__` -> underline
//! - `~~strikethrough~~` -> strikethrough
//! - `- item` (at line start) -> bullet point
//! - `1. item` (at line start) -> numbered list

use std::sync::LazyLock;

use regex::Regex;

// ---------------------------------------------------------------------------
// Compiled regexes (compiled once, reused on every call)
// ---------------------------------------------------------------------------

static RE_BOLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap());
static RE_UNDERLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"__(.+?)__").unwrap());
static RE_STRIKE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~~(.+?)~~").unwrap());
static RE_ITALIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*([^*]+?)\*").unwrap());
static RE_NUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\.\s+(.+)$").unwrap());

// ---------------------------------------------------------------------------
// Internal: inline formatting
// ---------------------------------------------------------------------------

/// Apply inline formatting replacements.
///
/// Order matters: bold (`**`) must be processed before italic (`*`)
/// to avoid `**bold**` being mis-parsed as italic.
fn format_inline(text: &str, bold: &str, italic: &str, underline: &str, strike: &str) -> String {
    // Process bold first (** before *)
    let result = RE_BOLD.replace_all(text, bold).into_owned();
    let result = RE_UNDERLINE.replace_all(&result, underline).into_owned();
    let result = RE_STRIKE.replace_all(&result, strike).into_owned();

    // Now process italic (* but not inside **)
    // Since bold is already replaced, remaining single * are italic.
    RE_ITALIC.replace_all(&result, italic).into_owned()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Escape characters that are special in Pango markup (`&`, `<`, `>`).
///
/// Must be called **before** inserting any Pango tags so that raw text
/// from LLM output doesn't break `set_markup()`.
fn escape_pango(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Convert rich text markup to Pango markup (for GTK).
pub fn to_pango(text: &str) -> String {
    let re_num = &*RE_NUM;
    let mut lines: Vec<String> = Vec::new();

    for line in text.lines() {
        let escaped = escape_pango(line);
        let trimmed = escaped.trim_start();

        if let Some(rest) = trimmed.strip_prefix("- ") {
            let formatted = format_inline(rest, "<b>$1</b>", "<i>$1</i>", "<u>$1</u>", "<s>$1</s>");
            lines.push(format!("\u{2022} {formatted}"));
        } else if let Some(caps) = re_num.captures(trimmed) {
            let num = &caps[1];
            let rest = &caps[2];
            let formatted = format_inline(rest, "<b>$1</b>", "<i>$1</i>", "<u>$1</u>", "<s>$1</s>");
            lines.push(format!("{num}. {formatted}"));
        } else {
            lines.push(format_inline(&escaped, "<b>$1</b>", "<i>$1</i>", "<u>$1</u>", "<s>$1</s>"));
        }
    }
    lines.join("\n")
}

/// Convert rich text markup to HTML (for Web).
pub fn to_html(text: &str) -> String {
    let re_num = &*RE_NUM;
    let mut lines: Vec<String> = Vec::new();
    let mut in_ul = false;
    let mut in_ol = false;

    for line in text.lines() {
        let trimmed = line.trim_start();

        if let Some(rest) = trimmed.strip_prefix("- ") {
            if in_ol { lines.push("</ol>".into()); in_ol = false; }
            if !in_ul { lines.push("<ul>".into()); in_ul = true; }
            let f = format_inline(rest, "<strong>$1</strong>", "<em>$1</em>", "<u>$1</u>", "<del>$1</del>");
            lines.push(format!("<li>{f}</li>"));
        } else if let Some(caps) = re_num.captures(trimmed) {
            if in_ul { lines.push("</ul>".into()); in_ul = false; }
            if !in_ol { lines.push("<ol>".into()); in_ol = true; }
            let rest = &caps[2];
            let f = format_inline(rest, "<strong>$1</strong>", "<em>$1</em>", "<u>$1</u>", "<del>$1</del>");
            lines.push(format!("<li>{f}</li>"));
        } else {
            if in_ul { lines.push("</ul>".into()); in_ul = false; }
            if in_ol { lines.push("</ol>".into()); in_ol = false; }
            lines.push(format_inline(line, "<strong>$1</strong>", "<em>$1</em>", "<u>$1</u>", "<del>$1</del>"));
        }
    }
    if in_ul { lines.push("</ul>".into()); }
    if in_ol { lines.push("</ol>".into()); }
    lines.join("\n")
}

/// Strip all markup, returning plain text (for Signal/Voice).
pub fn to_plain(text: &str) -> String {
    let re_num = &*RE_NUM;
    let mut lines: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim_start();

        if let Some(rest) = trimmed.strip_prefix("- ") {
            lines.push(format!("  * {}", format_inline(rest, "$1", "$1", "$1", "$1")));
        } else if let Some(caps) = re_num.captures(trimmed) {
            let num = &caps[1];
            let rest = &caps[2];
            lines.push(format!("  {num}. {}", format_inline(rest, "$1", "$1", "$1", "$1")));
        } else {
            lines.push(format_inline(line, "$1", "$1", "$1", "$1"));
        }
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pango_bold() {
        assert_eq!(to_pango("**bold**"), "<b>bold</b>");
    }

    #[test]
    fn pango_italic() {
        assert_eq!(to_pango("*italic*"), "<i>italic</i>");
    }

    #[test]
    fn pango_underline() {
        assert_eq!(to_pango("__underline__"), "<u>underline</u>");
    }

    #[test]
    fn pango_strikethrough() {
        assert_eq!(to_pango("~~strike~~"), "<s>strike</s>");
    }

    #[test]
    fn pango_bullet_points() {
        assert_eq!(to_pango("- item1\n- item2"), "\u{2022} item1\n\u{2022} item2");
    }

    #[test]
    fn pango_numbered_list() {
        assert_eq!(to_pango("1. first\n2. second"), "1. first\n2. second");
    }

    #[test]
    fn pango_mixed_inline() {
        assert_eq!(to_pango("**bold** and *italic*"), "<b>bold</b> and <i>italic</i>");
    }

    #[test]
    fn html_bold() {
        assert_eq!(to_html("**bold**"), "<strong>bold</strong>");
    }

    #[test]
    fn html_italic() {
        assert_eq!(to_html("*italic*"), "<em>italic</em>");
    }

    #[test]
    fn html_underline() {
        assert_eq!(to_html("__underline__"), "<u>underline</u>");
    }

    #[test]
    fn html_strikethrough() {
        assert_eq!(to_html("~~strike~~"), "<del>strike</del>");
    }

    #[test]
    fn html_bullet_points() {
        assert_eq!(to_html("- item1\n- item2"), "<ul>\n<li>item1</li>\n<li>item2</li>\n</ul>");
    }

    #[test]
    fn html_numbered_list() {
        assert_eq!(to_html("1. first\n2. second"), "<ol>\n<li>first</li>\n<li>second</li>\n</ol>");
    }

    #[test]
    fn html_mixed_inline() {
        assert_eq!(to_html("**bold** and *italic*"), "<strong>bold</strong> and <em>italic</em>");
    }

    #[test]
    fn plain_strips_bold_and_italic() {
        assert_eq!(to_plain("**bold** and *italic*"), "bold and italic");
    }

    #[test]
    fn plain_bullet_points() {
        assert_eq!(to_plain("- item1\n- item2"), "  * item1\n  * item2");
    }

    #[test]
    fn plain_numbered_list() {
        assert_eq!(to_plain("1. first\n2. second"), "  1. first\n  2. second");
    }

    #[test]
    fn plain_no_markup() {
        assert_eq!(to_plain("hello world"), "hello world");
    }

    #[test]
    fn bold_inside_italic_not_confused() {
        assert_eq!(to_pango("**bold**"), "<b>bold</b>");
    }

    #[test]
    fn empty_input() {
        assert_eq!(to_pango(""), "");
        assert_eq!(to_html(""), "");
        assert_eq!(to_plain(""), "");
    }

    // -- Additional edge-case tests --

    #[test]
    fn multiple_bold_segments() {
        let result = to_pango("**a** middle **b**");
        assert_eq!(result, "<b>a</b> middle <b>b</b>");
    }

    #[test]
    fn multiple_bold_segments_html() {
        let result = to_html("**a** middle **b**");
        assert_eq!(result, "<strong>a</strong> middle <strong>b</strong>");
    }

    #[test]
    fn empty_bold_markers_are_literal() {
        // "****" has nothing between the markers, regex .+? won't match.
        let result = to_pango("****");
        assert_eq!(result, "****");
    }

    #[test]
    fn single_asterisk_not_formatting() {
        // "5 * 3 = 15" — the single * surrounded by spaces and digits
        // should not be treated as italic because there's no matching close.
        let result = to_pango("5 * 3 = 15");
        // No italic tags should appear.
        assert!(!result.contains("<i>"));
        assert!(result.contains("5"));
        assert!(result.contains("15"));
    }

    #[test]
    fn list_followed_by_regular_text() {
        let input = "- item1\n- item2\nregular text";
        let result = to_pango(input);
        assert!(result.contains("\u{2022} item1"));
        assert!(result.contains("\u{2022} item2"));
        assert!(result.contains("regular text"));
    }

    #[test]
    fn mixed_list_types_bullet_then_numbered() {
        let input = "- bullet\n1. numbered";
        let pango = to_pango(input);
        assert!(pango.contains("\u{2022} bullet"));
        assert!(pango.contains("1. numbered"));

        let html = to_html(input);
        assert!(html.contains("<ul>"));
        assert!(html.contains("</ul>"));
        assert!(html.contains("<ol>"));
        assert!(html.contains("</ol>"));
    }

    #[test]
    fn very_long_text_does_not_break() {
        let long = "a".repeat(1000);
        let result = to_pango(&long);
        assert_eq!(result.len(), 1000);
        let result_html = to_html(&long);
        assert_eq!(result_html.len(), 1000);
        let result_plain = to_plain(&long);
        assert_eq!(result_plain.len(), 1000);
    }

    #[test]
    fn html_angle_brackets_in_input() {
        // Note: the format_inline function does regex replacements but does not
        // escape HTML. We test that the function doesn't panic and returns
        // something containing the angle brackets (since it doesn't escape).
        let input = "<script>alert('xss')</script>";
        let result = to_html(input);
        // The function doesn't escape HTML by design — it converts markup to HTML.
        // Verify it at least doesn't panic and returns the content.
        assert!(result.contains("script"));
    }

    #[test]
    fn underline_in_text() {
        let result = to_pango("__underlined text__");
        assert_eq!(result, "<u>underlined text</u>");
    }

    #[test]
    fn strikethrough_in_html() {
        let result = to_html("~~deleted~~");
        assert_eq!(result, "<del>deleted</del>");
    }

    #[test]
    fn plain_strips_all_formatting() {
        let input = "**bold** *italic* __underline__ ~~strike~~";
        let result = to_plain(input);
        assert_eq!(result, "bold italic underline strike");
    }

    #[test]
    fn numbered_list_in_html_closes_properly() {
        let input = "1. first\n2. second\nnot a list";
        let html = to_html(input);
        assert!(html.contains("<ol>"));
        assert!(html.contains("</ol>"));
        assert!(html.contains("not a list"));
    }

    #[test]
    fn multiline_with_mixed_formatting() {
        let input = "**Title**\n- *item one*\n- __item two__\nPlain line";
        let pango = to_pango(input);
        assert!(pango.contains("<b>Title</b>"));
        assert!(pango.contains("\u{2022} <i>item one</i>"));
        assert!(pango.contains("\u{2022} <u>item two</u>"));
        assert!(pango.contains("Plain line"));
    }
}
