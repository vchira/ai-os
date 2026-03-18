# Cost Optimization

AiOS implements eight optimization strategies to minimize LLM API costs while maintaining response quality. Combined, these can reduce costs by 70-90% compared to sending every message to the most expensive model.

## Quality modes

Three user-selectable modes control the cost vs quality tradeoff:

```
/mode saver        # Cheapest models, aggressive optimization
/mode balanced     # Default -- good quality, reasonable cost
/mode thorough     # Best models, no cost optimization
```

| Mode | Starting model | When it escalates | Best for |
|------|---------------|-------------------|----------|
| **Saver** | Cheapest (Haiku / GPT-4o-mini) | Aggressively -- short responses, hedging, tool errors | Budget-conscious use, simple tasks |
| **Balanced** | Auto-detected by complexity | Only on tool errors, empty responses, user retry | Daily use (default) |
| **Thorough** | Best (Opus / GPT-4) | Never -- already at top tier | Complex analysis, critical decisions |

## Effort levels

AiOS automatically detects query complexity and picks the right model tier:

```
/effort auto       # Let the system decide (default)
/effort low        # Always use fast, cheap model
/effort medium     # Always use mid-tier model
/effort high       # Always use top-tier model with extended thinking
```

Auto-detection examples:

| Your message | Detected effort | Model tier |
|-------------|----------------|------------|
| "Hi" | Low | Haiku / GPT-4o-mini |
| "List files in my home directory" | Low | Haiku / GPT-4o-mini |
| "Explain how TCP/IP works" | Medium | Sonnet / GPT-4o |
| "Refactor this code for security" | High | Opus / GPT-4 with thinking |
| "Analyze all log files for anomalies" | High | Opus / GPT-4 with thinking |

## Cascading model routing

Instead of always using the most expensive model, AiOS starts cheap and escalates only when the response quality is poor:

1. Your message arrives
2. AiOS picks the starting model tier based on effort level and quality mode
3. Sends to the cheap model
4. Checks the response quality
5. If good -- done (saved money)
6. If poor (empty response, tool error, hedging) -- escalates to the next tier
7. Maximum 2 escalations: Low -> Medium -> High -> give up

**Estimated savings: ~60%.** Most queries are handled by cheap models.

## Semantic response cache

If you ask the same (or a very similar) question again, AiOS returns the cached response without calling the LLM:

- "What's the disk usage?" -- calls the LLM
- "How much disk space is left?" -- similar enough (Jaccard similarity >= 0.7), returns cached answer
- Cache TTL: 5 minutes
- Max 200 cached entries

**Estimated savings: 100% on cache hits.** Repeated questions (common in an OS context) cost nothing.

## Dynamic tool bundling

AiOS categorizes tools (filesystem, memory, system, network, ui) and only sends relevant tools to the LLM based on your message:

- "Read my config file" -- sends only filesystem tools
- "Search the web" -- sends only network tools
- Unclear intent -- sends all tools (fallback)

**Estimated savings: ~80% fewer tool-definition tokens per request.**

## Context pruning

When conversation history exceeds approximately 10,000 tokens, AiOS summarizes older messages:

1. Takes the first ~8,000 tokens of history
2. Asks the AI to summarize into a ~500 token memo
3. Replaces the old messages with the memo
4. The AI still "remembers" key points from hours of conversation

**Savings: Prevents linear cost growth** over long conversations.

## Prompt caching

For providers that support it (currently Claude), AiOS uses cloud-side prompt caching:

- System prompt and tool definitions are marked with cache hints
- A fingerprint (hash) is saved to `~/.aios/cache_fingerprint.json`
- On restart, the same fingerprint triggers a cloud cache hit
- Background keep-warm ping every 55 minutes when idle
- **90% cost reduction** on cached prompt tokens

This is automatic -- no configuration needed.

## Speculative pre-fetch

When you mention a file, URL, or system resource, AiOS starts fetching data in parallel while the LLM processes your request:

- "Analyze my log files" -- starts reading logs immediately
- When the tool call arrives -- data is already in memory
- 2-second timeout, non-blocking

This reduces latency rather than direct cost.

## Zero-copy data processing

Large files are never sent to the LLM. The `process_data` tool processes them locally:

- CSV: sends column names + row count + 5 sample rows (not the full file)
- Logs: extracts error/warning lines only
- JSON: queries by dot-path, returns matching values

**Savings: Variable.** Avoids sending megabytes of context tokens.

## Summary

| Optimization | Savings | How |
|---|---|---|
| Quality modes + cascading | ~60% | Start cheap, escalate only when needed |
| Semantic cache | 100% on hits | Similar questions return cached answers |
| Tool bundling | ~80% tool tokens | Send only relevant tools |
| Context pruning | Prevents runaway growth | Summarize instead of growing forever |
| Prompt caching | 90% on cached prefix | Cloud cache for system prompt + tools |
| Zero-copy processing | Variable | Process data locally, send summaries |
| **Combined estimate** | **70-90%** | Compared to naive send-everything approach |

## Practical tips

- Use **saver mode** for routine tasks like file operations and system checks
- Use **thorough mode** only when you need deep analysis or complex reasoning
- The default **balanced mode** is a good choice for most users
- Let effort auto-detection do its job -- it is usually right
- If a response seems weak, say "try again" -- the system will escalate to a better model
