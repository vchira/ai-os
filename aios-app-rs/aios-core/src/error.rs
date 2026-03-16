//! Unified error type for the AiOS application.

/// All errors produced by the AiOS core library.
#[derive(Debug, thiserror::Error)]
pub enum AiosError {
    /// Configuration loading / saving failures.
    #[error("config error: {0}")]
    Config(String),

    /// Filesystem I/O errors.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization / deserialization errors.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// LLM provider errors (API failures, auth, rate-limiting, etc.).
    #[error("provider error: {0}")]
    Provider(String),

    /// Voice subsystem errors (STT / TTS).
    #[error("voice error: {0}")]
    Voice(String),

    /// Tool execution or plugin errors.
    #[error("tool error: {0}")]
    Tool(String),

    /// Secure storage / crypto errors (vault, auth, permissions).
    #[error("secure error: {0}")]
    Secure(String),

    /// Catch-all for anything that doesn't fit the above categories.
    #[error("{0}")]
    Other(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, AiosError>;
