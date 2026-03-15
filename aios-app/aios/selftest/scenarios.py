"""Self-test scenario definitions for /selftest."""

# Each scenario has:
#   name: Display name for the test
#   test: A callable(runner) -> True | error_string

SCENARIOS = [
    {
        "name": "Config read/write roundtrip",
        "test": lambda runner: runner.test_config_roundtrip(),
    },
    {
        "name": "Tool registry loads built-in tools",
        "test": lambda runner: runner.test_tool_registry(),
    },
    {
        "name": "Memory tool: memorize/recall/forget",
        "test": lambda runner: runner.test_tool_execution_memory(),
    },
    {
        "name": "System tool: get_datetime/get_system_info",
        "test": lambda runner: runner.test_tool_execution_system(),
    },
    {
        "name": "Command handler: /help, /info, /theme",
        "test": lambda runner: runner.test_command_handler(),
    },
    {
        "name": "Mock conversation with tool calls",
        "test": lambda runner: runner.test_mock_conversation(),
    },
    {
        "name": "Tool schemas valid for LLM consumption",
        "test": lambda runner: runner.test_tool_schema_format(),
    },
    {
        "name": "Keyboard layout config command",
        "test": lambda runner: runner.test_config_commands_keyboard(),
    },
    {
        "name": "Provider switching command",
        "test": lambda runner: runner.test_provider_switching(),
    },
]
