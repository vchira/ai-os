"""LLM provider manager with automatic tool-call loop for AiOS."""

from __future__ import annotations

import logging
from datetime import datetime, timezone
from typing import Any, Callable

from .base import (
    LLMProvider,
    LLMResponse,
    Message,
    Role,
    ToolCall,
    Usage,
)

log = logging.getLogger(__name__)

# Safety limit so a misbehaving model cannot loop forever.
_MAX_TOOL_ROUNDS = 25

ToolExecutor = Callable[[str, dict[str, Any]], str]
"""Signature for the tool executor callback.

Receives ``(tool_name, arguments)`` and returns a string result that
is fed back to the model as a tool-result message.
"""


class LLMManager:
    """Manages registered LLM providers and drives the tool-call loop.

    Typical usage::

        manager = LLMManager()
        manager.register_provider(ClaudeProvider())
        manager.register_provider(OpenAIProvider())
        manager.set_api_key("claude", "sk-ant-...")
        manager.set_active("claude")
        manager.tool_executor = my_tool_executor
        response = manager.chat("What time is it?", tools=my_tools)

    The :meth:`chat` method implements the **full tool-call loop**: it
    sends the user message, inspects the response for tool calls,
    executes them via :attr:`tool_executor`, feeds the results back,
    and repeats until the model produces a final text answer (or the
    safety limit is reached).
    """

    def __init__(self) -> None:
        self._providers: dict[str, LLMProvider] = {}
        self._active_name: str | None = None
        self._tool_executor: ToolExecutor | None = None
        self._max_tool_rounds: int = _MAX_TOOL_ROUNDS

    # -- Provider registry ------------------------------------------------

    def register_provider(self, provider: LLMProvider) -> None:
        """Register (or replace) a provider by its :pyattr:`name`."""
        self._providers[provider.name] = provider
        log.info("Registered LLM provider: %s", provider.name)
        # Auto-select the first provider registered.
        if self._active_name is None:
            self._active_name = provider.name

    def set_active(self, name: str) -> None:
        """Switch the active provider.

        Raises:
            KeyError: If *name* has not been registered.
        """
        if name not in self._providers:
            available = ", ".join(sorted(self._providers)) or "(none)"
            raise KeyError(
                f"Unknown provider '{name}'.  Registered: {available}"
            )
        self._active_name = name
        log.info("Active LLM provider set to: %s", name)

    @property
    def active_provider(self) -> LLMProvider:
        """The currently active provider.

        Raises:
            RuntimeError: If no provider has been registered / selected.
        """
        if self._active_name is None or self._active_name not in self._providers:
            raise RuntimeError(
                "No active LLM provider.  Register at least one provider "
                "with register_provider() and then call set_active()."
            )
        return self._providers[self._active_name]

    @property
    def providers(self) -> dict[str, LLMProvider]:
        """Read-only view of registered providers keyed by name."""
        return dict(self._providers)

    # -- API key management -----------------------------------------------

    def set_api_key(self, provider_name: str, key: str) -> None:
        """Set the API key for *provider_name* at runtime.

        The key is written through to the provider's ``api_key`` property.
        If the provider has not been registered yet a :class:`KeyError`
        is raised.
        """
        if provider_name not in self._providers:
            available = ", ".join(sorted(self._providers)) or "(none)"
            raise KeyError(
                f"Unknown provider '{provider_name}'.  Registered: {available}"
            )
        provider = self._providers[provider_name]
        if not hasattr(provider, "api_key"):
            raise AttributeError(
                f"Provider '{provider_name}' does not expose a settable "
                "'api_key' attribute."
            )
        provider.api_key = key  # type: ignore[attr-defined]
        log.info("API key updated for provider: %s", provider_name)

    # -- Tool executor callback -------------------------------------------

    @property
    def tool_executor(self) -> ToolExecutor | None:
        return self._tool_executor

    @tool_executor.setter
    def tool_executor(self, callback: ToolExecutor | None) -> None:
        self._tool_executor = callback

    # -- System prompt builder --------------------------------------------

    @staticmethod
    def get_system_prompt(
        context: dict[str, Any] | None = None,
        available_tools: list[dict[str, Any]] | None = None,
    ) -> str:
        """Build the system prompt injected at the start of every request.

        The prompt includes the current UTC date/time, a summary of
        available tools (if any), and any extra *context* key-value
        pairs the caller wants to surface.

        Args:
            context: Arbitrary key-value pairs merged into the prompt
                (e.g. ``{"hardware": "x86 + RTL8139 NIC"}``).
            available_tools: Tool definitions list; only the names and
                descriptions are included in the prompt body (the full
                schemas are sent separately via the API's tool parameter).

        Returns:
            The assembled system-prompt string.
        """
        now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")

        parts: list[str] = [
            "You are AiOS, an AI-native operating system.  You are the "
            "primary actor: humans express intent through the prompt and "
            "you decide how to fulfill it.",
            f"Current date/time: {now}",
        ]

        if available_tools:
            tool_lines = []
            for t in available_tools:
                desc = t.get("description", "")
                tool_lines.append(f"  - {t['name']}: {desc}")
            parts.append("Available tools:\n" + "\n".join(tool_lines))

        if context:
            ctx_lines = [f"  {k}: {v}" for k, v in context.items()]
            parts.append("System context:\n" + "\n".join(ctx_lines))

        return "\n\n".join(parts)

    # -- Core chat loop ---------------------------------------------------

    def chat(
        self,
        user_message: str,
        conversation_history: list[Message] | None = None,
        tools: list[dict[str, Any]] | None = None,
        system_prompt: str | None = None,
    ) -> LLMResponse:
        """Send a user message and run the tool-call loop to completion.

        1. Append the user message to *conversation_history*.
        2. Call the active provider's :meth:`send_message`.
        3. If the response contains tool calls **and** :attr:`tool_executor`
           is set, execute each tool, append the results, and go to step 2.
        4. Return the final :class:`LLMResponse` (which has no outstanding
           tool calls).

        Args:
            user_message: The human's input.
            conversation_history: Mutable list that is updated in-place.
                Pass ``None`` (or omit) to start a fresh conversation --
                a new list will be created internally.
            tools: Tool definitions in internal AiOS format.
            system_prompt: Explicit system prompt.  If ``None`` a default
                is generated via :meth:`get_system_prompt`.

        Returns:
            The model's final text response (with accumulated usage).
        """
        provider = self.active_provider

        if conversation_history is None:
            conversation_history = []

        conversation_history.append(Message.user(user_message))

        if system_prompt is None:
            system_prompt = self.get_system_prompt(available_tools=tools)

        total_usage = Usage()
        rounds = 0

        while True:
            response = provider.send_message(
                messages=conversation_history,
                tools=tools,
                system_prompt=system_prompt,
            )

            # Accumulate token usage across rounds.
            total_usage.input_tokens += response.usage.input_tokens
            total_usage.output_tokens += response.usage.output_tokens

            if not response.has_tool_calls:
                # No tool calls -- we have the final answer.
                conversation_history.append(
                    Message.assistant(text=response.content)
                )
                response.usage = total_usage
                return response

            # --- Tool-call round ----------------------------------------
            rounds += 1
            if rounds > self._max_tool_rounds:
                log.warning(
                    "Tool-call loop exceeded %d rounds; returning last "
                    "response as-is.",
                    self._max_tool_rounds,
                )
                conversation_history.append(
                    Message.assistant(
                        text=response.content,
                        tool_calls=response.tool_calls,
                    )
                )
                response.usage = total_usage
                return response

            if self._tool_executor is None:
                log.warning(
                    "Model requested tool calls but no tool_executor is "
                    "configured.  Returning response with pending tool calls."
                )
                conversation_history.append(
                    Message.assistant(
                        text=response.content,
                        tool_calls=response.tool_calls,
                    )
                )
                response.usage = total_usage
                return response

            # Record assistant message with its tool calls.
            conversation_history.append(
                Message.assistant(
                    text=response.content,
                    tool_calls=response.tool_calls,
                )
            )

            # Execute each tool and feed results back.
            for tc in response.tool_calls:
                try:
                    result = self._tool_executor(tc.name, tc.arguments)
                except Exception as exc:
                    log.error(
                        "Tool '%s' raised an exception: %s", tc.name, exc
                    )
                    result = f"Error executing tool '{tc.name}': {exc}"

                conversation_history.append(
                    Message.tool_result(tool_call_id=tc.id, content=result)
                )

            # Loop back to send the updated conversation.
