//! AiOS GTK application entry point.
//!
//! Initializes tracing, creates a Tokio runtime, and launches the
//! libadwaita application.

mod app;
mod ui;

use gtk4::prelude::*;
use tracing_subscriber::EnvFilter;

fn main() {
    // Initialize tracing subscriber with RUST_LOG support.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("AiOS v{} starting", env!("CARGO_PKG_VERSION"));

    // Create a multi-threaded Tokio runtime for async LLM calls.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create Tokio runtime");

    let rt_handle = runtime.handle().clone();

    // Initialize libadwaita (which also initializes GTK4).
    let application = libadwaita::Application::builder()
        .application_id("dev.aios.app")
        .build();

    // Connect the activate signal to build the UI.
    let handle = rt_handle.clone();
    application.connect_activate(move |app| {
        app::AiosApp::activate(app, handle.clone());
    });

    // Run the GTK event loop.  The Tokio runtime lives alongside it.
    let _exit_code = application.run();

    // Shutdown the runtime gracefully after GTK exits.
    drop(runtime);
}
