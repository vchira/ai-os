# Keyboard Shortcuts

Complete reference of all keyboard shortcuts in AiOS.

## System shortcuts

These shortcuts work system-wide, regardless of which window is focused.

| Shortcut | Action |
|----------|--------|
| `Super` | Open or focus the AiOS chat window |
| `Ctrl+Space` | Open or focus the AiOS chat window |
| `Alt+Enter` | Open a new terminal (foot terminal emulator) |
| `Alt+F4` | Close the focused window |
| `Alt+F11` | Toggle fullscreen for the focused window |
| `Print` | Take a screenshot (captured by grim, saved to disk) |

## Chat input shortcuts

These shortcuts work within the AiOS chat text input.

| Shortcut | Action |
|----------|--------|
| `Enter` | Send the current message |
| `Shift+Enter` | Insert a new line (multi-line input) |
| `/` | Begin a slash command (triggers autocomplete) |
| `Tab` | Accept the current autocomplete suggestion |
| `Escape` | Dismiss the autocomplete popup |

## Notes

- **Super key:** This is the Windows key on most keyboards, or the Command key on Mac keyboards.
- **Alt+Enter:** Opens a new terminal instance each time. Close terminals with `Alt+F4` or by typing `exit`.
- **Print key:** Screenshots are saved by `grim`. The save location depends on system configuration (typically the home directory or a screenshots folder).
- **Fullscreen:** The AiOS window starts in fullscreen mode by default. Press `Alt+F11` to toggle between fullscreen and windowed mode.

## Customizing shortcuts

Keyboard shortcuts are defined by the labwc Wayland compositor. To customize them, edit the labwc configuration:

```
~/.config/labwc/rc.xml
```

Or system-wide:

```
/etc/labwc/rc.xml
```

Refer to the [labwc documentation](https://labwc.github.io/) for the configuration format.
