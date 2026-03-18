# Text Commands

AiOS supports slash commands -- text commands that start with `/` and control system settings directly. These are faster than asking the AI for configuration changes.

## How commands work

Type a command in the text input and press Enter. Commands are processed immediately by the system without going through the AI. Results appear in the chat.

Commands support autocomplete -- as you type, suggestions appear above the input bar.

## Common commands

### Getting help

```
/help              # Show all available commands
/info              # Show system information (version, provider, settings)
/sysinfo           # Show system monitor (CPU, RAM, disk, processes)
```

### AI provider settings

```
/key claude sk-ant-...    # Set Claude API key
/key openai sk-...        # Set OpenAI API key
/provider claude          # Switch to Claude
/provider openai          # Switch to OpenAI
/model claude-sonnet-4-20250514      # Set specific model
```

### Voice settings

```
/mic on                   # Enable microphone
/mic off                  # Disable microphone
/speaker on               # Enable spoken responses
/speaker off              # Disable spoken responses
/voice                    # List available TTS voices
/voice en-us              # Set TTS voice
/language en              # Set STT language
/language                 # Reset to auto-detect
/wake hey assistant       # Set wake word
/wake                     # Disable wake word
```

### Display and input

```
/theme dark               # Dark theme
/theme light              # Light theme
/theme auto               # Follow system setting
/resolution 1920x1080     # Set screen resolution
/keyboard de              # Set keyboard layout
```

### AI behavior

```
/effort low               # Fast, simple responses
/effort medium            # Balanced responses
/effort high              # Thorough, detailed responses
/effort auto              # Let the system decide (default)
/mode saver               # Cheapest models, aggressive optimization
/mode balanced            # Default cost/quality balance
/mode thorough            # Best models, no cost optimization
```

### Channels

```
/channel                  # Show channel status
/channel web on           # Enable web channel
/channel web off          # Disable web channel
/channel web port 8080    # Change web server port
/channel signal on        # Enable Signal messenger channel
/channel signal off       # Disable Signal channel
/channel signal phone +1234567890  # Set Signal phone number
```

### System

```
/tools                    # List all available AI tools
/selftest                 # Run self-tests
/selftest quick           # Run only quick tests
/selftest audio           # Run audio-specific tests
/update https://...       # Update AiOS binary from URL
/configure                # Re-run the setup wizard
/clear                    # Clear conversation history
/close                    # Close the topmost panel or dialog
```

## Command autocomplete

When you type `/` in the text input, a popup appears showing matching commands. Continue typing to narrow the list. Press Tab or click to select a command.

For example, typing `/ch` shows `/channel` and `/clear`. Typing `/cha` narrows it to `/channel`.

## Commands vs natural language

You can achieve most configuration changes either way:

| Command | Natural language equivalent |
|---------|---------------------------|
| `/mic off` | "Turn off the microphone" |
| `/theme dark` | "Switch to dark theme" |
| `/provider openai` | "Use OpenAI instead" |
| `/keyboard de` | "Change keyboard to German layout" |

Commands are instant and unambiguous. Natural language is more flexible but goes through the AI, which takes a moment to process. Use whichever feels more natural for the situation.

## Full reference

For a complete list of all commands with detailed descriptions, see the [All Commands](../reference/commands.md) reference.
