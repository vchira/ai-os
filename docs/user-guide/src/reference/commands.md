# All Commands

Complete reference of all AiOS slash commands. Type any command in the chat input and press Enter.

## Help & Information

### /help

Show all available commands.

```
/help
```

### /info

Show system information including current provider, model, effort level, quality mode, and version.

```
/info
```

### /sysinfo

Show system monitor with CPU usage, memory usage, disk space, network status, audio devices, and running processes.

```
/sysinfo
```

### /tools

List all available AI tools with their categories and descriptions.

```
/tools
```

## LLM Provider

### /key

Set an API key for a provider. The key is stored in the encrypted vault.

```
/key claude sk-ant-api03-your-key-here
/key openai sk-your-key-here
```

### /provider

Switch the active LLM provider.

```
/provider claude
/provider openai
```

### /model

Set a specific model for the current provider. Overrides automatic model selection.

```
/model claude-sonnet-4-20250514
/model gpt-4o
```

## AI Behavior

### /effort

Set the AI effort level. Controls how much thinking the AI does per response.

```
/effort auto       # System decides based on query complexity (default)
/effort low        # Quick responses, cheapest model
/effort medium     # Standard responses
/effort high       # Thorough responses, best model with extended thinking
```

### /mode

Set the quality/cost mode. Controls the tradeoff between response quality and API cost.

```
/mode saver        # Cheapest models, aggressive cost optimization
/mode balanced     # Default balance of quality and cost
/mode thorough     # Best models, no cost optimization
```

## Voice

### /mic

Toggle voice input (microphone).

```
/mic on            # Enable voice input
/mic off           # Disable voice input
```

### /speaker

Toggle voice output (text-to-speech).

```
/speaker on        # Enable spoken responses
/speaker off       # Disable spoken responses
```

### /voice

List available TTS voices or set a specific voice.

```
/voice             # List available voices
/voice en-us       # Set US English voice
/voice de          # Set German voice
```

### /language

Set the speech recognition language. Blank resets to auto-detect.

```
/language en       # English
/language de       # German
/language fr       # French
/language          # Auto-detect (default)
```

### /wake

Set or clear the wake word phrase. When set, the AI only activates after hearing this phrase.

```
/wake hey assistant    # Set wake word
/wake                  # Disable wake word (always listening)
```

## Display & Input

### /theme

Set the UI theme.

```
/theme dark        # Dark theme (default)
/theme light       # Light theme
/theme auto        # Follow system setting
```

### /resolution

Set the screen resolution.

```
/resolution 1920x1080
/resolution 1280x720
/resolution 2560x1440
```

### /keyboard

Set the keyboard layout.

```
/keyboard us       # US English
/keyboard de       # German
/keyboard fr       # French
/keyboard gb       # UK English
```

## Channels

### /channel

Show channel status or configure channels.

```
/channel                         # Show status of all channels
/channel web on                  # Enable web channel
/channel web off                 # Disable web channel
/channel web port 8080           # Set web server port
/channel signal on               # Enable Signal channel
/channel signal off              # Disable Signal channel
/channel signal phone +1234567890  # Set Signal phone number
```

## System

### /selftest

Run self-tests to verify system health.

```
/selftest              # Run all tests
/selftest quick        # Fast checks only
/selftest channel      # Channel connectivity tests
/selftest tools        # Tool verification tests
/selftest interactive  # Tests requiring user interaction
```

### /update

Update the AiOS binary from a URL.

```
/update https://your-server.com/aios-binary
```

### /configure

Re-run the first-boot setup wizard. Preserves the existing vault.

```
/configure
```

## Conversation

### /clear

Clear the conversation history. Starts a fresh conversation with the AI.

```
/clear
```

### /close

Close the topmost panel or dialog overlay.

```
/close
```
