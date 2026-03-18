# System Requirements

## Minimum hardware

| Component | Requirement |
|-----------|-------------|
| **Processor** | x86_64 (AMD64) -- any 64-bit Intel or AMD CPU |
| **RAM** | 4 GB |
| **Storage** | 4 GB USB drive for live mode, 8 GB disk for installation |
| **Graphics** | Any GPU (integrated or discrete) |
| **Network** | Ethernet or Wi-Fi (for LLM API access) |
| **Audio** | Microphone + speakers recommended (for voice interaction) |

## Recommended hardware

| Component | Recommendation |
|-----------|----------------|
| **RAM** | 8 GB or more -- allows larger Whisper models for better voice recognition |
| **Storage** | SSD with 16 GB or more |
| **Audio** | USB headset or built-in mic/speakers |

## Software requirements

AiOS is a standalone operating system. It boots directly from a USB drive or installs to a hard drive. You do not need any other operating system installed.

To create the bootable USB, you will need a working computer with one of:
- Linux (use `dd` or a graphical tool like GNOME Disks)
- macOS (use `dd` or balenaEtcher)
- Windows (use Rufus or balenaEtcher)

## Network access

AiOS requires an internet connection for its core AI functionality. The AI assistant communicates with cloud LLM providers (Anthropic Claude or OpenAI) over HTTPS.

Local network access is optional but enables the web channel -- other devices on your network can interact with AiOS through a browser at `http://aios.local`.

## API key

You will need at least one API key:

- **Anthropic Claude** -- get one at [console.anthropic.com](https://console.anthropic.com)
- **OpenAI** -- get one at [platform.openai.com](https://platform.openai.com)

You can use both providers and switch between them at any time. Having a backup provider is recommended in case one service is unavailable.

## Firmware notes

AiOS is based on Debian Bookworm and includes non-free firmware packages. This means most hardware -- including Wi-Fi adapters, graphics cards, and Bluetooth modules -- should work out of the box without manually downloading drivers.

## Virtual machines

AiOS runs well in virtual machines for testing and development. See [Running in a VM](../tips/vm.md) for setup instructions with QEMU and VirtualBox.
