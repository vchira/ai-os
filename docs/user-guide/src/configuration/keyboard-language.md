# Keyboard & Language

AiOS supports multiple keyboard layouts and languages. You can change these settings at any time.

## Keyboard layout

Set the keyboard layout to match your physical keyboard:

```
/keyboard us       # US English (QWERTY)
/keyboard de       # German (QWERTZ)
/keyboard fr       # French (AZERTY)
/keyboard gb       # UK English
/keyboard es       # Spanish
/keyboard it       # Italian
/keyboard pt       # Portuguese
/keyboard ru       # Russian
```

The change takes effect immediately. The layout affects all text input across the system, including the chat input and any terminal windows.

### Finding your layout code

Keyboard layout codes follow the XKB naming convention. Common layouts:

| Layout | Code | Notes |
|--------|------|-------|
| US English | `us` | Default |
| German | `de` | QWERTZ, includes umlauts |
| French | `fr` | AZERTY |
| UK English | `gb` | Similar to US with some key differences |
| Spanish | `es` | Includes accented characters |
| Italian | `it` | Standard Italian layout |
| Portuguese | `pt` | Portuguese layout |
| Brazilian Portuguese | `br` | ABNT2 layout |
| Russian | `ru` | Cyrillic layout |
| Japanese | `jp` | Japanese 109-key layout |
| Korean | `kr` | Korean layout |

You can also ask the AI: "What keyboard layouts are available?"

## Language settings

### Speech recognition language

Set the language for voice recognition:

```
/language en       # English
/language de       # German
/language fr       # French
/language          # Auto-detect (default)
```

This affects only the speech-to-text system (Whisper). See [Voice Settings](voice.md) for more details.

### AI response language

The AI responds in whatever language you speak or write to it. If you write in German, it responds in German. If you speak in French, it responds in French.

There is no separate setting for the AI's language -- it follows your lead automatically.

### System language

The AiOS system interface (labels, status messages, error messages) is in English. The AI conversation itself can be in any language that the LLM provider supports.

## Autoconfig

Keyboard and language can be pre-configured through the autoconfig file:

```json
{
  "system": {
    "keyboard": "de",
    "language": "en",
    "timezone": "Europe/Berlin"
  }
}
```

- `keyboard` -- the keyboard layout code
- `language` -- the default STT language code
- `timezone` -- the system timezone (e.g., "America/New_York", "Europe/Berlin", "Asia/Tokyo")

## Changing settings through the AI

You can also configure these settings through natural language:

- "Change the keyboard layout to German"
- "Set the timezone to Pacific time"
- "I want to speak to you in French"

The AI will use the appropriate command or configuration change.
