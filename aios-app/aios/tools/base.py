"""Tool interface and result types for the AiOS tool plugin system."""

from __future__ import annotations

import abc
from dataclasses import dataclass, field
from typing import Any, Optional


@dataclass
class ToolResult:
    """Result returned by a tool execution.

    Attributes:
        success: Whether the tool executed successfully.
        output: Human-readable output text.
        data: Optional structured data (e.g. image paths, tables) for the UI layer.
        error: Error description when *success* is ``False``.
    """

    success: bool
    output: str
    data: Optional[dict[str, Any]] = field(default=None)
    error: Optional[str] = field(default=None)

    # Convenience constructors ---------------------------------------------------

    @classmethod
    def ok(cls, output: str, data: Optional[dict[str, Any]] = None) -> ToolResult:
        """Create a successful result."""
        return cls(success=True, output=output, data=data)

    @classmethod
    def fail(cls, error: str, output: str = "") -> ToolResult:
        """Create a failed result."""
        return cls(success=False, output=output, error=error)


class Tool(abc.ABC):
    """Abstract base class for all AiOS tools.

    Every tool must declare a *name*, *description*, and *parameters* JSON
    Schema, and implement :meth:`execute`.
    """

    @property
    @abc.abstractmethod
    def name(self) -> str:
        """Unique tool name (lowercase, underscores allowed)."""

    @property
    @abc.abstractmethod
    def description(self) -> str:
        """One-line human-readable description of what the tool does."""

    @property
    @abc.abstractmethod
    def parameters(self) -> dict:
        """JSON Schema describing the keyword arguments accepted by :meth:`execute`.

        Example::

            {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"}
                },
                "required": ["query"]
            }
        """

    @abc.abstractmethod
    def execute(self, **kwargs: Any) -> ToolResult:
        """Run the tool with the given parameters and return a :class:`ToolResult`."""

    # Helpers -------------------------------------------------------------------

    def to_schema(self) -> dict:
        """Return a dict suitable for LLM function-calling schemas."""
        return {
            "name": self.name,
            "description": self.description,
            "parameters": self.parameters,
        }

    def __repr__(self) -> str:
        return f"<Tool {self.name!r}>"
