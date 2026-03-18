# Built-in Tools

The AI interacts with your system through tools. AiOS includes 12 built-in tools organized into categories. The AI selects the right tool automatically based on your request.

## How tools work

When you ask the AI to do something that requires system access, it:

1. Identifies the appropriate tool
2. Tells you what it plans to do
3. Executes the tool (with sandboxing for safety)
4. Shows you the result

You do not need to know which tool the AI uses -- just express your intent naturally.

## Tool categories

### Memory tools

These tools let the AI remember information across conversations.

| Tool | Description |
|------|-------------|
| **memory** | Key-value store for persistent information. The AI can save and retrieve facts, preferences, and notes. |
| **reflect** | Records structured reflections about what happened, the outcome, and lessons learned. Builds the AI's "experience." |
| **recall_episodes** | Searches past experiences and reflections. The AI can learn from previous interactions. |

**Examples:**
- "Remember that my favorite color is blue"
- "What did we talk about yesterday?"
- "What went wrong last time I tried to configure Wi-Fi?"

### System tools

These tools interact with the operating system.

| Tool | Description |
|------|-------------|
| **system** | Runs commands, gets system information, lists processes. Commands are sandboxed based on risk level. |
| **execute_code** | Runs code (Python, Bash, JavaScript, Rust) in an isolated sandbox. Uses Docker when available, with process isolation as fallback. |
| **delegate_to** | Spawns specialized sub-agents for complex tasks (coder, researcher, sysadmin, analyst). |

**Examples:**
- "What processes are using the most memory?"
- "Run this Python script for me"
- "Analyze the security of my SSH configuration" (delegates to a specialized agent)

### Filesystem tools

These tools work with files and data.

| Tool | Description |
|------|-------------|
| **files** | Reads, writes, and searches files. Scoped to your home directory for safety. |
| **process_data** | Processes large files locally (CSV, JSON, logs) without sending them to the AI. Extracts summaries, errors, and specific fields. |
| **find_content** | Semantic file search -- finds files by meaning rather than exact name. "Find meeting notes from last week" actually works. |

**Examples:**
- "Read my .bashrc file"
- "Create a file called notes.txt with today's date"
- "Summarize that 50MB CSV file" (processes locally, sends only the summary)
- "Find documents about the budget"

### Network tools

| Tool | Description |
|------|-------------|
| **web** | Fetches URLs, searches the web (via DuckDuckGo), and downloads files. |

**Examples:**
- "Search the web for Rust async tutorials"
- "Download the file at https://example.com/data.csv"
- "What does the AiOS website say?"

### UI tools

These tools present information and collect input from you.

| Tool | Description |
|------|-------------|
| **display** | Shows images, notifications, and formatted content. Adapts to the current channel. |
| **ui_panel** | Shows interactive input panels -- text fields, dropdowns, toggles, password inputs, choices. Used by the setup wizard and settings. |

**Examples:**
- "Show me that screenshot"
- The AI uses ui_panel internally when it needs structured input from you

## Sandbox levels

Commands and code are executed with appropriate isolation:

| Level | When used | Protection |
|-------|-----------|-----------|
| **None** | Read-only commands (`ls`, `cat`, `ps`) | Direct execution |
| **Process** | Modifying commands (`mv`, `cp`, `mkdir`) | Timeout + resource limits |
| **Docker** | Dangerous commands (`rm`, `pip`, `python`, arbitrary code) | Isolated container, no network, memory limits |

The AI does not choose the sandbox level -- it is determined automatically based on the command being run.

## Dynamic tool bundling

To save costs, AiOS does not send all 12 tool definitions with every request. It analyzes your message and sends only relevant tools:

- "Read my config file" -- only filesystem tools are sent
- "Search the web" -- only network tools are sent
- "How's the weather?" -- could be web or direct answer, so more tools are included

This happens automatically. You do not need to do anything.

## Multi-agent delegation

For complex tasks, the AI can delegate work to specialized sub-agents:

| Agent | Specialization | Example task |
|-------|---------------|-------------|
| **Coder** | Code analysis, refactoring | "Review this Python script for bugs" |
| **Researcher** | Web search, information gathering | "Research the best backup solutions for Linux" |
| **SysAdmin** | System management, process control | "Optimize my system for performance" |
| **Analyst** | Data analysis, CSV/JSON processing | "Analyze the trends in this sales data" |

Each sub-agent gets its own conversation context and relevant tool subset. The main AI coordinates and presents the results.

## Full reference

For a complete list of all tools with their parameters and detailed descriptions, see the [All Tools](../reference/tools.md) reference.
