# License

## AiOS application

AiOS (the Rust application in `aios-app-rs/` and all scripts in this repository) is licensed under the **Business Source License 1.1 (BSL 1.1)**.

### What BSL 1.1 means in plain language

BSL 1.1 is a source-available license designed to be fair to both users and developers. Here is what it means for you:

**You are free to:**
- Use AiOS personally on your own machines
- Use AiOS in an educational context (schools, universities, self-learning)
- Deploy AiOS internally within your company for your own employees
- Evaluate AiOS to decide if it fits your needs
- Read, study, and modify the source code for the above purposes

**You need written approval from [swIT.work GmbH](https://swit.work) for:**
- Distributing AiOS or a modified version to others (commercially or publicly)
- Building a derivative distribution based on AiOS
- Offering AiOS as a hosted or cloud service to customers
- Bundling AiOS with hardware for sale

**Change date:** After 5 years from the release date of each version, that version automatically converts to the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0), which is a fully permissive open-source license.

The full license text is in the repository:

> **[github.com/swit-work/ai-os — LICENSE](https://github.com/swit-work/ai-os/blob/main/LICENSE)**

For commercial licensing inquiries, contact: [swit.work](https://swit.work)

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
