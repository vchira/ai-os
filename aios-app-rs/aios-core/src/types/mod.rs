//! Shared domain types for AiOS.

pub mod message;
pub mod richtext;
pub mod status;
pub mod tool;
pub mod voice;

pub use message::*;
pub use richtext::{to_html, to_pango, to_plain};
pub use status::*;
pub use tool::*;
pub use voice::*;
