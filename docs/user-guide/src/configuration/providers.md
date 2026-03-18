# LLM Providers

AiOS uses cloud-based large language models to power its AI assistant. It supports two providers, and you can switch between them at any time.

## Supported providers

| Provider | Models | API key prefix |
|----------|--------|---------------|
| **Anthropic Claude** | Claude Haiku, Sonnet, Opus | `sk-ant-` |
| **OpenAI** | GPT-4o-mini, GPT-4o, GPT-4 | `sk-` |

## Setting up a provider

### During first boot

The setup wizard asks you to choose a provider and enter an API key. This is the easiest way to get started.

### After setup

You can add or change API keys at any time:

```
/key claude sk-ant-api03-your-key-here
/key openai sk-your-key-here
```

Keys are stored in the encrypted vault (`~/.aios/vault.enc`), never in plain text.

## Switching providers

To switch the active provider:

```
/provider claude
/provider openai
```

The switch takes effect immediately. The next message you send will use the new provider.

## Choosing a specific model

Each provider has multiple models at different price and capability tiers. AiOS selects the model automatically based on your effort level and quality mode, but you can override it:

```
/model claude-sonnet-4-20250514
/model gpt-4o
```

To see which model is currently active:

```
/info
```

## Primary and backup providers

If you configure both providers, you can designate one as primary and the other as backup. The backup provider is used when the primary is unavailable (API errors, rate limiting, outages).

Set up both during the setup wizard, or add a second provider later with `/key`.

## Provider comparison

| Aspect | Claude | OpenAI |
|--------|--------|--------|
| Best for | Nuanced text, careful reasoning | Broad knowledge, code generation |
| Cheapest model | Haiku (very fast, very cheap) | GPT-4o-mini (fast, cheap) |
| Mid-tier model | Sonnet (balanced) | GPT-4o (balanced) |
| Top model | Opus (thorough thinking) | GPT-4 (detailed) |
| Prompt caching | Yes (90% cost savings on cached prompts) | Limited |
| Extended thinking | Yes (high effort level) | No |

AiOS is model-agnostic -- the `LlmProvider` trait abstracts away provider differences. Both providers support all AiOS features including tool use, effort levels, and cascading model routing.

## Getting API keys

### Anthropic Claude

1. Go to [console.anthropic.com](https://console.anthropic.com)
2. Create an account or sign in
3. Navigate to API Keys
4. Create a new key
5. Copy the key (starts with `sk-ant-`)

### OpenAI

1. Go to [platform.openai.com](https://platform.openai.com)
2. Create an account or sign in
3. Navigate to API Keys
4. Create a new key
5. Copy the key (starts with `sk-`)

Both services require a payment method. Costs depend on usage -- see [Cost Optimization](cost.md) for how AiOS minimizes API spending.

## Verifying your provider

To confirm your provider is working:

```
/selftest quick
```

This runs a quick test that includes verifying API connectivity. You can also simply send a message -- if the AI responds, your provider is working.
