# Keyboard Shortcuts

AiOS provides keyboard shortcuts for common actions. These are handled by the Wayland compositor (labwc) and work system-wide.

## System shortcuts

| Shortcut | Action |
|----------|--------|
| `Super` | Open or focus the AiOS window |
| `Ctrl+Space` | Open or focus the AiOS window |
| `Alt+Enter` | Open a terminal (foot terminal emulator) |
| `Alt+F4` | Close the focused window |
| `Alt+F11` | Toggle fullscreen mode |
| `Print` | Take a screenshot (saved by grim) |

## Window management

The AiOS window runs fullscreen by default. When you open a terminal with `Alt+Enter`, you can manage windows:

- **Focus switching:** Click on a window to focus it, or use `Alt+Tab` if your compositor supports it
- **Close window:** `Alt+F4` closes whichever window is currently focused
- **Fullscreen toggle:** `Alt+F11` toggles the focused window between fullscreen and windowed mode

## Chat input shortcuts

Within the AiOS chat input:

| Shortcut | Action |
|----------|--------|
| `Enter` | Send message |
| `Shift+Enter` | New line (multi-line input) |
| `/` | Start a command (triggers autocomplete) |
| `Tab` | Accept autocomplete suggestion |
| `Escape` | Close autocomplete popup |

## Tips

- **Quick access from anywhere:** Press `Super` or `Ctrl+Space` to jump back to the AiOS window, even if a terminal is focused.
- **Screenshot for the AI:** Press `Print` to capture the screen. You can then ask the AI to analyze the screenshot.
- **Terminal when you need it:** `Alt+Enter` gives you a full Linux shell for anything the AI cannot do or when you prefer direct command-line access.

## Customization

Keyboard shortcuts are configured in the labwc compositor configuration. Advanced users can modify them by editing the labwc config files in `/etc/labwc/` or `~/.config/labwc/`.

For a printable reference card, see the [Keyboard Shortcuts Reference](../reference/shortcuts.md).
