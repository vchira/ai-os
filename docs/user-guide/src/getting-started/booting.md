# Booting AiOS

With your bootable USB drive ready, you can now start AiOS on your computer.

## Accessing the boot menu

1. Insert the AiOS USB drive into your computer.
2. Power on (or restart) the computer.
3. Press the boot menu key during startup. Common keys by manufacturer:

    | Manufacturer | Boot Menu Key |
    |-------------|---------------|
    | Dell | F12 |
    | HP | F9 |
    | Lenovo | F12 |
    | ASUS | F8 or Esc |
    | Acer | F12 |
    | MSI | F11 |
    | Gigabyte | F12 |
    | Intel NUC | F10 |

    If you are unsure, try pressing F12 repeatedly as soon as the computer powers on. You may also need to check your specific model's documentation.

4. Select the USB drive from the boot menu.

## UEFI vs Legacy BIOS

AiOS supports both UEFI and Legacy BIOS boot modes. Most modern computers use UEFI. If your USB drive does not appear in the boot menu:

- Enter BIOS/UEFI settings (usually Del or F2 at startup).
- Look for "Secure Boot" and disable it.
- Ensure USB boot is enabled in the boot order.
- If using Legacy BIOS, enable "Legacy Boot" or "CSM" mode.

## The boot screen

After selecting the USB drive, you will see a brief boot sequence. AiOS uses a minimal boot -- there is no graphical splash screen. The system loads the Linux kernel, starts the Wayland compositor, and launches the AiOS desktop application.

The entire boot process typically takes 15-30 seconds, depending on your hardware.

## What happens at first boot

When AiOS starts for the first time, it runs the **setup wizard**. The AI assistant will greet you with a spoken welcome message and walk you through initial configuration:

1. Choose your AI provider (Claude or OpenAI)
2. Enter your API key
3. Create a master password for the encrypted vault
4. Optionally configure a backup provider

See [Interactive Setup Wizard](../installation/setup-wizard.md) for details on this process.

If an autoconfig file is present (baked into the ISO or on the USB drive), the setup wizard is skipped and AiOS configures itself automatically. See [Unattended Installation](../installation/unattended.md) for details.

## Boot status

After setup completes, AiOS displays a status message showing which components are available:

```
[INFO] AiOS System Status
  Boot time: 2026-03-18 12:00:00 UTC

  Desktop: available -- GTK4/libadwaita
  Web Channel: available -- http://aios.local:80
  Signal: unavailable -- disabled (/channel signal on)
  LLM Provider: available -- claude
  Voice: available -- STT: on | TTS: on
```

Green indicators show working components. Red indicators show components that are disabled or encountered errors. Each red indicator includes a hint about how to resolve the issue.

## Troubleshooting boot issues

- **USB drive not detected:** Try a different USB port. Use a USB 2.0 port if USB 3.0 is not working.
- **Black screen after boot:** Your graphics hardware may need a specific kernel parameter. Try adding `nomodeset` to the boot parameters.
- **No audio at startup:** Audio is not required for AiOS to function. You can use the text prompt instead. See [Audio Problems](../troubleshooting/audio.md) for fixes.
- **No network:** AiOS needs internet access for AI functionality. Connect via Ethernet for the most reliable experience. See [Network & Connectivity](../troubleshooting/network.md).
