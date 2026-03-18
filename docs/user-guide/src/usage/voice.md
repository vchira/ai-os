# Voice Interaction

AiOS is designed for voice-first interaction. You speak to the AI, and it speaks back. Voice recognition and text-to-speech both run locally on your machine -- your audio is never sent to the cloud.

## How voice input works

AiOS uses **whisper.cpp** for speech-to-text (STT). Whisper is a multilingual speech recognition model that runs entirely on your hardware.

When voice input is enabled:
1. AiOS continuously listens through your microphone
2. When it detects speech, it transcribes your words to text
3. The transcribed text appears in the chat as your message
4. The AI processes and responds

Whisper supports dozens of languages and handles accents well. The model size is selected automatically based on your hardware:

| Hardware | Whisper Model | Quality |
|----------|--------------|---------|
| Low RAM (< 4 GB) | whisper-tiny | Basic accuracy |
| Standard (4-8 GB) | whisper-medium | Good accuracy |
| High RAM + GPU | whisper-large | Best accuracy |

## How voice output works

AiOS uses **Piper** for text-to-speech (TTS) by default, with **espeak-ng** as a fallback. Both run locally.

- **Piper** produces natural-sounding speech and works well even on low-power hardware like Raspberry Pi
- **espeak-ng** is lighter and supports more languages (including Romanian) but sounds more robotic

The system auto-selects the best backend based on your hardware. You can change the voice with the `/voice` command.

## Controlling voice

### Toggle microphone (voice input)

```
/mic on       # Start listening
/mic off      # Stop listening
```

### Toggle speaker (voice output)

```
/speaker on   # AI speaks responses aloud
/speaker off  # AI responds with text only
```

### Set STT language

By default, Whisper auto-detects your language. You can force a specific language for better accuracy:

```
/language en     # English
/language de     # German
/language fr     # French
/language ro     # Romanian
/language        # Reset to auto-detect
```

### Change TTS voice

```
/voice           # List available voices
/voice en-us     # Set to US English voice
```

## Wake word

Instead of always listening and processing everything, you can configure a wake word. The AI only activates when it hears the wake phrase:

```
/wake hey assistant
```

After setting a wake word, the AI ignores ambient sound until it hears "hey assistant" (or whatever phrase you chose). To disable the wake word and return to always-listening mode:

```
/wake
```

## Voice and text together

Voice and text are not mutually exclusive. You can:

- Speak a question, then type a follow-up
- Type while the AI is speaking
- Use voice for quick requests and text for complex ones (like pasting an API key)

The text prompt is always available regardless of voice settings.

## Supported languages

Voice recognition (Whisper) supports these languages and many more:

| Language | Code |
|----------|------|
| English (US/UK) | en |
| German | de |
| French | fr |
| Spanish | es |
| Italian | it |
| Portuguese | pt |
| Romanian | ro |
| Japanese | ja |
| Chinese | zh |
| Korean | ko |
| Hindi | hi |

Text-to-speech language support depends on the backend:
- **Piper**: English, German, French, Spanish, Italian, and more (check available voices)
- **espeak-ng**: Nearly all languages, including Romanian

## Troubleshooting voice

- **AI does not hear me:** Check `/mic on` is set. Verify your microphone is detected with `/sysinfo`.
- **Poor recognition accuracy:** Try setting the language explicitly with `/language`. A larger Whisper model helps if you have enough RAM.
- **No spoken responses:** Check `/speaker on` is set. See [Audio Problems](../troubleshooting/audio.md) for PipeWire troubleshooting.
- **Wrong voice:** Use `/voice` to list and select a different TTS voice.
