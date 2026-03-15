"""Tests for the configuration manager."""

import json
import tempfile
from pathlib import Path

import pytest

from aios.config.manager import ConfigManager


@pytest.fixture
def tmp_config(tmp_path):
    config_file = tmp_path / "config.json"
    return ConfigManager(config_path=config_file)


class TestConfigManager:
    def test_default_values(self, tmp_config):
        assert tmp_config.get("llm.provider") == "claude"
        assert tmp_config.get("voice.stt_enabled") is True
        assert tmp_config.get("ui.theme") == "dark"

    def test_set_and_get(self, tmp_config):
        tmp_config.set("llm.provider", "openai")
        assert tmp_config.get("llm.provider") == "openai"

    def test_persistence(self, tmp_path):
        config_file = tmp_path / "config.json"

        mgr1 = ConfigManager(config_path=config_file)
        mgr1.set("llm.provider", "openai")

        mgr2 = ConfigManager(config_path=config_file)
        assert mgr2.get("llm.provider") == "openai"

    def test_nested_set(self, tmp_config):
        tmp_config.set("custom.nested.key", "value")
        assert tmp_config.get("custom.nested.key") == "value"

    def test_get_default(self, tmp_config):
        assert tmp_config.get("nonexistent.key", "fallback") == "fallback"

    def test_reset_key(self, tmp_config):
        tmp_config.set("llm.provider", "openai")
        tmp_config.reset("llm.provider")
        assert tmp_config.get("llm.provider") == "claude"

    def test_reset_all(self, tmp_config):
        tmp_config.set("llm.provider", "openai")
        tmp_config.set("ui.theme", "light")
        tmp_config.reset()
        assert tmp_config.get("llm.provider") == "claude"
        assert tmp_config.get("ui.theme") == "dark"

    def test_callback(self, tmp_config):
        results = []
        tmp_config.on_change("llm.provider", lambda v: results.append(v))
        tmp_config.set("llm.provider", "openai")
        assert results == ["openai"]

    def test_get_section(self, tmp_config):
        section = tmp_config.get_section("llm")
        assert isinstance(section, dict)
        assert "provider" in section
