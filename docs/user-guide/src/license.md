# License

AiOS is open-source software. The project and its components are licensed as follows:

## AiOS application

The AiOS application (the Rust codebase in `aios-app-rs/`) is licensed under the terms specified in the project repository.

See the LICENSE file in the repository root for the full license text:

> **[github.com/AiOS-Project/ai-os](https://github.com/AiOS-Project/ai-os)**

## Base system

AiOS is built on Debian Bookworm (12). Debian packages are licensed under their respective licenses (GPL, LGPL, MIT, BSD, and others). AiOS includes non-free firmware packages for maximum hardware compatibility.

## Third-party components

| Component | License | Project |
|-----------|---------|---------|
| Debian Bookworm | Various (GPL, LGPL, etc.) | [debian.org](https://www.debian.org) |
| labwc | GPL-2.0 | [labwc.github.io](https://labwc.github.io) |
| GTK4 | LGPL-2.1 | [gtk.org](https://www.gtk.org) |
| libadwaita | LGPL-2.1 | [gnome.org](https://gnome.pages.gitlab.gnome.org/libadwaita/) |
| whisper.cpp | MIT | [github.com/ggerganov/whisper.cpp](https://github.com/ggerganov/whisper.cpp) |
| Piper | MIT | [github.com/rhasspy/piper](https://github.com/rhasspy/piper) |
| espeak-ng | GPL-3.0 | [github.com/espeak-ng/espeak-ng](https://github.com/espeak-ng/espeak-ng) |
| PipeWire | MIT / LGPL-2.1 | [pipewire.org](https://pipewire.org) |
| signal-cli | GPL-3.0 | [github.com/AsamK/signal-cli](https://github.com/AsamK/signal-cli) |
| foot | MIT | [codeberg.org/dnkl/foot](https://codeberg.org/dnkl/foot) |
| grim | MIT | [sr.ht/~emersion/grim](https://sr.ht/~emersion/grim/) |

## LLM provider terms

Use of Anthropic Claude and OpenAI APIs is subject to their respective terms of service:

- [Anthropic Terms of Service](https://www.anthropic.com/terms)
- [OpenAI Terms of Use](https://openai.com/terms)

AiOS does not embed or redistribute any LLM models. It communicates with these services over their public APIs using your own API keys.
