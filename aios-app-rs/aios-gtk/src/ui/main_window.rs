//! Main application window using libadwaita.
//!
//! Builds an [`adw::ApplicationWindow`] with a header bar, provider dropdown,
//! voice toggle buttons, settings button, chat view, and prompt input.

use gtk4::prelude::*;
use gtk4::{self as gtk, Orientation, Separator};
use libadwaita as adw;

use super::channel_overlay::ChannelOverlay;
use super::chat_view::ChatView;
use super::prompt_input::PromptInput;

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

/// CSS stylesheet for chat message styling.
const APP_CSS: &str = r#"
/* Dark theme chat styling */
.chat-container {
    background-color: @window_bg_color;
}

.message-row {
    padding: 6px 12px;
}

.message-bubble {
    padding: 10px 14px;
    border-radius: 12px;
}

.message-user .message-bubble {
    background-color: @accent_bg_color;
    color: @accent_fg_color;
    border-bottom-right-radius: 4px;
}

.message-assistant .message-bubble {
    background-color: alpha(@card_bg_color, 0.8);
    border-bottom-left-radius: 4px;
}

.message-system .message-bubble {
    background-color: transparent;
    color: alpha(@window_fg_color, 0.6);
    font-style: italic;
}

.message-tool .message-bubble {
    background-color: alpha(@accent_bg_color, 0.15);
    font-family: monospace;
    font-size: 0.9em;
}

.message-role-label {
    font-size: 0.75em;
    font-weight: bold;
    color: alpha(@window_fg_color, 0.5);
    margin-bottom: 2px;
}

/* Message level styling */
.msg-level-label {
    font-weight: 700;
    font-size: 0.8em;
    letter-spacing: 0.5px;
    margin-bottom: 4px;
}

.msg-info .msg-level-label { color: #4a9eff; }
.msg-success .msg-level-label { color: #2ed573; }
.msg-warning .msg-level-label { color: #ffa502; }
.msg-important .msg-level-label { color: #ff6348; }
.msg-error .msg-level-label { color: #ff4757; }

.msg-info .message-bubble { border-left: 3px solid #4a9eff; }
.msg-success .message-bubble { border-left: 3px solid #2ed573; }
.msg-warning .message-bubble { border-left: 3px solid #ffa502; }
.msg-important .message-bubble { border-left: 3px solid #ff6348; }
.msg-error .message-bubble { border-left: 3px solid #ff4757; }

.status-available { color: #2ed573; font-weight: 600; }
.status-unavailable { color: #ff4757; font-weight: 600; }

/* Mic indicator */
.mic-listening {
    color: #2ed573;
    background-color: alpha(#2ed573, 0.15);
}

.mic-muted {
    color: alpha(@window_fg_color, 0.3);
}

/* Muted voice toggle */
.muted { color: #ff4757; }

/* Disabled prompt */
.prompt-entry:disabled {
    opacity: 0.4;
}

.code-block {
    background-color: alpha(@window_fg_color, 0.05);
    border-radius: 6px;
    padding: 8px 10px;
    font-family: monospace;
    font-size: 0.9em;
}

.prompt-area {
    padding: 8px 12px;
    background-color: @headerbar_bg_color;
}

.prompt-entry {
    min-height: 36px;
}

.level-bar-recording {
    margin: 4px 12px;
}

/* VU meter — audio level indicator above prompt (discrete segments) */
.vu-meter {
    margin: 4px 12px;
    min-height: 10px;
}
.vu-meter trough {
    min-height: 10px;
    background-color: transparent;
    border-radius: 0;
}
.vu-meter block {
    min-height: 10px;
    border-radius: 2px;
    margin: 0 1px;
}
.vu-meter block.filled {
    background-color: #2ed573;
}
.vu-meter block.empty {
    background-color: alpha(@window_fg_color, 0.12);
}

.welcome-label {
    color: alpha(@window_fg_color, 0.4);
    font-size: 1.1em;
}

/* Setup card styling (first-boot conversational flow) */
.setup-card {
    padding: 16px 18px;
    border-radius: 14px;
    background-color: alpha(@card_bg_color, 0.9);
    border: 1px solid alpha(@window_fg_color, 0.08);
}

.setup-card-icon {
    color: @accent_bg_color;
    min-width: 32px;
    min-height: 32px;
}

.setup-card-title {
    font-size: 1.15em;
    font-weight: bold;
}

.setup-card-description {
    color: alpha(@window_fg_color, 0.7);
}

.setup-card-input {
    padding-top: 4px;
}

.setup-provider-button {
    padding: 8px 12px;
    border-radius: 10px;
    background-color: alpha(@window_fg_color, 0.04);
    border: 1px solid alpha(@window_fg_color, 0.1);
}

.setup-provider-button:hover {
    background-color: alpha(@accent_bg_color, 0.15);
    border-color: @accent_bg_color;
}

.setup-input {
    min-height: 36px;
}
"#;

// ---------------------------------------------------------------------------
// Widget names used for lookups
// ---------------------------------------------------------------------------

const PROVIDER_DROPDOWN_NAME: &str = "provider-dropdown";
const MIC_BUTTON_NAME: &str = "mic-toggle";
const SPEAKER_BUTTON_NAME: &str = "speaker-toggle";
const SETTINGS_BUTTON_NAME: &str = "settings-button";
const INFO_BUTTON_NAME: &str = "info-button";

// ---------------------------------------------------------------------------
// build_main_window
// ---------------------------------------------------------------------------

/// Build and return the main application window.
///
/// The window contains:
/// - An `adw::HeaderBar` with provider dropdown, mic/speaker toggles, and
///   a settings button.
/// - A `ScrolledWindow` holding the chat view (vertical, expands).
/// - A `Separator` and the prompt input area at the bottom.
/// - A [`ChannelOverlay`] that appears when the AI is talking on another channel.
/// Result of building the main window — the window plus the VU meter widget.
pub struct MainWindowResult {
    pub window: adw::ApplicationWindow,
    pub vu_meter: gtk::LevelBar,
}

// ---------------------------------------------------------------------------
// Widget factory helpers (avoid code duplication)
// ---------------------------------------------------------------------------

/// Create a header bar button with icon, tooltip, name, and initial visibility.
fn make_header_button(icon: &str, tooltip: &str, name: &str, visible: bool) -> gtk::Button {
    let btn = gtk::Button::from_icon_name(icon);
    btn.set_tooltip_text(Some(tooltip));
    btn.set_widget_name(name);
    btn.set_visible(visible);
    btn
}

/// Create a header bar toggle button with icon, tooltip, name, and initial state.
fn make_toggle_button(icon: &str, tooltip: &str, name: &str, active: bool) -> gtk::ToggleButton {
    let btn = gtk::ToggleButton::new();
    btn.set_icon_name(icon);
    btn.set_tooltip_text(Some(tooltip));
    btn.set_widget_name(name);
    btn.set_active(active);
    btn
}

pub fn build_main_window(
    app: &adw::Application,
    chat_view: &ChatView,
    prompt_input: &PromptInput,
    channel_overlay: &ChannelOverlay,
    available_providers: &[&str],
) -> MainWindowResult {
    // Load CSS.
    let css_provider = gtk::CssProvider::new();
    #[allow(deprecated)]
    css_provider.load_from_data(APP_CSS);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not get default display"),
        &css_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // --- Header bar ---
    let header = adw::HeaderBar::new();

    // Title: "AiOS v2.0.1 | 2026-03-17 12:34" with live clock
    let version = option_env!("AIOS_VERSION").unwrap_or("dev");
    let title_box = gtk::Box::new(Orientation::Horizontal, 8);
    title_box.set_halign(gtk::Align::Center);

    let title_label = gtk::Label::new(Some(&format!("AiOS v{version}")));
    title_label.add_css_class("heading");
    title_box.append(&title_label);

    let sep = gtk::Label::new(Some("|"));
    sep.set_opacity(0.4);
    title_box.append(&sep);

    let clock_label = gtk::Label::new(Some(""));
    clock_label.set_opacity(0.6);
    title_box.append(&clock_label);

    // Update clock every second
    let clock_ref = clock_label.clone();
    gtk::glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
        let now = chrono::Local::now();
        clock_ref.set_text(&now.format("%Y-%m-%d %H:%M:%S").to_string());
        gtk::glib::ControlFlow::Continue
    });
    // Set initial value
    {
        let now = chrono::Local::now();
        clock_label.set_text(&now.format("%Y-%m-%d %H:%M:%S").to_string());
    }

    header.set_title_widget(Some(&title_box));

    // Provider dropdown (left side) — only shows providers with API keys.
    let provider_names: Vec<&str> = if available_providers.is_empty() {
        vec!["No provider"]
    } else {
        available_providers.to_vec()
    };
    let provider_model = gtk::StringList::new(&provider_names);
    let provider_dropdown = gtk::DropDown::new(Some(provider_model), gtk::Expression::NONE);
    provider_dropdown.set_widget_name(PROVIDER_DROPDOWN_NAME);
    provider_dropdown.set_tooltip_text(Some("Select LLM provider"));
    header.pack_start(&provider_dropdown);

    // Right side buttons — using helper to avoid duplication.
    header.pack_end(&make_toggle_button("audio-input-microphone-symbolic", "Toggle microphone", MIC_BUTTON_NAME, true));
    header.pack_end(&make_toggle_button("audio-volume-high-symbolic", "Toggle speaker", SPEAKER_BUTTON_NAME, true));
    header.pack_end(&make_header_button("emblem-system-symbolic", "Settings", SETTINGS_BUTTON_NAME, false));
    header.pack_end(&make_header_button("dialog-information-symbolic", "Info", INFO_BUTTON_NAME, false));

    // --- Main content ---
    let content_box = gtk::Box::new(Orientation::Vertical, 0);

    // Scrolled window for chat.
    let scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    scrolled.set_child(Some(chat_view.widget()));
    chat_view.set_scroll_window(&scrolled);
    content_box.append(&scrolled);

    // Separator.
    let separator = Separator::new(Orientation::Horizontal);
    content_box.append(&separator);

    // VU meter — real-time audio level indicator above the prompt.
    // Shows whether the mic is picking up sound (green = audio, gray = silent).
    // Discrete mode with 20 segments for a classic "squares" VU look.
    let vu_meter = gtk::LevelBar::builder()
        .min_value(0.0)
        .max_value(20.0)
        .value(0.0)
        .mode(gtk::LevelBarMode::Discrete)
        .build();
    vu_meter.set_widget_name("vu-meter");
    vu_meter.add_css_class("vu-meter");
    content_box.append(&vu_meter);

    // Prompt input.
    content_box.append(prompt_input.widget());

    // --- Overlay for channel switching ---
    // The content_box is the base, and the channel overlay sits on top.
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&content_box));
    overlay.add_overlay(channel_overlay.widget());

    // --- Assemble window ---
    let outer_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer_box.append(&header);
    outer_box.append(&overlay);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("AiOS")
        .default_width(700)
        .default_height(800)
        .content(&outer_box)
        .build();

    // Fullscreen on startup — AiOS IS the desktop
    window.fullscreen();

    // AiOS is the desktop shell — prevent closing the window.
    window.connect_close_request(|_| {
        gtk::glib::Propagation::Stop
    });

    MainWindowResult { window, vu_meter }
}

// ---------------------------------------------------------------------------
// Visibility helpers
// ---------------------------------------------------------------------------

/// Show or hide the settings button.
pub fn set_settings_button_visible(window: &adw::ApplicationWindow, visible: bool) {
    if let Some(button) = find_widget_by_name::<gtk::Button>(window.upcast_ref(), SETTINGS_BUTTON_NAME) {
        button.set_visible(visible);
    }
    if let Some(button) = find_widget_by_name::<gtk::Button>(window.upcast_ref(), INFO_BUTTON_NAME) {
        button.set_visible(visible);
    }
}

/// Update the provider dropdown model with the given provider names.
pub fn update_provider_dropdown(window: &adw::ApplicationWindow, providers: &[&str]) {
    if let Some(dropdown) = find_widget_by_name::<gtk::DropDown>(window.upcast_ref(), PROVIDER_DROPDOWN_NAME) {
        let names: Vec<&str> = if providers.is_empty() {
            vec!["No provider"]
        } else {
            providers.to_vec()
        };
        let model = gtk::StringList::new(&names);
        dropdown.set_model(Some(&model));
    }
}

// ---------------------------------------------------------------------------
// Signal connectors
// ---------------------------------------------------------------------------

/// Find the provider dropdown in the window and connect to selection changes.
pub fn connect_provider_dropdown(
    window: &adw::ApplicationWindow,
    callback: impl Fn(&str) + 'static,
) {
    if let Some(dropdown) = find_widget_by_name::<gtk::DropDown>(window.upcast_ref(), PROVIDER_DROPDOWN_NAME) {
        dropdown.connect_selected_notify(move |dd| {
            // Read the actual selected string from the model instead of
            // hardcoding index → name, since the dropdown may only show
            // providers that have API keys configured.
            let selected = dd.selected();
            let name = dd
                .model()
                .and_then(|m| m.downcast::<gtk::StringList>().ok())
                .and_then(|sl| {
                    if selected < sl.n_items() {
                        sl.string(selected).map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "Claude".to_string());
            callback(&name);
        });
    }
}

/// Find the settings button and connect its clicked signal.
pub fn connect_settings_button(
    window: &adw::ApplicationWindow,
    callback: impl Fn() + 'static,
) {
    if let Some(button) = find_widget_by_name::<gtk::Button>(window.upcast_ref(), SETTINGS_BUTTON_NAME) {
        button.connect_clicked(move |_| {
            callback();
        });
    }
}

/// Find the info button and connect its clicked signal.
pub fn connect_info_button(
    window: &adw::ApplicationWindow,
    callback: impl Fn() + 'static,
) {
    if let Some(button) = find_widget_by_name::<gtk::Button>(window.upcast_ref(), INFO_BUTTON_NAME) {
        button.connect_clicked(move |_| {
            callback();
        });
    }
}

/// Find the mic toggle button and connect its toggled signal.
///
/// When toggled ON: microphone icon, green tint.
/// When toggled OFF: muted icon, red "muted" CSS class.
pub fn connect_mic_toggle(
    window: &adw::ApplicationWindow,
    callback: impl Fn(bool) + 'static,
) {
    if let Some(button) = find_widget_by_name::<gtk::ToggleButton>(window.upcast_ref(), MIC_BUTTON_NAME) {
        button.connect_toggled(move |btn| {
            let active = btn.is_active();
            if active {
                btn.set_icon_name("audio-input-microphone-symbolic");
                btn.remove_css_class("muted");
            } else {
                btn.set_icon_name("microphone-sensitivity-muted-symbolic");
                btn.add_css_class("muted");
            }
            callback(active);
        });
    }
}

/// Find the speaker toggle button and connect its toggled signal.
///
/// When toggled ON: speaker icon, normal color.
/// When toggled OFF: muted speaker icon, red "muted" CSS class.
pub fn connect_speaker_toggle(
    window: &adw::ApplicationWindow,
    callback: impl Fn(bool) + 'static,
) {
    if let Some(button) = find_widget_by_name::<gtk::ToggleButton>(window.upcast_ref(), SPEAKER_BUTTON_NAME) {
        button.connect_toggled(move |btn| {
            let active = btn.is_active();
            if active {
                btn.set_icon_name("audio-volume-high-symbolic");
                btn.remove_css_class("muted");
            } else {
                btn.set_icon_name("audio-volume-muted-symbolic");
                btn.add_css_class("muted");
            }
            callback(active);
        });
    }
}

// ---------------------------------------------------------------------------
// Widget finder helper
// ---------------------------------------------------------------------------

/// Recursively search the widget tree for a widget with the given name.
///
/// Returns the first match, cast to `T`, or `None`.
pub fn find_widget_by_name<T: IsA<gtk::Widget>>(
    root: &gtk::Widget,
    name: &str,
) -> Option<T> {
    if root.widget_name() == name {
        return root.downcast_ref::<T>().cloned();
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_widget_by_name::<T>(&widget, name) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}
