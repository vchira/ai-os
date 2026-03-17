//! Self-test system — end-to-end validation of AiOS subsystems.
//!
//! The selftest runs on whatever channel it's started from (`/selftest`).
//! It simulates realistic conversations at a very low level: mock LLM
//! responses trigger real tool execution, channel switching, panel
//! rendering, and memory operations.
//!
//! Some tests are **interactive** — they ask the user to fill in a
//! `ui_panel` or confirm something on screen.
//!
//! # Usage
//!
//! ```text
//! /selftest           Run all tests
//! /selftest quick     Run only non-interactive tests
//! /selftest channel   Run only channel tests
//! /selftest tools     Run only tool tests
//! /selftest interactive  Run only interactive tests
//! ```

pub mod conversation_sim;
pub mod runner;
pub mod scenarios;

pub use conversation_sim::{Dialog, DialogResult, DialogStep, run_dialog, run_all_dialogs};
pub use runner::{SelfTestRunner, TestResult};
