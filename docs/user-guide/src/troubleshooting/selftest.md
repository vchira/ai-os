# Self-Test

AiOS includes a built-in self-test system that verifies all components are working correctly. Running a self-test is the fastest way to diagnose problems.

## Running self-tests

### From the chat

Type in the chat input:

```
/selftest
```

This runs all available tests.

### From the command line

Open a terminal (`Alt+Enter`) and run:

```bash
aios-test
```

### Filtered tests

Run only specific test categories:

```
/selftest quick           # Fast checks -- API, filesystem, basic functionality
/selftest channel         # Channel connectivity tests
/selftest tools           # Verify all 12 built-in tools
/selftest interactive     # Tests that require user interaction (voice, UI)
```

## What gets tested

The self-test suite covers 14 scenarios across all system components:

### Quick tests
- **API connectivity** -- can the system reach the configured LLM provider?
- **Vault access** -- is the encrypted vault readable and decryptable?
- **Filesystem access** -- can the files tool read and write in the home directory?
- **Configuration** -- are all settings valid and consistent?

### Channel tests
- **Desktop** -- is the GTK4 interface running?
- **Web channel** -- is the HTTP server listening and WebSocket functional?
- **Signal** -- is signal-cli running and connected? (if enabled)

### Tool tests
- **Each built-in tool** is invoked with a test input to verify it executes without errors
- Memory tools: write and read back a test value
- System tools: run a safe command (like `echo test`)
- File tools: create, read, and delete a temporary file
- Network tools: fetch a known URL
- UI tools: display a test notification

### Interactive tests
- **Voice input** -- can Whisper transcribe a test phrase?
- **Voice output** -- can Piper/espeak-ng produce audio?
- **Panel rendering** -- does a test panel display correctly?

## Reading results

Self-test results are displayed as a list of pass/fail indicators:

```
Self-Test Results
=================
 [PASS] API connectivity (claude)
 [PASS] Vault access
 [PASS] Filesystem read/write
 [PASS] Configuration valid
 [PASS] Desktop channel
 [PASS] Web channel
 [FAIL] Signal channel -- signal-cli not running
 [PASS] memory tool
 [PASS] system tool
 [PASS] files tool
 [PASS] web tool
 [PASS] display tool
 [PASS] Voice input (STT)
 [PASS] Voice output (TTS)

13/14 passed, 1 failed
```

Each failed test includes a brief explanation of what went wrong and often a hint about how to fix it.

## When to run self-tests

- **After first boot** -- verify everything is set up correctly
- **When something stops working** -- pinpoint which component failed
- **After updates** -- confirm the update did not break anything
- **Before reporting a bug** -- include self-test results in your report

## Continuous monitoring

You can also ask the AI for a quick health check at any time:

```
"Is everything working?"
"Run a health check"
"Check if the web channel is up"
```

The AI can run self-tests and interpret the results for you.

## Interpreting common failures

| Failed test | Likely cause | Fix |
|------------|-------------|-----|
| API connectivity | No internet or invalid key | Check network, verify API key with `/key` |
| Vault access | Locked vault or corrupted file | Enter master password, or `/configure` to recreate |
| Signal channel | signal-cli not running | `/channel signal on` |
| Voice input | No microphone or PipeWire issue | Check `/mic on`, see [Audio Problems](audio.md) |
| Voice output | No speakers or TTS engine error | Check `/speaker on`, see [Audio Problems](audio.md) |
| Web channel | Port conflict or server not started | `/channel web on`, check port with `/channel` |
