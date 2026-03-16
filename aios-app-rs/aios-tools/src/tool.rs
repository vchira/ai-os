//! The [`Tool`] trait — interface that every AiOS tool must implement.
//!
//! Tools receive a [`serde_json::Value`] argument object and return a
//! [`ToolResult`] from `aios-core`.  The trait also provides a default
//! [`to_schema`](Tool::to_schema) method that packages the tool's metadata
//! into a [`ToolSchema`] suitable for LLM function-calling APIs.

use aios_core::types::{ToolResult, ToolSchema};

/// Interface for all AiOS tools (built-in and plugins).
///
/// Every tool declares its `name`, `description`, and a JSON Schema describing
/// the parameters it accepts.  The [`execute`](Tool::execute) method performs
/// the actual work and returns a [`ToolResult`].
pub trait Tool: Send + Sync {
    /// Unique tool name (lowercase, underscores allowed).
    fn name(&self) -> &str;

    /// One-line human-readable description of what the tool does.
    fn description(&self) -> &str;

    /// JSON Schema describing the keyword arguments accepted by [`execute`](Tool::execute).
    ///
    /// Example:
    /// ```json
    /// {
    ///     "type": "object",
    ///     "properties": {
    ///         "query": { "type": "string", "description": "Search query" }
    ///     },
    ///     "required": ["query"]
    /// }
    /// ```
    fn parameters(&self) -> serde_json::Value;

    /// Run the tool with the given arguments and return a [`ToolResult`].
    ///
    /// `args` is a JSON object whose shape matches [`parameters`](Tool::parameters).
    fn execute(&self, args: serde_json::Value) -> ToolResult;

    /// Build a [`ToolSchema`] from this tool's metadata.
    ///
    /// The default implementation packages [`name`](Tool::name),
    /// [`description`](Tool::description), and [`parameters`](Tool::parameters)
    /// into a [`ToolSchema`].
    fn to_schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }
}
