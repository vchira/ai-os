//! Built-in tools shipped with AiOS.
//!
//! These tools are always available and registered by
//! [`ToolRegistry::load_builtins`](crate::registry::ToolRegistry::load_builtins).

pub mod code_exec;
pub mod conversation_history;
pub mod data_process;
pub mod delegate;
pub mod display;
pub mod files;
pub mod find_content;
pub mod memory;
pub mod recall_episodes;
pub mod reflect;
pub mod system;
pub mod ui_panel;
pub mod web;

pub use code_exec::CodeExecTool;
pub use conversation_history::ConversationHistoryTool;
pub use data_process::DataProcessTool;
pub use delegate::DelegateTool;
pub use display::DisplayTool;
pub use files::FilesTool;
pub use find_content::FindContentTool;
pub use memory::MemoryTool;
pub use recall_episodes::RecallEpisodesTool;
pub use reflect::ReflectTool;
pub use system::SystemTool;
pub use ui_panel::UiPanelTool;
pub use web::WebTool;
