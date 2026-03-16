//! Channel overlay — semi-transparent modal shown when the AI is talking
//! on a different channel (Signal, Web).
//!
//! When the conversation moves away from the Desktop, this overlay covers
//! the chat area with a dimmed background, a status label ("AI is talking
//! on Signal"), and a "Switch back here" button.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};

use aios_core::channel::ChannelKind;

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

const OVERLAY_CSS: &str = r#"
.channel-overlay {
    background-color: alpha(black, 0.75);
}

.channel-overlay-content {
    padding: 32px 48px;
    border-radius: 16px;
    background-color: @window_bg_color;
}

.channel-overlay-icon {
    color: @accent_bg_color;
    min-width: 48px;
    min-height: 48px;
}

.channel-overlay-title {
    font-size: 1.4em;
    font-weight: bold;
}

.channel-overlay-subtitle {
    color: alpha(@window_fg_color, 0.6);
    margin-bottom: 16px;
}

.channel-overlay-button {
    padding: 8px 24px;
    font-weight: 600;
}
"#;

// ---------------------------------------------------------------------------
// ChannelOverlay
// ---------------------------------------------------------------------------

/// A semi-transparent overlay shown when the active channel is not Desktop.
///
/// Contains a centered card with:
/// - An icon representing the remote channel
/// - "AI is talking on {channel}" title
/// - A subtitle with guidance
/// - A "Switch back here" button
#[allow(dead_code)]
#[derive(Clone)]
pub struct ChannelOverlay {
    /// The overlay container (covers the full chat area).
    widget: gtk::Box,
    /// The title label showing which channel is active.
    title_label: gtk::Label,
    /// The icon representing the remote channel.
    icon: gtk::Image,
    /// The "Switch back here" button.
    switch_button: gtk::Button,
}

impl ChannelOverlay {
    /// Create a new channel overlay (hidden by default).
    pub fn new() -> Self {
        // Load CSS.
        let css_provider = gtk::CssProvider::new();
        #[allow(deprecated)]
        css_provider.load_from_data(OVERLAY_CSS);
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("Could not get default display"),
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // Outer container — fills the overlay area with a dark background.
        let widget = gtk::Box::new(Orientation::Vertical, 0);
        widget.add_css_class("channel-overlay");
        widget.set_halign(Align::Fill);
        widget.set_valign(Align::Fill);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget.set_visible(false);

        // Center the card in the overlay.
        let center_box = gtk::Box::new(Orientation::Vertical, 0);
        center_box.set_halign(Align::Center);
        center_box.set_valign(Align::Center);
        center_box.set_vexpand(true);

        // Card content.
        let card = gtk::Box::new(Orientation::Vertical, 12);
        card.add_css_class("channel-overlay-content");
        card.set_halign(Align::Center);

        let icon = gtk::Image::from_icon_name("phone-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("channel-overlay-icon");
        icon.set_halign(Align::Center);
        card.append(&icon);

        let title_label = gtk::Label::new(Some("AI is talking on Signal"));
        title_label.add_css_class("channel-overlay-title");
        title_label.set_halign(Align::Center);
        card.append(&title_label);

        let subtitle = gtk::Label::new(Some(
            "The conversation is active on another channel.\n\
             Messages are still visible here.",
        ));
        subtitle.add_css_class("channel-overlay-subtitle");
        subtitle.set_halign(Align::Center);
        subtitle.set_justify(gtk::Justification::Center);
        card.append(&subtitle);

        let switch_button = gtk::Button::with_label("Switch back here");
        switch_button.add_css_class("suggested-action");
        switch_button.add_css_class("channel-overlay-button");
        switch_button.set_halign(Align::Center);
        card.append(&switch_button);

        center_box.append(&card);
        widget.append(&center_box);

        Self {
            widget,
            title_label,
            icon,
            switch_button,
        }
    }

    /// Get the underlying GTK widget (for adding to an overlay).
    pub fn widget(&self) -> &gtk::Box {
        &self.widget
    }

    /// Show the overlay with a message for the given channel.
    pub fn show(&self, channel: ChannelKind) {
        let (title, icon_name) = match channel {
            ChannelKind::Signal => ("AI is talking on Signal", "phone-symbolic"),
            ChannelKind::Web => ("AI is talking on Web", "web-browser-symbolic"),
            ChannelKind::Voice => ("AI is in voice mode", "audio-speakers-symbolic"),
            ChannelKind::Desktop => {
                // Should not happen — Desktop means hide the overlay.
                self.hide();
                return;
            }
        };

        self.title_label.set_text(title);
        self.icon.set_icon_name(Some(icon_name));
        self.widget.set_visible(true);
    }

    /// Hide the overlay (when Desktop becomes the active channel again).
    pub fn hide(&self) {
        self.widget.set_visible(false);
    }

    /// Connect the "Switch back here" button to a callback.
    pub fn on_switch_back(&self, cb: impl Fn() + 'static) {
        self.switch_button.connect_clicked(move |_| {
            cb();
        });
    }
}

impl Default for ChannelOverlay {
    fn default() -> Self {
        Self::new()
    }
}
