//! Chat message display widget.
//!
//! [`ChatView`] wraps a vertical `gtk::Box` and provides methods to append
//! styled chat messages (user, assistant, system, tool) with automatic
//! code block detection.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};

// ---------------------------------------------------------------------------
// ChatView
// ---------------------------------------------------------------------------

/// Chat message display area.
///
/// This is a plain Rust struct that owns a `gtk::Box` and the optional
/// reference to the parent `ScrolledWindow` for auto-scrolling.
#[derive(Clone)]
pub struct ChatView {
    container: gtk::Box,
    scroll_window: std::cell::RefCell<Option<gtk::ScrolledWindow>>,
}

impl ChatView {
    /// Create a new, empty chat view.
    pub fn new() -> Self {
        let container = gtk::Box::new(Orientation::Vertical, 4);
        container.add_css_class("chat-container");
        container.set_valign(Align::Start);
        container.set_margin_top(8);
        container.set_margin_bottom(8);

        Self {
            container,
            scroll_window: std::cell::RefCell::new(None),
        }
    }

    /// Return the underlying GTK widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// Set the parent `ScrolledWindow` for auto-scrolling.
    pub fn set_scroll_window(&self, sw: &gtk::ScrolledWindow) {
        *self.scroll_window.borrow_mut() = Some(sw.clone());
    }

    /// Append a message to the chat view.
    ///
    /// # Arguments
    ///
    /// * `role` — One of `"user"`, `"assistant"`, `"system"`, `"tool"`.
    /// * `content` — The message text. Code blocks delimited by ``` are
    ///   rendered in monospace.
    pub fn add_message(&self, role: &str, content: &str) {
        let row = gtk::Box::new(Orientation::Vertical, 2);
        row.add_css_class("message-row");
        row.add_css_class(&format!("message-{role}"));

        // Alignment based on role.
        let (halign, margin_start, margin_end) = match role {
            "user" => (Align::End, 60, 0),
            "assistant" => (Align::Start, 0, 60),
            _ => (Align::Center, 30, 30),
        };
        row.set_halign(halign);
        row.set_margin_start(margin_start);
        row.set_margin_end(margin_end);
        row.set_hexpand(true);

        // Role label (not shown for system messages to keep them cleaner).
        if role != "system" {
            let role_label = gtk::Label::new(Some(&role_display_name(role)));
            role_label.add_css_class("message-role-label");
            role_label.set_halign(match role {
                "user" => Align::End,
                _ => Align::Start,
            });
            row.append(&role_label);
        }

        // Message bubble.
        let bubble = gtk::Box::new(Orientation::Vertical, 4);
        bubble.add_css_class("message-bubble");

        // Split content by code blocks (```...```).
        let parts = split_code_blocks(content);
        for part in parts {
            match part {
                ContentPart::Text(text) => {
                    if !text.is_empty() {
                        // Convert rich text markup (**bold**, *italic*, etc.) to Pango.
                        let pango = aios_core::types::to_pango(&text);
                        let label = gtk::Label::new(None);
                        label.set_markup(&pango);
                        label.set_wrap(true);
                        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                        label.set_xalign(0.0);
                        label.set_selectable(true);
                        bubble.append(&label);
                    }
                }
                ContentPart::Code(code) => {
                    let frame = gtk::Frame::new(None);
                    frame.add_css_class("code-block");

                    let code_label = gtk::Label::new(Some(&code));
                    code_label.set_wrap(true);
                    code_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    code_label.set_xalign(0.0);
                    code_label.set_selectable(true);

                    frame.set_child(Some(&code_label));
                    bubble.append(&frame);
                }
            }
        }

        row.append(&bubble);
        self.container.append(&row);

        self.scroll_to_bottom();
    }

    /// Append a message with a severity level (INFO, SUCCESS, WARNING, etc.).
    ///
    /// Renders with a colored left border, level icon + label, and the content.
    pub fn add_level_message(&self, level: aios_core::types::MessageLevel, content: &str) {
        let row = gtk::Box::new(Orientation::Vertical, 2);
        row.add_css_class("message-row");
        row.add_css_class("message-system");
        row.add_css_class(level.css_class());
        row.set_halign(Align::Start);
        row.set_margin_start(0);
        row.set_margin_end(40);
        row.set_hexpand(true);

        // Level label: "ℹ️ [INFO]" / "✅ [SUCCESS]" etc.
        let level_label = gtk::Label::new(Some(&format!(
            "{} [{}]",
            level.icon(),
            level.label()
        )));
        level_label.add_css_class("msg-level-label");
        level_label.set_halign(Align::Start);
        row.append(&level_label);

        // Message bubble with content.
        let bubble = gtk::Box::new(Orientation::Vertical, 4);
        bubble.add_css_class("message-bubble");

        let label = gtk::Label::new(Some(content));
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        label.set_xalign(0.0);
        label.set_selectable(true);
        // Use Pango markup so we can render bold/italic.
        label.set_use_markup(true);
        bubble.append(&label);

        row.append(&bubble);
        self.container.append(&row);
        self.scroll_to_bottom();
    }

    /// Append a rich "setup card" to the chat view.
    ///
    /// Setup cards are used during the first-boot conversation. They look like
    /// assistant messages but have a colored icon, bold title, description, and
    /// an optional inline input widget (entry field, buttons, etc.).
    ///
    /// # Arguments
    ///
    /// * `icon_name` — A GTK4 symbolic icon name (e.g. `"dialog-password-symbolic"`).
    /// * `title` — Bold heading text for the card.
    /// * `description` — Body text (lighter color, supports newlines).
    /// * `input_widget` — Optional inline widget displayed below the description.
    pub fn add_setup_card(
        &self,
        icon_name: &str,
        title: &str,
        description: &str,
        input_widget: Option<&gtk::Widget>,
    ) {
        let row = gtk::Box::new(Orientation::Vertical, 2);
        row.add_css_class("message-row");
        row.add_css_class("message-assistant");
        row.set_halign(Align::Start);
        row.set_margin_start(0);
        row.set_margin_end(40);
        row.set_hexpand(true);

        // Role label.
        let role_label = gtk::Label::new(Some("AiOS"));
        role_label.add_css_class("message-role-label");
        role_label.set_halign(Align::Start);
        row.append(&role_label);

        // Card container with special styling.
        let card = gtk::Box::new(Orientation::Vertical, 8);
        card.add_css_class("setup-card");

        // Header row: icon + title.
        let header = gtk::Box::new(Orientation::Horizontal, 10);
        header.set_margin_bottom(4);

        let icon = gtk::Image::from_icon_name(icon_name);
        icon.set_pixel_size(32);
        icon.add_css_class("setup-card-icon");
        header.append(&icon);

        let title_label = gtk::Label::new(Some(title));
        title_label.add_css_class("setup-card-title");
        title_label.set_halign(Align::Start);
        title_label.set_hexpand(true);
        title_label.set_wrap(true);
        header.append(&title_label);

        card.append(&header);

        // Description.
        let desc_label = gtk::Label::new(Some(description));
        desc_label.add_css_class("setup-card-description");
        desc_label.set_halign(Align::Start);
        desc_label.set_xalign(0.0);
        desc_label.set_wrap(true);
        desc_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        card.append(&desc_label);

        // Optional input widget.
        if let Some(widget) = input_widget {
            let input_area = gtk::Box::new(Orientation::Vertical, 0);
            input_area.add_css_class("setup-card-input");
            input_area.set_margin_top(8);
            input_area.append(widget);
            card.append(&input_area);
        }

        row.append(&card);
        self.container.append(&row);

        self.scroll_to_bottom();
    }

    /// Remove all messages from the chat view.
    pub fn clear(&self) {
        while let Some(child) = self.container.first_child() {
            self.container.remove(&child);
        }
    }

    /// Scroll the parent `ScrolledWindow` to the bottom.
    fn scroll_to_bottom(&self) {
        if let Some(sw) = self.scroll_window.borrow().as_ref() {
            let adj = sw.vadjustment();
            // Use idle_add to ensure the layout has been computed.
            let adj_clone = adj.clone();
            gtk4::glib::idle_add_local_once(move || {
                adj_clone.set_value(adj_clone.upper());
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Code block splitting
// ---------------------------------------------------------------------------

/// A piece of message content — either plain text or a code block.
enum ContentPart {
    Text(String),
    Code(String),
}

/// Split message content by triple-backtick code fences.
///
/// Returns alternating Text/Code parts. The language identifier after
/// the opening ``` is stripped.
fn split_code_blocks(content: &str) -> Vec<ContentPart> {
    let mut parts = Vec::new();
    let mut remaining = content;

    loop {
        if let Some(start) = remaining.find("```") {
            // Text before the code block.
            let text_before = &remaining[..start];
            if !text_before.trim().is_empty() {
                parts.push(ContentPart::Text(text_before.trim().to_string()));
            }

            let after_fence = &remaining[start + 3..];

            // Find the closing ```.
            if let Some(end) = after_fence.find("```") {
                let code_block = &after_fence[..end];
                // Strip the optional language identifier on the first line.
                let code = if let Some(newline) = code_block.find('\n') {
                    code_block[newline + 1..].to_string()
                } else {
                    code_block.to_string()
                };

                if !code.trim().is_empty() {
                    parts.push(ContentPart::Code(code.trim_end().to_string()));
                }

                remaining = &after_fence[end + 3..];
            } else {
                // No closing fence — treat rest as code.
                let code = after_fence;
                if !code.trim().is_empty() {
                    parts.push(ContentPart::Code(code.trim().to_string()));
                }
                break;
            }
        } else {
            // No more code blocks.
            if !remaining.trim().is_empty() {
                parts.push(ContentPart::Text(remaining.trim().to_string()));
            }
            break;
        }
    }

    // If nothing was parsed (empty input), return empty.
    parts
}

/// Human-friendly display name for a role.
fn role_display_name(role: &str) -> String {
    match role {
        "user" => "You".to_string(),
        "assistant" => "AiOS".to_string(),
        "system" => "System".to_string(),
        "tool" => "Tool".to_string(),
        other => other.to_string(),
    }
}
