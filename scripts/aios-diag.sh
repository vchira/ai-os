#!/bin/bash
# AiOS Diagnostic Script — run inside the VM to check everything
# Usage: ssh aios@assistant.local 'bash /opt/aios-app/aios-diag.sh'

RED='\033[0;31m'
GRN='\033[0;32m'
YLW='\033[0;33m'
NC='\033[0m'

pass() { echo -e "  ${GRN}✓ PASS${NC}: $1"; }
fail() { echo -e "  ${RED}✗ FAIL${NC}: $1"; }
warn() { echo -e "  ${YLW}⚠ WARN${NC}: $1"; }
info() { echo -e "  ${NC}  INFO${NC}: $1"; }

echo "═══ AiOS Diagnostic ═══"
echo ""

# 1. AiOS process
echo "── AiOS Process ──"
if pgrep -x aios >/dev/null; then
    pass "AiOS running (PID $(pgrep -x aios | head -1))"
else
    fail "AiOS NOT running"
    echo "  Boot log tail:"
    tail -5 /tmp/aios-boot.log 2>/dev/null
fi

# 2. LLM Provider
echo ""
echo "── LLM Provider ──"
PROVIDER=$(python3 -c "import json; print(json.load(open('/home/aios/.aios/config.json')).get('llm',{}).get('provider','?'))" 2>/dev/null)
MODEL=$(python3 -c "import json; c=json.load(open('/home/aios/.aios/config.json')).get('llm',{}); print(c.get(c.get('provider','')+'_model', '?'))" 2>/dev/null)
info "Active provider: $PROVIDER, model: $MODEL"

if [ "$PROVIDER" = "ollama" ]; then
    if pgrep ollama >/dev/null; then
        pass "Ollama process running"
    else
        fail "Ollama NOT running"
    fi

    if curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
        pass "Ollama API responding"
        MODELS=$(curl -s http://localhost:11434/api/tags | python3 -c "import sys,json; [print('    -', m['name']) for m in json.load(sys.stdin).get('models',[])]" 2>/dev/null)
        info "Installed models:"
        echo "$MODELS"

        # Test actual inference
        echo "  Testing inference with $MODEL..."
        RESP=$(timeout 30 curl -s http://localhost:11434/api/generate -d "{\"model\":\"$MODEL\",\"prompt\":\"say hello in one word\",\"stream\":false}" 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('response','')[:50])" 2>/dev/null)
        if [ -n "$RESP" ]; then
            pass "Inference works: '$RESP'"
        else
            fail "Inference returned empty response"
            # Check if model exists
            if curl -s http://localhost:11434/api/tags | grep -q "$MODEL"; then
                warn "Model exists but inference failed"
            else
                fail "Model '$MODEL' NOT installed — download may not have completed"
                info "Run: ollama pull $MODEL"
            fi
        fi
    else
        fail "Ollama API not responding on port 11434"
    fi
else
    info "Using cloud provider '$PROVIDER' — checking API key"
    KEY_SET=$(python3 -c "import json; c=json.load(open('/home/aios/.aios/config.json')).get('llm',{}); print('yes' if c.get('${PROVIDER}_api_key') else 'no')" 2>/dev/null)
    if [ "$KEY_SET" = "yes" ]; then
        pass "API key is set for $PROVIDER"
    else
        fail "No API key for $PROVIDER"
    fi
fi

# 3. KWS / Wake Word
echo ""
echo "── Wake Word (KWS) ──"
WAKE_ENABLED=$(python3 -c "import json; print(json.load(open('/home/aios/.aios/config.json')).get('voice',{}).get('wake_enabled', False))" 2>/dev/null)
WAKE_WORD=$(python3 -c "import json; print(json.load(open('/home/aios/.aios/config.json')).get('voice',{}).get('wake_word', '?'))" 2>/dev/null)
info "Wake word: '$WAKE_WORD', enabled: $WAKE_ENABLED"

KWS_LOGS=$(grep -c "kws\|KWS\|wake.*model\|tract" ~/.aios/logs/aios.log 2>/dev/null)
if [ "$KWS_LOGS" -gt 0 ]; then
    pass "KWS has $KWS_LOGS log entries"
    grep -i "kws\|wake.*model\|tract" ~/.aios/logs/aios.log 2>/dev/null | tail -3
else
    fail "ZERO KWS log entries — engine never initialized"
fi

# Check infrastructure models
if [ -d /opt/aios-app/models/kws/infrastructure ]; then
    MEL=$(ls /opt/aios-app/models/kws/infrastructure/melspectrogram.onnx 2>/dev/null && echo "yes" || echo "no")
    EMB=$(ls /opt/aios-app/models/kws/infrastructure/embedding_model.onnx 2>/dev/null && echo "yes" || echo "no")
    VAD=$(ls /opt/aios-app/models/kws/infrastructure/silero_vad.onnx 2>/dev/null && echo "yes" || echo "no")
    if [ "$MEL" = "yes" ] && [ "$EMB" = "yes" ] && [ "$VAD" = "yes" ]; then
        pass "Infrastructure models present (mel, emb, vad)"
    else
        fail "Missing infrastructure models: mel=$MEL emb=$EMB vad=$VAD"
    fi
else
    fail "Infrastructure models directory missing"
fi

# Check pretrained model
WW_FILE=$(echo "$WAKE_WORD" | tr ' ' '_')
if [ -f "/opt/aios-app/models/kws/pretrained/${WW_FILE}.onnx" ]; then
    pass "Pretrained model: ${WW_FILE}.onnx exists"
else
    fail "Pretrained model ${WW_FILE}.onnx NOT found"
    info "Available models:"
    ls /opt/aios-app/models/kws/pretrained/*.onnx 2>/dev/null | while read f; do echo "    - $(basename $f)"; done
fi

# 4. Voice / Mic
echo ""
echo "── Voice / Microphone ──"
STT_ENABLED=$(python3 -c "import json; print(json.load(open('/home/aios/.aios/config.json')).get('voice',{}).get('stt_enabled', False))" 2>/dev/null)
info "STT enabled: $STT_ENABLED"

if pgrep -f "arecord.*raw" >/dev/null; then
    pass "arecord capture running"
else
    fail "arecord NOT running — voice listener not capturing"
fi

if pgrep pipewire >/dev/null; then
    pass "PipeWire running"
else
    fail "PipeWire NOT running"
fi

# Test mic by temporarily stealing the device
echo "  Testing mic audio (2s)..."
pkill -f "arecord.*raw" 2>/dev/null
sleep 0.5
timeout 2 arecord -f S16_LE -r 16000 -c 1 -D default /tmp/diag-mic.wav 2>/dev/null
if [ -f /tmp/diag-mic.wav ] && [ $(stat -c%s /tmp/diag-mic.wav) -gt 1000 ]; then
    RMS=$(python3 -c "
import struct, math
with open('/tmp/diag-mic.wav','rb') as f: data=f.read()
if len(data)<100: print('0')
else:
    samples=struct.unpack(f'<{(len(data)-44)//2}h',data[44:])
    print(f'{math.sqrt(sum(s*s for s in samples)/max(len(samples),1)):.0f}')
" 2>/dev/null)
    if [ "$RMS" -gt 10 ]; then
        pass "Mic audio detected (RMS: $RMS)"
    else
        warn "Mic recording is SILENCE (RMS: $RMS) — speak louder or check QEMU audio passthrough"
    fi
else
    fail "Could not record from mic"
fi
rm -f /tmp/diag-mic.wav

# 5. TTS
echo ""
echo "── TTS (Text-to-Speech) ──"
if command -v piper >/dev/null 2>&1; then
    pass "Piper installed"
else
    warn "Piper not found"
fi
if command -v espeak-ng >/dev/null 2>&1; then
    pass "espeak-ng installed"
    espeak-ng -v en "test" -w /tmp/diag-tts.wav 2>/dev/null
    if [ -f /tmp/diag-tts.wav ]; then
        pass "espeak-ng generates audio"
    fi
    rm -f /tmp/diag-tts.wav
fi

# 6. STT (Whisper)
echo ""
echo "── STT (Whisper) ──"
WHISPER_MODEL=$(ls ~/.aios/models/whisper/ggml-*.bin 2>/dev/null | head -1)
if [ -n "$WHISPER_MODEL" ]; then
    pass "Whisper model: $(basename $WHISPER_MODEL)"
    # Quick transcription test
    espeak-ng -v en "hello world" -w /tmp/diag-stt.wav 2>/dev/null
    RESULT=$(whisper-cpp-cli -m "$WHISPER_MODEL" -f /tmp/diag-stt.wav --no-timestamps -nt 2>/dev/null | tr -d '[:space:]')
    if [ -n "$RESULT" ]; then
        pass "Whisper transcribes: '$RESULT'"
    else
        warn "Whisper returned empty transcription"
    fi
    rm -f /tmp/diag-stt.wav
else
    fail "No Whisper model found"
fi

# 7. Web Server
echo ""
echo "── Web Server ──"
if ss -tlnp 2>/dev/null | grep -q ":80 "; then
    pass "Web server on port 80"
    HTTP=$(curl -s -o /dev/null -w "%{http_code}" http://localhost/ 2>/dev/null)
    if [ "$HTTP" = "200" ]; then
        pass "HTTP 200 OK"
    else
        warn "HTTP $HTTP"
    fi
else
    fail "Web server NOT listening on port 80"
fi

# 8. Errors in log
echo ""
echo "── Error Summary ──"
ERRORS=$(grep -c "ERROR\|panicked\|FAIL" ~/.aios/logs/aios.log 2>/dev/null)
WARNS=$(grep -c "WARN" ~/.aios/logs/aios.log 2>/dev/null)
info "Log: $ERRORS errors, $WARNS warnings"
if [ "$ERRORS" -gt 0 ]; then
    echo "  Recent errors:"
    grep "ERROR\|panicked" ~/.aios/logs/aios.log 2>/dev/null | tail -3 | while read l; do echo "    $l"; done
fi

echo ""
echo "═══ Done ═══"
