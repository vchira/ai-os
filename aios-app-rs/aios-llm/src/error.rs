//! LLM-specific error types.
//!
//! [`LlmError`] covers all failure modes that can arise when communicating
//! with an LLM provider — API errors, network failures, parse issues, and
//! configuration problems.

/// All errors that can originate from the LLM subsystem.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// The remote API returned an error response (status + body).
    #[error("API error (HTTP {status}): {message}")]
    ApiError {
        /// HTTP status code returned by the provider.
        status: u16,
        /// Error message extracted from the response body.
        message: String,
    },

    /// A network-level failure (DNS, timeout, connection refused, etc.).
    #[error("network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Failed to parse the provider's response body.
    #[error("parse error: {0}")]
    ParseError(String),

    /// No LLM provider has been registered / selected.
    #[error("no active LLM provider — register at least one and call set_active()")]
    NoProvider,

    /// The selected provider does not have an API key configured.
    #[error("no API key set for provider '{0}'")]
    NoApiKey(String),

    /// The tool-call loop exceeded the safety limit.
    #[error("tool-call loop exceeded {0} rounds")]
    ToolLoopExceeded(usize),

    /// JSON serialization / deserialization failures.
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Convenience alias for `Result<T, LlmError>`.
pub type Result<T> = std::result::Result<T, LlmError>;
