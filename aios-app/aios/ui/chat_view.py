"""Chat view widget — displays conversation messages."""

import gi

gi.require_version("Gtk", "4.0")

from gi.repository import Gtk, Pango, GLib, GdkPixbuf


class ChatView(Gtk.Box):
    """Scrollable chat display showing user, assistant, and tool messages."""

    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.set_margin_top(16)
        self.set_margin_bottom(8)
        self.messages = []

        # Welcome message
        self._add_welcome()

    def _add_welcome(self):
        welcome = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        welcome.set_halign(Gtk.Align.CENTER)
        welcome.set_valign(Gtk.Align.CENTER)
        welcome.set_margin_top(64)
        welcome.set_margin_bottom(32)

        title = Gtk.Label(label="AiOS")
        title.add_css_class("title-1")
        welcome.append(title)

        subtitle = Gtk.Label(label="AI-native operating system. Ask me anything.")
        subtitle.add_css_class("dim-label")
        welcome.append(subtitle)

        hint = Gtk.Label(label="Speak or type your request below. Use /help for commands.")
        hint.add_css_class("dim-label")
        hint.set_margin_top(8)
        welcome.append(hint)

        self.welcome_widget = welcome
        self.append(welcome)

    def add_message(self, role: str, content: str):
        """Add a message to the chat view.

        Args:
            role: One of 'user', 'assistant', 'system', 'tool'
            content: The message text
        """
        # Remove welcome on first real message
        if self.welcome_widget and role in ("user", "assistant"):
            self.remove(self.welcome_widget)
            self.welcome_widget = None

        msg_widget = self._create_message_widget(role, content)
        self.append(msg_widget)
        self.messages.append({"role": role, "content": content, "widget": msg_widget})

    def _create_message_widget(self, role: str, content: str) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)

        # Role label
        role_label = Gtk.Label(label=self._role_display(role))
        role_label.set_halign(Gtk.Align.START)
        role_label.add_css_class("caption")
        role_label.add_css_class("dim-label")

        if role == "user":
            role_label.set_margin_start(96)
        elif role == "tool":
            role_label.set_margin_start(64)
        else:
            role_label.set_margin_start(48)

        box.append(role_label)

        # Content
        content_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)

        if role == "tool":
            # Monospace for tool output
            label = Gtk.Label(label=content)
            label.set_wrap(True)
            label.set_wrap_mode(Pango.WrapMode.WORD_CHAR)
            label.set_halign(Gtk.Align.START)
            label.set_selectable(True)
            label.set_use_markup(False)
            content_box.append(label)
            content_box.add_css_class("chat-message-tool")
        else:
            # Parse content for code blocks and regular text
            segments = self._parse_content(content)
            for seg_type, seg_text in segments:
                if seg_type == "code":
                    code_frame = Gtk.Frame()
                    code_label = Gtk.Label(label=seg_text)
                    code_label.set_wrap(True)
                    code_label.set_wrap_mode(Pango.WrapMode.WORD_CHAR)
                    code_label.set_halign(Gtk.Align.START)
                    code_label.set_selectable(True)
                    code_label.add_css_class("monospace")
                    code_label.set_margin_start(8)
                    code_label.set_margin_end(8)
                    code_label.set_margin_top(4)
                    code_label.set_margin_bottom(4)
                    code_frame.set_child(code_label)
                    content_box.append(code_frame)
                else:
                    label = Gtk.Label(label=seg_text)
                    label.set_wrap(True)
                    label.set_wrap_mode(Pango.WrapMode.WORD_CHAR)
                    label.set_halign(Gtk.Align.START)
                    label.set_selectable(True)
                    label.set_use_markup(False)
                    content_box.append(label)

            css_class = f"chat-message-{role}" if role in ("user", "assistant") else "chat-message-assistant"
            content_box.add_css_class(css_class)

        box.append(content_box)
        return box

    def _parse_content(self, content: str) -> list[tuple[str, str]]:
        """Parse content into segments of text and code blocks."""
        segments = []
        parts = content.split("```")

        for i, part in enumerate(parts):
            if i % 2 == 0:
                # Regular text
                text = part.strip()
                if text:
                    segments.append(("text", text))
            else:
                # Code block - strip language identifier from first line
                lines = part.split("\n", 1)
                code = lines[1] if len(lines) > 1 else lines[0]
                if code.strip():
                    segments.append(("code", code.strip()))

        if not segments:
            segments.append(("text", content))

        return segments

    def _role_display(self, role: str) -> str:
        return {
            "user": "You",
            "assistant": "AiOS",
            "system": "System",
            "tool": "Tool",
        }.get(role, role.capitalize())

    def add_image(self, path: str, caption: str = ""):
        """Display an image in the chat."""
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        box.add_css_class("chat-message-assistant")

        try:
            pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(path, 600, 400, True)
            image = Gtk.Picture.new_for_pixbuf(pixbuf)
            image.set_can_shrink(True)
            box.append(image)
        except Exception as e:
            label = Gtk.Label(label=f"[Image: {path}] (Error: {e})")
            box.append(label)

        if caption:
            cap_label = Gtk.Label(label=caption)
            cap_label.add_css_class("caption")
            box.append(cap_label)

        self.append(box)

    def clear(self):
        """Clear all messages."""
        while self.get_first_child():
            self.remove(self.get_first_child())
        self.messages.clear()
        self._add_welcome()
