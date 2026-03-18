# First Boot

After completing the setup wizard (or having autoconfig handle it), AiOS is ready to use. This chapter explains what you see and how to start interacting.

## The AiOS desktop

The AiOS desktop is intentionally minimal. There is no traditional desktop with icons, taskbars, or file managers. Instead, you see:

- **The chat window** -- a full-screen conversation interface, similar to a messaging app
- **A text input bar** at the bottom for typing messages
- **The AI's responses** displayed as chat bubbles above

This is the Desktop channel -- the primary way to interact with AiOS.

## Boot status message

The first message you see after setup is a system status report:

```
[INFO] AiOS System Status
  Boot time: 2026-03-18 12:00:00 UTC

  Desktop: available -- GTK4/libadwaita
  Web Channel: available -- http://aios.local:80
  Signal: unavailable -- disabled (/channel signal on)
  LLM Provider: available -- claude
  Voice: available -- STT: on | TTS: on
```

This tells you which components are working. If anything shows as unavailable, the status message includes a hint about how to fix it.

## Your first conversation

Try saying or typing something:

- "Hello" -- the AI will greet you and explain what it can do
- "What can you do?" -- lists the AI's capabilities
- "What time is it?" -- the AI knows the current date and time
- "Show me disk usage" -- the AI uses the system tool to check and report
- "What files are in my home directory?" -- the AI uses the files tool to look

The AI responds both in text (on screen) and in speech (through your speakers), unless you have disabled voice output.

## Understanding the interaction model

AiOS follows a strict interaction model:

1. **You express intent** -- through voice or text
2. **The AI interprets** -- it understands what you want
3. **If a tool is needed**, the AI tells you what it wants to do and asks for permission
4. **You approve** -- the AI executes the tool
5. **The AI reports** -- it explains what happened and shows the result

The AI will never silently run a command or access a resource without telling you first. This transparency is a core design principle.

## Voice interaction

If your microphone is working, AiOS is listening for your voice by default. Just speak naturally -- the AI will hear you, transcribe your words, and respond.

To check or change voice settings:

```
/mic on          # Enable microphone
/mic off         # Disable microphone
/speaker on      # Enable spoken responses
/speaker off     # Disable spoken responses
```

See [Voice Interaction](../usage/voice.md) for more details.

## What to try next

Here are some things to explore on your first boot:

- Ask the AI about itself: "Tell me about AiOS"
- Check system information: "How much RAM does this machine have?"
- Try a command: type `/help` to see all available slash commands
- Open a terminal: press `Alt+Enter`
- Try the web channel: open `http://aios.local` from another device on your network
- Run a self-test: type `/selftest` to verify everything is working

## Next steps

- Learn about the [AI Interface](../usage/ai-interface.md) in depth
- Explore [Voice Interaction](../usage/voice.md)
- Discover [Text Commands](../usage/commands.md)
- See the full list of [Built-in Tools](../usage/tools.md)
