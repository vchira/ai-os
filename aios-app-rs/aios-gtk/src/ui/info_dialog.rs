//! Info dialog — Costs and About pages using adw::PreferencesWindow.

use gtk4::prelude::*;
use libadwaita::prelude::*;
use libadwaita as adw;

use aios_core::config::ConfigManager;

/// Show the info dialog with Costs and About pages.
pub fn show_info_dialog(parent: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesWindow::builder()
        .title("AiOS Info")
        .transient_for(parent)
        .modal(true)
        .build();

    dialog.add(&build_costs_page());
    dialog.add(&build_about_page());

    dialog.present();
}

/// Build the Costs page.
fn build_costs_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Costs")
        .icon_name("accessories-calculator-symbolic")
        .build();

    // Pricing table group
    let pricing_group = adw::PreferencesGroup::builder()
        .title("AI Provider Pricing (per 1M tokens)")
        .description("Compare costs across providers to choose the best value.")
        .build();

    let pricing = [
        ("Claude Sonnet 4", "$3.00 in / $15.00 out", "Excellent quality"),
        ("Claude Haiku 3.5", "$0.80 in / $4.00 out", "Good, fast"),
        ("ChatGPT GPT-4o", "$2.50 in / $10.00 out", "Excellent quality"),
        ("ChatGPT GPT-4o Mini", "$0.15 in / $0.60 out", "Good, cheap"),
        ("DeepSeek V3", "$0.27 in / $1.10 out", "Great, very cheap"),
        ("Mistral Small", "$0.10 in / $0.30 out", "Good, cheapest cloud"),
        ("Groq Llama 3.3 70B", "$0.59 in / $0.79 out", "Good, very fast"),
        ("Gemini 2.0 Flash", "$0.075 in / $0.30 out", "Good, cheapest"),
        ("Ollama (local)", "Free", "Varies, runs on your hardware"),
    ];

    for (name, cost, quality) in &pricing {
        let row = adw::ActionRow::builder()
            .title(*name)
            .subtitle(&format!("{cost} — {quality}"))
            .build();
        pricing_group.add(&row);
    }

    page.add(&pricing_group);

    // Current usage group
    let usage_group = adw::PreferencesGroup::builder()
        .title("Current Session")
        .build();

    let config = ConfigManager::new().ok();
    let provider = config.as_ref()
        .map(|c| c.get_str("llm.provider", "claude"))
        .unwrap_or_else(|| "claude".to_string());
    let model_key = format!("llm.{provider}_model");
    let model = config.as_ref()
        .map(|c| c.get_str(&model_key, "unknown"))
        .unwrap_or_else(|| "unknown".to_string());

    let provider_row = adw::ActionRow::builder()
        .title("Active Provider")
        .subtitle(&format!("{provider} ({model})"))
        .build();
    usage_group.add(&provider_row);

    let tip_row = adw::ActionRow::builder()
        .title("Tip")
        .subtitle("Use /provider to switch. DeepSeek offers great quality at 10x lower cost.")
        .build();
    usage_group.add(&tip_row);

    page.add(&usage_group);
    page
}

/// Build the About page.
fn build_about_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("About")
        .icon_name("help-about-symbolic")
        .build();

    // Version group
    let version_group = adw::PreferencesGroup::builder()
        .title("AiOS — The AI-Native Linux Distribution")
        .description(&format!("Version {}", env!("CARGO_PKG_VERSION")))
        .build();

    let info = [
        ("License", "BSL 1.1 (Business Source License)"),
        ("Change Date", "5 years from release → Apache 2.0"),
        ("Licensor", "swIT.work GmbH"),
    ];

    for (key, value) in &info {
        let row = adw::ActionRow::builder()
            .title(*key)
            .subtitle(*value)
            .build();
        version_group.add(&row);
    }

    page.add(&version_group);

    // Tech stack group
    let tech_group = adw::PreferencesGroup::builder()
        .title("Technology")
        .build();

    let tech = [
        ("Base", "Debian Bookworm (12)"),
        ("Display", "Wayland (labwc compositor)"),
        ("Application", "Rust + GTK4/libadwaita"),
        ("Voice STT", "Whisper (local inference)"),
        ("Voice TTS", "Piper + espeak-ng"),
        ("Wake Word", "openWakeWord (ONNX)"),
        ("AI Providers", "Claude, ChatGPT, DeepSeek, Mistral, Groq, Gemini, Ollama"),
    ];

    for (key, value) in &tech {
        let row = adw::ActionRow::builder()
            .title(*key)
            .subtitle(*value)
            .build();
        tech_group.add(&row);
    }

    page.add(&tech_group);

    // Links group
    let links_group = adw::PreferencesGroup::builder()
        .title("Links")
        .build();

    let links = [
        ("Website", "https://aios.pages.dev"),
        ("GitHub", "https://github.com/swit-work/ai-os"),
        ("Documentation", "https://aios.pages.dev/docs"),
    ];

    for (key, value) in &links {
        let row = adw::ActionRow::builder()
            .title(*key)
            .subtitle(*value)
            .build();
        links_group.add(&row);
    }

    let copyright_row = adw::ActionRow::builder()
        .title("© 2026 swIT.work GmbH")
        .build();
    links_group.add(&copyright_row);

    page.add(&links_group);
    page
}
