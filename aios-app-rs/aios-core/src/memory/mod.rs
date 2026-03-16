//! Memory subsystem — episodic memory, semantic reflections, and file indexing.
//!
//! This module provides structured memory storage for the AI, moving beyond
//! raw conversation logs to semantic reflections about what happened, what
//! worked, and what failed.
//!
//! * [`episodic`] — Structured episode storage with outcomes and reflections
//! * [`semantic`] — TF-IDF keyword-based file index for semantic search

pub mod episodic;
pub mod semantic;

pub use episodic::{Episode, EpisodeCategory, EpisodicMemory, Outcome};
pub use semantic::{FileIndex, SemanticIndex};
