//! Chat message display widget.
//!
//! [`ChatView`] wraps a vertical `gtk::Box` and provides methods to append
//! styled chat messages (user, assistant, system, tool) with automatic
//! code block detection.

use std::cell::RefCell;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};

// ---------------------------------------------------------------------------
// Assistant display name — configurable at runtime
// ---------------------------------------------------------------------------

thread_local! {
    static ASSISTANT_NAME: RefCell<String> = RefCell::new("Assistant".to_string());
}

/// Set the display name shown for assistant messages in the chat view.
pub fn set_assistant_display_name(name: &str) {
    ASSISTANT_NAME.with(|n| *n.borrow_mut() = name.to_string());
}

// ---------------------------------------------------------------------------
// CardHandle — allows a card to dismiss its own interactive input
// ---------------------------------------------------------------------------

/// Handle to a setup card's interactive input area.
///
/// When a button in the card is clicked, call [`dismiss`] to replace all
/// interactive elements with a compact label showing what was chosen.
#[derive(Clone)]
pub struct CardHandle {
    input_area: gtk::Box,
    parent_card: gtk::Box,
}

impl CardHandle {
    /// Remove all interactive widgets from this card and show what was chosen.
    pub fn dismiss(&self, choice_text: &str) {
        // Remove the input area from the card.
        self.parent_card.remove(&self.input_area);

        // Show a compact summary of what was chosen.
        let chosen = gtk::Label::new(None);
        let escaped = choice_text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        chosen.set_markup(&format!("<i>\u{2714} {escaped}</i>"));
        chosen.set_xalign(0.0);
        chosen.set_margin_top(4);
        chosen.set_opacity(0.7);
        self.parent_card.append(&chosen);
    }
}

// ---------------------------------------------------------------------------
// ChatView
// ---------------------------------------------------------------------------

/// Handle to a thinking/loading placeholder message.
///
/// Call [`ChatView::remove_thinking`] with this handle to remove the
/// placeholder when the real response is ready to display.
#[derive(Clone)]
pub struct ThinkingHandle {
    widget: gtk::Box,
}

/// Chat message display area.
///
/// This is a plain Rust struct that owns a `gtk::Box` and the optional
/// reference to the parent `ScrolledWindow` for auto-scrolling.
#[derive(Clone)]
pub struct ChatView {
    container: gtk::Box,
    scroll_window: std::cell::RefCell<Option<gtk::ScrolledWindow>>,
    /// The input area of the last setup card (so we can remove it once answered).
    last_card_input: std::cell::RefCell<Option<gtk::Box>>,
    /// ID of the oldest rendered message (for scroll-back loading).
    #[allow(dead_code)]
    oldest_rendered_id: std::cell::Cell<Option<i64>>,
    /// ID of the newest rendered message.
    #[allow(dead_code)]
    newest_rendered_id: std::cell::Cell<Option<i64>>,
    /// Maximum number of message widgets to keep rendered.
    #[allow(dead_code)]
    max_rendered_widgets: std::cell::Cell<usize>,
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
            last_card_input: std::cell::RefCell::new(None),
            #[allow(dead_code)]
    oldest_rendered_id: std::cell::Cell::new(None),
            #[allow(dead_code)]
    newest_rendered_id: std::cell::Cell::new(None),
            #[allow(dead_code)]
    max_rendered_widgets: std::cell::Cell::new(50),
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

    /// Connect a callback that fires when the user scrolls near the top.
    ///
    /// When the vertical adjustment value drops below `100.0` (i.e. the
    /// user is near the top of the chat), `on_scroll_back` is called with
    /// the [`oldest_rendered_id`] so the caller can load older messages
    /// from the message queue.
    #[allow(dead_code)]
    pub fn connect_scroll_back<F: Fn(i64) + 'static>(&self, on_scroll_back: F) {
        if let Some(sw) = self.scroll_window.borrow().as_ref() {
            let oldest_id = self.oldest_rendered_id.clone();
            let adj = sw.vadjustment();
            adj.connect_value_changed(move |adj| {
                if adj.value() < 100.0 {
                    if let Some(id) = oldest_id.get() {
                        on_scroll_back(id);
                    }
                }
            });
        }
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
            let role_row = gtk::Box::new(Orientation::Horizontal, 4);
            role_row.set_halign(match role {
                "user" => Align::End,
                _ => Align::Start,
            });

            let role_label = gtk::Label::new(Some(&role_display_name(role)));
            role_label.add_css_class("message-role-label");
            role_row.append(&role_label);

            // Stop-reading button for assistant messages.
            if role == "assistant" {
                role_row.append(&build_stop_reading_button());
            }

            row.append(&role_row);
        }

        row.append(&build_message_bubble(content));
        self.container.append(&row);

        self.scroll_to_bottom();
    }

    /// Append an assistant message with model attribution in the role label.
    ///
    /// Renders as: "Assistant — DeepSeek Reasoner" or with sentinel in parens.
    pub fn add_assistant_message(&self, content: &str, model_label: &str) {
        let row = gtk::Box::new(Orientation::Vertical, 2);
        row.add_css_class("message-row");
        row.add_css_class("message-assistant");

        row.set_halign(Align::Start);
        row.set_margin_start(0);
        row.set_margin_end(60);
        row.set_hexpand(true);

        // Role label with model attribution.
        let role_row = gtk::Box::new(Orientation::Horizontal, 4);
        role_row.set_halign(Align::Start);

        let display_name = ASSISTANT_NAME.with(|n| n.borrow().clone());
        let full_label = format!("{display_name} \u{2014} {model_label}");
        let role_label = gtk::Label::new(Some(&full_label));
        role_label.add_css_class("message-role-label");
        role_row.append(&role_label);

        role_row.append(&build_stop_reading_button());
        row.append(&role_row);

        row.append(&build_message_bubble(content));
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

        // If content already contains Pango markup (e.g. <span>, <b>), use it directly.
        // Otherwise, convert rich text to Pango.
        let pango = if content.contains("<span") || content.contains("<b>") {
            content.to_string()
        } else {
            aios_core::types::to_pango(content)
        };
        let label = gtk::Label::new(None);
        label.set_markup(&pango);
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        label.set_xalign(0.0);
        label.set_selectable(true);
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
    ) -> Option<CardHandle> {
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

        // Optional input widget — returns a CardHandle for self-dismissal.
        let handle = if let Some(widget) = input_widget {
            let input_area = gtk::Box::new(Orientation::Vertical, 0);
            input_area.add_css_class("setup-card-input");
            input_area.set_margin_top(8);
            input_area.append(widget);
            card.append(&input_area);
            *self.last_card_input.borrow_mut() = Some(input_area.clone());
            Some(CardHandle {
                input_area,
                parent_card: card.clone(),
            })
        } else {
            *self.last_card_input.borrow_mut() = None;
            None
        };

        row.append(&card);
        self.container.append(&row);
        self.scroll_to_bottom();

        handle
    }

    /// Remove the interactive input area from the last setup card.
    ///
    /// Call this after the user has answered a setup step. The card's
    /// title and description remain visible, but the buttons/inputs
    /// are removed so the user can't click them again.
    pub fn dismiss_last_card_input(&self) {
        self.dismiss_last_card_input_with_choice(None);
    }

    /// Dismiss the last card's interactive input area and optionally replace
    /// it with a label showing what the user chose.
    pub fn dismiss_last_card_input_with_choice(&self, choice_text: Option<&str>) {
        if let Some(input_area) = self.last_card_input.borrow_mut().take() {
            if let Some(parent) = input_area.parent() {
                if let Some(parent_box) = parent.downcast_ref::<gtk::Box>() {
                    parent_box.remove(&input_area);

                    // Show a compact summary of what was chosen.
                    if let Some(text) = choice_text {
                        let chosen = gtk::Label::new(None);
                        let escaped = text
                            .replace('&', "&amp;")
                            .replace('<', "&lt;")
                            .replace('>', "&gt;");
                        chosen.set_markup(&format!(
                            "<i>\u{2714} {escaped}</i>"
                        ));
                        chosen.set_xalign(0.0);
                        chosen.set_margin_top(4);
                        chosen.set_opacity(0.7);
                        parent_box.append(&chosen);
                    }
                }
            }
        }
    }

    /// Add a thinking/loading placeholder message.
    ///
    /// Display an image inline in the chat (with optional caption).
    #[allow(dead_code)]
    pub fn add_image(&self, path: &str, caption: Option<&str>) {
        let row = gtk::Box::new(Orientation::Vertical, 4);
        row.add_css_class("message-row");
        row.add_css_class("message-assistant");
        row.set_halign(Align::Start);
        row.set_margin_start(0);
        row.set_margin_end(60);
        row.set_hexpand(true);

        // Try to load the image.
        let picture = gtk::Picture::for_filename(path);
        picture.set_can_shrink(true);
        picture.set_size_request(400, 300);
        picture.add_css_class("chat-image");
        row.append(&picture);

        // Optional caption below the image.
        if let Some(cap) = caption {
            let label = gtk::Label::new(Some(cap));
            label.add_css_class("message-content");
            label.set_wrap(true);
            label.set_halign(Align::Start);
            row.append(&label);
        }

        self.container.append(&row);
        self.scroll_to_bottom();
    }

    /// Shows an animated "thinking" indicator in the assistant's position.
    /// Returns a [`ThinkingHandle`] — pass it to [`remove_thinking`] to
    /// remove the placeholder when the real response is ready.
    pub fn add_thinking(&self) -> ThinkingHandle {
        let row = gtk::Box::new(Orientation::Vertical, 2);
        row.add_css_class("message-row");
        row.add_css_class("message-assistant");
        row.set_halign(Align::Start);
        row.set_margin_start(0);
        row.set_margin_end(60);
        row.set_hexpand(true);

        // Role label
        let role_label = gtk::Label::new(Some(&role_display_name("assistant")));
        role_label.add_css_class("message-role-label");
        role_label.set_halign(Align::Start);
        row.append(&role_label);

        // Thinking bubble with animated dots
        let bubble = gtk::Box::new(Orientation::Horizontal, 6);
        bubble.add_css_class("message-bubble");
        bubble.add_css_class("thinking-bubble");

        let dots_label = gtk::Label::new(Some("\u{2022} \u{2022} \u{2022}"));
        dots_label.add_css_class("thinking-dots");
        dots_label.set_opacity(0.5);
        bubble.append(&dots_label);

        // Animate the dots opacity
        let dots = dots_label.clone();
        let tick = std::cell::Cell::new(0u32);
        glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
            let t = tick.get();
            tick.set(t + 1);
            let opacity = match t % 3 {
                0 => 0.3,
                1 => 0.6,
                _ => 0.9,
            };
            dots.set_opacity(opacity);
            // Stop if the widget has been removed from the tree
            if dots.parent().is_none() {
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });

        row.append(&bubble);
        self.container.append(&row);
        self.scroll_to_bottom();

        ThinkingHandle { widget: row }
    }

    /// Remove a thinking placeholder previously added with [`add_thinking`].
    pub fn remove_thinking(&self, handle: &ThinkingHandle) {
        self.container.remove(&handle.widget);
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
            // Immediate scroll (works if layout is already computed).
            adj.set_value(adj.upper() - adj.page_size());
            // Also schedule a scroll after the idle phase (layout recomputation).
            let adj2 = adj.clone();
            gtk4::glib::idle_add_local_once(move || {
                adj2.set_value(adj2.upper() - adj2.page_size());
            });
            // And another after a short delay to catch late layout updates
            // (e.g., images loading, label rewrapping).
            let adj3 = adj.clone();
            gtk4::glib::timeout_add_local_once(
                std::time::Duration::from_millis(100),
                move || {
                    adj3.set_value(adj3.upper() - adj3.page_size());
                },
            );
        }
    }

    // --- Queue-based rendering methods (Phase 3) ---
    // These methods will be used when the queue-driven rendering is fully wired.
    #[allow(dead_code)]

    /// Render a batch of QueuedMessages (initial load or scroll-back).
    #[allow(dead_code)]
    pub fn render_messages(&self, msgs: &[aios_core::queue::QueuedMessage]) {
        for msg in msgs {
            self.render_queued_message(msg);
            // Track ID range.
            if self.oldest_rendered_id.get().is_none() || msg.id < self.oldest_rendered_id.get().unwrap_or(i64::MAX) {
                self.oldest_rendered_id.set(Some(msg.id));
            }
            if msg.id > self.newest_rendered_id.get().unwrap_or(0) {
                self.newest_rendered_id.set(Some(msg.id));
            }
        }
    }

    /// Append a single new QueuedMessage. Manages the widget window.
    #[allow(dead_code)]
    pub fn append_message_from_queue(&self, msg: &aios_core::queue::QueuedMessage) {
        self.render_queued_message(msg);
        self.newest_rendered_id.set(Some(msg.id));
        if self.oldest_rendered_id.get().is_none() {
            self.oldest_rendered_id.set(Some(msg.id));
        }

        // Trim oldest widgets if we exceed the max.
        self.trim_oldest_widgets();
    }

    /// Prepend older messages at the top for scroll-back.
    #[allow(dead_code)]
    pub fn prepend_messages(&self, msgs: &[aios_core::queue::QueuedMessage]) {
        // Iterate in reverse so the oldest message ends up at the top.
        for msg in msgs.iter().rev() {
            self.prepend_queued_message(msg);
            if msg.id < self.oldest_rendered_id.get().unwrap_or(i64::MAX) {
                self.oldest_rendered_id.set(Some(msg.id));
            }
        }
    }

    /// Clear all message widgets from the display.
    #[allow(dead_code)]
    pub fn clear_display(&self) {
        while let Some(child) = self.container.first_child() {
            self.container.remove(&child);
        }
        self.oldest_rendered_id.set(None);
        self.newest_rendered_id.set(None);
    }

    /// Get the oldest rendered message ID (for scroll-back queries).
    #[allow(dead_code)]
    pub fn oldest_rendered_id(&self) -> Option<i64> {
        self.oldest_rendered_id.get()
    }

    /// Remove the oldest widgets from the top to stay within the max limit.
    #[allow(dead_code)]
    fn trim_oldest_widgets(&self) {
        let max = self.max_rendered_widgets.get();
        let mut count = 0;
        let mut child = self.container.first_child();
        while child.is_some() {
            count += 1;
            child = child.and_then(|c| c.next_sibling());
        }

        while count > max {
            if let Some(first) = self.container.first_child() {
                self.container.remove(&first);
                count -= 1;
            } else {
                break;
            }
        }
    }

    /// Prepend a single QueuedMessage at the top of the container.
    #[allow(dead_code)]
    fn prepend_queued_message(&self, msg: &aios_core::queue::QueuedMessage) {
        // Build the role string.
        let role_str = match msg.role {
            aios_core::types::Role::User => "user",
            aios_core::types::Role::Assistant => "assistant",
            aios_core::types::Role::System => "system",
            aios_core::types::Role::Tool => "tool",
        };

        if let Some(ref content) = msg.content {
            // Create a message widget the same way add_message does,
            // but prepend it instead of appending. For now, use add_message
            // then move the last child to the front.
            let count_before = {
                let mut n = 0;
                let mut child = self.container.first_child();
                while child.is_some() {
                    n += 1;
                    child = child.and_then(|c| c.next_sibling());
                }
                n
            };

            self.add_message(role_str, content);

            // If a new widget was added, move it from end to beginning.
            if let Some(last) = self.container.last_child() {
                let mut current_count = 0;
                let mut child = self.container.first_child();
                while child.is_some() {
                    current_count += 1;
                    child = child.and_then(|c| c.next_sibling());
                }
                if current_count > count_before {
                    self.container.reorder_child_after(&last, Option::<&gtk::Widget>::None);
                }
            }
        }
    }

    /// Render a single QueuedMessage using the appropriate method (appends).
    #[allow(dead_code)]
    fn render_queued_message(&self, msg: &aios_core::queue::QueuedMessage) {
        // System messages with a level get special formatting.
        if msg.role == aios_core::types::Role::System {
            if let Some(level) = msg.level {
                if let Some(ref content) = msg.content {
                    self.add_level_message(level, content);
                    return;
                }
            }
        }

        let role_str = match msg.role {
            aios_core::types::Role::User => "user",
            aios_core::types::Role::Assistant => "assistant",
            aios_core::types::Role::System => "system",
            aios_core::types::Role::Tool => "tool",
        };

        if let Some(ref content) = msg.content {
            self.add_message(role_str, content);
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

/// Build a message bubble with code block detection.
fn build_message_bubble(content: &str) -> gtk::Box {
    let bubble = gtk::Box::new(Orientation::Vertical, 4);
    bubble.add_css_class("message-bubble");

    let parts = split_code_blocks(content);
    for part in parts {
        match part {
            ContentPart::Text(text) => {
                if !text.is_empty() {
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

    bubble
}

/// Build the stop-reading button that kills TTS processes.
fn build_stop_reading_button() -> gtk::Button {
    let stop_btn = gtk::Button::from_icon_name("audio-volume-muted-symbolic");
    stop_btn.add_css_class("flat");
    stop_btn.add_css_class("circular");
    stop_btn.set_tooltip_text(Some("Stop reading"));
    stop_btn.connect_clicked(|_btn| {
        std::thread::spawn(|| {
            let _ = std::process::Command::new("pkill")
                .args(["-f", "piper"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            let _ = std::process::Command::new("pkill")
                .args(["-f", "espeak-ng"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            let _ = std::process::Command::new("pkill")
                .args(["-f", "aplay.*raw"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    });
    stop_btn
}

/// Human-friendly display name for a role.
fn role_display_name(role: &str) -> String {
    match role {
        "user" => "You".to_string(),
        "assistant" => ASSISTANT_NAME.with(|n| n.borrow().clone()),
        "system" => "System".to_string(),
        "tool" => "Tool".to_string(),
        other => other.to_string(),
    }
}
