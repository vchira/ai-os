"""Prompt input widget with text entry and voice recording button."""

import gi

gi.require_version("Gtk", "4.0")

from gi.repository import Gtk, GObject, Gdk


class PromptInput(Gtk.Box):
    """Text input area with send button and voice recording toggle."""

    __gsignals__ = {
        "message-submitted": (GObject.SignalFlags.RUN_FIRST, None, (str,)),
    }

    def __init__(self):
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.add_css_class("prompt-input")

        # Voice record button (push-to-talk)
        self.record_button = Gtk.Button(icon_name="media-record-symbolic")
        self.record_button.set_tooltip_text("Push to talk (hold Space)")
        self.record_button.add_css_class("circular")
        self.record_button.add_css_class("suggested-action")

        # Use press/release for push-to-talk
        press_gesture = Gtk.GestureLongPress.new()
        press_gesture.set_touch_only(False)
        click_gesture = Gtk.GestureClick.new()
        click_gesture.connect("pressed", self._on_record_pressed)
        click_gesture.connect("released", self._on_record_released)
        self.record_button.add_controller(click_gesture)
        self.append(self.record_button)

        # Text entry
        self.entry = Gtk.Entry()
        self.entry.set_hexpand(True)
        self.entry.set_placeholder_text("Ask AiOS anything...")
        self.entry.add_css_class("prompt-entry")
        self.entry.connect("activate", self._on_entry_activate)
        self.append(self.entry)

        # Send button
        self.send_button = Gtk.Button(icon_name="go-next-symbolic")
        self.send_button.set_tooltip_text("Send message")
        self.send_button.add_css_class("circular")
        self.send_button.add_css_class("suggested-action")
        self.send_button.connect("clicked", self._on_send_clicked)
        self.append(self.send_button)

        # Key controller for keyboard shortcuts
        key_ctrl = Gtk.EventControllerKey.new()
        key_ctrl.connect("key-pressed", self._on_key_pressed)
        self.entry.add_controller(key_ctrl)

        self._recording = False
        self._on_record_start = None
        self._on_record_stop = None

    def _on_entry_activate(self, entry):
        self._submit()

    def _on_send_clicked(self, button):
        self._submit()

    def _submit(self):
        text = self.entry.get_text().strip()
        if text:
            self.entry.set_text("")
            self.emit("message-submitted", text)

    def _on_key_pressed(self, controller, keyval, keycode, state):
        # Ctrl+Enter or just Enter submits
        if keyval == Gdk.KEY_Return and (state & Gdk.ModifierType.CONTROL_MASK):
            self._submit()
            return True
        return False

    def _on_record_pressed(self, gesture, n_press, x, y):
        """Start recording on button press."""
        self._recording = True
        self.record_button.add_css_class("voice-recording")
        self.record_button.set_icon_name("media-playback-stop-symbolic")
        if self._on_record_start:
            self._on_record_start()

    def _on_record_released(self, gesture, n_press, x, y):
        """Stop recording on button release."""
        if self._recording:
            self._recording = False
            self.record_button.remove_css_class("voice-recording")
            self.record_button.set_icon_name("media-record-symbolic")
            if self._on_record_stop:
                self._on_record_stop()

    def set_record_callbacks(self, on_start, on_stop):
        """Set callbacks for voice recording start/stop."""
        self._on_record_start = on_start
        self._on_record_stop = on_stop

    def set_text(self, text: str):
        """Set the entry text (used by voice input)."""
        self.entry.set_text(text)

    def grab_focus(self):
        self.entry.grab_focus()

    def set_sensitive(self, sensitive: bool):
        self.entry.set_sensitive(sensitive)
        self.send_button.set_sensitive(sensitive)
