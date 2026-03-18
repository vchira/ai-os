# The AI Interface

The AI is the primary interface to AiOS. Instead of navigating menus and clicking buttons, you express what you want in natural language and the AI makes it happen.

## How it works

AiOS uses a cloud-based large language model (Claude from Anthropic or GPT from OpenAI) as its brain. When you send a message, the AI receives:

- Your message
- The conversation history
- Current system context (date, time, hardware state)
- A set of tools it can use on your behalf

The AI processes your request, decides whether it can answer directly or needs to use a tool, and responds accordingly.

## Direct answers vs tool use

Many questions can be answered directly:

- "What is the capital of France?" -- the AI knows this
- "Explain how DNS works" -- the AI can explain
- "Write me a poem" -- the AI generates text

Other requests require the AI to interact with the system:

- "How much disk space do I have?" -- the AI needs the **system** tool
- "Read my config file" -- the AI needs the **files** tool
- "Search the web for weather in Berlin" -- the AI needs the **web** tool

## The permission model

When the AI needs to use a tool, it follows a transparent process:

1. The AI tells you what it wants to do: "I'll check disk usage using the system tool."
2. The tool executes with appropriate sandboxing.
3. The AI reports the result: "Your root partition has 12 GB free out of 32 GB."

For sensitive operations (accessing secrets, running destructive commands), the AI may request additional authorization through the permission system.

## Conversation context

The AI maintains context throughout your conversation. You can refer to previous messages:

- "What did I ask you earlier?"
- "Run that command again"
- "Save what you just showed me to a file"

When the conversation grows long, AiOS automatically summarizes older messages to keep costs manageable while preserving important context. This happens transparently -- the AI still "remembers" the key points from your earlier conversation.

## Effort levels

The AI adapts its thinking effort based on your request:

| Request type | Effort | What happens |
|-------------|--------|-------------|
| Simple questions, greetings | Low | Fast, cheap model |
| Standard tasks | Medium | Balanced model |
| Complex analysis, refactoring | High | Deep thinking model |

AiOS auto-detects the appropriate effort level. You can override it:

```
/effort low      # Always use fast responses
/effort medium   # Standard responses
/effort high     # Thorough, detailed responses
/effort auto     # Let the system decide (default)
```

## System context

The AI automatically knows:

- Current date and time
- What hardware the system is running on
- Which tools are available
- Which channel you are communicating through
- Your configured language and preferences

You do not need to provide this information -- it is included in every request.

## Tips for effective interaction

**Be specific.** "Show me the large files in my home directory" works better than "find files."

**Ask follow-up questions.** The AI remembers context, so you can say "now delete the largest one" after seeing results.

**Use natural language.** You do not need to memorize commands. "Turn off the microphone" works just as well as `/mic off`.

**Let the AI explain.** If you are unsure what the AI did, ask: "What exactly did that command do?"
