//! Rich text formatting — converts a simple markup to channel-specific output.
//!
//! Markup syntax (similar to Markdown subset):
//! - `**bold**` -> bold
//! - `*italic*` -> italic
//! - `__underline__` -> underline
//! - `~~strikethrough~~` -> strikethrough
//! - `- item` (at line start) -> bullet point
//! - `1. item` (at line start) -> numbered list

use regex::Regex;

// ---------------------------------------------------------------------------
// Internal: inline formatting
// ---------------------------------------------------------------------------

/// Apply inline formatting replacements.
///
/// Order matters: bold (`**`) must be processed before italic (`*`)
/// to avoid `**bold**` being mis-parsed as italic.
fn format_inline(text: &str, bold: &str, italic: &str, underline: &str, strike: &str) -> String {
    let re_bold = Regex::new(r"\*\*(.+?)\*\*").unwrap();
    let re_underline = Regex::new(r"__(.+?)__").unwrap();
    let re_strike = Regex::new(r"~~(.+?)~~").unwrap();

    // Process bold first (** before *)
    let result = re_bold.replace_all(text, bold).into_owned();
    let result = re_underline.replace_all(&result, underline).into_owned();
    let result = re_strike.replace_all(&result, strike).into_owned();

    // Now process italic (* but not inside **)
    // Since bold is already replaced, remaining single * are italic.
    let re_italic = Regex::new(r"\*([^*]+?)\*").unwrap();
    re_italic.replace_all(&result, italic).into_owned()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Convert rich text markup to Pango markup (for GTK).
pub fn to_pango(text: &str) -> String {
    let re_num = Regex::new(r"^(\d+)\.\s+(.+)$").unwrap();
    let mut lines: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim_start();

        if let Some(rest) = trimmed.strip_prefix("- ") {
            let formatted = format_inline(rest, "<b>$1</b>", "<i>$1</i>", "<u>$1</u>", "<s>$1</s>");
            lines.push(format!("\u{2022} {formatted}"));
        } else if let Some(caps) = re_num.captures(trimmed) {
            let num = &caps[1];
            let rest = &caps[2];
            let formatted = format_inline(rest, "<b>$1</b>", "<i>$1</i>", "<u>$1</u>", "<s>$1</s>");
            lines.push(format!("{num}. {formatted}"));
        } else {
            lines.push(format_inline(line, "<b>$1</b>", "<i>$1</i>", "<u>$1</u>", "<s>$1</s>"));
        }
    }
    lines.join("\n")
}

/// Convert rich text markup to HTML (for Web).
pub fn to_html(text: &str) -> String {
    let re_num = Regex::new(r"^(\d+)\.\s+(.+)$").unwrap();
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
    let re_num = Regex::new(r"^(\d+)\.\s+(.+)$").unwrap();
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
}
