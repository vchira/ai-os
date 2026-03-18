# Introduction

AiOS is an AI-native Linux distribution. Instead of menus, file browsers, and settings panels, you talk to an AI assistant that controls the entire operating system on your behalf.

You speak or type what you want. The AI figures out how to do it, asks for permission if needed, and executes it using built-in tools. Need to find a file? Ask. Want to change the screen resolution? Say so. Curious about disk usage? Just ask.

AiOS is a complete operating system. It boots from a USB drive or installs to a hard drive. It runs on standard x86_64 hardware. There is nothing else to install -- the AI, voice recognition, text-to-speech, and all system tools are included in a single ISO image.

## What makes AiOS different

**The AI is the interface.** Traditional operating systems put a graphical shell between you and the machine. AiOS puts an AI there instead. The AI understands natural language, remembers context from your conversation, and has direct access to the system through a set of safe, sandboxed tools.

**Voice-first, text-always.** AiOS listens for your voice by default using local speech recognition (Whisper). It responds with spoken words using local text-to-speech (Piper). If you prefer typing, the text prompt is always available. You can use both at the same time.

**Multiple channels.** You can talk to AiOS from its desktop interface, from a web browser on any device on your network, or through Signal messenger on your phone. All channels share the same AI brain and conversation.

**Privacy by default.** Voice recognition runs locally on your machine. Your API keys are stored in an encrypted vault. The AI never silently executes something you did not ask for -- it always explains what it wants to do and waits for your approval.

## Who is AiOS for

- People who want a simpler way to interact with a computer
- Developers and tinkerers exploring AI-native interfaces
- Anyone who prefers talking to clicking
- Users who want a lightweight, single-purpose system for AI interaction

## What you will need

- A computer with an x86_64 processor (most PCs and laptops made in the last 15 years)
- At least 4 GB of RAM
- A USB drive (4 GB or larger) to boot from, or 8 GB of disk space for installation
- An API key from Anthropic (Claude) or OpenAI -- the AI needs a language model provider

## How this guide is organized

- **Getting Started** walks you through downloading AiOS, creating a bootable USB, and booting it for the first time.
- **Installation** covers live mode, the setup wizard, hard drive installation, and unattended configuration.
- **Using AiOS** explains how to interact with the AI, use voice, run commands, and work with the multi-channel system.
- **Configuration** details how to set up LLM providers, voice, keyboard layouts, themes, cost controls, and security.
- **Tips & Tricks** shares keyboard shortcuts, power user commands, and customization ideas.
- **Troubleshooting** helps you solve common problems with audio, networking, and system diagnostics.
- **Reference** provides complete lists of all commands, tools, autoconfig options, and keyboard shortcuts.
