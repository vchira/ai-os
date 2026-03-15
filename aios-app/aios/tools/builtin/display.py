"""Display tool -- render images, notifications, and markdown in the UI.

The display tool does not directly manipulate GTK widgets.  Instead it invokes
registered *callback* functions that the UI layer provides at startup.  This
keeps the tool layer decoupled from any particular frontend.
"""

from __future__ import annotations

import logging
from typing import Any, Callable, Optional

from aios.tools.base import Tool, ToolResult

logger = logging.getLogger(__name__)

# Type aliases for the callbacks the UI layer registers.
ShowImageCallback = Callable[[str], None]           # path_or_url
ShowNotificationCallback = Callable[[str, str], None]  # title, message
ShowMarkdownCallback = Callable[[str], None]        # markdown_text


class DisplayTool(Tool):
    """Display images, notifications, and rendered markdown via the host UI."""

    # Callbacks are set by the UI layer at startup.
    _show_image_cb: Optional[ShowImageCallback] = None
    _show_notification_cb: Optional[ShowNotificationCallback] = None
    _show_markdown_cb: Optional[ShowMarkdownCallback] = None

    # -- Callback registration (class-level so they survive re-instantiation) ---

    @classmethod
    def set_show_image_callback(cls, cb: ShowImageCallback) -> None:
        cls._show_image_cb = cb

    @classmethod
    def set_show_notification_callback(cls, cb: ShowNotificationCallback) -> None:
        cls._show_notification_cb = cb

    @classmethod
    def set_show_markdown_callback(cls, cb: ShowMarkdownCallback) -> None:
        cls._show_markdown_cb = cb

    # -- Tool interface ---------------------------------------------------------

    @property
    def name(self) -> str:
        return "display"

    @property
    def description(self) -> str:
        return (
            "Display content to the user: show an image, send a desktop "
            "notification, or render markdown text."
        )

    @property
    def parameters(self) -> dict:
        return {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["show_image", "show_notification", "show_markdown"],
                    "description": "Display action to perform.",
                },
                "path": {
                    "type": "string",
                    "description": "File path or URL of the image (for show_image).",
                },
                "title": {
                    "type": "string",
                    "description": "Notification title (for show_notification).",
                },
                "message": {
                    "type": "string",
                    "description": "Notification body text (for show_notification).",
                },
                "text": {
                    "type": "string",
                    "description": "Markdown text to render (for show_markdown).",
                },
            },
            "required": ["action"],
        }

    def execute(self, **kwargs: Any) -> ToolResult:
        action: str = kwargs.get("action", "")

        if action == "show_image":
            return self._show_image(kwargs.get("path", ""))
        if action == "show_notification":
            return self._show_notification(
                kwargs.get("title", "AiOS"),
                kwargs.get("message", ""),
            )
        if action == "show_markdown":
            return self._show_markdown(kwargs.get("text", ""))

        return ToolResult.fail(
            f"Unknown action {action!r}. Use: show_image, show_notification, show_markdown."
        )

    # -- Action implementations -------------------------------------------------

    def _show_image(self, path_or_url: str) -> ToolResult:
        if not path_or_url:
            return ToolResult.fail("'path' is required for show_image.")
        if self._show_image_cb is None:
            logger.warning("show_image callback not registered; returning path only")
            return ToolResult.ok(
                f"Image ready: {path_or_url}",
                data={"type": "image", "path": path_or_url},
            )
        try:
            self._show_image_cb(path_or_url)
            return ToolResult.ok(
                f"Displayed image: {path_or_url}",
                data={"type": "image", "path": path_or_url},
            )
        except Exception as exc:
            return ToolResult.fail(f"Failed to display image: {exc}")

    def _show_notification(self, title: str, message: str) -> ToolResult:
        if not message:
            return ToolResult.fail("'message' is required for show_notification.")
        if self._show_notification_cb is None:
            logger.warning("show_notification callback not registered; logging only")
            logger.info("NOTIFICATION [%s]: %s", title, message)
            return ToolResult.ok(
                f"Notification: [{title}] {message}",
                data={"type": "notification", "title": title, "message": message},
            )
        try:
            self._show_notification_cb(title, message)
            return ToolResult.ok(
                f"Notification sent: [{title}] {message}",
                data={"type": "notification", "title": title, "message": message},
            )
        except Exception as exc:
            return ToolResult.fail(f"Failed to show notification: {exc}")

    def _show_markdown(self, text: str) -> ToolResult:
        if not text:
            return ToolResult.fail("'text' is required for show_markdown.")
        if self._show_markdown_cb is None:
            logger.warning("show_markdown callback not registered; returning raw text")
            return ToolResult.ok(text, data={"type": "markdown", "text": text})
        try:
            self._show_markdown_cb(text)
            return ToolResult.ok(
                "Markdown rendered.",
                data={"type": "markdown", "text": text},
            )
        except Exception as exc:
            return ToolResult.fail(f"Failed to render markdown: {exc}")
