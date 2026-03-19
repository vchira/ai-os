//! Built-in tools shipped with AiOS.
//!
//! These tools are always available and registered by
//! [`ToolRegistry::load_builtins`](crate::registry::ToolRegistry::load_builtins).

pub mod browse_url;
pub mod code_exec;
pub mod conversation_history;
pub mod data_process;
pub mod delegate;
pub mod display;
pub mod files;
pub mod find_content;
pub mod memory;
pub mod notes;
pub mod recall_episodes;
pub mod reflect;
pub mod send_email;
pub mod show_image;
pub mod show_map;
pub mod system;
pub mod timer;
pub mod ui_panel;
pub mod web;

pub use browse_url::BrowseUrlTool;
pub use code_exec::CodeExecTool;
pub use conversation_history::ConversationHistoryTool;
pub use data_process::DataProcessTool;
pub use delegate::DelegateTool;
pub use display::DisplayTool;
pub use files::FilesTool;
pub use find_content::FindContentTool;
pub use memory::MemoryTool;
pub use notes::NotesTool;
pub use recall_episodes::RecallEpisodesTool;
pub use reflect::ReflectTool;
pub use send_email::SendEmailTool;
pub use show_image::ShowImageTool;
pub use show_map::ShowMapTool;
pub use system::SystemTool;
pub use timer::TimerTool;
pub use ui_panel::UiPanelTool;
pub use web::WebTool;
