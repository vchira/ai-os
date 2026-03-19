//! Info dialog — tabbed panel with Costs and About tabs.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};
use libadwaita as adw;

use aios_core::config::ConfigManager;

/// Show the info dialog with Costs and About tabs.
pub fn show_info_dialog(parent: &adw::ApplicationWindow) {
    let dialog = adw::Window::builder()
        .title("AiOS Info")
        .default_width(500)
        .default_height(450)
        .modal(true)
        .transient_for(parent)
        .build();

    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header bar with close button.
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("AiOS Info"))));
    content.append(&header);

    // Tabbed notebook.
    let notebook = gtk::Notebook::new();
    notebook.set_vexpand(true);
    notebook.set_hexpand(true);

    // --- Tab 1: Costs ---
    let costs_page = build_costs_tab();
    notebook.append_page(&costs_page, Some(&gtk::Label::new(Some("Costs"))));

    // --- Tab 2: About ---
    let about_page = build_about_tab();
    notebook.append_page(&about_page, Some(&gtk::Label::new(Some("About"))));

    content.append(&notebook);
    dialog.set_child(Some(&content));
    dialog.present();
}

/// Build the Costs tab content.
fn build_costs_tab() -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(16);
    page.set_margin_bottom(16);
    page.set_margin_start(16);
    page.set_margin_end(16);

    // Title
    let title = gtk::Label::new(Some("AI Provider Costs"));
    title.add_css_class("title-3");
    title.set_halign(Align::Start);
    page.append(&title);

    // Pricing table
    let pricing_data = [
        ("Provider", "Model", "Input/1M", "Output/1M", "Quality"),
        ("Claude", "Sonnet 4", "$3.00", "$15.00", "Excellent"),
        ("Claude", "Haiku 3.5", "$0.80", "$4.00", "Good"),
        ("ChatGPT", "GPT-4o", "$2.50", "$10.00", "Excellent"),
        ("ChatGPT", "GPT-4o Mini", "$0.15", "$0.60", "Good"),
        ("DeepSeek", "DeepSeek V3", "$0.27", "$1.10", "Great"),
        ("Mistral", "Small", "$0.10", "$0.30", "Good"),
        ("Groq", "Llama 3.3 70B", "$0.59", "$0.79", "Good"),
        ("Gemini", "2.0 Flash", "$0.075", "$0.30", "Good"),
        ("Ollama", "Local models", "Free", "Free", "Varies"),
    ];

    let grid = gtk::Grid::new();
    grid.set_row_spacing(4);
    grid.set_column_spacing(12);
    grid.add_css_class("monospace");

    for (row, (provider, model, input, output, quality)) in pricing_data.iter().enumerate() {
        let r = row as i32;
        let is_header = row == 0;

        let cells = [provider, model, input, output, quality];
        for (col, text) in cells.iter().enumerate() {
            let label = gtk::Label::new(Some(text));
            label.set_halign(if col >= 2 { Align::End } else { Align::Start });
            if is_header {
                label.add_css_class("heading");
            }
            if col == 2 || col == 3 {
                label.add_css_class("numeric");
            }
            grid.attach(&label, col as i32, r, 1, 1);
        }
    }

    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    scroll.set_child(Some(&grid));
    page.append(&scroll);

    // Current usage section
    let usage_title = gtk::Label::new(Some("Current Session Usage"));
    usage_title.add_css_class("title-4");
    usage_title.set_halign(Align::Start);
    usage_title.set_margin_top(12);
    page.append(&usage_title);

    let config = ConfigManager::new().ok();
    let input_tokens = config.as_ref()
        .map(|c| c.get_str("llm.total_input_tokens", "0"))
        .unwrap_or_else(|| "0".to_string());
    let output_tokens = config.as_ref()
        .map(|c| c.get_str("llm.total_output_tokens", "0"))
        .unwrap_or_else(|| "0".to_string());
    let provider = config.as_ref()
        .map(|c| c.get_str("llm.provider", "claude"))
        .unwrap_or_else(|| "claude".to_string());
    let model = config.as_ref()
        .map(|c| c.get_str(&format!("llm.{provider}_model"), "unknown"))
        .unwrap_or_else(|| "unknown".to_string());

    let usage_text = format!(
        "Provider: {provider}\nModel: {model}\nInput tokens: {input_tokens}\nOutput tokens: {output_tokens}"
    );
    let usage_label = gtk::Label::new(Some(&usage_text));
    usage_label.set_halign(Align::Start);
    usage_label.set_wrap(true);
    page.append(&usage_label);

    // Tip
    let tip = gtk::Label::new(Some(
        "Tip: Use /provider to switch providers. DeepSeek offers great quality at 10x lower cost than Claude."
    ));
    tip.set_halign(Align::Start);
    tip.set_wrap(true);
    tip.add_css_class("dim-label");
    tip.set_margin_top(8);
    page.append(&tip);

    page
}

/// Build the About tab content.
fn build_about_tab() -> gtk::Box {
    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(16);
    page.set_margin_bottom(16);
    page.set_margin_start(16);
    page.set_margin_end(16);

    // Logo / Title
    let title = gtk::Label::new(Some("AiOS"));
    title.add_css_class("title-1");
    title.set_halign(Align::Center);
    page.append(&title);

    let subtitle = gtk::Label::new(Some("The AI-Native Linux Distribution"));
    subtitle.add_css_class("title-3");
    subtitle.set_halign(Align::Center);
    page.append(&subtitle);

    // Version
    let version = env!("CARGO_PKG_VERSION");
    let version_label = gtk::Label::new(Some(&format!("Version {version}")));
    version_label.add_css_class("dim-label");
    version_label.set_halign(Align::Center);
    page.append(&version_label);

    // Separator
    let sep = gtk::Separator::new(Orientation::Horizontal);
    sep.set_margin_top(8);
    sep.set_margin_bottom(8);
    page.append(&sep);

    // Info grid
    let info_items = [
        ("License", "BSL 1.1 (Business Source License)"),
        ("Change Date", "5 years from release → Apache 2.0"),
        ("Licensor", "swIT.work GmbH"),
        ("Base", "Debian Bookworm (12)"),
        ("Display", "Wayland (labwc compositor)"),
        ("Application", "Rust + GTK4/libadwaita"),
        ("Voice", "Whisper STT + Piper TTS"),
        ("AI Providers", "Claude, ChatGPT, DeepSeek, Mistral, Groq, Gemini, Ollama"),
    ];

    let info_grid = gtk::Grid::new();
    info_grid.set_row_spacing(6);
    info_grid.set_column_spacing(16);

    for (row, (key, value)) in info_items.iter().enumerate() {
        let key_label = gtk::Label::new(Some(key));
        key_label.set_halign(Align::End);
        key_label.add_css_class("dim-label");

        let val_label = gtk::Label::new(Some(value));
        val_label.set_halign(Align::Start);
        val_label.set_wrap(true);
        val_label.set_max_width_chars(40);

        info_grid.attach(&key_label, 0, row as i32, 1, 1);
        info_grid.attach(&val_label, 1, row as i32, 1, 1);
    }

    page.append(&info_grid);

    // Links
    let sep2 = gtk::Separator::new(Orientation::Horizontal);
    sep2.set_margin_top(8);
    sep2.set_margin_bottom(8);
    page.append(&sep2);

    let links_box = gtk::Box::new(Orientation::Vertical, 4);
    links_box.set_halign(Align::Center);

    let website = gtk::Label::new(Some("Website: https://aios.pages.dev"));
    website.add_css_class("dim-label");
    links_box.append(&website);

    let github = gtk::Label::new(Some("GitHub: https://github.com/swit-work/ai-os"));
    github.add_css_class("dim-label");
    links_box.append(&github);

    let copyright = gtk::Label::new(Some("© 2026 swIT.work GmbH"));
    copyright.add_css_class("dim-label");
    copyright.set_margin_top(8);
    links_box.append(&copyright);

    page.append(&links_box);

    page
}
