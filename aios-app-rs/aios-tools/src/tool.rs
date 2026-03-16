//! The [`Tool`] trait — interface that every AiOS tool must implement.
//!
//! Tools receive a [`serde_json::Value`] argument object and return a
//! [`ToolResult`] from `aios-core`.  The trait also provides a default
//! [`to_schema`](Tool::to_schema) method that packages the tool's metadata
//! into a [`ToolSchema`] suitable for LLM function-calling APIs.
//!
//! ## Channel awareness
//!
//! Tools that need to adapt to the active channel (Desktop, Web, Signal,
//! Voice) override [`execute_on_channel`](Tool::execute_on_channel).
//! Most tools are channel-agnostic and only need to implement
//! [`execute`](Tool::execute) — the default `execute_on_channel` simply
//! delegates to `execute`, ignoring the channel.

use aios_core::channel::ChannelContext;
use aios_core::types::{ToolResult, ToolSchema};

/// Interface for all AiOS tools (built-in and plugins).
///
/// Every tool declares its `name`, `description`, and a JSON Schema describing
/// the parameters it accepts.  The [`execute`](Tool::execute) method performs
/// the actual work and returns a [`ToolResult`].
///
/// ## Channel-aware tools
///
/// Most tools are channel-agnostic — they read files, run commands, and
/// return text regardless of where the user is.  Only tools that interact
/// with the user's display (e.g. `ui_panel`, `display`) need to override
/// [`execute_on_channel`](Tool::execute_on_channel) to adapt their output.
///
/// **Important**: Every tool must implement at least `execute`.
/// Channel-aware tools should also override `execute_on_channel`.
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
    ///
    /// This is the simple, channel-agnostic entry point.  The LLM manager
    /// calls [`execute_on_channel`](Tool::execute_on_channel) instead, which
    /// delegates here by default.
    fn execute(&self, args: serde_json::Value) -> ToolResult;

    /// Run the tool with channel awareness.
    ///
    /// The default implementation ignores the channel and delegates to
    /// [`execute`](Tool::execute).  Channel-aware tools (e.g. `ui_panel`,
    /// `display`) override this to adapt their rendering based on the
    /// channel's capabilities.
    fn execute_on_channel(
        &self,
        args: serde_json::Value,
        _channel: &ChannelContext,
    ) -> ToolResult {
        self.execute(args)
    }

    /// Tool category for bundle-based discovery.
    ///
    /// Tools are grouped into categories so that only relevant tool schemas
    /// are sent with each LLM request.  The default category is `"general"`.
    ///
    /// Standard categories:
    /// - `"memory"` — key-value persistence
    /// - `"system"` — shell commands, system info, processes
    /// - `"filesystem"` — file read/write/list/search
    /// - `"network"` — web fetch, search, download
    /// - `"ui"` — display, panels, notifications
    /// - `"general"` — catch-all default
    fn category(&self) -> &str {
        "general"
    }

    /// Hint for the speculative pre-fetcher about what data this tool might need.
    ///
    /// The pre-fetcher inspects the user message and, for tools that are likely
    /// to be called, uses this hint to start fetching data in parallel with
    /// the LLM call.  Return `None` (the default) if no pre-fetch is useful.
    ///
    /// The `args` parameter contains the partial/predicted arguments inferred
    /// from the user message.
    fn prefetch_hint(&self, _args: &serde_json::Value) -> Option<String> {
        None
    }

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
