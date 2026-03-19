//! AiOS tool/plugin system — built-in tools and registry.
//!
//! This crate provides the [`Tool`] trait that every tool must implement,
//! a [`ToolRegistry`] for managing available tools, and the suite of built-in
//! tools shipped with AiOS:
//!
//! | Tool | Description |
//! |------|-------------|
//! | [`builtin::MemoryTool`] | Persistent key-value memory store |
//! | [`builtin::SystemTool`] | Shell commands, system info, processes |
//! | [`builtin::FilesTool`] | File read/write/list/search/info |
//! | [`builtin::WebTool`] | HTTP fetch, web search, file download |
//! | [`builtin::DisplayTool`] | Images, notifications, markdown rendering |
//! | [`builtin::UiPanelTool`] | Show input panels to the user and collect responses |
//! | [`builtin::CodeExecTool`] | Sandboxed code execution (Python, Bash, JS, Rust) |
//! | [`builtin::DataProcessTool`] | Local data processing (CSV, JSON, logs, grep) |
//! | [`builtin::FindContentTool`] | Semantic file search by content/meaning |
//!
//! # Quick start
//!
//! ```rust,no_run
//! use aios_tools::registry::ToolRegistry;
//!
//! let mut registry = ToolRegistry::new();
//! registry.load_builtins();
//!
//! let result = registry.execute("memory", serde_json::json!({
//!     "action": "list_keys"
//! }));
//! println!("{}", result.output);
//! ```

pub mod builtin;
pub mod error;
pub mod registry;
pub mod sandbox;
pub mod tool;

// Re-export the most commonly used items.
pub use error::ToolError;
pub use registry::ToolRegistry;
pub use sandbox::{Sandbox, SandboxType};
pub use tool::Tool;

/// Return the user's home directory, falling back to `/home/aios`.
///
/// The fallback is the AiOS default user's home directory, which is
/// guaranteed to exist on the target system.  We never fall back to
/// `/tmp` because that is ephemeral and would silently lose data.
pub fn home_dir() -> std::path::PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("/home/aios"))
}
