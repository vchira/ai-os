//! Built-in tools shipped with AiOS.
//!
//! These tools are always available and registered by
//! [`ToolRegistry::load_builtins`](crate::registry::ToolRegistry::load_builtins).

pub mod display;
pub mod files;
pub mod memory;
pub mod system;
pub mod web;

pub use display::DisplayTool;
pub use files::FilesTool;
pub use memory::MemoryTool;
pub use system::SystemTool;
pub use web::WebTool;
