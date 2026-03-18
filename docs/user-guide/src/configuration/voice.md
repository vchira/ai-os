# Voice Settings

AiOS provides detailed control over voice input (speech-to-text) and voice output (text-to-speech). Both systems run locally on your machine.

## Quick controls

```
/mic on           # Enable voice input
/mic off          # Disable voice input
/speaker on       # Enable voice output
/speaker off      # Disable voice output
```

## Speech-to-text (STT) configuration

### Backend

AiOS uses **whisper.cpp** for speech recognition. The Whisper model size is selected automatically based on your hardware:

- **Low RAM (< 4 GB):** whisper-tiny -- basic accuracy, minimal resource use
- **Standard (4-8 GB):** whisper-medium -- good accuracy, moderate resource use
- **High RAM + GPU:** whisper-large -- best accuracy, uses GPU acceleration when available

### Language

By default, Whisper auto-detects the spoken language. For better accuracy, you can force a specific language:

```
/language en      # English
/language de      # German
/language fr      # French
/language es      # Spanish
/language ro      # Romanian
/language ja      # Japanese
/language         # Reset to auto-detect
```

Setting the language is especially helpful if you consistently speak one language or if auto-detection makes mistakes.

### Wake word

Instead of processing all audio, you can set a wake word so the AI only activates when it hears a specific phrase:

```
/wake hey assistant       # Set wake word
/wake                     # Disable wake word (always listening)
```

When a wake word is set, AiOS ignores ambient sound until it detects the phrase.

## Text-to-speech (TTS) configuration

### Backends

AiOS supports two TTS backends:

| Backend | Quality | Languages | Resource use |
|---------|---------|-----------|-------------|
| **Piper** | Natural, human-like | Major languages | Moderate (works on RPi) |
| **espeak-ng** | Robotic but clear | Nearly all languages | Very light |

The system auto-selects Piper when a suitable voice model is available, falling back to espeak-ng otherwise. espeak-ng is particularly useful for languages that Piper does not yet support, such as Romanian.

### Choosing a voice

List available voices:

```
/voice
```

Set a specific voice:

```
/voice en-us        # US English
/voice en-gb        # British English
/voice de           # German
```

The available voices depend on which Piper models are bundled in the ISO and which espeak-ng voices are installed.

## Hardware-adaptive selection

AiOS checks your hardware at boot and selects the best voice configuration:

| Hardware profile | STT | TTS |
|-----------------|-----|-----|
| Raspberry Pi / low-end | whisper-tiny | Piper (light voice) |
| Standard desktop | whisper-medium | Piper (quality voice) |
| Desktop with GPU | whisper-large | Piper (quality voice) |

This happens automatically. You do not need to configure anything unless you want to override the defaults.

## Audio system

AiOS uses **PipeWire** for audio input and output. PipeWire is the modern Linux audio system that replaces PulseAudio and JACK.

To check audio status:

```
/sysinfo
```

This shows detected audio devices, active sources, and sinks.

## Disabling voice entirely

If you prefer a text-only experience:

```
/mic off
/speaker off
```

With both disabled, AiOS operates purely through the text input and chat display. No audio processing occurs, which also saves CPU resources.

## Troubleshooting

- **Microphone not detected:** Check that your microphone is connected and recognized by PipeWire. See [Audio Problems](../troubleshooting/audio.md).
- **Voice recognition is poor:** Try setting the language explicitly with `/language`. Ensure you are in a reasonably quiet environment.
- **TTS voice sounds bad:** Try a different voice with `/voice`. Piper voices sound significantly better than espeak-ng.
- **High CPU usage from voice:** The Whisper model may be too large for your hardware. This is auto-detected, but if you upgraded RAM and the model did not change, try rebooting.
