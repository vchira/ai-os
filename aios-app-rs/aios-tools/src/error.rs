//! Tool-specific error types.
//!
//! [`ToolError`] covers all failure modes that can occur when registering,
//! discovering, or executing tools.

/// Errors produced by the tool subsystem.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// A tool with the given name was not found in the registry.
    #[error("tool not found: {0}")]
    NotFound(String),

    /// A tool with this name is already registered.
    #[error("tool already registered: {0}")]
    AlreadyRegistered(String),

    /// The arguments supplied to a tool were invalid.
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),

    /// The tool execution itself failed.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// An I/O error occurred during tool execution.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A JSON serialization / deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// An HTTP request error (e.g. from the web tool).
    #[error("http error: {0}")]
    Http(String),

    /// The requested operation was blocked by a safety rule.
    #[error("blocked: {0}")]
    Blocked(String),

    /// A timeout expired while executing the tool.
    #[error("timeout: {0}")]
    Timeout(String),
}
