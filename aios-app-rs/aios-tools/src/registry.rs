//! Tool registry — stores and manages all available tools.
//!
//! The [`ToolRegistry`] owns every registered tool and provides lookup by name,
//! listing, and schema export.  Call [`load_builtins`](ToolRegistry::load_builtins)
//! to populate the registry with the built-in tools shipped with AiOS.

use std::collections::HashMap;

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
    /// This registers: `memory`, `system`, `files`, `web`, `display`.
    pub fn load_builtins(&mut self) {
        let builtins: Vec<Box<dyn Tool>> = vec![
            Box::new(builtin::MemoryTool::new(None)),
            Box::new(builtin::SystemTool),
            Box::new(builtin::FilesTool),
            Box::new(builtin::WebTool::new()),
            Box::new(builtin::DisplayTool::new()),
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
}
