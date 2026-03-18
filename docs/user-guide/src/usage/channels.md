# Multi-Channel System

AiOS has one AI brain but multiple ways to interact with it. These are called **channels**. You can talk to the same AI from the desktop, a web browser, or Signal messenger -- and the conversation follows you.

## Available channels

| Channel | Interface | How to access |
|---------|-----------|--------------|
| **Desktop** | GTK4 native app | Always available, default channel |
| **Web** | Browser via WebSocket | `http://aios.local` from any device on the LAN |
| **Signal** | Signal messenger | Via signal-cli daemon on the AiOS machine |
| **Voice** | Microphone + speakers | Overlays on top of other channels |

## One brain, many surfaces

All channels share the same AI, the same conversation, and the same tools. If you ask a question on the desktop and then switch to the web channel, the AI remembers what you talked about.

## Channel capabilities

Not all channels can display the same things. The AI adapts its output based on what the current channel supports:

| Capability | Desktop | Web | Signal | Voice |
|-----------|---------|-----|--------|-------|
| Rich panels (forms, dialogs) | Yes | Yes | No (text fallback) | No |
| Images | Yes | Yes | Yes | No |
| Markdown formatting | Yes | Yes | No | No |
| Notifications/toasts | Yes | Yes | No | No |
| Structured input (forms) | Yes | Yes | No | No |
| Password input (masked) | Yes | Yes | No | No |
| Max message length | Unlimited | Unlimited | 4096 chars | Unlimited |

When the AI uses a tool that produces rich output (like a panel or image), it automatically falls back to text on channels that cannot display it. For example, a multi-field form on the desktop becomes a numbered list of choices on Signal.

## Channel switching

The active channel is determined by where the last message came from:

- If you send a message from Signal, Signal becomes the active channel
- If you then type something on the desktop, the desktop becomes active
- Signal gets a message: "Conversation moved to desktop"

On the desktop, when another channel is active, you see an overlay: "AI is talking on Signal" with a button to switch back.

## Managing channels

Check channel status:

```
/channel
```

Enable or disable channels:

```
/channel web on
/channel web off
/channel signal on
/channel signal off
```

Configure channel settings:

```
/channel web port 8080           # Change web server port
/channel signal phone +1234567890  # Set Signal phone number
```

## Channel details

Each channel has its own chapter with setup instructions and usage details:

- [Desktop](channels/desktop.md) -- the default GTK4 interface
- [Web Interface](channels/web.md) -- browser access from any device
- [Signal Messenger](channels/signal.md) -- mobile messaging integration
