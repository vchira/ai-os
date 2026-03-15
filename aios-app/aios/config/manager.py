"""Configuration manager — TOML-based persistent settings."""

import os
import json
from pathlib import Path
from typing import Any


CONFIG_DIR = Path.home() / ".aios"
CONFIG_FILE = CONFIG_DIR / "config.json"

DEFAULTS = {
    "llm": {
        "provider": "claude",
        "claude_api_key": "",
        "claude_model": "claude-sonnet-4-20250514",
        "openai_api_key": "",
        "openai_model": "gpt-4o",
        "extra_system_prompt": "",
        "max_tool_rounds": 10,
    },
    "voice": {
        "stt_enabled": True,
        "stt_model": "medium",
        "stt_language": "",
        "tts_enabled": True,
        "tts_voice": "en_US-amy-medium",
        "tts_gender": "female",
        "tts_rate": 1.0,
    },
    "ui": {
        "theme": "dark",
        "font_size": 14,
    },
    "system": {
        "keyboard_layout": "us",
        "keyboard_variant": "",
        "locale": "en_US.UTF-8",
        "timezone": "",
    },
    "tools": {
        "plugins_dir": str(CONFIG_DIR / "plugins"),
        "store_url": "https://store.aios.dev/api/v1",
    },
}


class ConfigManager:
    """Manages AiOS configuration with persistent storage."""

    def __init__(self, config_path: Path | None = None):
        self.config_path = config_path or CONFIG_FILE
        self.config_dir = self.config_path.parent
        self._config: dict = {}
        self._callbacks: dict[str, list] = {}
        self._load()

    def _load(self):
        """Load config from disk, merging with defaults."""
        import copy

        self.config_dir.mkdir(parents=True, exist_ok=True)

        if self.config_path.exists():
            try:
                with open(self.config_path) as f:
                    self._config = json.load(f)
            except (json.JSONDecodeError, IOError):
                self._config = {}

        # Merge defaults for any missing keys (deep copy to avoid mutating DEFAULTS)
        self._config = self._deep_merge(copy.deepcopy(DEFAULTS), self._config)

    def _save(self):
        """Persist config to disk."""
        self.config_dir.mkdir(parents=True, exist_ok=True)
        with open(self.config_path, "w") as f:
            json.dump(self._config, f, indent=2)

    def get(self, key: str, default: Any = None) -> Any:
        """Get a config value by dotted key path (e.g., 'voice.tts_voice')."""
        parts = key.split(".")
        value = self._config
        for part in parts:
            if isinstance(value, dict) and part in value:
                value = value[part]
            else:
                return default
        return value

    def set(self, key: str, value: Any):
        """Set a config value by dotted key path."""
        parts = key.split(".")
        config = self._config
        for part in parts[:-1]:
            if part not in config or not isinstance(config[part], dict):
                config[part] = {}
            config = config[part]
        old_value = config.get(parts[-1])
        config[parts[-1]] = value
        self._save()

        # Fire callbacks
        if key in self._callbacks and old_value != value:
            for cb in self._callbacks[key]:
                cb(value)

    def on_change(self, key: str, callback):
        """Register a callback for when a config key changes."""
        if key not in self._callbacks:
            self._callbacks[key] = []
        self._callbacks[key].append(callback)

    def get_section(self, section: str) -> dict:
        """Get an entire config section."""
        return self._config.get(section, {})

    def reset(self, key: str | None = None):
        """Reset a key or entire config to defaults."""
        if key is None:
            import copy
            self._config = copy.deepcopy(DEFAULTS)
            self._save()
        else:
            parts = key.split(".")
            default_val = DEFAULTS
            for part in parts:
                if isinstance(default_val, dict):
                    default_val = default_val.get(part)
                else:
                    default_val = None
                    break
            if default_val is not None:
                self.set(key, default_val)

    @staticmethod
    def _deep_merge(defaults: dict, overrides: dict) -> dict:
        """Deep merge overrides into defaults."""
        result = defaults.copy()
        for key, value in overrides.items():
            if key in result and isinstance(result[key], dict) and isinstance(value, dict):
                result[key] = ConfigManager._deep_merge(result[key], value)
            else:
                result[key] = value
        return result

    @property
    def data_dir(self) -> Path:
        """Get the AiOS data directory (~/.aios)."""
        self.config_dir.mkdir(parents=True, exist_ok=True)
        return self.config_dir

    @property
    def plugins_dir(self) -> Path:
        """Get the plugins directory."""
        d = Path(self.get("tools.plugins_dir", str(self.config_dir / "plugins")))
        d.mkdir(parents=True, exist_ok=True)
        return d

    @property
    def models_dir(self) -> Path:
        """Get the models directory."""
        d = self.config_dir / "models"
        d.mkdir(parents=True, exist_ok=True)
        return d
