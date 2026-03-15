"""Memory tool -- persistent key-value store for the AI.

Data is stored as a single JSON file at ``~/.aios/memory.json``.
"""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

from aios.tools.base import Tool, ToolResult

logger = logging.getLogger(__name__)

_MEMORY_PATH = Path.home() / ".aios" / "memory.json"


class MemoryTool(Tool):
    """Key-value memory store that persists across sessions."""

    def __init__(self, storage_path: Path | None = None):
        self._storage_path = storage_path or _MEMORY_PATH

    @property
    def name(self) -> str:
        return "memory"

    @property
    def description(self) -> str:
        return (
            "Persistent key-value memory store. "
            "Memorize facts, recall them later, forget them, or list all keys."
        )

    @property
    def parameters(self) -> dict:
        return {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["memorize", "recall", "forget", "list_keys"],
                    "description": "The memory operation to perform.",
                },
                "key": {
                    "type": "string",
                    "description": "The key to memorize/recall/forget (not required for list_keys).",
                },
                "value": {
                    "type": "string",
                    "description": "The value to memorize (required for 'memorize' action).",
                },
            },
            "required": ["action"],
        }

    def execute(self, **kwargs: Any) -> ToolResult:
        action: str = kwargs.get("action", "")
        key: str | None = kwargs.get("key")
        value: str | None = kwargs.get("value")

        try:
            store = self._load()
        except Exception as exc:
            return ToolResult.fail(f"Failed to load memory store: {exc}")

        if action == "memorize":
            if not key:
                return ToolResult.fail("'key' is required for memorize.")
            if value is None:
                return ToolResult.fail("'value' is required for memorize.")
            store[key] = value
            self._save(store)
            return ToolResult.ok(f"Memorized key {key!r}.")

        if action == "recall":
            if not key:
                return ToolResult.fail("'key' is required for recall.")
            if key not in store:
                return ToolResult.fail(f"No memory found for key {key!r}.")
            return ToolResult.ok(store[key], data={"key": key, "value": store[key]})

        if action == "forget":
            if not key:
                return ToolResult.fail("'key' is required for forget.")
            if key not in store:
                return ToolResult.fail(f"No memory found for key {key!r}.")
            del store[key]
            self._save(store)
            return ToolResult.ok(f"Forgot key {key!r}.")

        if action == "list_keys":
            keys = sorted(store.keys())
            return ToolResult.ok(
                "\n".join(keys) if keys else "(no memories stored)",
                data={"keys": keys},
            )

        return ToolResult.fail(
            f"Unknown action {action!r}. Use: memorize, recall, forget, list_keys."
        )

    # -- Persistence helpers ----------------------------------------------------

    def _load(self) -> dict[str, str]:
        if not self._storage_path.exists():
            return {}
        text = self._storage_path.read_text(encoding="utf-8")
        if not text.strip():
            return {}
        return json.loads(text)

    def _save(self, store: dict[str, str]) -> None:
        self._storage_path.parent.mkdir(parents=True, exist_ok=True)
        self._storage_path.write_text(
            json.dumps(store, indent=2, ensure_ascii=False), encoding="utf-8"
        )
