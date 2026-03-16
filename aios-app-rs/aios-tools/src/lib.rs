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
pub mod tool;

// Re-export the most commonly used items.
pub use error::ToolError;
pub use registry::ToolRegistry;
pub use tool::Tool;
