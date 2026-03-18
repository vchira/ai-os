# Customizing the AI

AiOS lets you adjust how the AI behaves, responds, and processes your requests. Here are ways to customize the experience.

## Effort and quality

The two main dials for controlling AI behavior:

### Effort level

Controls how hard the AI thinks about each response:

```
/effort auto       # System decides based on query complexity (default)
/effort low        # Quick, concise answers
/effort medium     # Standard analysis
/effort high       # Deep thinking, thorough responses
```

Use `/effort high` when you need the AI to really think through a problem. Use `/effort low` when you want fast answers to simple questions.

### Quality mode

Controls the cost vs quality tradeoff:

```
/mode saver        # Use cheapest models, save money
/mode balanced     # Good balance of quality and cost (default)
/mode thorough     # Use the best models available
```

These two settings combine. For example, `/effort high` + `/mode thorough` gives you the absolute best response quality (at the highest cost). `/effort low` + `/mode saver` gives the cheapest possible operation.

## Teaching the AI

The AI has an episodic memory system. It remembers interactions and learns from them:

- **It remembers what you told it.** Say "Remember that my project is in /home/user/myproject" and it will use that context in future conversations.
- **It learns from outcomes.** If a tool operation fails, the AI records what went wrong and adjusts its approach next time.
- **It stores reflections.** The AI periodically reflects on interactions -- what worked, what did not, and what to do differently.

You can actively teach it:

- "Remember that I prefer Python over JavaScript"
- "Note that the database is on port 5433, not the default"
- "I always want verbose output from system commands"

Check what the AI remembers:

- "What do you remember about me?"
- "What did you learn from our last conversation?"

## Choosing your provider

Different providers have different strengths:

- **Claude** tends to be more careful and nuanced in its responses
- **OpenAI** tends to be broader in knowledge and code generation

Switch between them based on the task:

```
/provider claude     # For careful analysis
/provider openai     # For broad tasks
```

## Assistant personality

Through the autoconfig file, you can set the assistant's display name:

```json
{
  "assistant": {
    "name": "Jarvis"
  }
}
```

## Channel preferences

If you primarily use one channel, you can disable the others to simplify the system:

```
/channel web off       # Disable web channel
/channel signal off    # Disable Signal
```

This reduces resource usage and eliminates channel-switching notifications.

## Voice customization

Customize the voice experience:

- Change the TTS voice: `/voice`
- Set your language for better recognition: `/language de`
- Set a wake word: `/wake hey computer`
- Disable voice entirely: `/mic off` and `/speaker off`

## Compositor customization

Advanced users can customize the labwc Wayland compositor by editing its configuration files:

- `~/.config/labwc/rc.xml` -- keybindings, window rules
- `~/.config/labwc/autostart` -- programs to start with the compositor
- `~/.config/labwc/environment` -- environment variables

This allows you to add custom keyboard shortcuts, change window behavior, or auto-start additional programs.

## Tips for a personalized experience

1. **Start with defaults.** The auto settings work well for most people.
2. **Adjust effort if responses are too long or too short.** `/effort low` for snappy responses, `/effort high` for detailed ones.
3. **Teach the AI your preferences early.** It remembers and applies them.
4. **Use both providers.** Switch based on the task at hand.
5. **Set your keyboard and language first.** It affects everything else.
