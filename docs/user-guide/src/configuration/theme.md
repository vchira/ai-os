# Theme & Display

AiOS provides basic theme and display settings to customize the visual appearance and screen configuration.

## Theme

AiOS uses GTK4/libadwaita, which supports light and dark themes:

```
/theme dark        # Dark theme (default)
/theme light       # Light theme
/theme auto        # Follow system setting
```

The dark theme is the default and matches the AiOS aesthetic -- a dark background with blue accents. The light theme provides a bright background for high-ambient-light environments.

## Screen resolution

Set the display resolution:

```
/resolution 1920x1080
/resolution 1280x720
/resolution 2560x1440
```

The resolution is applied to the Wayland compositor (labwc). The available resolutions depend on your monitor and graphics hardware.

To see the current resolution and display information, ask the AI: "What is my screen resolution?" or check `/sysinfo`.

## The AiOS desktop

The AiOS desktop is minimal by design. There is no traditional desktop with wallpaper, icons, or a taskbar. The entire screen is the AI chat interface.

The window compositor (labwc) is a lightweight, wlroots-based Wayland compositor. It handles:

- Window management (for the AiOS app, terminals, and any other windows)
- Keyboard shortcuts
- Display output

### Fullscreen

The AiOS window runs fullscreen by default. Toggle fullscreen mode with:

```
Alt+F11
```

### Opening other windows

Although AiOS is designed as a single-window experience, you can open additional windows:

- **Terminal:** Press `Alt+Enter` to open a foot terminal
- **Screenshot:** Press `Print` to capture the screen with grim

Windows can be moved and resized in the standard Wayland way. `Alt+F4` closes the focused window.

## Display in virtual machines

When running AiOS in a VM, the resolution is often limited by the virtual display adapter. Tips:

- **QEMU/KVM:** Use `-vga virtio` for the best resolution support. The AiOS VM launcher script sets this automatically.
- **VirtualBox:** Install Guest Additions or use VBoxSVGA adapter for dynamic resolution.

See [Running in a VM](../tips/vm.md) for more details.

## Changing settings through the AI

You can also use natural language:

- "Switch to light theme"
- "Set the resolution to 1080p"
- "Make it dark mode"

The AI translates your request into the appropriate command.
