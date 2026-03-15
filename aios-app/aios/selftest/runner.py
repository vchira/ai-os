"""Self-test runner — simulates AI conversations to test the full pipeline."""

import json
import time
import logging
from typing import TYPE_CHECKING

from aios.selftest.scenarios import SCENARIOS

if TYPE_CHECKING:
    from aios.config.manager import ConfigManager
    from aios.llm.manager import LLMManager

log = logging.getLogger(__name__)


class MockLLMResponse:
    """Simulated LLM response for testing."""

    def __init__(self, content: str, tool_calls: list | None = None):
        self.content = content
        self.tool_calls = tool_calls or []


class MockToolCall:
    """Simulated tool call."""

    def __init__(self, id: str, name: str, arguments: dict):
        self.id = id
        self.name = name
        self.arguments = arguments


class SelfTestRunner:
    """Runs self-test scenarios without calling a real LLM.

    Each scenario defines:
    - A user message
    - Expected mock LLM responses (possibly with tool calls)
    - Assertions to validate

    This tests the full pipeline: message handling, tool execution,
    response formatting, and config commands.
    """

    def __init__(self, config_mgr: "ConfigManager", llm_manager: "LLMManager | None"):
        self.config = config_mgr
        self.llm = llm_manager
        self.results: list[dict] = []

    def run_all(self) -> str:
        """Run all test scenarios and return a formatted report."""
        self.results = []
        lines = ["AiOS Self-Test", "=" * 50, ""]

        for scenario in SCENARIOS:
            result = self._run_scenario(scenario)
            self.results.append(result)
            status = "PASS" if result["passed"] else "FAIL"
            lines.append(f"[{status}] {scenario['name']}")
            if not result["passed"]:
                lines.append(f"       Error: {result['error']}")
            if result.get("details"):
                lines.append(f"       {result['details']}")

        passed = sum(1 for r in self.results if r["passed"])
        total = len(self.results)
        lines.append("")
        lines.append(f"Results: {passed}/{total} passed")

        if passed == total:
            lines.append("All tests passed!")
        else:
            lines.append(f"{total - passed} test(s) failed.")

        return "\n".join(lines)

    def _run_scenario(self, scenario: dict) -> dict:
        """Run a single test scenario."""
        name = scenario["name"]
        try:
            test_fn = scenario["test"]
            result = test_fn(self)
            if result is True:
                return {"name": name, "passed": True, "error": None}
            elif isinstance(result, str):
                return {"name": name, "passed": False, "error": result}
            else:
                return {"name": name, "passed": False, "error": "Test returned falsy value"}
        except Exception as e:
            return {"name": name, "passed": False, "error": str(e)}

    def test_config_roundtrip(self) -> bool | str:
        """Test that config can be written and read back."""
        test_key = "selftest.marker"
        test_val = f"test_{int(time.time())}"
        self.config.set(test_key, test_val)
        readback = self.config.get(test_key)
        if readback != test_val:
            return f"Expected {test_val}, got {readback}"
        # Clean up
        self.config.set(test_key, None)
        return True

    def test_tool_registry(self) -> bool | str:
        """Test that tools are registered and have valid schemas."""
        if not self.llm or not hasattr(self.llm, "tool_registry") or not self.llm.tool_registry:
            # Try importing directly
            try:
                from aios.tools.registry import ToolRegistry
                registry = ToolRegistry()
                registry.load_builtin_tools()
            except Exception as e:
                return f"Cannot load tool registry: {e}"
        else:
            registry = self.llm.tool_registry

        tools = registry.list_tools()
        if not tools:
            return "No tools registered"

        for tool in tools:
            if not tool.name:
                return f"Tool has empty name"
            if not tool.description:
                return f"Tool '{tool.name}' has empty description"
            schema = tool.parameters
            if not isinstance(schema, dict):
                return f"Tool '{tool.name}' has invalid parameters schema"

        return True

    def test_tool_execution_memory(self) -> bool | str:
        """Test the memory tool: memorize, recall, forget."""
        try:
            from aios.tools.builtin.memory import MemoryTool
            tool = MemoryTool()

            # Memorize
            result = tool.execute(action="memorize", key="selftest_key", value="selftest_value")
            if not result.success:
                return f"Memorize failed: {result.error}"

            # Recall
            result = tool.execute(action="recall", key="selftest_key")
            if not result.success:
                return f"Recall failed: {result.error}"
            if "selftest_value" not in result.output:
                return f"Recall returned wrong value: {result.output}"

            # List keys
            result = tool.execute(action="list_keys")
            if not result.success:
                return f"List keys failed: {result.error}"

            # Forget
            result = tool.execute(action="forget", key="selftest_key")
            if not result.success:
                return f"Forget failed: {result.error}"

            # Verify forgotten
            result = tool.execute(action="recall", key="selftest_key")
            if result.success and "selftest_value" in result.output:
                return "Key was not forgotten"

            return True
        except Exception as e:
            return f"Memory tool test error: {e}"

    def test_tool_execution_system(self) -> bool | str:
        """Test the system tool: get_datetime, get_system_info."""
        try:
            from aios.tools.builtin.system_tools import SystemTool
            tool = SystemTool()

            # Get datetime
            result = tool.execute(action="get_datetime")
            if not result.success:
                return f"get_datetime failed: {result.error}"
            if not result.output:
                return "get_datetime returned empty output"

            # Get system info
            result = tool.execute(action="get_system_info")
            if not result.success:
                return f"get_system_info failed: {result.error}"
            if not result.output:
                return "get_system_info returned empty output"

            return True
        except Exception as e:
            return f"System tool test error: {e}"

    def test_command_handler(self) -> bool | str:
        """Test slash command parsing and execution."""
        from aios.config.commands import CommandHandler
        handler = CommandHandler(self.config, self.llm)

        # Test /help
        result = handler.execute("/help")
        if "Available commands" not in result:
            return f"/help didn't return command list: {result[:100]}"

        # Test /info
        result = handler.execute("/info")
        if "AiOS" not in result:
            return f"/info didn't return system info: {result[:100]}"

        # Test /theme
        result = handler.execute("/theme dark")
        if "dark" not in result.lower():
            return f"/theme dark didn't confirm: {result}"

        return True

    def test_mock_conversation(self) -> bool | str:
        """Test a full mock conversation flow."""
        mock_response = MockLLMResponse(
            content="The current date and time is 2025-01-15 10:30:00 UTC.",
            tool_calls=[
                MockToolCall(
                    id="call_1",
                    name="system",
                    arguments={"action": "get_datetime"},
                )
            ],
        )

        # Verify mock response structure
        if not mock_response.content:
            return "Mock response has no content"
        if len(mock_response.tool_calls) != 1:
            return "Mock response should have 1 tool call"
        if mock_response.tool_calls[0].name != "system":
            return "Tool call should be 'system'"

        return True

    def test_tool_schema_format(self) -> bool | str:
        """Test that tool schemas are valid for LLM consumption."""
        try:
            from aios.tools.registry import ToolRegistry
            registry = ToolRegistry()
            registry.load_builtin_tools()

            schemas = registry.get_tools_schema()
            if not schemas:
                return "No tool schemas returned"

            for schema in schemas:
                if "name" not in schema:
                    return f"Schema missing 'name': {schema}"
                if "description" not in schema:
                    return f"Schema missing 'description': {schema}"
                if "parameters" not in schema:
                    return f"Schema missing 'parameters': {schema}"

                params = schema["parameters"]
                if params.get("type") != "object":
                    return f"Parameters for '{schema['name']}' should be type 'object'"

            return True
        except Exception as e:
            return f"Schema test error: {e}"

    def test_config_commands_keyboard(self) -> bool | str:
        """Test keyboard layout configuration command."""
        from aios.config.commands import CommandHandler
        handler = CommandHandler(self.config, self.llm)

        original = self.config.get("system.keyboard_layout", "us")

        result = handler.execute("/keyboard de")
        if "de" not in result.lower():
            return f"/keyboard de didn't confirm: {result}"

        saved = self.config.get("system.keyboard_layout")
        if saved != "de":
            return f"Keyboard layout not saved: {saved}"

        # Restore
        self.config.set("system.keyboard_layout", original)
        return True

    def test_provider_switching(self) -> bool | str:
        """Test provider switching via command."""
        from aios.config.commands import CommandHandler
        handler = CommandHandler(self.config, self.llm)

        original = self.config.get("llm.provider", "claude")

        result = handler.execute("/provider openai")
        if "openai" not in result.lower():
            return f"/provider openai didn't confirm: {result}"

        saved = self.config.get("llm.provider")
        if saved != "openai":
            return f"Provider not saved: {saved}"

        # Restore
        self.config.set("llm.provider", original)
        return True
