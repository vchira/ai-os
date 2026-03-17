//! Tool registry — stores and manages all available tools.
//!
//! The [`ToolRegistry`] owns every registered tool and provides lookup by name,
//! listing, and schema export.  Call [`load_builtins`](ToolRegistry::load_builtins)
//! to populate the registry with the built-in tools shipped with AiOS.

use std::collections::HashMap;

use aios_core::channel::ChannelContext;
use aios_core::types::{ToolResult, ToolSchema};
use tracing::info;

use crate::builtin;
use crate::error::ToolError;
use crate::tool::Tool;

/// Central registry of all available tools.
///
/// Tools are stored as boxed trait objects keyed by their unique name.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool.
    ///
    /// Returns an error if a tool with the same name is already registered.
    pub fn register(&mut self, tool: Box<dyn Tool>) -> Result<(), ToolError> {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            return Err(ToolError::AlreadyRegistered(name));
        }
        info!(tool = %name, "registered tool");
        self.tools.insert(name, tool);
        Ok(())
    }

    /// Remove a tool by name.
    ///
    /// Returns an error if no tool with that name exists.
    pub fn unregister(&mut self, name: &str) -> Result<(), ToolError> {
        if self.tools.remove(name).is_none() {
            return Err(ToolError::NotFound(name.to_string()));
        }
        info!(tool = %name, "unregistered tool");
        Ok(())
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Execute a tool by name with the given arguments.
    pub fn execute(&self, name: &str, args: serde_json::Value) -> ToolResult {
        match self.get(name) {
            Some(tool) => tool.execute(args),
            None => ToolResult::fail(format!("Tool not found: {name}")),
        }
    }

    /// Execute a tool by name with channel awareness.
    ///
    /// Channel-aware tools (e.g. `ui_panel`, `display`) adapt their
    /// behaviour based on the active channel.  All other tools ignore
    /// the channel and behave identically to [`execute`](Self::execute).
    pub fn execute_on_channel(
        &self,
        name: &str,
        args: serde_json::Value,
        channel: &ChannelContext,
    ) -> ToolResult {
        match self.get(name) {
            Some(tool) => tool.execute_on_channel(args, channel),
            None => ToolResult::fail(format!("Tool not found: {name}")),
        }
    }

    /// List all registered tool names (sorted alphabetically).
    pub fn list_tools(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }

    /// Return schemas for all registered tools (sorted by name).
    ///
    /// The returned schemas are suitable for passing to LLM function-calling
    /// APIs.
    pub fn get_schemas(&self) -> Vec<ToolSchema> {
        let mut schemas: Vec<ToolSchema> = self.tools.values().map(|t| t.to_schema()).collect();
        schemas.sort_by(|a, b| a.name.cmp(&b.name));
        schemas
    }

    /// Get all tools belonging to a given category.
    pub fn get_tools_by_category(&self, category: &str) -> Vec<&dyn Tool> {
        self.tools
            .values()
            .filter(|t| t.category() == category)
            .map(|t| t.as_ref())
            .collect()
    }

    /// Return tool schemas for tools belonging to any of the given categories.
    ///
    /// Results are sorted by tool name.
    pub fn get_schemas_by_categories(&self, categories: &[&str]) -> Vec<ToolSchema> {
        let mut schemas: Vec<ToolSchema> = self
            .tools
            .values()
            .filter(|t| categories.contains(&t.category()))
            .map(|t| t.to_schema())
            .collect();
        schemas.sort_by(|a, b| a.name.cmp(&b.name));
        schemas
    }

    /// List all unique categories across registered tools (sorted).
    pub fn categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self
            .tools
            .values()
            .map(|t| t.category().to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        cats.sort();
        cats
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Populate the registry with all built-in tools.
    ///
    /// This registers: `memory`, `system`, `files`, `web`, `display`,
    /// `ui_panel`, `delegate_to`, `reflect`, `recall_episodes`,
    /// `execute_code`, `process_data`, `find_content`.
    pub fn load_builtins(&mut self) {
        let builtins: Vec<Box<dyn Tool>> = vec![
            Box::new(builtin::MemoryTool::new(None)),
            Box::new(builtin::SystemTool),
            Box::new(builtin::FilesTool),
            Box::new(builtin::WebTool::new()),
            Box::new(builtin::DisplayTool::new()),
            Box::new(builtin::UiPanelTool::new()),
            Box::new(builtin::DelegateTool::new()),
            Box::new(builtin::ReflectTool::new()),
            Box::new(builtin::RecallEpisodesTool::new()),
            Box::new(builtin::CodeExecTool),
            Box::new(builtin::DataProcessTool),
            Box::new(builtin::FindContentTool::with_default_index()),
        ];

        for tool in builtins {
            let name = tool.name().to_string();
            if let Err(e) = self.register(tool) {
                tracing::warn!(tool = %name, error = %e, "failed to register built-in tool");
            }
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial tool for testing.
    struct DummyTool;

    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "A dummy tool for tests."
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({ "type": "object", "properties": {} })
        }
        fn execute(&self, _args: serde_json::Value) -> ToolResult {
            ToolResult::ok("dummy ok")
        }
    }

    #[test]
    fn register_and_get() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool)).unwrap();
        assert!(reg.get("dummy").is_some());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn duplicate_registration_fails() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool)).unwrap();
        let err = reg.register(Box::new(DummyTool)).unwrap_err();
        assert!(matches!(err, ToolError::AlreadyRegistered(_)));
    }

    #[test]
    fn unregister_removes_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool)).unwrap();
        reg.unregister("dummy").unwrap();
        assert!(reg.get("dummy").is_none());
        assert!(reg.is_empty());
    }

    #[test]
    fn unregister_missing_fails() {
        let mut reg = ToolRegistry::new();
        let err = reg.unregister("nonexistent").unwrap_err();
        assert!(matches!(err, ToolError::NotFound(_)));
    }

    #[test]
    fn list_tools_sorted() {
        let mut reg = ToolRegistry::new();
        reg.load_builtins();
        let names = reg.list_tools();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        assert!(names.contains(&"memory".to_string()));
        assert!(names.contains(&"system".to_string()));
    }

    #[test]
    fn get_schemas_returns_all() {
        let mut reg = ToolRegistry::new();
        reg.load_builtins();
        let schemas = reg.get_schemas();
        assert_eq!(schemas.len(), reg.len());
    }

    #[test]
    fn execute_missing_tool() {
        let reg = ToolRegistry::new();
        let result = reg.execute("nope", serde_json::json!({}));
        assert!(!result.success);
    }

    #[test]
    fn categories_lists_unique_sorted() {
        let mut reg = ToolRegistry::new();
        reg.load_builtins();
        let cats = reg.categories();
        // Should contain at least memory, system, filesystem, network, ui
        assert!(cats.contains(&"memory".to_string()));
        assert!(cats.contains(&"system".to_string()));
        assert!(cats.contains(&"filesystem".to_string()));
        assert!(cats.contains(&"network".to_string()));
        assert!(cats.contains(&"ui".to_string()));
        // Should be sorted
        let mut sorted = cats.clone();
        sorted.sort();
        assert_eq!(cats, sorted);
    }

    #[test]
    fn get_tools_by_category() {
        let mut reg = ToolRegistry::new();
        reg.load_builtins();
        let ui_tools = reg.get_tools_by_category("ui");
        assert_eq!(ui_tools.len(), 2);
        let names: Vec<&str> = ui_tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"display"));
        assert!(names.contains(&"ui_panel"));
    }

    #[test]
    fn get_schemas_by_categories() {
        let mut reg = ToolRegistry::new();
        reg.load_builtins();
        let schemas = reg.get_schemas_by_categories(&["memory", "network"]);
        // memory category: memory, recall_episodes, reflect (3) + network: web (1) = 4
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"memory"));
        assert!(names.contains(&"recall_episodes"));
        assert!(names.contains(&"reflect"));
        assert!(names.contains(&"web"));
        assert_eq!(schemas.len(), 4);
    }

    #[test]
    fn get_schemas_by_categories_empty_input() {
        let mut reg = ToolRegistry::new();
        reg.load_builtins();
        let schemas = reg.get_schemas_by_categories(&[]);
        assert!(schemas.is_empty());
    }

    #[test]
    fn get_tools_by_nonexistent_category() {
        let mut reg = ToolRegistry::new();
        reg.load_builtins();
        let tools = reg.get_tools_by_category("nonexistent");
        assert!(tools.is_empty());
    }

    // -- Additional edge-case tests --

    /// A second dummy tool for testing multiple registrations.
    struct DummyTool2;

    impl Tool for DummyTool2 {
        fn name(&self) -> &str {
            "dummy2"
        }
        fn description(&self) -> &str {
            "Another dummy tool."
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({ "type": "object", "properties": {} })
        }
        fn execute(&self, _args: serde_json::Value) -> ToolResult {
            ToolResult::ok("dummy2 ok")
        }
        fn category(&self) -> &str {
            "test_category"
        }
    }

    #[test]
    fn execute_on_channel_with_desktop_channel_delegates_to_execute() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool)).unwrap();

        let channel = ChannelContext::desktop();
        let result = reg.execute_on_channel("dummy", serde_json::json!({}), &channel);
        assert!(result.success);
        assert_eq!(result.output, "dummy ok");
    }

    #[test]
    fn execute_on_channel_missing_tool_returns_fail() {
        let reg = ToolRegistry::new();
        let channel = ChannelContext::desktop();
        let result = reg.execute_on_channel("nonexistent", serde_json::json!({}), &channel);
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap().contains("not found"));
    }

    #[test]
    fn execute_on_channel_signal_still_works_for_non_ui_tools() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool)).unwrap();

        let channel = ChannelContext::signal();
        let result = reg.execute_on_channel("dummy", serde_json::json!({}), &channel);
        // DummyTool doesn't override execute_on_channel, so it delegates to execute.
        assert!(result.success);
        assert_eq!(result.output, "dummy ok");
    }

    #[test]
    fn execute_on_channel_voice() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool)).unwrap();

        let channel = ChannelContext::voice();
        let result = reg.execute_on_channel("dummy", serde_json::json!({}), &channel);
        assert!(result.success);
    }

    #[test]
    fn get_schemas_by_custom_category() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool2)).unwrap();

        let schemas = reg.get_schemas_by_categories(&["test_category"]);
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "dummy2");
    }

    #[test]
    fn categories_include_custom() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool2)).unwrap();

        let cats = reg.categories();
        assert!(cats.contains(&"test_category".to_string()));
    }

    #[test]
    fn execute_existing_tool_returns_success() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(DummyTool)).unwrap();
        let result = reg.execute("dummy", serde_json::json!({}));
        assert!(result.success);
        assert_eq!(result.output, "dummy ok");
    }

    #[test]
    fn default_registry_is_empty() {
        let reg = ToolRegistry::default();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn load_builtins_registers_expected_count() {
        let mut reg = ToolRegistry::new();
        reg.load_builtins();
        // Should have 12 built-in tools.
        assert_eq!(reg.len(), 12);
    }
}
