# Signal Messenger

The Signal channel lets you interact with AiOS through the Signal messaging app on your phone. Send a message to AiOS from Signal, and the AI responds right in your chat.

## Overview

Signal integration uses `signal-cli`, a command-line client for the Signal protocol. The AiOS machine runs a signal-cli daemon that listens for incoming messages and sends responses back.

## Setting up Signal

### Prerequisites

- A phone number that can receive SMS (for Signal registration)
- The Signal app installed on your phone

### Enable the Signal channel

```
/channel signal on
/channel signal phone +1234567890
```

Replace `+1234567890` with the phone number you want AiOS to use. This number will be registered with Signal as a new device.

The setup process will guide you through Signal registration, which may require verifying with an SMS code.

### Messaging AiOS

Once set up, send a message to the AiOS phone number from your Signal app just like you would message any contact. The AI will respond in the same chat.

## Capabilities and limitations

Signal is a text-based messaging channel. Compared to the Desktop and Web channels, it has some limitations:

| Feature | Support |
|---------|---------|
| Text messages | Yes |
| Images (sent by AI) | Yes |
| Markdown formatting | No (plain text only) |
| Rich panels / forms | No (text fallback) |
| Password input | No |
| Notifications | No |
| Max message length | 4096 characters |

When the AI would normally show a form or panel, Signal users see a text-based alternative. For example, a dropdown menu becomes a numbered list:

```
Choose your AI provider:
1. Claude
2. OpenAI

Reply with the number of your choice.
```

## Channel switching

Sending a message from Signal makes it the active channel. The AiOS desktop will show:

> "AI is talking on Signal"
>
> [Switch back here]

If someone interacts on the desktop or web, Signal receives a message: "Conversation moved to desktop."

## Use cases

- **Remote access** -- interact with AiOS from anywhere with internet, not just your local network
- **On the go** -- ask your AI assistant questions from your phone
- **Quick checks** -- "How much disk space is left?" without sitting at the computer
- **Notifications** -- the AI can proactively message you on Signal (e.g., "disk is 90% full")

## Disabling Signal

To turn off the Signal channel:

```
/channel signal off
```

This stops the signal-cli daemon. The phone number registration persists -- re-enabling Signal will resume using the same number.

## Troubleshooting

- **Messages not received:** Check that the signal-cli daemon is running. Run `/channel` to see the Signal channel status.
- **Registration failed:** Ensure the phone number can receive SMS. Some VoIP numbers do not work with Signal.
- **Long response truncated:** Signal has a 4096-character message limit. Very long AI responses are split across multiple messages automatically.
