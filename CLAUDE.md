# AiOS — AI-Native Linux Distribution

## Core Philosophy

AiOS is an AI-first Linux distribution. **Humans interact through voice and text.** The OS exists as a substrate for AI — the AI is the primary interface between human intent and system execution.

The AI is the primary actor. Humans ask questions or express intent through voice or the text prompt. The AI interprets intent, decides what to do, and uses tools to fulfill it.

## AI Interaction Model

- Humans interact through **voice** (default) or **text prompt**
- Voice input uses local Whisper STT (multilingual, accent-aware)
- Voice output uses local TTS (Piper for quality, espeak-ng as fallback — configurable)
- Voice can be toggled independently: mic on/off, speaker on/off
- When voice is disabled, the interface falls back to text-only
- The AI receives the human's request along with system context (date/time, tools, hardware state)
- If the AI can answer directly, it does
- **If a tool is needed** to execute what the human wants:
  1. The AI responds that it needs a tool and explains exactly what it can do
  2. If the human allows it, the AI uses the tool
  3. The AI explains exactly what the tool does, what it did, and shows the result
- The AI should never silently execute something the human didn't ask for
- Tools are expandable via a plugin system
- API keys are stored in an encrypted vault (AES-256-GCM + Argon2id)

## Multi-Channel System

AiOS has **one AI brain, one conversation, multiple output surfaces**. The active channel is where the user is currently interacting. Whoever sends a message last owns the channel.

### Channels

| Channel | Interface | Capabilities | How |
|---------|-----------|-------------|-----|
| **Desktop** | GTK4/libadwaita | Full (panels, images, markdown) | Default, always available |
| **Web** | Browser via WebSocket | Full (HTML forms, images, markdown) | `http://aios.local` on the LAN |
| **Signal** | Signal messenger | Limited (text, images, no panels) | Via `signal-cli` daemon |
| **Voice** | TTS/STT | Minimal (spoken text only) | Via PipeWire + Whisper/Piper |

### Channel Switching

- When a message arrives from Signal → channel switches to Signal
- Desktop shows a modal overlay: "AI is talking on Signal" + "Switch back here" button
- Clicking the button switches back to Desktop; Signal gets "Conversation moved to desktop"
- Same for Web: opening the browser UI and sending a message switches the channel

### Tool Channel Awareness

Tools check per-channel capabilities (`ChannelCapabilities` struct) to adapt their output:
- `rich_panels` — GTK dialogs, HTML forms (Desktop, Web only)
- `images` — image display (Desktop, Web, Signal — not Voice)
- `markdown` — rich text formatting (Desktop, Web — not Signal, Voice)
- `notifications` — popups/toasts (Desktop, Web only)
- `structured_input` — multi-field forms (Desktop, Web only)
- `password_input` — masked entry (Desktop, Web only)
- `max_text_length` — message size limits (Signal: 4096 chars)

Most tools (10/12) are channel-agnostic — they return text regardless. Two tools adapt:
- **`ui_panel`**: GTK dialog on Desktop, HTML form on Web, numbered text choices on Signal
- **`display`**: Native rendering on Desktop/Web, text fallbacks on Signal/Voice

Tools implement `execute_on_channel(args, channel)` — the default delegates to `execute()` ignoring the channel.

### Configuration

```
/channel                    Show channel status
/channel web on|off         Enable/disable web channel
/channel signal on|off      Enable/disable Signal
/channel web port <N>       Set web server port (default: 80)
/channel signal phone <N>   Set Signal phone number
```

### Boot Status

On startup, AiOS shows an INFO message with green/red indicators for each channel:
```
ℹ️ [INFO] AiOS System Status
  Boot time: 2026-03-16 12:00:00 UTC

  ✅ Desktop: available — GTK4/libadwaita
  ✅ Web Channel: available — http://aios.local:80
  ❌ Signal: unavailable — disabled (/channel signal on)
  ✅ LLM Provider: available — claude
  ✅ Voice: available — STT: on | TTS: on
```

## Architecture

- **Base**: Debian Bookworm (12) — maximum hardware support with non-free firmware
- **Display**: Wayland compositor (labwc, built from source) — lightweight, wlroots-based
- **Application**: **Rust** + GTK4/libadwaita — native binary, ~6MB, no runtime dependencies
- **Voice STT**: whisper.cpp (local inference, swappable via SttEngine trait)
- **Voice TTS**: Piper / espeak-ng (swappable via TtsEngine trait, auto-selects based on hardware)
- **LLM Providers**: Claude API, OpenAI API — switchable at runtime, model-agnostic via LlmProvider trait
- **Tools**: 12 built-in tools + plugin system with categories and dynamic routing
- **Multi-Channel**: Desktop (GTK), Web (axum + WebSocket), Signal (signal-cli) — one AI brain, multiple surfaces
- **Security**: Encrypted vault for secrets, permission system with auth framework
- **Audio**: PipeWire (modern Linux audio)
- **mDNS**: Avahi — web UI reachable as `http://aios.local` on the LAN

### Rust Workspace (`aios-app-rs/`)

| Crate | Purpose |
|-------|---------|
| `aios-core` | Shared types, config, commands, channel system, AppRuntime, secure vault, auth, permissions, episodic memory, semantic index, selftest |
| `aios-llm` | LLM provider trait + Claude/OpenAI implementations, caching, context pruning, pre-fetch, effort levels, multi-agent orchestrator |
| `aios-tools` | Tool trait + 12 built-in tools (channel-aware), plugin registry, sandbox execution, tool categories |
| `aios-voice` | TTS/STT engine traits + Piper, espeak-ng, Whisper backends, audio I/O via cpal |
| `aios-gtk` | GTK4/libadwaita UI binary — chat, prompt, settings, auth dialogs, setup wizard, panel renderer, channel overlay |
| `aios-web` | Web channel — axum HTTP server, WebSocket, full HTML/CSS/JS chat client (mirrors desktop) |
| `aios-signal` | Signal messenger channel — signal-cli integration, listener + sender |

## Build

```bash
# Build and boot in QEMU (incremental — only rebuilds what changed)
./start.sh

# Full clean rebuild + boot
./start.sh --clean

# Clean all build artifacts
./clean.sh

# Deep clean (also purge Docker cache volumes)
./clean.sh --deep

# Release
./release.sh alpha|beta|release   # Build + publish to GitHub Releases
./deploy-website.sh               # Deploy website to Cloudflare Pages

# Development workflow
make check              # Fast compilation check
make test               # Run all 563+ workspace tests
make build              # Build release binary
make dev                # check + test

# Build only the Rust binary (for development)
cd aios-app-rs && cargo build --release
```

### Prerequisites

For ISO building:
```bash
# Docker is required (builds inside Debian Bookworm container)
docker --version
```

For local Rust development:
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libasound2-dev libssl-dev
```

For QEMU testing with clipboard sharing:
```bash
sudo apt install virt-viewer
```

### API Keys

Set in `.env` (baked into ISO at build time):
```
CLAUDE_API_KEY = sk-ant-...
OPENAI_API_KEY = sk-...
```

Or set at runtime via the first-boot setup wizard, `/key` command, or Settings dialog.

API keys are stored in the encrypted vault (`~/.aios/vault.enc`).

## Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/key <provider> <key>` | Set API key (stored in encrypted vault) |
| `/provider <name>` | Switch LLM provider (claude, openai) |
| `/model <name>` | Set model for current provider |
| `/effort <low\|medium\|high\|auto>` | Set AI effort level (low=fast, high=thorough thinking) |
| `/mode <saver\|balanced\|thorough>` | Set quality/cost mode (see Cost Optimization section) |
| `/keyboard <layout>` | Set keyboard layout (e.g., de, fr, us) |
| `/resolution <WxH>` | Set screen resolution |
| `/theme <dark\|light\|auto>` | Set UI theme |
| `/voice <voice-id>` | Set TTS voice |
| `/mic <on\|off>` | Toggle voice input |
| `/speaker <on\|off>` | Toggle voice output |
| `/language <code>` | Set STT language (blank=auto) |
| `/tools` | List available tools |
| `/channel` | Show/configure channels (web on/off, signal on/off, port, phone) |
| `/selftest [filter]` | Run self-tests (quick, channel, tools, interactive) |
| `/sysinfo` | Show system monitor (CPU, memory, disk, processes) |
| `/close` | Close topmost panel/dialog |
| `/update <url>` | Self-update AiOS binary from URL |
| `/wake <phrase>` | Set wake word phrase |
| `/configure` | Re-run first-boot setup wizard |
| `/clear` | Clear chat history |
| `/info` | Show system information |

## Built-in Tools (12)

| Tool | Category | Description |
|------|----------|-------------|
| `memory` | memory | Key-value persistent store for AI to remember things |
| `reflect` | memory | Record structured reflections (what happened, outcome, lessons) |
| `recall_episodes` | memory | Search past experiences and reflections |
| `system` | system | Run commands (sandboxed), get system info, list processes |
| `execute_code` | system | Run Python/Bash/JS/Rust in sandbox (Docker or process isolation) |
| `delegate_to` | system | Spawn specialized sub-agents (coder, researcher, sysadmin, analyst) |
| `files` | filesystem | Read/write/search files (scoped to home dir) |
| `process_data` | filesystem | Zero-copy local data processing (CSV, JSON, logs — never loads full files) |
| `find_content` | filesystem | Semantic file search by meaning ("find meeting notes from last week") |
| `web` | network | Fetch URLs, DuckDuckGo search, download files |
| `display` | ui | Show images, notifications, markdown |
| `ui_panel` | ui | Show input panels to user (text, password, dropdown, choice, toggle, etc.) |

## Cost Optimization & LLM Intelligence

AiOS implements 8 optimizations to minimize API costs while maximizing response quality. Combined, these can reduce LLM costs by 70-90% compared to naive usage.

### Quality Modes (`/mode` command)

Three user-selectable modes that control cost vs quality tradeoff:

| Mode | Command | Starting Model | Escalation | Use When |
|------|---------|---------------|------------|----------|
| **Saver** | `/mode saver` | Cheapest (Haiku/mini) | Aggressive heuristics | Budget-conscious, simple tasks |
| **Balanced** | `/mode balanced` | Auto-detected | Only on tool errors, empty response, user "try again" | Daily use (default) |
| **Thorough** | `/mode thorough` | Best (Opus/GPT-4) | None — already at top | Complex analysis, critical decisions |

Each LLM adapter maps modes to their own model lineup. Easy to tune per-provider without changing the manager.

### Cascading Model Routing (Escalation)

Instead of always using the most expensive model, AiOS starts with the cheapest appropriate model and escalates only when needed:

1. User sends message → auto-detect complexity → pick starting tier
2. Send to cheap model → check response quality
3. If response is good → done (saved money)
4. If response failed → escalate to next tier → retry

**Escalation triggers by mode:**
- **Saver**: Short response, "I cannot" phrases, hedging, tool errors, user retry
- **Balanced**: Tool errors, empty response, user says "try again" (reliable signals only)
- **Thorough**: No escalation (already at top)

Escalation path: Low → Medium → High → give up (max 2 escalations).

**Estimated savings**: ~60% — most queries are handled by cheap models, only complex ones escalate.

### Semantic Response Cache

Identical or similar questions return cached results without calling the LLM at all:

- "What's the disk usage?" → cached
- "How much space is left on the drive?" → same keywords, cache hit (Jaccard similarity ≥ 0.7)
- TTL: 5 minutes, max 200 entries
- Skips: slash commands, tool-call responses, responses > 2000 chars

**Estimated savings**: 100% on cache hits. Repeated questions (common in an OS) cost zero.

### Dynamic Tool Bundling

Tools are categorized (filesystem, memory, system, network, ui). Only relevant tools are sent to the LLM based on message keywords:

- "Read my config file" → sends only filesystem tools
- "Search the web" → sends only network tools
- Unknown intent → sends all tools (fallback)

**Estimated savings**: ~80% fewer tool-definition tokens per request.

### Context Pruning (Recursive Summarization)

When conversation history exceeds 10K tokens, old messages are summarized into a compact memo:

1. First 8K tokens → summarized by the AI into a ~500 token memo
2. Memo replaces the old messages
3. AI "remembers" hours of conversation without paying for every word

**Estimated savings**: Prevents linear cost growth over long conversations.

### Effort Levels (Auto-Detection)

System automatically detects query complexity and picks the right model tier:

| Detection | Effort | Model |
|-----------|--------|-------|
| Short/simple, greetings, file listings | Low | Haiku / GPT-4o-mini |
| Standard questions, most tasks | Medium | Sonnet / GPT-4o |
| "refactor", "security audit", "analyze all", destructive ops | High | Extended thinking |

User can override with `/effort low|medium|high|auto`.

### Cache Warming (Prompt Caching)

Provider-controlled cloud prompt caching (currently Claude-specific):

- System prompt + tools marked with `cache_control` and 1-hour TTL
- Fingerprint (hash of prompt + tools) saved to `~/.aios/cache_fingerprint.json`
- On restart: re-sends identical fingerprint → cloud cache hit in milliseconds
- Background keep-warm ping every 55 min when idle
- Provider controls all timing via `cache_config()` trait method
- **90% cost reduction** on cached prompt tokens

### Speculative Pre-fetch

When the user mentions a file, URL, or system info, AiOS starts fetching data in parallel while the LLM thinks:

- "Analyze my log files" → starts reading logs immediately
- When the tool call arrives → data already in memory
- 2-second timeout, non-blocking

**Estimated savings**: Latency reduction (faster UX), not direct cost savings.

### Zero-Copy Data Processing

Large files are never sent to the LLM. The `process_data` tool processes locally:

- CSV: column names + row count + 5 sample rows (not the full 10MB file)
- Logs: extracted error/warning lines only
- JSON: queried by dot-path, returns matching values

**Estimated savings**: Avoids sending megabytes of data as context tokens.

### Cost Savings Summary

| Optimization | Savings | How |
|---|---|---|
| Quality modes + cascading | ~60% | Start cheap, escalate only when needed |
| Semantic cache | 100% on hits | Similar questions return cached answers |
| Tool bundling | ~80% tool tokens | Send only relevant tools |
| Context pruning | Prevents runaway | Summarize instead of growing forever |
| Prompt caching | 90% on prefix | Cloud cache for system prompt + tools |
| Zero-copy | Variable | Process data locally, send results |
| **Combined estimate** | **70-90%** | Compared to sending everything every time |

## Security

### Encrypted Vault
- Master password → Argon2id key derivation → AES-256-GCM encryption
- Stores API keys, passwords, logins, personal info
- File: `~/.aios/vault.enc` (16-byte salt + 12-byte nonce + ciphertext)

### Authentication Framework
- Pluggable `Authenticator` trait — password built-in, voice/biometric/hardware key extensible
- "Don't ask for [N] minutes" cache with configurable timeout
- Password always available as fallback

### Permission System
- AI must request permission before accessing secrets
- Shows: what's being accessed, why, with auth if needed
- Per-key permission cache with independent timeout from auth cache

## Multi-Agent System

The AI can delegate complex tasks to specialized sub-agents via the `delegate_to` tool:

| Agent Type | Specialization | Tools |
|------------|---------------|-------|
| Coder | Code analysis, refactoring, debugging | filesystem |
| Researcher | Web search, information gathering | network, memory |
| SysAdmin | System commands, process management | system |
| FileManager | File operations, organization | filesystem |
| Analyst | Data analysis, CSV/JSON processing | filesystem, network |

Each sub-agent gets its own conversation context, specialized system prompt, and relevant tool subset.

## Episodic Memory

The AI stores structured reflections about past interactions:
- **What happened** (summary + details)
- **Outcome** (success, failure with reason, partial)
- **Lessons learned** (reusable insights)
- **Tags** (searchable categories)

Recent episodes are injected into the system prompt, giving the AI "experience" to draw from.

## Voice System

### Swappable Backends
```
TtsEngine trait → PiperTts (quality, RPi-compatible)
                → EspeakTts (fallback, all languages including Romanian)
                → [future: CoquiXtts for voice cloning]

SttEngine trait → WhisperStt (whisper.cpp, multiple model sizes)
                → [future: Vosk for lightweight STT]
```

### Hardware-Adaptive
Auto-selects the best backend based on available hardware (RAM, GPU, CPU cores):
- RPi: Piper + whisper-tiny, espeak-ng fallback
- Desktop: Piper + whisper-medium
- GPU: Piper + whisper-large

### Supported Languages
Romanian, English (US/UK), German, French, Spanish, Italian, Portuguese, Japanese, Chinese, Korean, Hindi, and more.

## Sandbox Execution

Commands and code are executed in isolated environments:

| Level | When | How |
|-------|------|-----|
| None | Read-only commands (ls, cat, ps) | Direct execution |
| Process | Modifying commands (mv, cp, mkdir) | Timeout + resource limits |
| Docker | Dangerous commands (rm, pip, python, bash -c) | Container with --rm, --network=none, memory limits |

Code execution (`execute_code` tool) always uses sandbox — Docker preferred, process fallback.

## Key Files

### Rust Application (`aios-app-rs/`)
- `aios-core/src/config/` — Configuration manager, defaults, slash commands, command autocomplete
- `aios-core/src/types/` — Message, ToolResult, Voice types, EffortLevel, MessageLevel, BootStatus, RichText
- `aios-core/src/channel/` — ChannelKind, ChannelContext, ChannelSwitcher, AppRuntime
- `aios-core/src/selftest/` — Self-test runner + 14 scenarios (auto + interactive)
- `aios-core/src/secure/` — Vault, auth framework, permission system
- `aios-core/src/memory/` — Episodic memory, semantic file index
- `aios-llm/src/provider.rs` — LlmProvider trait, CacheConfig
- `aios-llm/src/claude.rs` — Claude API with caching + effort levels
- `aios-llm/src/openai.rs` — OpenAI API with effort levels
- `aios-llm/src/manager.rs` — Provider manager, tool-call loop, all optimizations, channel-aware system prompt
- `aios-llm/src/agents.rs` — Multi-agent orchestrator
- `aios-llm/src/context.rs` — Context pruning via recursive summarization
- `aios-llm/src/prefetch.rs` — Speculative pre-fetch engine
- `aios-tools/src/tool.rs` — Tool trait with categories + `execute_on_channel`
- `aios-tools/src/sandbox.rs` — Sandbox execution (Process + Docker)
- `aios-tools/src/builtin/` — 12 built-in tools (ui_panel + display are channel-aware)
- `aios-voice/src/stt/` — STT engine trait + Whisper backend
- `aios-voice/src/tts/` — TTS engine trait + Piper, espeak-ng backends
- `aios-voice/src/audio/` — Audio capture/playback via cpal
- `aios-gtk/src/app.rs` — Application entry, first-boot, LLM wiring, channel startup, unified message loop
- `aios-gtk/src/ui/` — GTK4 widgets (chat, prompt, settings, auth, panels, setup, channel overlay)
- `aios-web/src/server.rs` — axum HTTP + WebSocket server
- `aios-web/src/protocol.rs` — JSON message protocol (client ↔ server)
- `aios-web/src/static/index.html` — Full web chat client (HTML/CSS/JS, no build system)
- `aios-signal/src/listener.rs` — signal-cli daemon reader
- `aios-signal/src/sender.rs` — Signal message/image sender
- `aios-signal/src/panel.rs` — Panel → text format converter for Signal

### Distribution (`distro/`)
- `build.sh` — Docker-based ISO builder (compiles Rust inside Bookworm)
- `_inner_build.sh` — live-build configuration, packages, hooks
- `run-vm.sh` — QEMU launcher with audio + SPICE clipboard
- `Dockerfile` — Bookworm builder image with Rust + GTK4 dev libs

### Root Scripts
- `start.sh` — Build + boot (`--clean` for full rebuild)
- `clean.sh` — Remove all artifacts (`--deep` for Docker volumes)
- `release.sh` — Secret scan + ISO build + GitHub Release publishing
- `deploy-website.sh` — Deploy website to Cloudflare Pages
- `RELEASE.md` — Release process documentation
- `.env` — API keys (not in git, baked into ISO at build time)

## First-Boot Setup

On first launch (no vault exists), AiOS runs a conversational setup:

1. AI speaks: "Welcome to AiOS!"
2. Choose AI provider (Claude / OpenAI) — voice or click
3. Enter API key — typed (secure input)
4. Create master password — typed
5. Optionally add backup provider
6. If multiple: choose primary/fallback order
7. All stored in encrypted vault → ready to chat

The setup uses the same `ui_panel` tool the AI uses for all user input — not hardcoded UI.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Super` | Open/focus AiOS window |
| `Ctrl+Space` | Open/focus AiOS window |
| `Alt+Enter` | Open terminal (foot) |
| `Alt+F4` | Close window |
| `Alt+F11` | Toggle fullscreen |
| `Print` | Screenshot (grim) |
