"""Claude (Anthropic) LLM provider for AiOS."""

from __future__ import annotations

import json
import logging
from typing import Any, Iterator

from .base import (
    LLMProvider,
    LLMResponse,
    Message,
    Role,
    StreamChunk,
    ToolCall,
    Usage,
)

log = logging.getLogger(__name__)

_DEFAULT_MODEL = "claude-sonnet-4-20250514"
_DEFAULT_MAX_TOKENS = 8192


class ClaudeProvider(LLMProvider):
    """LLM provider backed by the Anthropic Claude API.

    Args:
        api_key: Anthropic API key.  Can be updated later via
            :pyattr:`api_key`.
        model: Model identifier.  Defaults to ``claude-sonnet-4-20250514``.
        max_tokens: Maximum tokens in the response.
    """

    def __init__(
        self,
        api_key: str | None = None,
        model: str = _DEFAULT_MODEL,
        max_tokens: int = _DEFAULT_MAX_TOKENS,
    ) -> None:
        self._api_key = api_key
        self._model = model
        self._max_tokens = max_tokens
        self._client: Any | None = None  # lazy-initialised anthropic.Anthropic

    # -- Public configuration --------------------------------------------

    @property
    def name(self) -> str:  # noqa: D401
        return "claude"

    @property
    def api_key(self) -> str | None:
        return self._api_key

    @api_key.setter
    def api_key(self, value: str) -> None:
        self._api_key = value
        self._client = None  # force re-creation on next call

    @property
    def model(self) -> str:
        return self._model

    @model.setter
    def model(self, value: str) -> None:
        self._model = value

    # -- Private helpers --------------------------------------------------

    def _get_client(self) -> Any:
        """Return (and lazily create) the ``anthropic.Anthropic`` client."""
        if self._client is None:
            try:
                import anthropic  # type: ignore[import-untyped]
            except ImportError as exc:
                raise RuntimeError(
                    "The 'anthropic' package is required for the Claude "
                    "provider.  Install it with:  pip install anthropic"
                ) from exc

            if not self._api_key:
                raise ValueError(
                    "No Anthropic API key configured.  Set one with "
                    "manager.set_api_key('claude', '<key>') or pass it "
                    "to ClaudeProvider(api_key=...)."
                )
            self._client = anthropic.Anthropic(api_key=self._api_key)
        return self._client

    # -- Format conversion ------------------------------------------------

    @staticmethod
    def _convert_tools(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Convert internal AiOS tool defs to Claude ``tool_use`` format.

        Internal format::

            {"name": "...", "description": "...", "parameters": {JSON Schema}}

        Claude format::

            {"name": "...", "description": "...", "input_schema": {JSON Schema}}
        """
        converted: list[dict[str, Any]] = []
        for tool in tools:
            converted.append(
                {
                    "name": tool["name"],
                    "description": tool.get("description", ""),
                    "input_schema": tool.get("parameters", {"type": "object", "properties": {}}),
                }
            )
        return converted

    @staticmethod
    def _convert_messages(
        messages: list[Message],
    ) -> list[dict[str, Any]]:
        """Translate internal :class:`Message` list to the Anthropic wire
        format.

        The Anthropic API does **not** accept ``role="system"`` in the
        messages array (the system prompt is a separate top-level
        parameter), so system messages are silently dropped here.  The
        caller is responsible for extracting them.

        Tool-result messages are sent as ``role="user"`` with a
        ``tool_result`` content block, matching the Anthropic spec.
        """
        out: list[dict[str, Any]] = []
        for msg in messages:
            if msg.role == Role.SYSTEM:
                continue  # handled separately

            if msg.role == Role.TOOL:
                # Anthropic expects tool results inside a user turn.
                out.append(
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "tool_result",
                                "tool_use_id": msg.tool_call_id,
                                "content": msg.content or "",
                            }
                        ],
                    }
                )
                continue

            if msg.role == Role.ASSISTANT and msg.tool_calls:
                # Build mixed content: optional text + tool_use blocks.
                content: list[dict[str, Any]] = []
                if msg.content:
                    content.append({"type": "text", "text": msg.content})
                for tc in msg.tool_calls:
                    content.append(
                        {
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments,
                        }
                    )
                out.append({"role": "assistant", "content": content})
                continue

            # Plain user or assistant text message.
            out.append({"role": msg.role.value, "content": msg.content or ""})

        return out

    def _parse_response(self, response: Any) -> LLMResponse:
        """Parse an ``anthropic.types.Message`` into :class:`LLMResponse`."""
        text_parts: list[str] = []
        tool_calls: list[ToolCall] = []

        for block in response.content:
            if block.type == "text":
                text_parts.append(block.text)
            elif block.type == "tool_use":
                tool_calls.append(
                    ToolCall(
                        id=block.id,
                        name=block.name,
                        arguments=self._safe_parse_arguments(block.input),
                    )
                )

        usage = Usage(
            input_tokens=getattr(response.usage, "input_tokens", 0),
            output_tokens=getattr(response.usage, "output_tokens", 0),
        )

        return LLMResponse(
            content="\n".join(text_parts) if text_parts else None,
            tool_calls=tool_calls,
            usage=usage,
        )

    # -- LLMProvider interface --------------------------------------------

    def send_message(
        self,
        messages: list[Message],
        tools: list[dict[str, Any]] | None = None,
        system_prompt: str | None = None,
    ) -> LLMResponse:
        client = self._get_client()

        kwargs: dict[str, Any] = {
            "model": self._model,
            "max_tokens": self._max_tokens,
            "messages": self._convert_messages(messages),
        }
        if system_prompt:
            kwargs["system"] = system_prompt
        if tools:
            kwargs["tools"] = self._convert_tools(tools)

        try:
            response = client.messages.create(**kwargs)
        except Exception as exc:
            log.error("Claude API request failed: %s", exc)
            raise

        return self._parse_response(response)

    def stream_message(
        self,
        messages: list[Message],
        tools: list[dict[str, Any]] | None = None,
        system_prompt: str | None = None,
    ) -> Iterator[StreamChunk]:
        client = self._get_client()

        kwargs: dict[str, Any] = {
            "model": self._model,
            "max_tokens": self._max_tokens,
            "messages": self._convert_messages(messages),
        }
        if system_prompt:
            kwargs["system"] = system_prompt
        if tools:
            kwargs["tools"] = self._convert_tools(tools)

        # Accumulators for the in-progress tool_use block.
        current_tool_id: str | None = None
        current_tool_name: str | None = None
        current_tool_json: list[str] = []

        input_tokens = 0
        output_tokens = 0

        try:
            with client.messages.stream(**kwargs) as stream:
                for event in stream:
                    event_type = getattr(event, "type", None)

                    # --- message_start: grab initial usage -----------------
                    if event_type == "message_start":
                        msg = getattr(event, "message", None)
                        if msg and hasattr(msg, "usage"):
                            input_tokens = getattr(msg.usage, "input_tokens", 0)
                        continue

                    # --- content_block_start: new text or tool_use ---------
                    if event_type == "content_block_start":
                        block = getattr(event, "content_block", None)
                        if block and getattr(block, "type", None) == "tool_use":
                            current_tool_id = block.id
                            current_tool_name = block.name
                            current_tool_json = []
                        continue

                    # --- content_block_delta: incremental content ----------
                    if event_type == "content_block_delta":
                        delta = getattr(event, "delta", None)
                        if delta is None:
                            continue

                        delta_type = getattr(delta, "type", None)
                        if delta_type == "text_delta":
                            yield StreamChunk(text=delta.text)
                        elif delta_type == "input_json_delta":
                            current_tool_json.append(delta.partial_json)
                        continue

                    # --- content_block_stop: flush tool call ---------------
                    if event_type == "content_block_stop":
                        if current_tool_id is not None:
                            raw = "".join(current_tool_json) or "{}"
                            args = self._safe_parse_arguments(raw)
                            yield StreamChunk(
                                tool_call=ToolCall(
                                    id=current_tool_id,
                                    name=current_tool_name or "",
                                    arguments=args,
                                )
                            )
                            current_tool_id = None
                            current_tool_name = None
                            current_tool_json = []
                        continue

                    # --- message_delta: final usage info -------------------
                    if event_type == "message_delta":
                        u = getattr(event, "usage", None)
                        if u:
                            output_tokens = getattr(u, "output_tokens", 0)
                        continue

            # Final chunk signals stream end.
            yield StreamChunk(
                done=True,
                usage=Usage(input_tokens=input_tokens, output_tokens=output_tokens),
            )

        except Exception as exc:
            log.error("Claude streaming request failed: %s", exc)
            raise
