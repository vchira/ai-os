//! AiOS GTK application entry point.
//!
//! Initializes tracing (with file logging + rotation), creates a Tokio runtime,
//! and launches the libadwaita application.

mod app;
mod boot_status;
mod command_handler;
mod first_boot_flow;
mod llm_handler;
mod message_loop;
mod setup;
mod tts;
mod ui;
mod voice_listener;
mod voice_setup;

use gtk4::prelude::*;
use tracing_subscriber::EnvFilter;

/// Log directory inside the user's home.
const LOG_DIR: &str = ".aios/logs";
/// Max log file size before rotation (2 MB).
const MAX_LOG_SIZE: u64 = 2 * 1024 * 1024;
/// Max number of rotated log files to keep.
const MAX_LOG_FILES: usize = 5;

fn main() {
    // Ensure log directory exists.
    let log_dir = dirs_home().join(LOG_DIR);
    let _ = std::fs::create_dir_all(&log_dir);

    // Rotate logs if current one is too large.
    rotate_logs(&log_dir);

    // Set up file + stderr logging.
    let log_file = log_dir.join("aios.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    match file {
        Ok(f) => {
            // Log to both stderr and file.
            use tracing_subscriber::layer::SubscriberExt;
            use tracing_subscriber::util::SubscriberInitExt;

            let file_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Mutex::new(f))
                .with_ansi(false);

            let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

            tracing_subscriber::registry()
                .with(env_filter)
                .with(file_layer)
                .with(stderr_layer)
                .init();
        }
        Err(_) => {
            // Fallback: stderr only.
            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .init();
        }
    }

    tracing::info!("AiOS v{} starting", env!("CARGO_PKG_VERSION"));
    tracing::info!("Log file: {}", log_dir.join("aios.log").display());

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
    // On first call: creates the window. On subsequent calls (e.g., from
    // keybinding launching a second instance): raises the existing window.
    let handle = rt_handle.clone();
    application.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
        } else {
            app::AiosApp::activate(app, handle.clone());
        }
    });

    let _exit_code = application.run();
    drop(runtime);
}

/// Get the user's home directory.
fn dirs_home() -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
}

/// Simple log rotation: if aios.log > MAX_LOG_SIZE, rename to aios.log.1, etc.
fn rotate_logs(log_dir: &std::path::Path) {
    let current = log_dir.join("aios.log");
    if !current.exists() {
        return;
    }
    let size = std::fs::metadata(&current).map(|m| m.len()).unwrap_or(0);
    if size < MAX_LOG_SIZE {
        return;
    }

    // Shift old logs: aios.log.4 -> delete, aios.log.3 -> .4, ..., aios.log -> .1
    for i in (1..MAX_LOG_FILES).rev() {
        let from = log_dir.join(format!("aios.log.{i}"));
        let to = log_dir.join(format!("aios.log.{}", i + 1));
        let _ = std::fs::rename(&from, &to);
    }
    let _ = std::fs::rename(&current, log_dir.join("aios.log.1"));

    // Delete oldest if over limit.
    let oldest = log_dir.join(format!("aios.log.{}", MAX_LOG_FILES + 1));
    let _ = std::fs::remove_file(oldest);
}
