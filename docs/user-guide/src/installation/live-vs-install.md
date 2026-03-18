# Live Mode vs Installation

AiOS can run in two ways: live mode (directly from the USB drive) or installed to a hard drive. Both give you the full AiOS experience.

## Live mode

When you boot AiOS from a USB drive without installing, you are running in **live mode**.

**Advantages:**
- No changes to your computer's hard drive
- Try AiOS without commitment
- Portable -- carry your AI assistant on a USB stick
- Boot on any compatible computer

**Limitations:**
- Changes are lost when you shut down (conversation history, settings, vault)
- Slightly slower than a hard drive installation (USB read speeds)
- The USB drive is occupied while in use

Live mode is ideal for trying AiOS, demonstrations, and portable use. If you use an autoconfig file on the USB drive, you can have AiOS pre-configured and ready to go every time you boot.

## Hard drive installation

AiOS can be installed permanently to your computer's internal hard drive or SSD.

**Advantages:**
- Settings, vault, and conversation history persist across reboots
- Faster boot and operation (SSD/HDD speeds)
- The full disk is available for file storage

**Limitations:**
- Erases the target drive (AiOS uses the entire disk)
- Dedicated hardware -- the computer becomes an AiOS machine

Installation is recommended if you want AiOS as a permanent, dedicated system.

## Choosing the right option

| Use case | Recommended mode |
|----------|-----------------|
| Trying AiOS for the first time | Live mode |
| Portable AI assistant | Live mode + autoconfig |
| Dedicated AI workstation | Hard drive installation |
| Testing and development | Live mode or VM |
| Kiosk / appliance deployment | Hard drive + autoconfig |

## How to install

If you decide to install AiOS to a hard drive, see [Hard Drive Installation](hard-drive.md). The installation can be triggered during the setup wizard or through an autoconfig file for unattended deployment.
