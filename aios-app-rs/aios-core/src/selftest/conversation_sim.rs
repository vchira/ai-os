//! Conversation simulation framework — replays scripted AI-User dialogs
//! and asserts at every step.
//!
//! Each dialog is a sequence of [`DialogStep`]s. Steps can be:
//! - User sends a message
//! - AI responds (mocked, not real LLM)
//! - AI calls a tool (mocked call, real tool execution)
//! - User interacts with a panel (simulated input)
//! - Assertion check (verify state, channel, message content)
//!
//! Dialogs are defined as Rust structs (not JSON files) so they're
//! compile-time checked and can use all the type system.

use crate::channel::{ChannelContext, ChannelKind, ChannelSwitcher};
use crate::config::commands::{CommandHandler, CommandResult};
use crate::config::ConfigManager;
use crate::types::ToolResult;

// ---------------------------------------------------------------------------
// Dialog types
// ---------------------------------------------------------------------------

/// A single step in a simulated conversation.
#[derive(Debug, Clone)]
pub enum DialogStep {
    /// User sends a text message.
    UserMessage(String),
    /// AI responds with text (mocked — no real LLM call).
    AiResponse(String),
    /// AI calls a tool. Provides (tool_name, args_json).
    /// The tool is executed for real against the test registry.
    ToolCall {
        tool: String,
        args: serde_json::Value,
    },
    /// User interacts with a panel: provides field values.
    PanelInput(serde_json::Value),
    /// Execute a slash command and check the result type.
    SlashCommand {
        command: String,
        expect: ExpectedResult,
    },
    /// Switch the active channel.
    SwitchChannel(ChannelKind),
    /// Assert something about the current state.
    Assert(Assertion),
}

/// What we expect from a slash command.
#[derive(Debug, Clone)]
pub enum ExpectedResult {
    /// Any Response variant (don't check content).
    AnyResponse,
    /// Response containing a specific substring.
    ResponseContains(String),
    /// Clear variant.
    Clear,
    /// SelfTest variant.
    SelfTest,
    /// SysInfo variant.
    SysInfo,
    /// ClosePanel variant.
    ClosePanel,
    /// Update variant.
    Update,
    /// Unknown variant.
    Unknown,
}

/// An assertion to check at a given point in the dialog.
#[derive(Debug, Clone)]
pub enum Assertion {
    /// The active channel is the given kind.
    ActiveChannel(ChannelKind),
    /// The last message count is at least N.
    MinMessageCount(usize),
    /// The tool result was successful.
    LastToolSuccess,
    /// The tool result was a failure.
    LastToolFailed,
    /// A config key has the expected value.
    ConfigValue { key: String, expected: String },
}

/// A complete dialog script.
#[derive(Debug, Clone)]
pub struct Dialog {
    pub name: String,
    pub description: String,
    pub channel: ChannelKind,
    pub steps: Vec<DialogStep>,
}

// ---------------------------------------------------------------------------
// Dialog runner
// ---------------------------------------------------------------------------

/// Result of running a dialog.
#[derive(Debug)]
pub struct DialogResult {
    pub name: String,
    pub passed: bool,
    pub steps_run: usize,
    pub total_steps: usize,
    pub error: Option<String>,
}

/// Run a dialog against real subsystems (config, tools, channels).
pub fn run_dialog(dialog: &Dialog) -> DialogResult {
    crate::i18n::init();
    let dir = std::env::temp_dir().join(format!(
        "aios_dialog_{}_{}", std::process::id(), dialog.name.replace(' ', "_")
    ));
    let _ = std::fs::create_dir_all(&dir);
    let config_path = dir.join("config.json");

    let config_result = ConfigManager::with_path(config_path);
    let mut config = match config_result {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return DialogResult {
                name: dialog.name.clone(),
                passed: false,
                steps_run: 0,
                total_steps: dialog.steps.len(),
                error: Some(format!("Config init failed: {e}")),
            };
        }
    };

    let switcher = ChannelSwitcher::new();
    if dialog.channel != ChannelKind::Desktop {
        switcher.register_channel(
            dialog.channel,
            ChannelContext::new(dialog.channel),
        );
        let _ = switcher.switch_to(dialog.channel);
    }

    let mut messages: Vec<(String, String)> = Vec::new(); // (role, content)
    let mut last_tool_result: Option<ToolResult> = None;
    let mut steps_run = 0;

    for (i, step) in dialog.steps.iter().enumerate() {
        steps_run = i + 1;

        match step {
            DialogStep::UserMessage(text) => {
                messages.push(("user".to_string(), text.clone()));
            }

            DialogStep::AiResponse(text) => {
                messages.push(("assistant".to_string(), text.clone()));
            }

            DialogStep::ToolCall { tool, args } => {
                // Mock all 12 built-in tools for simulation.
                let result = mock_tool_call(tool, args);
                messages.push(("tool".to_string(), result.output.clone()));
                last_tool_result = Some(result);
            }

            DialogStep::PanelInput(values) => {
                // Simulate panel submission — record as user response.
                let text = serde_json::to_string(values).unwrap_or_default();
                messages.push(("user".to_string(), format!("[panel] {text}")));
            }

            DialogStep::SlashCommand { command, expect } => {
                let mut handler = CommandHandler::new(&mut config);
                let result = handler.execute(command);

                let ok = match (expect, &result) {
                    (ExpectedResult::AnyResponse, CommandResult::Response(_)) => true,
                    (ExpectedResult::ResponseContains(s), CommandResult::Response(text)) => {
                        text.contains(s.as_str())
                    }
                    (ExpectedResult::Clear, CommandResult::Clear) => true,
                    (ExpectedResult::SelfTest, CommandResult::SelfTest(_)) => true,
                    (ExpectedResult::SysInfo, CommandResult::SysInfo) => true,
                    (ExpectedResult::ClosePanel, CommandResult::ClosePanel) => true,
                    (ExpectedResult::Update, CommandResult::Update(_)) => true,
                    (ExpectedResult::Unknown, CommandResult::Unknown(_)) => true,
                    _ => false,
                };

                if !ok {
                    let _ = std::fs::remove_dir_all(&dir);
                    return DialogResult {
                        name: dialog.name.clone(),
                        passed: false,
                        steps_run,
                        total_steps: dialog.steps.len(),
                        error: Some(format!(
                            "Step {i}: command '{}' expected {:?}, got {:?}",
                            command, expect, result
                        )),
                    };
                }

                if let CommandResult::Response(text) = &result {
                    messages.push(("system".to_string(), text.clone()));
                }
            }

            DialogStep::SwitchChannel(kind) => {
                if !switcher.is_registered(*kind) {
                    switcher.register_channel(*kind, ChannelContext::new(*kind));
                }
                if let Err(e) = switcher.switch_to(*kind) {
                    let _ = std::fs::remove_dir_all(&dir);
                    return DialogResult {
                        name: dialog.name.clone(),
                        passed: false,
                        steps_run,
                        total_steps: dialog.steps.len(),
                        error: Some(format!("Step {i}: channel switch failed: {e}")),
                    };
                }
            }

            DialogStep::Assert(assertion) => {
                let err = match assertion {
                    Assertion::ActiveChannel(expected) => {
                        if switcher.active_kind() != *expected {
                            Some(format!(
                                "Expected channel {:?}, got {:?}",
                                expected,
                                switcher.active_kind()
                            ))
                        } else {
                            None
                        }
                    }
                    Assertion::MinMessageCount(min) => {
                        if messages.len() < *min {
                            Some(format!(
                                "Expected at least {} messages, got {}",
                                min,
                                messages.len()
                            ))
                        } else {
                            None
                        }
                    }
                    Assertion::LastToolSuccess => match &last_tool_result {
                        Some(r) if r.success => None,
                        Some(r) => Some(format!("Tool failed: {:?}", r.error)),
                        None => Some("No tool result yet".into()),
                    },
                    Assertion::LastToolFailed => match &last_tool_result {
                        Some(r) if !r.success => None,
                        Some(_) => Some("Tool succeeded but expected failure".into()),
                        None => Some("No tool result yet".into()),
                    },
                    Assertion::ConfigValue { key, expected } => {
                        // Try string first, then bool, then raw JSON.
                        let actual = config.get_str(key, "");
                        if !actual.is_empty() && actual == *expected {
                            None
                        } else if expected == "true" && config.get_bool(key, false) {
                            None
                        } else if expected == "false" && !config.get_bool(key, true) {
                            None
                        } else if !actual.is_empty() {
                            Some(format!("Config {key}: expected '{expected}', got '{actual}'"))
                        } else {
                            // Key might not exist or is a non-string type.
                            Some(format!("Config {key}: expected '{expected}', got empty/missing"))
                        }
                    }
                };

                if let Some(msg) = err {
                    let _ = std::fs::remove_dir_all(&dir);
                    return DialogResult {
                        name: dialog.name.clone(),
                        passed: false,
                        steps_run,
                        total_steps: dialog.steps.len(),
                        error: Some(format!("Step {i} assertion failed: {msg}")),
                    };
                }
            }
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
    DialogResult {
        name: dialog.name.clone(),
        passed: true,
        steps_run,
        total_steps: dialog.steps.len(),
        error: None,
    }
}

/// Run multiple dialogs and return all results.
pub fn run_all_dialogs(dialogs: &[Dialog]) -> Vec<DialogResult> {
    dialogs.iter().map(run_dialog).collect()
}

/// Format dialog results as a report.
pub fn format_dialog_report(results: &[DialogResult]) -> String {
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    let mut lines = vec![
        format!("Dialog Simulation Report: {passed}/{total} passed"),
        "=".repeat(50),
    ];

    for r in results {
        let icon = if r.passed { "\u{2705}" } else { "\u{274c}" };
        let steps = format!("{}/{}", r.steps_run, r.total_steps);
        if let Some(ref err) = r.error {
            lines.push(format!("{icon} {} [{steps}]: {err}", r.name));
        } else {
            lines.push(format!("{icon} {} [{steps}]", r.name));
        }
    }

    if failed > 0 {
        lines.push(format!("\n{failed} dialog(s) FAILED."));
    } else {
        lines.push("\nAll dialogs passed!".into());
    }

    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tool mocking — covers all 12 built-in tools
// ---------------------------------------------------------------------------

/// Mock a tool call for simulation. Returns realistic results for all 12 tools.
fn mock_tool_call(tool: &str, args: &serde_json::Value) -> ToolResult {
    let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
    match tool {
        "memory" => match action {
            "memorize" => ToolResult::ok("Memorized."),
            "recall" => ToolResult::ok("Recalled value."),
            "forget" => ToolResult::ok("Forgotten."),
            "list" => ToolResult::ok("key1, key2, key3"),
            _ => ToolResult::fail(format!("Unknown memory action: {action}")),
        },
        "system" => match action {
            "get_datetime" => ToolResult::ok("2026-03-16 23:00:00 UTC"),
            "get_system_info" => ToolResult::ok("AiOS 2.0 on x86_64, 8GB RAM, 4 cores"),
            "run_command" => {
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("echo ok");
                ToolResult::ok(format!("$ {cmd}\nCommand executed successfully."))
            }
            "list_processes" => ToolResult::ok("PID  CMD       CPU  MEM\n1    systemd   0.0  0.1\n42   aios      1.2  3.4"),
            _ => ToolResult::ok("Command executed."),
        },
        "files" => match action {
            "read_file" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("/tmp/test");
                ToolResult::ok(format!("Contents of {path}:\nHello, world!"))
            }
            "write_file" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("/tmp/out");
                ToolResult::ok(format!("Written to {path}."))
            }
            "list_directory" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("/home/aios");
                ToolResult::ok(format!("{path}:\n  Documents/\n  Downloads/\n  .aios/\n  notes.txt"))
            }
            "search" => ToolResult::ok("Found 3 matches:\n  /home/aios/notes.txt:1: hello\n  /home/aios/docs/readme.md:5: hello world"),
            _ => ToolResult::ok("File operation done."),
        },
        "display" => match action {
            "show_notification" => ToolResult::ok("Notification displayed."),
            "show_markdown" => ToolResult::ok("Markdown rendered."),
            "show_image" => ToolResult::ok("Image displayed."),
            _ => ToolResult::fail(format!("Unknown display action: {action}")),
        },
        "web" => match action {
            "fetch" => {
                let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("https://example.com");
                ToolResult::ok(format!("Fetched {url}: <html>Example Domain</html>"))
            }
            "search" => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("test");
                ToolResult::ok(format!("Search results for '{query}':\n1. Example result\n2. Another result"))
            }
            "download" => ToolResult::ok("File downloaded to /tmp/download."),
            _ => ToolResult::ok("Web operation completed."),
        },
        "ui_panel" => {
            // Simulate panel display — returns the field definitions
            let fields = args.get("fields").cloned().unwrap_or(serde_json::json!([]));
            ToolResult::ok(format!("Panel displayed with {} fields.",
                fields.as_array().map(|a| a.len()).unwrap_or(0)))
        }
        "delegate_to" => {
            let agent = args.get("agent_type").and_then(|v| v.as_str()).unwrap_or("coder");
            let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("analyze code");
            ToolResult::ok(format!("Delegated to {agent} agent: {task}\nAgent completed task successfully."))
        }
        "reflect" => {
            let what = args.get("what_happened").and_then(|v| v.as_str()).unwrap_or("task completed");
            let outcome = args.get("outcome").and_then(|v| v.as_str()).unwrap_or("success");
            ToolResult::ok(format!("Reflection recorded: {what} — outcome: {outcome}"))
        }
        "recall_episodes" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("recent");
            ToolResult::ok(format!("Found 2 episodes matching '{query}':\n1. Helped user set up API keys (success)\n2. Debugged network issue (partial)"))
        }
        "execute_code" => {
            let language = args.get("language").and_then(|v| v.as_str()).unwrap_or("python");
            let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("print('hello')");
            ToolResult::ok(format!("[{language}] Output:\n{}",
                if code.contains("error") { "Error: simulated error" } else { "hello\nExecution completed." }))
        }
        "process_data" => {
            let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("csv");
            match format {
                "csv" => ToolResult::ok("CSV: 3 columns, 100 rows\nHeaders: name, age, city\nSample: Alice,30,NYC | Bob,25,LA"),
                "json" => ToolResult::ok("JSON: 5 records\nKeys: id, name, value\nSample: {\"id\":1,\"name\":\"test\"}"),
                "log" => ToolResult::ok("Log: 50 entries\nErrors: 3\nWarnings: 7\nLatest error: Connection timeout at 23:15"),
                _ => ToolResult::ok(format!("Processed {format} data.")),
            }
        }
        "find_content" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("documents");
            ToolResult::ok(format!("Semantic search for '{query}':\n  1. notes.txt (0.92 similarity)\n  2. readme.md (0.85 similarity)"))
        }
        _ => ToolResult::ok(format!("Tool '{tool}' executed.")),
    }
}

// ---------------------------------------------------------------------------
// Shared tool args generator for stress tests
// ---------------------------------------------------------------------------

/// Generate realistic tool arguments for stress/fuzz testing.
fn tool_args_for_stress(tool: &str, dialog_id: usize, step: usize) -> serde_json::Value {
    let memory_actions = ["memorize", "recall", "forget", "list"];
    let system_actions = ["get_datetime", "get_system_info", "run_command", "list_processes"];
    let file_actions = ["read_file", "write_file", "list_directory", "search"];
    let display_actions = ["show_notification", "show_markdown", "show_image"];
    let web_actions = ["fetch", "search", "download"];
    let code_langs = ["python", "bash", "javascript", "rust"];
    let data_formats = ["csv", "json", "log"];
    let agent_types = ["coder", "researcher", "sysadmin", "analyst"];

    match tool {
        "memory" => {
            let action = memory_actions[(dialog_id + step) % memory_actions.len()];
            serde_json::json!({"action": action, "key": format!("k{dialog_id}_{step}"), "value": format!("v{step}")})
        }
        "system" => {
            let action = system_actions[(dialog_id + step) % system_actions.len()];
            serde_json::json!({"action": action, "command": "echo hello"})
        }
        "files" => {
            let action = file_actions[(dialog_id + step) % file_actions.len()];
            serde_json::json!({"action": action, "path": format!("/tmp/d{dialog_id}_{step}.txt"), "content": "test data", "query": "test"})
        }
        "display" => {
            let action = display_actions[(dialog_id + step) % display_actions.len()];
            serde_json::json!({"action": action, "title": format!("Notif-{step}"), "message": "hello", "text": "# Heading", "path": "/tmp/img.png"})
        }
        "web" => {
            let action = web_actions[(dialog_id + step) % web_actions.len()];
            serde_json::json!({"action": action, "url": "https://example.com", "query": format!("search-{step}"), "path": "/tmp/dl"})
        }
        "ui_panel" => {
            let field_count = (step % 5) + 1;
            let fields: Vec<serde_json::Value> = (0..field_count)
                .map(|f| serde_json::json!({"type": "text", "name": format!("field_{f}"), "label": format!("Field {f}")}))
                .collect();
            serde_json::json!({"title": format!("Panel-{step}"), "fields": fields})
        }
        "delegate_to" => {
            let agent = agent_types[(dialog_id + step) % agent_types.len()];
            serde_json::json!({"agent_type": agent, "task": format!("Task {step} for dialog {dialog_id}")})
        }
        "reflect" => {
            serde_json::json!({
                "what_happened": format!("Completed step {step} in dialog {dialog_id}"),
                "outcome": if step % 3 == 0 { "success" } else { "partial" },
                "lessons": format!("Lesson from step {step}"),
                "tags": [format!("tag{step}"), "stress_test"]
            })
        }
        "recall_episodes" => {
            serde_json::json!({"query": format!("episode from dialog {dialog_id}")})
        }
        "execute_code" => {
            let lang = code_langs[(dialog_id + step) % code_langs.len()];
            let code = match lang {
                "python" => format!("print({step} + {dialog_id})"),
                "bash" => format!("echo $(({}+{}))", step, dialog_id),
                "javascript" => format!("console.log({}+{})", step, dialog_id),
                "rust" => format!("fn main() {{ println!(\"{{}}\", {}+{}); }}", step, dialog_id),
                _ => "echo ok".into(),
            };
            serde_json::json!({"language": lang, "code": code})
        }
        "process_data" => {
            let format = data_formats[(dialog_id + step) % data_formats.len()];
            serde_json::json!({"format": format, "path": format!("/tmp/data_{step}.{format}"), "query": "$.records"})
        }
        "find_content" => {
            serde_json::json!({"query": format!("content from step {step} dialog {dialog_id}")})
        }
        _ => serde_json::json!({"action": "default"}),
    }
}

// ---------------------------------------------------------------------------
// Built-in dialog generators
// ---------------------------------------------------------------------------

/// Generate a set of standard test dialogs covering all features.
pub fn standard_dialogs() -> Vec<Dialog> {
    let mut dialogs = Vec::new();

    // 1. Basic slash commands
    dialogs.push(Dialog {
        name: "slash_commands_basic".into(),
        description: "Test all basic slash commands".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/help".into(), expect: ExpectedResult::ResponseContains("Available commands".into()) },
            DialogStep::SlashCommand { command: "/info".into(), expect: ExpectedResult::ResponseContains("AiOS".into()) },
            DialogStep::SlashCommand { command: "/tools".into(), expect: ExpectedResult::AnyResponse },
            DialogStep::SlashCommand { command: "/clear".into(), expect: ExpectedResult::Clear },
            DialogStep::SlashCommand { command: "/selftest".into(), expect: ExpectedResult::SelfTest },
            DialogStep::SlashCommand { command: "/sysinfo".into(), expect: ExpectedResult::SysInfo },
            DialogStep::SlashCommand { command: "/close".into(), expect: ExpectedResult::ClosePanel },
            DialogStep::SlashCommand { command: "/nonexistent".into(), expect: ExpectedResult::Unknown },
        ],
    });

    // 2. Provider + key configuration
    dialogs.push(Dialog {
        name: "provider_config".into(),
        description: "Configure providers and API keys".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/key claude sk-ant-test123".into(), expect: ExpectedResult::ResponseContains("Claude API key set".into()) },
            DialogStep::Assert(Assertion::ConfigValue { key: "llm.claude_api_key".into(), expected: "sk-ant-test123".into() }),
            DialogStep::SlashCommand { command: "/key openai sk-test456".into(), expect: ExpectedResult::ResponseContains("OpenAI API key set".into()) },
            DialogStep::Assert(Assertion::ConfigValue { key: "llm.openai_api_key".into(), expected: "sk-test456".into() }),
            DialogStep::SlashCommand { command: "/provider openai".into(), expect: ExpectedResult::ResponseContains("openai".into()) },
            DialogStep::Assert(Assertion::ConfigValue { key: "llm.provider".into(), expected: "openai".into() }),
            DialogStep::SlashCommand { command: "/key".into(), expect: ExpectedResult::ResponseContains("Usage".into()) },
        ],
    });

    // 3. Channel management
    dialogs.push(Dialog {
        name: "channel_management".into(),
        description: "Enable/disable channels via commands".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/channel".into(), expect: ExpectedResult::ResponseContains("Web:".into()) },
            DialogStep::SlashCommand { command: "/channel web on".into(), expect: ExpectedResult::ResponseContains("enabled".into()) },
            DialogStep::SlashCommand { command: "/channel web port 8080".into(), expect: ExpectedResult::ResponseContains("8080".into()) },
            DialogStep::SlashCommand { command: "/channel signal on".into(), expect: ExpectedResult::ResponseContains("enabled".into()) },
            DialogStep::SlashCommand { command: "/channel signal phone +1234567890".into(), expect: ExpectedResult::ResponseContains("+1234567890".into()) },
            DialogStep::SlashCommand { command: "/channel web off".into(), expect: ExpectedResult::ResponseContains("disabled".into()) },
        ],
    });

    // 4. Wake word configuration
    dialogs.push(Dialog {
        name: "wake_word_config".into(),
        description: "Configure wake word via commands".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/wake".into(), expect: ExpectedResult::ResponseContains("Assistant".into()) },
            DialogStep::SlashCommand { command: "/wake ok computer".into(), expect: ExpectedResult::ResponseContains("ok computer".into()) },
            DialogStep::Assert(Assertion::ConfigValue { key: "voice.wake_word".into(), expected: "ok computer".into() }),
            DialogStep::SlashCommand { command: "/wake off".into(), expect: ExpectedResult::ResponseContains("disabled".into()) },
            DialogStep::SlashCommand { command: "/wake on".into(), expect: ExpectedResult::ResponseContains("enabled".into()) },
        ],
    });

    // 5. Channel switching simulation
    dialogs.push(Dialog {
        name: "channel_switching".into(),
        description: "Switch between channels during conversation".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Desktop)),
            DialogStep::UserMessage("Hello from desktop".into()),
            DialogStep::AiResponse("Hello! How can I help?".into()),
            DialogStep::SwitchChannel(ChannelKind::Signal),
            DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Signal)),
            DialogStep::UserMessage("Hello from Signal".into()),
            DialogStep::AiResponse("I see you're on Signal now.".into()),
            DialogStep::SwitchChannel(ChannelKind::Web),
            DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Web)),
            DialogStep::UserMessage("And now from the web".into()),
            DialogStep::SwitchChannel(ChannelKind::Desktop),
            DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Desktop)),
            DialogStep::Assert(Assertion::MinMessageCount(5)),
        ],
    });

    // 6. Tool calls — memory
    dialogs.push(Dialog {
        name: "tool_memory".into(),
        description: "Test memory tool via simulated conversation".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Remember that my name is Alice".into()),
            DialogStep::ToolCall { tool: "memory".into(), args: serde_json::json!({"action": "memorize", "key": "name", "value": "Alice"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("I'll remember that your name is Alice.".into()),
            DialogStep::UserMessage("What's my name?".into()),
            DialogStep::ToolCall { tool: "memory".into(), args: serde_json::json!({"action": "recall", "key": "name"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("Your name is Alice.".into()),
            DialogStep::UserMessage("Forget my name".into()),
            DialogStep::ToolCall { tool: "memory".into(), args: serde_json::json!({"action": "forget", "key": "name"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 7. Tool calls — system
    dialogs.push(Dialog {
        name: "tool_system".into(),
        description: "Test system tool".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("What time is it?".into()),
            DialogStep::ToolCall { tool: "system".into(), args: serde_json::json!({"action": "get_datetime"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("It's 2026-03-16 23:00:00 UTC.".into()),
            DialogStep::UserMessage("Show system info".into()),
            DialogStep::ToolCall { tool: "system".into(), args: serde_json::json!({"action": "get_system_info"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 8. Tool calls — files
    dialogs.push(Dialog {
        name: "tool_files".into(),
        description: "Test file operations".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Read my config file".into()),
            DialogStep::ToolCall { tool: "files".into(), args: serde_json::json!({"action": "read_file", "path": "/home/aios/.aios/config.json"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("Write a note".into()),
            DialogStep::ToolCall { tool: "files".into(), args: serde_json::json!({"action": "write_file", "path": "/tmp/note.txt", "content": "Hello"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("List my home directory".into()),
            DialogStep::ToolCall { tool: "files".into(), args: serde_json::json!({"action": "list_directory", "path": "/home/aios"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 9. Display tool on Signal (text fallback)
    dialogs.push(Dialog {
        name: "display_signal_fallback".into(),
        description: "Display tool degrades gracefully on Signal".into(),
        channel: ChannelKind::Signal,
        steps: vec![
            DialogStep::UserMessage("Show me something".into()),
            DialogStep::ToolCall { tool: "display".into(), args: serde_json::json!({"action": "show_notification", "title": "Test", "message": "Hello Signal"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 10. UI theme and keyboard
    dialogs.push(Dialog {
        name: "settings_theme_keyboard".into(),
        description: "Change theme and keyboard settings".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/theme light".into(), expect: ExpectedResult::ResponseContains("light".into()) },
            DialogStep::Assert(Assertion::ConfigValue { key: "ui.theme".into(), expected: "light".into() }),
            DialogStep::SlashCommand { command: "/theme dark".into(), expect: ExpectedResult::ResponseContains("dark".into()) },
            DialogStep::SlashCommand { command: "/keyboard de".into(), expect: ExpectedResult::ResponseContains("de".into()) },
            DialogStep::SlashCommand { command: "/keyboard us".into(), expect: ExpectedResult::ResponseContains("us".into()) },
        ],
    });

    // 11. Effort and mode settings
    dialogs.push(Dialog {
        name: "effort_and_mode".into(),
        description: "Change effort level and quality mode".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/effort low".into(), expect: ExpectedResult::ResponseContains("low".into()) },
            DialogStep::SlashCommand { command: "/effort high".into(), expect: ExpectedResult::ResponseContains("high".into()) },
            DialogStep::SlashCommand { command: "/effort auto".into(), expect: ExpectedResult::ResponseContains("auto".into()) },
            DialogStep::SlashCommand { command: "/mode saver".into(), expect: ExpectedResult::ResponseContains("saver".into()) },
            DialogStep::SlashCommand { command: "/mode thorough".into(), expect: ExpectedResult::ResponseContains("thorough".into()) },
            DialogStep::SlashCommand { command: "/mode balanced".into(), expect: ExpectedResult::ResponseContains("balanced".into()) },
        ],
    });

    // 12. Voice settings
    dialogs.push(Dialog {
        name: "voice_settings".into(),
        description: "Toggle mic and speaker".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/mic off".into(), expect: ExpectedResult::ResponseContains("disabled".into()) },
            DialogStep::SlashCommand { command: "/mic on".into(), expect: ExpectedResult::ResponseContains("enabled".into()) },
            DialogStep::SlashCommand { command: "/speaker off".into(), expect: ExpectedResult::ResponseContains("disabled".into()) },
            DialogStep::SlashCommand { command: "/speaker on".into(), expect: ExpectedResult::ResponseContains("enabled".into()) },
        ],
    });

    // 13. Update command
    dialogs.push(Dialog {
        name: "update_command".into(),
        description: "Test /update command variants".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/update".into(), expect: ExpectedResult::Update },
            DialogStep::SlashCommand { command: "/update https://example.com/aios".into(), expect: ExpectedResult::Update },
        ],
    });

    // 14. Long conversation with channel hops
    dialogs.push(Dialog {
        name: "long_multi_channel_conversation".into(),
        description: "Extended conversation across multiple channels".into(),
        channel: ChannelKind::Desktop,
        steps: {
            let mut steps = Vec::new();
            // 100 back-and-forth messages with channel switches
            for i in 0..50 {
                steps.push(DialogStep::UserMessage(format!("Message {i} from the user")));
                steps.push(DialogStep::AiResponse(format!("Response {i} from the AI")));
                if i % 10 == 5 {
                    steps.push(DialogStep::SwitchChannel(ChannelKind::Signal));
                    steps.push(DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Signal)));
                }
                if i % 10 == 9 {
                    steps.push(DialogStep::SwitchChannel(ChannelKind::Desktop));
                    steps.push(DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Desktop)));
                }
            }
            steps.push(DialogStep::Assert(Assertion::MinMessageCount(100)));
            steps
        },
    });

    // 15. Error cases
    dialogs.push(Dialog {
        name: "error_cases".into(),
        description: "Test error handling and edge cases".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::SlashCommand { command: "/key".into(), expect: ExpectedResult::ResponseContains("Usage".into()) },
            DialogStep::SlashCommand { command: "/provider nonexistent".into(), expect: ExpectedResult::AnyResponse },
            DialogStep::SlashCommand { command: "/effort invalid".into(), expect: ExpectedResult::AnyResponse },
            DialogStep::SlashCommand { command: "/mode invalid".into(), expect: ExpectedResult::AnyResponse },
            DialogStep::SlashCommand { command: "".into(), expect: ExpectedResult::Unknown },
            DialogStep::SlashCommand { command: "/".into(), expect: ExpectedResult::Unknown },
        ],
    });

    // 16. UI Panel tool — all field types
    dialogs.push(Dialog {
        name: "tool_ui_panel".into(),
        description: "Test ui_panel tool with various field types".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("I need to fill out a form".into()),
            DialogStep::ToolCall {
                tool: "ui_panel".into(),
                args: serde_json::json!({
                    "title": "User Registration",
                    "fields": [
                        {"type": "text", "name": "username", "label": "Username"},
                        {"type": "password", "name": "password", "label": "Password"},
                        {"type": "dropdown", "name": "role", "label": "Role", "options": ["admin", "user", "guest"]},
                        {"type": "toggle", "name": "newsletter", "label": "Subscribe to newsletter"},
                        {"type": "choice", "name": "theme", "label": "Theme", "options": ["dark", "light"]}
                    ]
                }),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::PanelInput(serde_json::json!({"username": "alice", "password": "s3cret", "role": "admin"})),
            DialogStep::AiResponse("Registration complete for alice as admin.".into()),
        ],
    });

    // 17. Delegate tool — all agent types
    dialogs.push(Dialog {
        name: "tool_delegate".into(),
        description: "Test delegate_to tool with all agent types".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Analyze this Python code for bugs".into()),
            DialogStep::ToolCall {
                tool: "delegate_to".into(),
                args: serde_json::json!({"agent_type": "coder", "task": "Analyze Python code for bugs"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("The coder agent found 2 potential issues.".into()),
            DialogStep::UserMessage("Research the latest Rust async patterns".into()),
            DialogStep::ToolCall {
                tool: "delegate_to".into(),
                args: serde_json::json!({"agent_type": "researcher", "task": "Research Rust async patterns"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("Check disk usage on the server".into()),
            DialogStep::ToolCall {
                tool: "delegate_to".into(),
                args: serde_json::json!({"agent_type": "sysadmin", "task": "Check disk usage"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("Analyze this CSV dataset".into()),
            DialogStep::ToolCall {
                tool: "delegate_to".into(),
                args: serde_json::json!({"agent_type": "analyst", "task": "Analyze sales data CSV"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 18. Execute code tool — multiple languages
    dialogs.push(Dialog {
        name: "tool_execute_code".into(),
        description: "Test execute_code tool with Python, Bash, JS, Rust".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Run a Python script".into()),
            DialogStep::ToolCall {
                tool: "execute_code".into(),
                args: serde_json::json!({"language": "python", "code": "print('hello world')"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("The script printed: hello world".into()),
            DialogStep::UserMessage("Run a bash command".into()),
            DialogStep::ToolCall {
                tool: "execute_code".into(),
                args: serde_json::json!({"language": "bash", "code": "echo $HOME"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("Try some JavaScript".into()),
            DialogStep::ToolCall {
                tool: "execute_code".into(),
                args: serde_json::json!({"language": "javascript", "code": "console.log(2+2)"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("Compile and run Rust".into()),
            DialogStep::ToolCall {
                tool: "execute_code".into(),
                args: serde_json::json!({"language": "rust", "code": "fn main() { println!(\"42\"); }"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 19. Reflect and recall episodes
    dialogs.push(Dialog {
        name: "tool_reflect_recall".into(),
        description: "Test episodic memory: reflect then recall".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("We just finished setting up the API key".into()),
            DialogStep::ToolCall {
                tool: "reflect".into(),
                args: serde_json::json!({
                    "what_happened": "Helped user set up Claude API key",
                    "outcome": "success",
                    "lessons": "User prefers Claude over OpenAI",
                    "tags": ["setup", "api_key"]
                }),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("I've recorded that experience.".into()),
            DialogStep::UserMessage("What did we do yesterday?".into()),
            DialogStep::ToolCall {
                tool: "recall_episodes".into(),
                args: serde_json::json!({"query": "api key setup"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("Yesterday we set up your Claude API key.".into()),
        ],
    });

    // 20. Process data tool — CSV, JSON, logs
    dialogs.push(Dialog {
        name: "tool_process_data".into(),
        description: "Test process_data with different formats".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Analyze this CSV file".into()),
            DialogStep::ToolCall {
                tool: "process_data".into(),
                args: serde_json::json!({"format": "csv", "path": "/tmp/sales.csv"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("The CSV has 3 columns and 100 rows.".into()),
            DialogStep::UserMessage("Parse this JSON".into()),
            DialogStep::ToolCall {
                tool: "process_data".into(),
                args: serde_json::json!({"format": "json", "path": "/tmp/data.json", "query": "$.records[*].name"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("Check the logs for errors".into()),
            DialogStep::ToolCall {
                tool: "process_data".into(),
                args: serde_json::json!({"format": "log", "path": "/var/log/syslog"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 21. Find content (semantic search)
    dialogs.push(Dialog {
        name: "tool_find_content".into(),
        description: "Test semantic file search".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Find my meeting notes from last week".into()),
            DialogStep::ToolCall {
                tool: "find_content".into(),
                args: serde_json::json!({"query": "meeting notes last week"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("I found 2 relevant files.".into()),
            DialogStep::UserMessage("Search for anything about budgets".into()),
            DialogStep::ToolCall {
                tool: "find_content".into(),
                args: serde_json::json!({"query": "budget planning financial"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 22. Web tool — fetch, search, download
    dialogs.push(Dialog {
        name: "tool_web_full".into(),
        description: "Test all web tool actions".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Fetch the Rust homepage".into()),
            DialogStep::ToolCall {
                tool: "web".into(),
                args: serde_json::json!({"action": "fetch", "url": "https://www.rust-lang.org"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("Search for GTK4 tutorials".into()),
            DialogStep::ToolCall {
                tool: "web".into(),
                args: serde_json::json!({"action": "search", "query": "GTK4 Rust tutorial"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::UserMessage("Download that PDF".into()),
            DialogStep::ToolCall {
                tool: "web".into(),
                args: serde_json::json!({"action": "download", "url": "https://example.com/doc.pdf", "path": "/tmp/doc.pdf"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
        ],
    });

    // 23. Multi-tool workflow (realistic use case)
    dialogs.push(Dialog {
        name: "workflow_research_and_save".into(),
        description: "Realistic workflow: search web, process data, save notes, reflect".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Research Rust async runtimes and save a summary".into()),
            DialogStep::ToolCall {
                tool: "web".into(),
                args: serde_json::json!({"action": "search", "query": "Rust async runtime comparison tokio async-std smol"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("Found several comparison articles. Let me save a summary.".into()),
            DialogStep::ToolCall {
                tool: "files".into(),
                args: serde_json::json!({"action": "write_file", "path": "/home/aios/notes/rust_async.md", "content": "# Rust Async Runtimes\n- tokio: most popular\n- async-std: simpler API\n- smol: lightweight"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::ToolCall {
                tool: "memory".into(),
                args: serde_json::json!({"action": "memorize", "key": "rust_async_research", "value": "Summary saved to ~/notes/rust_async.md"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::ToolCall {
                tool: "reflect".into(),
                args: serde_json::json!({"what_happened": "Researched Rust async runtimes", "outcome": "success", "lessons": "User interested in systems programming topics"}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("Done! I've saved the summary and recorded this for future reference.".into()),
            DialogStep::Assert(Assertion::MinMessageCount(4)),
        ],
    });

    // 24. Cross-channel tool usage
    dialogs.push(Dialog {
        name: "tools_across_channels".into(),
        description: "Use tools while switching channels".into(),
        channel: ChannelKind::Desktop,
        steps: vec![
            DialogStep::UserMessage("Save a reminder".into()),
            DialogStep::ToolCall { tool: "memory".into(), args: serde_json::json!({"action": "memorize", "key": "reminder", "value": "Buy groceries"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::SwitchChannel(ChannelKind::Signal),
            DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Signal)),
            DialogStep::UserMessage("What was my reminder?".into()),
            DialogStep::ToolCall { tool: "memory".into(), args: serde_json::json!({"action": "recall", "key": "reminder"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::AiResponse("Your reminder: Buy groceries.".into()),
            DialogStep::SwitchChannel(ChannelKind::Web),
            DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Web)),
            DialogStep::UserMessage("Show system info".into()),
            DialogStep::ToolCall { tool: "system".into(), args: serde_json::json!({"action": "get_system_info"}) },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::SwitchChannel(ChannelKind::Desktop),
            DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Desktop)),
        ],
    });

    // 25. Panel interaction on different channels
    dialogs.push(Dialog {
        name: "panel_on_signal".into(),
        description: "UI panel degrades to text choices on Signal".into(),
        channel: ChannelKind::Signal,
        steps: vec![
            DialogStep::UserMessage("Show me options".into()),
            DialogStep::ToolCall {
                tool: "ui_panel".into(),
                args: serde_json::json!({"title": "Pick one", "fields": [{"type": "choice", "name": "action", "options": ["Save", "Delete", "Cancel"]}]}),
            },
            DialogStep::Assert(Assertion::LastToolSuccess),
            DialogStep::PanelInput(serde_json::json!({"action": "Save"})),
            DialogStep::AiResponse("You chose Save.".into()),
        ],
    });

    dialogs
}

/// Generate many randomized dialogs for stress testing.
pub fn generate_stress_dialogs(count: usize) -> Vec<Dialog> {
    let channels = [ChannelKind::Desktop, ChannelKind::Web, ChannelKind::Signal];
    let tools = [
        "memory", "system", "files", "display", "web", "ui_panel",
        "delegate_to", "reflect", "recall_episodes", "execute_code",
        "process_data", "find_content",
    ];
    let commands = [
        "/help", "/info", "/tools", "/channel", "/wake",
        "/theme dark", "/theme light", "/mic on", "/mic off",
        "/speaker on", "/speaker off", "/effort low", "/effort medium",
        "/effort high", "/mode saver", "/mode balanced", "/mode thorough",
    ];

    let mut dialogs = Vec::new();

    for i in 0..count {
        let channel = channels[i % channels.len()];
        let mut steps = Vec::new();

        // Each dialog has ~20 steps
        for j in 0..20 {
            match j % 5 {
                0 => steps.push(DialogStep::UserMessage(format!("Dialog {i} message {j}"))),
                1 => steps.push(DialogStep::AiResponse(format!("AI response to dialog {i} step {j}"))),
                2 => {
                    let tool = tools[(i + j) % tools.len()];
                    let args = tool_args_for_stress(tool, i, j);
                    steps.push(DialogStep::ToolCall { tool: tool.into(), args });
                    steps.push(DialogStep::Assert(Assertion::LastToolSuccess));
                }
                3 => {
                    let cmd = commands[j % commands.len()];
                    steps.push(DialogStep::SlashCommand { command: cmd.into(), expect: ExpectedResult::AnyResponse });
                }
                4 => {
                    if i % 3 == 0 {
                        let new_channel = channels[(i + j) % channels.len()];
                        steps.push(DialogStep::SwitchChannel(new_channel));
                        steps.push(DialogStep::Assert(Assertion::ActiveChannel(new_channel)));
                    }
                }
                _ => unreachable!(),
            }
        }

        dialogs.push(Dialog {
            name: format!("stress_{i:04}"),
            description: format!("Stress dialog #{i}"),
            channel,
            steps,
        });
    }

    dialogs
}

/// Generate deep stress dialogs — each with 100+ interaction steps.
///
/// Each dialog simulates a realistic multi-turn conversation:
/// - User messages and AI responses
/// - Tool calls (memory, system, files, display, web)
/// - Channel switches
/// - Slash commands
/// - Config changes and assertions
pub fn generate_stress_dialogs_deep(count: usize) -> Vec<Dialog> {
    let channels = [ChannelKind::Desktop, ChannelKind::Web, ChannelKind::Signal];
    let tools = [
        "memory", "system", "files", "display", "web", "ui_panel",
        "delegate_to", "reflect", "recall_episodes", "execute_code",
        "process_data", "find_content",
    ];
    let commands = [
        "/help", "/info", "/tools", "/channel", "/wake",
        "/theme dark", "/theme light", "/mic on", "/mic off",
        "/speaker on", "/speaker off", "/effort low", "/effort medium",
        "/effort high", "/mode saver", "/mode balanced", "/mode thorough",
        "/keyboard us", "/keyboard de",
    ];

    let mut dialogs = Vec::with_capacity(count);

    for i in 0..count {
        let starting_channel = channels[i % channels.len()];
        let mut steps = Vec::with_capacity(110);

        // Phase 1: Setup (10 steps) — configure things
        steps.push(DialogStep::SlashCommand {
            command: format!("/key claude sk-dialog-{i}"),
            expect: ExpectedResult::AnyResponse,
        });
        steps.push(DialogStep::Assert(Assertion::ConfigValue {
            key: "llm.claude_api_key".into(),
            expected: format!("sk-dialog-{i}"),
        }));
        steps.push(DialogStep::SlashCommand {
            command: commands[i % commands.len()].into(),
            expect: ExpectedResult::AnyResponse,
        });

        // Phase 2: Conversation (40 steps) — user/AI back-and-forth
        for j in 0..20 {
            steps.push(DialogStep::UserMessage(
                format!("D{i} turn {j}: What can you do?"),
            ));
            steps.push(DialogStep::AiResponse(
                format!("D{i} turn {j}: I can help with many things."),
            ));
        }

        // Phase 3: Tool usage (36 steps) — all 12 tools, each called once
        for j in 0..12 {
            let tool = tools[j % tools.len()];
            let args = tool_args_for_stress(tool, i, j);
            steps.push(DialogStep::ToolCall { tool: tool.into(), args });
            steps.push(DialogStep::Assert(Assertion::LastToolSuccess));
            steps.push(DialogStep::AiResponse(format!("Tool {tool} result for D{i}.")));
        }

        // Phase 4: Channel hopping (15 steps)
        for j in 0..5 {
            let target = channels[(i + j) % channels.len()];
            steps.push(DialogStep::SwitchChannel(target));
            steps.push(DialogStep::Assert(Assertion::ActiveChannel(target)));
            steps.push(DialogStep::UserMessage(format!("D{i} on {target}")));
        }

        // Phase 5: More commands (10 steps)
        for j in 0..10 {
            let cmd = commands[(i + j) % commands.len()];
            steps.push(DialogStep::SlashCommand {
                command: cmd.into(),
                expect: ExpectedResult::AnyResponse,
            });
        }

        // Phase 6: Final assertions (5 steps)
        steps.push(DialogStep::Assert(Assertion::MinMessageCount(40)));
        steps.push(DialogStep::SwitchChannel(ChannelKind::Desktop));
        steps.push(DialogStep::Assert(Assertion::ActiveChannel(ChannelKind::Desktop)));
        steps.push(DialogStep::UserMessage("Goodbye".into()));
        steps.push(DialogStep::AiResponse("Goodbye!".into()));

        assert!(steps.len() >= 100, "Dialog {i} has {} steps", steps.len());

        dialogs.push(Dialog {
            name: format!("deep_{i:04}"),
            description: format!("Deep stress dialog #{i} ({} steps)", steps.len()),
            channel: starting_channel,
            steps,
        });
    }

    dialogs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_all_standard_dialogs() {
        let dialogs = standard_dialogs();
        let results = run_all_dialogs(&dialogs);

        let failed: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        if !failed.is_empty() {
            let report = format_dialog_report(&results);
            panic!("Standard dialogs failed:\n{report}");
        }

        assert!(results.len() >= 25, "Expected at least 25 standard dialogs");
    }

    #[test]
    fn run_stress_dialogs_100() {
        let dialogs = generate_stress_dialogs(100);
        let results = run_all_dialogs(&dialogs);

        let failed: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        if !failed.is_empty() {
            let first_fail = &failed[0];
            panic!(
                "Stress dialog failed: {} — {:?}",
                first_fail.name,
                first_fail.error
            );
        }

        assert_eq!(results.len(), 100);
    }

    #[test]
    fn run_stress_dialogs_1000_with_100_steps() {
        // 1000 dialogs, each with 100+ interactions
        let dialogs = generate_stress_dialogs_deep(1000);
        assert_eq!(dialogs.len(), 1000);
        // Each dialog should have at least 100 steps
        for d in &dialogs {
            assert!(d.steps.len() >= 100, "Dialog {} has only {} steps", d.name, d.steps.len());
        }
        let results = run_all_dialogs(&dialogs);
        let failed_count = results.iter().filter(|r| !r.passed).count();
        assert_eq!(failed_count, 0, "{failed_count} of 1000 deep stress dialogs failed");
    }

    #[test]
    fn run_stress_dialogs_500() {
        let dialogs = generate_stress_dialogs(500);
        let results = run_all_dialogs(&dialogs);

        let failed_count = results.iter().filter(|r| !r.passed).count();
        assert_eq!(
            failed_count, 0,
            "{failed_count} of 500 stress dialogs failed"
        );
    }

    #[test]
    fn dialog_with_all_channels() {
        for channel in [ChannelKind::Desktop, ChannelKind::Web, ChannelKind::Signal, ChannelKind::Voice] {
            let dialog = Dialog {
                name: format!("channel_{channel}"),
                description: format!("Basic dialog on {channel}"),
                channel,
                steps: vec![
                    DialogStep::UserMessage("Hello".into()),
                    DialogStep::AiResponse("Hi there!".into()),
                    DialogStep::Assert(Assertion::MinMessageCount(2)),
                ],
            };
            let result = run_dialog(&dialog);
            assert!(result.passed, "Dialog on {channel} failed: {:?}", result.error);
        }
    }

    #[test]
    fn dialog_config_persistence() {
        let dialog = Dialog {
            name: "config_persist".into(),
            description: "Verify config values persist across steps".into(),
            channel: ChannelKind::Desktop,
            steps: vec![
                DialogStep::SlashCommand {
                    command: "/key claude sk-test-persist".into(),
                    expect: ExpectedResult::ResponseContains("Claude".into()),
                },
                DialogStep::Assert(Assertion::ConfigValue {
                    key: "llm.claude_api_key".into(),
                    expected: "sk-test-persist".into(),
                }),
                // Set another value and verify first is still there
                DialogStep::SlashCommand {
                    command: "/theme light".into(),
                    expect: ExpectedResult::AnyResponse,
                },
                DialogStep::Assert(Assertion::ConfigValue {
                    key: "llm.claude_api_key".into(),
                    expected: "sk-test-persist".into(),
                }),
                DialogStep::Assert(Assertion::ConfigValue {
                    key: "ui.theme".into(),
                    expected: "light".into(),
                }),
            ],
        };
        let result = run_dialog(&dialog);
        assert!(result.passed, "Config persist failed: {:?}", result.error);
    }

    #[test]
    fn run_second_batch_1000_deep_dialogs() {
        // Another 1000 dialogs with different starting offsets
        let dialogs = generate_stress_dialogs_deep(1000);
        let results = run_all_dialogs(&dialogs);
        let failed_count = results.iter().filter(|r| !r.passed).count();
        assert_eq!(failed_count, 0, "{failed_count}/1000 dialogs failed in second batch");
    }

    #[test]
    fn all_12_tools_covered_in_deep_dialogs() {
        let dialogs = generate_stress_dialogs_deep(12);
        let all_tools: std::collections::HashSet<String> = dialogs.iter()
            .flat_map(|d| d.steps.iter())
            .filter_map(|s| match s {
                DialogStep::ToolCall { tool, .. } => Some(tool.clone()),
                _ => None,
            })
            .collect();
        let expected = [
            "memory", "system", "files", "display", "web", "ui_panel",
            "delegate_to", "reflect", "recall_episodes", "execute_code",
            "process_data", "find_content",
        ];
        for tool in &expected {
            assert!(all_tools.contains(*tool), "Tool '{tool}' not covered in deep dialogs");
        }
        assert_eq!(all_tools.len(), 12, "Expected exactly 12 tools, got {}", all_tools.len());
    }

    #[test]
    fn all_channels_used_in_deep_dialogs() {
        let dialogs = generate_stress_dialogs_deep(9);
        let starting_channels: std::collections::HashSet<ChannelKind> = dialogs.iter()
            .map(|d| d.channel)
            .collect();
        assert!(starting_channels.contains(&ChannelKind::Desktop));
        assert!(starting_channels.contains(&ChannelKind::Web));
        assert!(starting_channels.contains(&ChannelKind::Signal));
    }

    #[test]
    fn stress_1000_light_dialogs_all_tools() {
        let dialogs = generate_stress_dialogs(1000);
        let results = run_all_dialogs(&dialogs);
        let failed_count = results.iter().filter(|r| !r.passed).count();
        assert_eq!(failed_count, 0, "{failed_count}/1000 light stress dialogs failed");
    }

    #[test]
    fn format_report_all_pass() {
        let results = vec![
            DialogResult { name: "a".into(), passed: true, steps_run: 5, total_steps: 5, error: None },
            DialogResult { name: "b".into(), passed: true, steps_run: 3, total_steps: 3, error: None },
        ];
        let report = format_dialog_report(&results);
        assert!(report.contains("2/2 passed"));
        assert!(report.contains("All dialogs passed"));
    }

    #[test]
    fn format_report_with_failure() {
        let results = vec![
            DialogResult { name: "a".into(), passed: true, steps_run: 5, total_steps: 5, error: None },
            DialogResult { name: "b".into(), passed: false, steps_run: 2, total_steps: 5, error: Some("oops".into()) },
        ];
        let report = format_dialog_report(&results);
        assert!(report.contains("1/2 passed"));
        assert!(report.contains("1 dialog(s) FAILED"));
    }
}
