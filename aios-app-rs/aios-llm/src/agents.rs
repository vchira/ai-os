//! Multi-agent orchestration — specialized sub-agents for complex tasks.
//!
//! The "kernel" AI can spawn specialized sub-agents for complex tasks using the
//! `delegate_to` tool.  Each sub-agent gets its own conversation context and a
//! tool subset tailored to its specialization.
//!
//! # Agent types
//!
//! | Type | Focus | Tools |
//! |------|-------|-------|
//! | [`Coder`](AgentType::Coder) | Code analysis, refactoring, debugging | filesystem |
//! | [`Researcher`](AgentType::Researcher) | Web search, information gathering | network, memory |
//! | [`SysAdmin`](AgentType::SysAdmin) | System commands, processes | system |
//! | [`FileManager`](AgentType::FileManager) | File operations, organization | filesystem |
//! | [`Analyst`](AgentType::Analyst) | Data analysis, CSV processing | filesystem, network |
//! | [`General`](AgentType::General) | Default, all tools | all |
//!
//! # Architecture
//!
//! The [`AgentOrchestrator`] manages sub-agent execution.  It constructs a
//! specialized system prompt and selects the appropriate tool categories for
//! each agent type.  The actual LLM call is performed by the provider passed
//! to [`delegate`](AgentOrchestrator::delegate).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AgentType
// ---------------------------------------------------------------------------

/// Specialization of a sub-agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// Code analysis, refactoring, debugging.
    Coder,
    /// Web search, information gathering.
    Researcher,
    /// System commands, process management.
    SysAdmin,
    /// File operations, organization.
    FileManager,
    /// Data analysis, CSV processing.
    Analyst,
    /// Default — all tools available.
    General,
}

impl AgentType {
    /// Parse an agent type from a string (case-insensitive).
    ///
    /// Returns `None` for unrecognised strings.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "coder" => Some(Self::Coder),
            "researcher" => Some(Self::Researcher),
            "sysadmin" => Some(Self::SysAdmin),
            "file_manager" => Some(Self::FileManager),
            "analyst" => Some(Self::Analyst),
            "general" => Some(Self::General),
            _ => None,
        }
    }

    /// Return the string label for this agent type.
    pub fn label(&self) -> &str {
        match self {
            Self::Coder => "coder",
            Self::Researcher => "researcher",
            Self::SysAdmin => "sysadmin",
            Self::FileManager => "file_manager",
            Self::Analyst => "analyst",
            Self::General => "general",
        }
    }
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// AgentTask
// ---------------------------------------------------------------------------

/// A sub-agent task with its own context.
#[derive(Debug, Clone)]
pub struct AgentTask {
    /// The type of sub-agent.
    pub agent_type: AgentType,
    /// Detailed description of what the sub-agent should do.
    pub task_description: String,
    /// Summary of the parent conversation for context.
    pub parent_context: String,
    /// The result returned by the sub-agent, if completed.
    pub result: Option<String>,
}

// ---------------------------------------------------------------------------
// AgentOrchestrator
// ---------------------------------------------------------------------------

/// Manages sub-agent execution.
///
/// The orchestrator constructs specialized system prompts, selects tool
/// categories, and tracks active tasks.  The actual LLM call is performed
/// externally — the orchestrator provides the configuration but does not
/// own the provider.
pub struct AgentOrchestrator {
    /// Maximum number of concurrent sub-agent tasks.
    max_concurrent: usize,
    /// Currently active tasks.
    active_tasks: Vec<AgentTask>,
}

impl AgentOrchestrator {
    /// Create a new orchestrator with the given concurrency limit.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            active_tasks: Vec::new(),
        }
    }

    /// Return the maximum concurrent task limit.
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    /// Return the number of currently active tasks.
    pub fn active_count(&self) -> usize {
        self.active_tasks.len()
    }

    /// Whether the orchestrator can accept another task.
    pub fn can_accept(&self) -> bool {
        self.active_tasks.len() < self.max_concurrent
    }

    /// Get the specialized system prompt for an agent type.
    ///
    /// Each agent type gets a tailored prompt that focuses its behaviour on
    /// the relevant domain.
    pub fn get_system_prompt(agent_type: &AgentType, task: &str, parent_context: &str) -> String {
        let role_prompt = match agent_type {
            AgentType::Coder => {
                "You are a coding specialist sub-agent within AiOS. Your focus is on \
                 code analysis, writing, refactoring, and debugging. You have access to \
                 filesystem tools to read, write, and search code files. Be precise, \
                 follow best practices, and explain your changes clearly. When fixing \
                 bugs, identify the root cause before applying a fix."
            }
            AgentType::Researcher => {
                "You are a research specialist sub-agent within AiOS. Your focus is on \
                 web search, information gathering, and knowledge synthesis. You have \
                 access to network tools for web searches and memory tools for storing \
                 findings. Be thorough in your research, cite sources when possible, \
                 and summarize findings clearly."
            }
            AgentType::SysAdmin => {
                "You are a system administrator sub-agent within AiOS. Your focus is on \
                 system commands, process management, service configuration, and \
                 troubleshooting. You have access to system tools for running commands \
                 and inspecting processes. Be careful with destructive operations, \
                 always explain what commands do before running them, and prefer safe \
                 approaches."
            }
            AgentType::FileManager => {
                "You are a file management specialist sub-agent within AiOS. Your focus \
                 is on file operations, organization, backup, and cleanup. You have \
                 access to filesystem tools for reading, writing, listing, and searching \
                 files. Be careful with delete operations, confirm before making \
                 destructive changes, and keep file structures organized."
            }
            AgentType::Analyst => {
                "You are a data analyst sub-agent within AiOS. Your focus is on data \
                 analysis, CSV processing, statistics, and generating insights. You \
                 have access to filesystem tools for reading data files and network \
                 tools for fetching data from URLs. Present your analysis clearly with \
                 key findings, and note any limitations in the data."
            }
            AgentType::General => {
                "You are a general-purpose sub-agent within AiOS. You have access to \
                 all available tools and can handle a wide variety of tasks. Focus on \
                 the specific task assigned to you and complete it thoroughly."
            }
        };

        let mut prompt = format!(
            "{role_prompt}\n\n\
             You are working on a delegated task from the main AiOS agent.\n\n\
             Task: {task}"
        );

        if !parent_context.is_empty() {
            prompt.push_str(&format!("\n\nContext from parent conversation:\n{parent_context}"));
        }

        prompt.push_str(
            "\n\nComplete the task and provide a clear, concise result. \
             Do not ask for clarification — work with the information provided."
        );

        prompt
    }

    /// Get the tool categories appropriate for an agent type.
    ///
    /// Returns a list of category names suitable for passing to
    /// [`ToolRegistry::get_schemas_by_categories`].
    pub fn get_tool_categories(agent_type: &AgentType) -> Vec<&'static str> {
        match agent_type {
            AgentType::Coder => vec!["filesystem"],
            AgentType::Researcher => vec!["network", "memory"],
            AgentType::SysAdmin => vec!["system"],
            AgentType::FileManager => vec!["filesystem"],
            AgentType::Analyst => vec!["filesystem", "network"],
            AgentType::General => vec![
                "filesystem", "memory", "system", "network", "ui",
            ],
        }
    }

    /// Register a task as active.
    ///
    /// Returns `Err` if the concurrency limit has been reached.
    pub fn start_task(
        &mut self,
        agent_type: AgentType,
        task: String,
        parent_context: String,
    ) -> Result<usize, String> {
        if !self.can_accept() {
            return Err(format!(
                "Maximum concurrent tasks ({}) reached.",
                self.max_concurrent,
            ));
        }

        let task = AgentTask {
            agent_type,
            task_description: task,
            parent_context,
            result: None,
        };

        self.active_tasks.push(task);
        Ok(self.active_tasks.len() - 1)
    }

    /// Mark a task as completed with a result.
    pub fn complete_task(&mut self, index: usize, result: String) -> Result<(), String> {
        if index >= self.active_tasks.len() {
            return Err(format!("Task index {index} out of range."));
        }
        self.active_tasks[index].result = Some(result);
        Ok(())
    }

    /// Remove a completed task and return it.
    pub fn remove_task(&mut self, index: usize) -> Result<AgentTask, String> {
        if index >= self.active_tasks.len() {
            return Err(format!("Task index {index} out of range."));
        }
        Ok(self.active_tasks.remove(index))
    }

    /// Return a read-only view of active tasks.
    pub fn active_tasks(&self) -> &[AgentTask] {
        &self.active_tasks
    }
}

impl Default for AgentOrchestrator {
    fn default() -> Self {
        Self::new(3)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- AgentType tests --------------------------------------------------

    #[test]
    fn agent_type_from_str_opt() {
        assert_eq!(AgentType::from_str_opt("coder"), Some(AgentType::Coder));
        assert_eq!(AgentType::from_str_opt("RESEARCHER"), Some(AgentType::Researcher));
        assert_eq!(AgentType::from_str_opt("sysadmin"), Some(AgentType::SysAdmin));
        assert_eq!(AgentType::from_str_opt("file_manager"), Some(AgentType::FileManager));
        assert_eq!(AgentType::from_str_opt("Analyst"), Some(AgentType::Analyst));
        assert_eq!(AgentType::from_str_opt("General"), Some(AgentType::General));
        assert_eq!(AgentType::from_str_opt("wizard"), None);
    }

    #[test]
    fn agent_type_label() {
        assert_eq!(AgentType::Coder.label(), "coder");
        assert_eq!(AgentType::Researcher.label(), "researcher");
        assert_eq!(AgentType::SysAdmin.label(), "sysadmin");
        assert_eq!(AgentType::FileManager.label(), "file_manager");
        assert_eq!(AgentType::Analyst.label(), "analyst");
        assert_eq!(AgentType::General.label(), "general");
    }

    #[test]
    fn agent_type_display() {
        assert_eq!(format!("{}", AgentType::Coder), "coder");
        assert_eq!(format!("{}", AgentType::General), "general");
    }

    #[test]
    fn agent_type_roundtrips_json() {
        for agent in &[
            AgentType::Coder,
            AgentType::Researcher,
            AgentType::SysAdmin,
            AgentType::FileManager,
            AgentType::Analyst,
            AgentType::General,
        ] {
            let json = serde_json::to_string(agent).unwrap();
            let back: AgentType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, agent);
        }
    }

    // -- Tool category mapping tests --------------------------------------

    #[test]
    fn coder_gets_filesystem() {
        let cats = AgentOrchestrator::get_tool_categories(&AgentType::Coder);
        assert_eq!(cats, vec!["filesystem"]);
    }

    #[test]
    fn researcher_gets_network_and_memory() {
        let cats = AgentOrchestrator::get_tool_categories(&AgentType::Researcher);
        assert!(cats.contains(&"network"));
        assert!(cats.contains(&"memory"));
        assert_eq!(cats.len(), 2);
    }

    #[test]
    fn sysadmin_gets_system() {
        let cats = AgentOrchestrator::get_tool_categories(&AgentType::SysAdmin);
        assert_eq!(cats, vec!["system"]);
    }

    #[test]
    fn file_manager_gets_filesystem() {
        let cats = AgentOrchestrator::get_tool_categories(&AgentType::FileManager);
        assert_eq!(cats, vec!["filesystem"]);
    }

    #[test]
    fn analyst_gets_filesystem_and_network() {
        let cats = AgentOrchestrator::get_tool_categories(&AgentType::Analyst);
        assert!(cats.contains(&"filesystem"));
        assert!(cats.contains(&"network"));
        assert_eq!(cats.len(), 2);
    }

    #[test]
    fn general_gets_all_categories() {
        let cats = AgentOrchestrator::get_tool_categories(&AgentType::General);
        assert!(cats.contains(&"filesystem"));
        assert!(cats.contains(&"memory"));
        assert!(cats.contains(&"system"));
        assert!(cats.contains(&"network"));
        assert!(cats.contains(&"ui"));
        assert_eq!(cats.len(), 5);
    }

    // -- System prompt tests ----------------------------------------------

    #[test]
    fn system_prompt_contains_role() {
        let prompt = AgentOrchestrator::get_system_prompt(
            &AgentType::Coder,
            "Fix the bug",
            "",
        );
        assert!(prompt.contains("coding specialist"));
        assert!(prompt.contains("Fix the bug"));
    }

    #[test]
    fn system_prompt_includes_context() {
        let prompt = AgentOrchestrator::get_system_prompt(
            &AgentType::Researcher,
            "Find info",
            "User is working on a project",
        );
        assert!(prompt.contains("research specialist"));
        assert!(prompt.contains("Find info"));
        assert!(prompt.contains("User is working on a project"));
    }

    #[test]
    fn system_prompt_omits_empty_context() {
        let prompt = AgentOrchestrator::get_system_prompt(
            &AgentType::SysAdmin,
            "Check disk space",
            "",
        );
        assert!(prompt.contains("system administrator"));
        assert!(prompt.contains("Check disk space"));
        assert!(!prompt.contains("Context from parent"));
    }

    #[test]
    fn each_agent_type_has_unique_prompt() {
        let types = [
            AgentType::Coder,
            AgentType::Researcher,
            AgentType::SysAdmin,
            AgentType::FileManager,
            AgentType::Analyst,
            AgentType::General,
        ];

        let prompts: Vec<String> = types
            .iter()
            .map(|t| AgentOrchestrator::get_system_prompt(t, "task", ""))
            .collect();

        // Every prompt should be unique.
        for i in 0..prompts.len() {
            for j in (i + 1)..prompts.len() {
                assert_ne!(
                    prompts[i], prompts[j],
                    "prompts for {:?} and {:?} should differ",
                    types[i], types[j],
                );
            }
        }
    }

    // -- Orchestrator task management tests --------------------------------

    #[test]
    fn new_orchestrator_is_empty() {
        let orch = AgentOrchestrator::new(3);
        assert_eq!(orch.active_count(), 0);
        assert_eq!(orch.max_concurrent(), 3);
        assert!(orch.can_accept());
    }

    #[test]
    fn start_task_increments_count() {
        let mut orch = AgentOrchestrator::new(3);
        let idx = orch.start_task(
            AgentType::Coder,
            "fix bug".into(),
            "context".into(),
        ).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(orch.active_count(), 1);
    }

    #[test]
    fn concurrency_limit_enforced() {
        let mut orch = AgentOrchestrator::new(2);
        orch.start_task(AgentType::Coder, "task 1".into(), "".into()).unwrap();
        orch.start_task(AgentType::Researcher, "task 2".into(), "".into()).unwrap();
        assert!(!orch.can_accept());

        let err = orch.start_task(AgentType::General, "task 3".into(), "".into());
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("Maximum concurrent"));
    }

    #[test]
    fn complete_and_remove_task() {
        let mut orch = AgentOrchestrator::new(3);
        let idx = orch.start_task(
            AgentType::Analyst,
            "analyze data".into(),
            "".into(),
        ).unwrap();

        orch.complete_task(idx, "analysis complete".into()).unwrap();
        assert!(orch.active_tasks()[idx].result.is_some());

        let task = orch.remove_task(idx).unwrap();
        assert_eq!(task.result.as_deref(), Some("analysis complete"));
        assert_eq!(orch.active_count(), 0);
    }

    #[test]
    fn complete_task_invalid_index() {
        let mut orch = AgentOrchestrator::new(3);
        assert!(orch.complete_task(99, "result".into()).is_err());
    }

    #[test]
    fn remove_task_invalid_index() {
        let mut orch = AgentOrchestrator::new(3);
        assert!(orch.remove_task(99).is_err());
    }

    #[test]
    fn default_orchestrator() {
        let orch = AgentOrchestrator::default();
        assert_eq!(orch.max_concurrent(), 3);
    }
}
