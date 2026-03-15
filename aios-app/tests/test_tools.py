"""Tests for the tool system."""

import json
import tempfile
from pathlib import Path

import pytest


class TestToolBase:
    def test_tool_result_success(self):
        from aios.tools.base import ToolResult

        r = ToolResult(success=True, output="hello")
        assert r.success
        assert r.output == "hello"
        assert r.error is None

    def test_tool_result_failure(self):
        from aios.tools.base import ToolResult

        r = ToolResult(success=False, output="", error="something broke")
        assert not r.success
        assert r.error == "something broke"


class TestMemoryTool:
    def test_memorize_and_recall(self, tmp_path):
        from aios.tools.builtin.memory import MemoryTool

        tool = MemoryTool(storage_path=tmp_path / "memory.json")

        result = tool.execute(action="memorize", key="test_key", value="test_value")
        assert result.success

        result = tool.execute(action="recall", key="test_key")
        assert result.success
        assert "test_value" in result.output

    def test_forget(self, tmp_path):
        from aios.tools.builtin.memory import MemoryTool

        tool = MemoryTool(storage_path=tmp_path / "memory.json")

        tool.execute(action="memorize", key="k", value="v")
        result = tool.execute(action="forget", key="k")
        assert result.success

        result = tool.execute(action="recall", key="k")
        assert "not found" in result.output.lower() or not result.success

    def test_list_keys(self, tmp_path):
        from aios.tools.builtin.memory import MemoryTool

        tool = MemoryTool(storage_path=tmp_path / "memory.json")

        tool.execute(action="memorize", key="a", value="1")
        tool.execute(action="memorize", key="b", value="2")

        result = tool.execute(action="list_keys")
        assert result.success
        assert "a" in result.output
        assert "b" in result.output


class TestSystemTool:
    def test_get_datetime(self):
        from aios.tools.builtin.system_tools import SystemTool

        tool = SystemTool()
        result = tool.execute(action="get_datetime")
        assert result.success
        assert len(result.output) > 0

    def test_get_system_info(self):
        from aios.tools.builtin.system_tools import SystemTool

        tool = SystemTool()
        result = tool.execute(action="get_system_info")
        assert result.success
        assert len(result.output) > 0


class TestFilesTool:
    def test_read_file(self, tmp_path, monkeypatch):
        from aios.tools.builtin import files as files_mod
        from aios.tools.builtin.files import FilesTool

        # Patch default root to tmp_path so the safety check passes
        monkeypatch.setattr(files_mod, "_DEFAULT_ROOT", tmp_path)

        test_file = tmp_path / "test.txt"
        test_file.write_text("hello world")

        tool = FilesTool()
        result = tool.execute(action="read_file", path=str(test_file))
        assert result.success
        assert "hello world" in result.output

    def test_write_file(self, tmp_path, monkeypatch):
        from aios.tools.builtin import files as files_mod
        from aios.tools.builtin.files import FilesTool

        monkeypatch.setattr(files_mod, "_DEFAULT_ROOT", tmp_path)

        test_file = tmp_path / "output.txt"

        tool = FilesTool()
        result = tool.execute(action="write_file", path=str(test_file), content="test content")
        assert result.success
        assert test_file.read_text() == "test content"

    def test_list_directory(self, tmp_path, monkeypatch):
        from aios.tools.builtin import files as files_mod
        from aios.tools.builtin.files import FilesTool

        monkeypatch.setattr(files_mod, "_DEFAULT_ROOT", tmp_path)

        (tmp_path / "a.txt").touch()
        (tmp_path / "b.txt").touch()

        tool = FilesTool()
        result = tool.execute(action="list_directory", path=str(tmp_path))
        assert result.success
        assert "a.txt" in result.output
        assert "b.txt" in result.output


class TestToolRegistry:
    def test_load_builtin_tools(self):
        from aios.tools.registry import ToolRegistry

        registry = ToolRegistry()
        registry.load_builtin_tools()

        tools = registry.list_tools()
        assert len(tools) > 0

        names = [t.name for t in tools]
        assert "memory" in names
        assert "system" in names

    def test_get_tools_schema(self):
        from aios.tools.registry import ToolRegistry

        registry = ToolRegistry()
        registry.load_builtin_tools()

        schemas = registry.get_tools_schema()
        assert len(schemas) > 0

        for s in schemas:
            assert "name" in s
            assert "description" in s
            assert "parameters" in s


class TestSelfTest:
    def test_selftest_runner(self, tmp_path):
        from aios.config.manager import ConfigManager
        from aios.selftest.runner import SelfTestRunner

        config = ConfigManager(config_path=tmp_path / "config.json")
        runner = SelfTestRunner(config, None)
        report = runner.run_all()

        assert "AiOS Self-Test" in report
        assert "Results:" in report
