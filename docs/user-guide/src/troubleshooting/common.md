# Common Issues

This chapter covers the most frequently encountered problems and their solutions.

## AI does not respond

**Symptoms:** You type a message but get no response, or you see an error about the LLM provider.

**Possible causes and fixes:**

1. **No API key configured.** Check with `/info`. If no provider is active, set your key:
   ```
   /key claude sk-ant-your-key-here
   ```

2. **Invalid API key.** The key may have been revoked or typed incorrectly. Set a new one:
   ```
   /key claude sk-ant-your-new-key
   ```

3. **No internet connection.** AiOS needs internet access to reach the LLM API. Check connectivity:
   - Ask the AI: "Can you reach the internet?" (if it is partially working)
   - Open a terminal (`Alt+Enter`) and run: `ping 8.8.8.8`
   - See [Network & Connectivity](network.md)

4. **Provider outage.** Switch to your backup provider:
   ```
   /provider openai
   ```

5. **Rate limiting.** If you are hitting API rate limits, wait a moment and try again. Consider switching to `/mode saver` to use cheaper models.

## Black screen after boot

**Symptoms:** The system boots but you see only a black screen or a cursor.

**Fixes:**

1. **Graphics driver issue.** Try adding `nomodeset` to the kernel boot parameters. At the boot menu, press `e` to edit the boot entry and add `nomodeset` to the kernel line.

2. **Resolution too high.** The display may be set to a resolution your monitor does not support. Try connecting a different monitor or booting in a VM.

3. **Wait longer.** On some hardware, the compositor takes a few extra seconds to start. Wait up to 30 seconds.

## Setup wizard does not appear

**Symptoms:** AiOS boots to a chat interface without running the setup wizard.

**Cause:** An autoconfig file was detected and applied automatically.

**Fix:** If you want to run the setup wizard manually:
```
/configure
```

## Vault is locked

**Symptoms:** The AI says it cannot access your API key or vault.

**Fix:** The vault may need to be unlocked with your master password. The AI will prompt you for the password. If you have forgotten it, the vault cannot be recovered -- you will need to delete `~/.aios/vault.enc` and run setup again:

```
/configure
```

## Commands do not work

**Symptoms:** Typing a slash command produces no result or an error.

**Fixes:**

1. **Check spelling.** Commands are case-sensitive. Use `/help` to see the exact command names.
2. **Check arguments.** Some commands require arguments. For example, `/key` needs both a provider name and a key value.
3. **Try autocomplete.** Type `/` and wait for the autocomplete popup to verify the command exists.

## System is slow

**Symptoms:** Responses take a long time, the interface is sluggish.

**Possible causes:**

1. **Insufficient RAM.** AiOS needs at least 4 GB. With less, the Whisper model and the application compete for memory.
2. **High effort level.** `/effort high` uses more expensive models that may take longer. Try `/effort auto` or `/effort low`.
3. **Large conversation history.** Context pruning happens automatically, but a very long conversation in a single session can slow things down. Try `/clear` to start fresh.
4. **Network latency.** Slow internet affects LLM response times. Check with `ping api.anthropic.com` from a terminal.

## Cannot find a setting

If you cannot find how to change something, ask the AI:

- "How do I change the keyboard layout?"
- "How do I set up a backup provider?"
- "Where are my settings stored?"

The AI has access to system documentation and can guide you to the right command or configuration file.
