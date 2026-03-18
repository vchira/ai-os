# Power User Commands

Beyond the basics, AiOS has commands and techniques for users who want to get the most out of the system.

## Combining commands with conversation

You can mix slash commands and natural language in a workflow:

1. `/mode thorough` -- switch to thorough mode for a complex task
2. "Analyze the security of my SSH configuration" -- let the AI do deep analysis
3. `/mode balanced` -- switch back to balanced mode for normal use

## Self-test for diagnostics

Run the self-test suite to verify system health:

```
/selftest              # Run all tests
/selftest quick        # Fast checks only (API, filesystem, basic functionality)
/selftest channel      # Test all channel connections
/selftest tools        # Verify all tools are working
/selftest interactive  # Tests that require user interaction
```

Self-test results show pass/fail for each component with details on failures. This is the first thing to run when something seems wrong.

## System monitoring

Get a live view of system resources:

```
/sysinfo
```

This shows CPU usage, memory usage, disk space, network status, audio devices, and running processes. It is like a quick `htop` + `df` + device overview in one command.

## Updating AiOS

Update the AiOS binary from a URL:

```
/update https://your-server.com/aios-binary
```

This downloads the new binary, replaces the current one, and restarts the application. Useful for development and testing.

## Re-running setup

If you need to reconfigure the system from scratch:

```
/configure
```

This re-runs the setup wizard. Your existing vault is preserved -- you can change providers, keys, and passwords.

## Advanced AI control

### Force a specific model

```
/model claude-sonnet-4-20250514
```

Overrides the automatic model selection. Useful when you know you need a specific model's capabilities.

### Chain requests efficiently

The AI maintains context, so you can build on previous results:

```
You: "List all Python files in my project"
AI: [shows file list]
You: "Now check which ones have syntax errors"
AI: [checks each file]
You: "Fix the errors in the first one"
AI: [fixes the file]
```

Each step builds on the previous one without repeating context.

### Delegate complex tasks

For multi-step tasks, the AI can delegate to specialized sub-agents:

- "Research the best practices for securing a Linux server" -- delegates to the Researcher agent
- "Refactor this Python code to use async/await" -- delegates to the Coder agent
- "Analyze this CSV file and create a summary report" -- delegates to the Analyst agent

## Terminal power moves

Press `Alt+Enter` to open a terminal. From there, you have full Linux access:

```bash
# Check AiOS process
ps aux | grep aios

# View AiOS logs
journalctl -u aios -f

# Check network
ip addr
ping 8.8.8.8

# Monitor resources
htop
```

The terminal runs alongside AiOS. Press `Super` to switch back to the chat.

## Useful natural language patterns

These patterns work well with the AI:

- **"Do X, then Y, then Z"** -- the AI executes multi-step tasks in sequence
- **"If X, then do Y, otherwise do Z"** -- conditional logic works
- **"Every file that matches..."** -- batch operations
- **"Compare A and B"** -- the AI can analyze differences
- **"Explain what you just did"** -- ask for clarification after any operation
- **"Undo that"** -- the AI remembers what it did and can often reverse it
