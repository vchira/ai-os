"""OpenAI LLM provider for AiOS."""

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

_DEFAULT_MODEL = "gpt-4o"
_DEFAULT_MAX_TOKENS = 4096


class OpenAIProvider(LLMProvider):
    """LLM provider backed by the OpenAI Chat Completions API.

    Args:
        api_key: OpenAI API key.
        model: Model identifier.  Defaults to ``gpt-4o``.
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
        self._client: Any | None = None

    # -- Public configuration --------------------------------------------

    @property
    def name(self) -> str:  # noqa: D401
        return "openai"

    @property
    def api_key(self) -> str | None:
        return self._api_key

    @api_key.setter
    def api_key(self, value: str) -> None:
        self._api_key = value
        self._client = None

    @property
    def model(self) -> str:
        return self._model

    @model.setter
    def model(self, value: str) -> None:
        self._model = value

    # -- Private helpers --------------------------------------------------

    def _get_client(self) -> Any:
        """Return (and lazily create) the ``openai.OpenAI`` client."""
        if self._client is None:
            try:
                import openai  # type: ignore[import-untyped]
            except ImportError as exc:
                raise RuntimeError(
                    "The 'openai' package is required for the OpenAI "
                    "provider.  Install it with:  pip install openai"
                ) from exc

            if not self._api_key:
                raise ValueError(
                    "No OpenAI API key configured.  Set one with "
                    "manager.set_api_key('openai', '<key>') or pass it "
                    "to OpenAIProvider(api_key=...)."
                )
            self._client = openai.OpenAI(api_key=self._api_key)
        return self._client

    # -- Format conversion ------------------------------------------------

    @staticmethod
    def _convert_tools(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Convert internal AiOS tool defs to OpenAI function-calling format.

        Internal format::

            {"name": "...", "description": "...", "parameters": {JSON Schema}}

        OpenAI format::

            {"type": "function", "function": {"name": "...", "description": "...", "parameters": {JSON Schema}}}
        """
        converted: list[dict[str, Any]] = []
        for tool in tools:
            converted.append(
                {
                    "type": "function",
                    "function": {
                        "name": tool["name"],
                        "description": tool.get("description", ""),
                        "parameters": tool.get(
                            "parameters",
                            {"type": "object", "properties": {}},
                        ),
                    },
                }
            )
        return converted

    @staticmethod
    def _convert_messages(
        messages: list[Message],
    ) -> list[dict[str, Any]]:
        """Translate internal :class:`Message` list to the OpenAI wire format."""
        out: list[dict[str, Any]] = []
        for msg in messages:
            if msg.role == Role.SYSTEM:
                out.append({"role": "system", "content": msg.content or ""})
                continue

            if msg.role == Role.TOOL:
                out.append(
                    {
                        "role": "tool",
                        "tool_call_id": msg.tool_call_id or "",
                        "content": msg.content or "",
                    }
                )
                continue

            if msg.role == Role.ASSISTANT and msg.tool_calls:
                # Build the assistant message with tool_calls array.
                api_tool_calls: list[dict[str, Any]] = []
                for tc in msg.tool_calls:
                    api_tool_calls.append(
                        {
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": json.dumps(tc.arguments),
                            },
                        }
                    )
                entry: dict[str, Any] = {
                    "role": "assistant",
                    "tool_calls": api_tool_calls,
                }
                if msg.content:
                    entry["content"] = msg.content
                out.append(entry)
                continue

            # Plain user or assistant text.
            out.append({"role": msg.role.value, "content": msg.content or ""})

        return out

    def _parse_response(self, response: Any) -> LLMResponse:
        """Parse an OpenAI ``ChatCompletion`` into :class:`LLMResponse`."""
        choice = response.choices[0]
        message = choice.message

        tool_calls: list[ToolCall] = []
        if message.tool_calls:
            for tc in message.tool_calls:
                tool_calls.append(
                    ToolCall(
                        id=tc.id,
                        name=tc.function.name,
                        arguments=self._safe_parse_arguments(tc.function.arguments),
                    )
                )

        usage_obj = getattr(response, "usage", None)
        usage = Usage(
            input_tokens=getattr(usage_obj, "prompt_tokens", 0) if usage_obj else 0,
            output_tokens=getattr(usage_obj, "completion_tokens", 0) if usage_obj else 0,
        )

        return LLMResponse(
            content=message.content,
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

        # Prepend system prompt as the first message if provided.
        api_messages = self._convert_messages(messages)
        if system_prompt:
            api_messages.insert(0, {"role": "system", "content": system_prompt})

        kwargs: dict[str, Any] = {
            "model": self._model,
            "max_tokens": self._max_tokens,
            "messages": api_messages,
        }
        if tools:
            kwargs["tools"] = self._convert_tools(tools)

        try:
            response = client.chat.completions.create(**kwargs)
        except Exception as exc:
            log.error("OpenAI API request failed: %s", exc)
            raise

        return self._parse_response(response)

    def stream_message(
        self,
        messages: list[Message],
        tools: list[dict[str, Any]] | None = None,
        system_prompt: str | None = None,
    ) -> Iterator[StreamChunk]:
        client = self._get_client()

        api_messages = self._convert_messages(messages)
        if system_prompt:
            api_messages.insert(0, {"role": "system", "content": system_prompt})

        kwargs: dict[str, Any] = {
            "model": self._model,
            "max_tokens": self._max_tokens,
            "messages": api_messages,
            "stream": True,
            "stream_options": {"include_usage": True},
        }
        if tools:
            kwargs["tools"] = self._convert_tools(tools)

        # Accumulators for in-flight tool calls keyed by index.
        pending_calls: dict[int, dict[str, Any]] = {}
        input_tokens = 0
        output_tokens = 0

        try:
            response_stream = client.chat.completions.create(**kwargs)

            for chunk in response_stream:
                # Usage may appear in the final chunk.
                if chunk.usage:
                    input_tokens = getattr(chunk.usage, "prompt_tokens", 0)
                    output_tokens = getattr(chunk.usage, "completion_tokens", 0)

                if not chunk.choices:
                    continue

                delta = chunk.choices[0].delta
                finish_reason = chunk.choices[0].finish_reason

                # --- Text delta ----------------------------------------
                if delta.content:
                    yield StreamChunk(text=delta.content)

                # --- Tool-call deltas ----------------------------------
                if delta.tool_calls:
                    for tc_delta in delta.tool_calls:
                        idx = tc_delta.index
                        if idx not in pending_calls:
                            pending_calls[idx] = {
                                "id": "",
                                "name": "",
                                "arguments_parts": [],
                            }
                        entry = pending_calls[idx]
                        if tc_delta.id:
                            entry["id"] = tc_delta.id
                        if tc_delta.function:
                            if tc_delta.function.name:
                                entry["name"] = tc_delta.function.name
                            if tc_delta.function.arguments:
                                entry["arguments_parts"].append(
                                    tc_delta.function.arguments
                                )

                # --- Finish: flush accumulated tool calls ---------------
                if finish_reason is not None:
                    for _idx in sorted(pending_calls):
                        entry = pending_calls[_idx]
                        raw_args = "".join(entry["arguments_parts"]) or "{}"
                        args = self._safe_parse_arguments(raw_args)
                        yield StreamChunk(
                            tool_call=ToolCall(
                                id=entry["id"],
                                name=entry["name"],
                                arguments=args,
                            )
                        )
                    pending_calls.clear()

            # Final "done" chunk.
            yield StreamChunk(
                done=True,
                usage=Usage(input_tokens=input_tokens, output_tokens=output_tokens),
            )

        except Exception as exc:
            log.error("OpenAI streaming request failed: %s", exc)
            raise
