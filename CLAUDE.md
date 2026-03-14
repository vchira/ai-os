# AiOS — AI-Native Operating System

## Core Philosophy

AiOS is an AI-first bare-metal x86 operating system. **Humans only interact through the prompt.** The OS exists as a substrate for AI — all system abstractions are designed for AI consumption, not human convenience.

The AI is the primary actor. Humans ask questions or express intent through the prompt. The AI interprets intent, decides what to do, and uses kernel primitives to fulfill it.

## AI Interaction Model

This OS is interrogated by humans, and it must be built so that AIs can use it easily to solve tasks for humans.

- Humans interact through the prompt — they describe what they want
- The AI receives the human's request along with system context (date/time, memory, hardware state)
- If the AI can answer directly, it does
- **If a tool is needed** to execute what the human wants:
  1. The AI responds that it needs a tool and explains exactly what it can do
  2. If the human allows it, the AI creates the tool and uses it
  3. The AI explains exactly what the tool does, what it did, and shows the result
- The AI should never silently execute something the human didn't ask for
- When a new capability is needed that doesn't exist yet, the AI should tell the human "I need a tool for X" and if approved, the tool gets built and registered
- Tools are text-based: `TOOL_CALL:<name>` / `TOOL_INPUT:<json>` format, works across all LLM providers
- API keys can be set at runtime via `/key claude <key>` or `/key openai <key>` — no rebuild needed

## Architecture

- **No filesystem** — AI uses a key-value memory store (memorize/recall/forget)
- **No human-centric abstractions** — no task managers, no file browsers, no app launchers
- **Intent Syscall API (INT 0x80)** — AI expresses intents (RENDER, NOTIFY, MEMORIZE, RECALL, QUERY), kernel fulfills them
- **Context Frame** — 32-byte kernel struct with system state that AI reads before acting
- **Direct TLS** — mbedTLS over lwIP for native HTTPS to API servers (no proxy needed)
- **Hardware**: i386, VGA text mode, RTL8139 NIC, CMOS RTC, PIT timer

## Build

```
make clean && make     # builds build/aios.iso
make run               # launches in QEMU with RTL8139 NIC
```

API keys can be set two ways:
1. **Build-time**: in `.env` file (compiled into kernel)
2. **Runtime**: type `/key claude sk-ant-...` or `/key openai sk-...` at the prompt

```
# .env (build-time defaults)
CLAUDE_API_KEY = sk-ant-...
OPENAI_API_KEY = sk-...
```

## Key Files

- `kernel/kernel.asm` — boot sequence and init chain
- `lib/llm_provider.c` — LLM provider abstraction, tool-call loop
- `lib/tls_client.c` — TLS HTTPS client (mbedTLS + lwIP)
- `lib/tool_executor.c` — AI memory store + tool dispatch
- `shell/ai_prompt.asm` — the prompt (human↔AI interface)
- `include/mbedtls_config.h` — minimal mbedTLS config for bare metal
