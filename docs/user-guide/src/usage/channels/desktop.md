# Desktop Channel

The Desktop channel is the primary interface for AiOS. It is a native GTK4/libadwaita application running on the Wayland compositor.

## Overview

The desktop interface is always available and is the default active channel when AiOS boots. It provides the richest interaction experience, including:

- Full conversation display with markdown rendering
- Rich panels with forms, dropdowns, toggles, and buttons
- Image display
- Notifications and toasts
- Secure password input
- Settings dialog
- Channel overlay indicator

## The chat window

The main window fills the screen and shows your conversation with the AI. Messages appear as chat bubbles:

- **Your messages** appear on the right side
- **AI responses** appear on the left side
- **System messages** (status, errors, info) appear centered with colored labels

The conversation scrolls automatically as new messages arrive. You can scroll up to review earlier messages.

## The text input

At the bottom of the screen is the text input bar. Type your message and press Enter to send it. Features:

- **Command autocomplete** -- type `/` and suggestions appear
- **Multi-line input** -- use Shift+Enter for new lines
- **History** -- the input remembers what you have typed

## Panels and dialogs

When the AI needs structured input (a form, a choice, a password), it shows a **panel** -- a dialog overlay on top of the chat. Panels can contain:

- Text fields
- Password fields (masked input)
- Dropdown menus
- Radio buttons (single choice)
- Checkboxes (multiple choice)
- Toggle switches
- Buttons

Panels are used by the setup wizard, settings, and any AI tool that needs structured input.

To close the topmost panel without submitting, use:

```
/close
```

## Settings dialog

Access settings through the AI ("open settings") or through the settings button. The settings dialog lets you view and change:

- LLM provider and model
- API keys
- Voice settings
- Theme
- Keyboard layout

## Channel overlay

When another channel (Web or Signal) is active, the desktop shows an overlay:

> "AI is talking on Signal"
>
> [Switch back here]

Click the button to reclaim the active channel for the desktop.

## Keyboard shortcuts

The desktop supports several keyboard shortcuts:

| Shortcut | Action |
|----------|--------|
| `Super` | Open or focus the AiOS window |
| `Ctrl+Space` | Open or focus the AiOS window |
| `Alt+Enter` | Open a terminal (foot) |
| `Alt+F4` | Close the current window |
| `Alt+F11` | Toggle fullscreen |
| `Print` | Take a screenshot (grim) |

See [Keyboard Shortcuts](../../tips/shortcuts.md) for the complete list.

## Terminal access

Press `Alt+Enter` to open a terminal emulator (foot). This gives you a standard Linux shell if you need direct command-line access. The terminal runs alongside AiOS -- you can switch back to the chat at any time.
