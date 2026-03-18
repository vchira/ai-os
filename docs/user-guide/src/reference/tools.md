# All Tools

Complete reference of all 12 built-in AiOS tools. The AI selects and uses these tools automatically based on your requests. You do not invoke tools directly -- the AI decides when to use them.

## Memory Category

### memory

Persistent key-value store. The AI uses this to remember information across conversations.

**Operations:**
- `set(key, value)` -- store a value
- `get(key)` -- retrieve a value
- `delete(key)` -- remove a value
- `list()` -- list all stored keys

**Example triggers:**
- "Remember that my server IP is 10.0.0.5"
- "What was my server IP?"
- "Forget my server IP"

### reflect

Records structured reflections about interactions. The AI uses this to learn from experience.

**Fields:**
- `summary` -- brief description of what happened
- `details` -- full account of the interaction
- `outcome` -- success, failure (with reason), or partial
- `lessons` -- reusable insights for future interactions
- `tags` -- searchable categories

**Example triggers:**
- This tool is used automatically by the AI after significant interactions
- It is not typically invoked by user request

### recall_episodes

Searches past experiences and reflections. The AI uses this to learn from previous interactions.

**Parameters:**
- `query` -- search term or description
- `limit` -- maximum number of results

**Example triggers:**
- "What happened last time I tried to set up Wi-Fi?"
- "Have you dealt with this kind of error before?"

## System Category

### system

Runs system commands, retrieves system information, and lists processes.

**Operations:**
- `run(command)` -- execute a shell command
- `info()` -- get system information (OS, kernel, CPU, RAM, disk)
- `processes()` -- list running processes

**Sandbox levels:**
- Read-only commands (`ls`, `cat`, `ps`): no sandbox
- Modifying commands (`mv`, `cp`): process isolation with timeout
- Dangerous commands (`rm`, `pip`): Docker container isolation

**Example triggers:**
- "How much disk space do I have?"
- "List running processes"
- "Kill the process using port 8080"
- "What kernel version am I running?"

### execute_code

Runs code in an isolated sandbox. Supports Python, Bash, JavaScript, and Rust.

**Parameters:**
- `language` -- the programming language
- `code` -- the code to execute

**Sandbox:** Always sandboxed. Docker preferred, process isolation as fallback. No network access in sandbox.

**Example triggers:**
- "Run this Python script: ..."
- "Calculate 2^64 in Python"
- "Write and run a Bash script that finds duplicate files"

### delegate_to

Spawns a specialized sub-agent for complex tasks.

**Agent types:**
- `coder` -- code analysis, refactoring, debugging (filesystem tools)
- `researcher` -- web search, information gathering (network + memory tools)
- `sysadmin` -- system management, process control (system tools)
- `file_manager` -- file operations, organization (filesystem tools)
- `analyst` -- data analysis, CSV/JSON processing (filesystem + network tools)

**Example triggers:**
- "Analyze this codebase for security vulnerabilities" (coder agent)
- "Research the best Linux backup solutions" (researcher agent)
- "Optimize my system performance" (sysadmin agent)

## Filesystem Category

### files

Reads, writes, and searches files. Scoped to the user's home directory.

**Operations:**
- `read(path)` -- read file contents
- `write(path, content)` -- create or overwrite a file
- `append(path, content)` -- append to a file
- `delete(path)` -- delete a file
- `list(path)` -- list directory contents
- `search(pattern)` -- search for files by name pattern

**Example triggers:**
- "Read my .bashrc"
- "Create a file called notes.txt"
- "Find all .py files in my project"
- "Delete the temporary files"

### process_data

Processes large files locally without sending them to the LLM. Zero-copy -- never loads the full file into memory.

**Supported formats:**
- **CSV** -- shows column names, row count, and 5 sample rows
- **JSON** -- queries by dot-path, returns matching values
- **Logs** -- extracts error and warning lines

**Example triggers:**
- "Summarize that 50MB CSV file"
- "How many rows are in data.csv?"
- "Extract all errors from the system log"
- "What is the value of config.database.host in settings.json?"

### find_content

Semantic file search. Finds files by meaning, not just filename.

**Parameters:**
- `query` -- natural language description of what you are looking for

**Example triggers:**
- "Find meeting notes from last week"
- "Find documents about the budget"
- "Where did I save that recipe?"

## Network Category

### web

Fetches URLs, searches the web, and downloads files.

**Operations:**
- `fetch(url)` -- retrieve the content of a URL
- `search(query)` -- search the web via DuckDuckGo
- `download(url, path)` -- download a file to a local path

**Example triggers:**
- "Search the web for Rust async tutorials"
- "Download the file at https://example.com/data.csv"
- "What does the AiOS website say?"
- "Check if example.com is up"

## UI Category

### display

Shows images, notifications, and formatted content. Adapts output to the current channel.

**Operations:**
- `image(path)` -- display an image
- `notification(title, message)` -- show a toast notification
- `markdown(content)` -- render formatted content

**Channel adaptation:**
- Desktop/Web: native rendering
- Signal: image attachments or text fallback
- Voice: text description only

**Example triggers:**
- "Show me that screenshot"
- "Display this image"
- Notifications are used internally by the system

### ui_panel

Shows interactive input panels to the user. Used by the setup wizard, settings, and any tool that needs structured input.

**Input types:**
- `text` -- single-line text field
- `password` -- masked text field
- `dropdown` -- selection from a list
- `choice` -- radio buttons (single selection)
- `multi_choice` -- checkboxes (multiple selection)
- `toggle` -- on/off switch
- `button` -- action button

**Channel adaptation:**
- Desktop: GTK dialog overlay
- Web: HTML form
- Signal: numbered text choices
- Voice: spoken options with voice selection

**Example triggers:**
- This tool is used internally by the AI when it needs structured input
- The setup wizard uses it extensively
- It is not typically invoked by direct user request

## Tool categories and bundling

Tools are grouped into categories for efficient bundling:

| Category | Tools |
|----------|-------|
| memory | memory, reflect, recall_episodes |
| system | system, execute_code, delegate_to |
| filesystem | files, process_data, find_content |
| network | web |
| ui | display, ui_panel |

Only relevant tool categories are sent to the LLM based on your message, reducing token usage by approximately 80%.
