"""Base LLM provider interface and shared data types for AiOS."""

from __future__ import annotations

import json
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Iterator


class Role(str, Enum):
    """Message roles in a conversation."""

    SYSTEM = "system"
    USER = "user"
    ASSISTANT = "assistant"
    TOOL = "tool"


@dataclass
class ToolCall:
    """A tool invocation requested by the LLM.

    Attributes:
        id: Provider-assigned identifier for correlating tool results.
        name: Name of the tool to execute.
        arguments: Parsed argument dictionary for the tool.
    """

    id: str
    name: str
    arguments: dict[str, Any]


@dataclass
class Usage:
    """Token usage statistics for a single LLM request.

    Attributes:
        input_tokens: Tokens consumed by the prompt.
        output_tokens: Tokens produced in the response.
    """

    input_tokens: int = 0
    output_tokens: int = 0

    @property
    def total_tokens(self) -> int:
        return self.input_tokens + self.output_tokens


@dataclass
class Message:
    """A single message in a conversation.

    Attributes:
        role: One of system / user / assistant / tool.
        content: Text content of the message.  May be ``None`` when the
            assistant message consists entirely of tool calls.
        tool_call_id: For tool-result messages, the id of the originating
            :class:`ToolCall`.
        tool_calls: For assistant messages, any tool calls the model wants
            to make.
    """

    role: Role
    content: str | None = None
    tool_call_id: str | None = None
    tool_calls: list[ToolCall] = field(default_factory=list)

    # Convenience constructors -------------------------------------------

    @classmethod
    def user(cls, text: str) -> Message:
        return cls(role=Role.USER, content=text)

    @classmethod
    def assistant(
        cls,
        text: str | None = None,
        tool_calls: list[ToolCall] | None = None,
    ) -> Message:
        return cls(
            role=Role.ASSISTANT,
            content=text,
            tool_calls=tool_calls or [],
        )

    @classmethod
    def tool_result(cls, tool_call_id: str, content: str) -> Message:
        return cls(role=Role.TOOL, content=content, tool_call_id=tool_call_id)

    @classmethod
    def system(cls, text: str) -> Message:
        return cls(role=Role.SYSTEM, content=text)


@dataclass
class LLMResponse:
    """Complete (non-streaming) response from an LLM provider.

    Attributes:
        content: The text portion of the response, if any.
        tool_calls: Tool invocations the model wants to make.
        usage: Token counts for billing / diagnostics.
    """

    content: str | None = None
    tool_calls: list[ToolCall] = field(default_factory=list)
    usage: Usage = field(default_factory=Usage)

    @property
    def has_tool_calls(self) -> bool:
        return len(self.tool_calls) > 0


@dataclass
class StreamChunk:
    """A single piece of a streaming LLM response.

    Exactly one of *text* or *tool_call* is set per chunk.  When *done*
    is ``True`` it signals the end of the stream.

    Attributes:
        text: Incremental text delta (may be empty string for heartbeats).
        tool_call: A completed tool-call object once the provider has
            finished emitting it.
        done: Whether this chunk marks the end of the stream.
        usage: Final token usage, typically only present on the last chunk.
    """

    text: str | None = None
    tool_call: ToolCall | None = None
    done: bool = False
    usage: Usage | None = None


class LLMProvider(ABC):
    """Abstract base class every LLM provider must implement.

    Subclasses supply a concrete connection to a specific LLM API
    (Claude, OpenAI, etc.) and translate between the internal
    :class:`Message`/:class:`ToolCall` types and the wire format.
    """

    @property
    @abstractmethod
    def name(self) -> str:
        """Short, unique identifier for this provider (e.g. ``'claude'``)."""
        ...

    @abstractmethod
    def send_message(
        self,
        messages: list[Message],
        tools: list[dict[str, Any]] | None = None,
        system_prompt: str | None = None,
    ) -> LLMResponse:
        """Send a non-streaming request and block until the full response.

        Args:
            messages: Conversation history.
            tools: Tool definitions in the *internal* AiOS format::

                {
                    "name": "memory_store",
                    "description": "Persist a key-value pair.",
                    "parameters": {          # JSON-Schema object
                        "type": "object",
                        "properties": { ... },
                        "required": [ ... ],
                    },
                }

            system_prompt: Optional system-level instruction prepended to
                the conversation.

        Returns:
            A fully populated :class:`LLMResponse`.
        """
        ...

    @abstractmethod
    def stream_message(
        self,
        messages: list[Message],
        tools: list[dict[str, Any]] | None = None,
        system_prompt: str | None = None,
    ) -> Iterator[StreamChunk]:
        """Stream a response, yielding :class:`StreamChunk` objects.

        The final chunk has ``done=True`` and may include ``usage``.
        """
        ...

    # Shared helpers used by concrete providers --------------------------

    @staticmethod
    def _safe_parse_arguments(raw: str | dict[str, Any]) -> dict[str, Any]:
        """Parse tool-call arguments that may arrive as a JSON string."""
        if isinstance(raw, dict):
            return raw
        try:
            return json.loads(raw)  # type: ignore[arg-type]
        except (json.JSONDecodeError, TypeError):
            return {"raw": raw}
