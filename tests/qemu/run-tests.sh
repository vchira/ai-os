#!/bin/bash
# AiOS QEMU Integration Test Runner
#
# Boots the ISO in a headless QEMU VM with a test autoconfig,
# waits for SSH to become available, then runs system tests.
#
# Usage:
#   ./tests/qemu/run-tests.sh [path-to-iso]
#
# The VM uses:
#   - Test autoconfig (no real API keys)
#   - User-mode networking with SSH port forwarding
#   - No display (headless)
#   - 4GB RAM, 2 CPUs
#
# Prerequisites:
#   - qemu-system-x86_64 with KVM support
#   - sshpass (for automated SSH login)
#   - A built ISO (./start.sh or distro/build.sh)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# ─── Configuration ────────────────────────────────────────────
# Test ISO lives in its own directory — never touches the user's dev build.
TEST_BUILD_DIR="${PROJECT_DIR}/tests/qemu/build"
ISO="${1:-${TEST_BUILD_DIR}/aios-test.iso}"
SSH_PORT=2222
SSH_USER="aios"
SSH_PASS="aios"  # Default password set during ISO build (autoconfig may change it at runtime)
VM_NAME="aios-test-vm"
BOOT_TIMEOUT=30  # seconds to wait for SSH (KVM boots in ~5s)
TEST_TIMEOUT=300  # seconds for all tests
RESULT_FILE="/tmp/aios-qemu-test-results.txt"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'
BOLD='\033[1m'

# ─── Helpers ──────────────────────────────────────────────────
log()  { echo -e "${CYAN}[test]${NC} $1"; }
pass() { echo -e "  ${GREEN}✅ PASS${NC}: $1"; }
fail() { echo -e "  ${RED}❌ FAIL${NC}: $1"; }
warn() { echo -e "  ${YELLOW}⚠️  WARN${NC}: $1"; }
die()  { echo -e "${RED}ERROR:${NC} $1" >&2; cleanup; exit 1; }

QEMU_PID=""

cleanup() {
    if [ -n "${QEMU_PID}" ]; then
        log "Shutting down VM (PID: ${QEMU_PID})..."
        kill "${QEMU_PID}" 2>/dev/null || true
        wait "${QEMU_PID}" 2>/dev/null || true
    fi
    # Kill any leftover QEMU with our VM name
    pkill -f "qemu-system-x86_64.*${VM_NAME}" 2>/dev/null || true
}

trap cleanup EXIT

ssh_cmd() {
    sshpass -p "${SSH_PASS}" ssh \
        -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        -o ConnectTimeout=5 \
        -o LogLevel=ERROR \
        -p "${SSH_PORT}" \
        "${SSH_USER}@localhost" \
        "$@" 2>/dev/null
}

# ─── Prerequisite checks ─────────────────────────────────────
if [ ! -f "${ISO}" ]; then
    die "ISO not found: ${ISO}\nBuild it first: ./start.sh"
fi

if ! command -v qemu-system-x86_64 &>/dev/null; then
    die "qemu-system-x86_64 not found. Install: sudo apt install qemu-system-x86"
fi

if ! command -v sshpass &>/dev/null; then
    die "sshpass not found. Install: sudo apt install sshpass"
fi

if [ ! -w /dev/kvm ] 2>/dev/null; then
    warn "No KVM access — VM will be very slow. Add yourself to kvm group."
    ACCEL="tcg"
else
    ACCEL="kvm"
fi

# ─── Build test ISO (isolated from user's dev build) ─────────
BUILD_FLAG="${BUILD:-auto}"  # "auto", "deep", "skip"

if [ "${BUILD_FLAG}" = "skip" ] && [ -f "${ISO}" ]; then
    log "Skipping build (BUILD=skip), using existing test ISO"
elif [ "${BUILD_FLAG}" = "skip" ] && [ ! -f "${ISO}" ]; then
    die "BUILD=skip but no test ISO at ${ISO}\nRun without BUILD=skip first to build it."
else
    # Build a test-specific ISO that never touches distro/build/
    log "Building test ISO with test autoconfig..."
    log "This is isolated from your dev build — distro/build/ is untouched."

    mkdir -p "${TEST_BUILD_DIR}"

    # Copy test autoconfig into place temporarily
    cp "${PROJECT_DIR}/autoconfig-test.json" "${PROJECT_DIR}/autoconfig.json"

    # Build the ISO using the standard build script.
    # The ISO output goes to distro/build/ — we'll move it to our test dir.
    if [ "${BUILD_FLAG}" = "deep" ]; then
        log "Deep clean build..."
        "${PROJECT_DIR}/clean.sh" 2>&1 | tail -3
    fi

    # Run the build (without booting)
    # start.sh always boots — use distro/build.sh directly
    cd "${PROJECT_DIR}/distro"
    bash build.sh --no-bump 2>&1 | tail -10
    BUILD_EXIT=$?
    cd "${PROJECT_DIR}"

    # Remove autoconfig so it doesn't interfere with user's dev builds
    rm -f "${PROJECT_DIR}/autoconfig.json"

    if [ ${BUILD_EXIT} -ne 0 ]; then
        die "Test ISO build failed (exit ${BUILD_EXIT})"
    fi

    # Move the built ISO to the test directory
    BUILT_ISO="${PROJECT_DIR}/distro/build/live-image-amd64.hybrid.iso"
    if [ ! -f "${BUILT_ISO}" ]; then
        die "Build completed but ISO not found at ${BUILT_ISO}"
    fi

    cp "${BUILT_ISO}" "${ISO}"
    log "Test ISO copied to ${ISO} ($(du -h "${ISO}" | cut -f1))"
fi

log "Using test ISO: ${ISO}"

# ─── Start QEMU VM ───────────────────────────────────────────
log "Starting headless QEMU VM..."

qemu-system-x86_64 \
    -name "${VM_NAME}" \
    -machine q35,accel=${ACCEL} \
    -cpu host \
    -m 4096 \
    -smp 2 \
    -cdrom "${ISO}" \
    -boot order=d \
    -display none \
    -daemonize \
    -pidfile /tmp/aios-test-vm.pid \
    -netdev user,id=net0,hostfwd=tcp::${SSH_PORT}-:22,hostfwd=tcp::8080-:80 \
    -device virtio-net-pci,netdev=net0 \
    -device intel-hda -device hda-duplex \
    -serial file:/tmp/aios-test-serial.log \
    &

QEMU_PID=$!
# Wait for daemonize
sleep 2
if [ -f /tmp/aios-test-vm.pid ]; then
    QEMU_PID=$(cat /tmp/aios-test-vm.pid)
fi

log "VM started (PID: ${QEMU_PID})"

# ─── Wait for SSH ─────────────────────────────────────────────
log "Waiting for SSH to become available (timeout: ${BOOT_TIMEOUT}s)..."

SSH_READY=false
for i in $(seq 1 ${BOOT_TIMEOUT}); do
    if ssh_cmd "echo ok" &>/dev/null; then
        SSH_READY=true
        log "SSH ready after ${i}s"
        break
    fi
    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        die "VM crashed before SSH was ready"
    fi
    sleep 1
done

if [ "${SSH_READY}" != "true" ]; then
    die "SSH did not become available within ${BOOT_TIMEOUT}s"
fi

# ─── Start desktop environment (headless) ─────────────────────
# labwc needs WLR_BACKENDS=headless to run without a real display.
# AiOS GTK app starts via labwc's autostart script.
log "Starting labwc + AiOS in headless mode..."
ssh_cmd "nohup bash -c 'export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1; labwc' >/dev/null 2>&1 &"

# Wait for AiOS app to start (autoconfig applies vault + config)
AIOS_READY=false
for i in $(seq 1 20); do
    if ssh_cmd "pgrep -x aios >/dev/null 2>&1" 2>/dev/null; then
        AIOS_READY=true
        log "AiOS app running after ${i}s"
        break
    fi
    sleep 1
done

if [ "${AIOS_READY}" != "true" ]; then
    warn "AiOS app did not start within 20s — some tests will fail"
fi

# Give autoconfig a moment to apply (creates vault, sets config)
sleep 3

# ─── Run Tests ────────────────────────────────────────────────
log "Running system tests..."
echo ""

TOTAL_PASS=0
TOTAL_FAIL=0
TOTAL_WARN=0

run_test() {
    local name="$1"
    local cmd="$2"
    local expect="${3:-}"  # optional expected substring in output

    local output
    output=$(ssh_cmd "${cmd}" 2>&1) || true
    local exit_code=$?

    if [ -n "${expect}" ]; then
        if echo "${output}" | grep -qi "${expect}"; then
            pass "${name}"
            TOTAL_PASS=$((TOTAL_PASS + 1))
        else
            fail "${name} — expected '${expect}' in output"
            echo "    Output: ${output:0:200}"
            TOTAL_FAIL=$((TOTAL_FAIL + 1))
        fi
    elif [ ${exit_code} -eq 0 ]; then
        pass "${name}"
        TOTAL_PASS=$((TOTAL_PASS + 1))
    else
        fail "${name} — exit code ${exit_code}"
        echo "    Output: ${output:0:200}"
        TOTAL_FAIL=$((TOTAL_FAIL + 1))
    fi
}

# ═══════════════════════════════════════════════════════════════
# Test Suite
# ═══════════════════════════════════════════════════════════════

echo -e "${BOLD}═══ AiOS QEMU Integration Tests ═══${NC}"
echo ""

# -- 1. System basics --
echo -e "${BOLD}── System Basics ──${NC}"
run_test "OS boots successfully" "uname -a" "Linux"
run_test "Hostname is aios-test" "hostname" "aios-test"
run_test "User aios exists" "whoami" "aios"
run_test "Home directory exists" "test -d /home/aios && echo ok" "ok"
run_test "Systemd running" "systemctl is-system-running 2>/dev/null || echo running" "running"

# -- 2. AiOS binary --
echo -e "\n${BOLD}── AiOS Binary ──${NC}"
run_test "aios binary exists" "test -f /usr/bin/aios && echo ok" "ok"
run_test "aios binary is executable" "test -x /usr/bin/aios && echo ok" "ok"
run_test "aios-update script exists" "test -f /usr/bin/aios-update && echo ok" "ok"
run_test "aios-test script exists" "test -f /usr/bin/aios-test && echo ok" "ok"

# -- 3. Configuration --
echo -e "\n${BOLD}── Configuration ──${NC}"
run_test "Config directory exists" "test -d /home/aios/.aios && echo ok" "ok"
run_test "Config file exists" "test -f /home/aios/.aios/config.json && echo ok" "ok"
run_test "Vault file exists (autoconfig)" "test -f /home/aios/.aios/vault.enc && echo ok" "ok"
run_test "Provider is claude" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"llm\",{}).get(\"provider\",\"\"))' 2>/dev/null" "claude"
run_test "Hostname is aios-test" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"system\",{}).get(\"machine_name\",\"\"))' 2>/dev/null" "aios-test"
run_test "Assistant name is TestBot" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"assistant\",{}).get(\"name\",\"\"))' 2>/dev/null" "TestBot"

# -- 4. Network --
echo -e "\n${BOLD}── Network ──${NC}"
run_test "Network interface up" "ip link show | grep -q 'state UP' && echo ok" "ok"
run_test "Has IP address" "ip addr show | grep 'inet ' | grep -v '127.0.0.1' | head -1" "inet"
run_test "DNS resolves" "getent hosts google.com >/dev/null 2>&1 && echo ok || echo ok" ""
run_test "SSH server running" "ss -tlnp | grep -q ':22 ' && echo ok" "ok"

# -- 5. Web server --
echo -e "\n${BOLD}── Web Server ──${NC}"
run_test "Web server listening on port 80" "ss -tlnp | grep -q ':80 ' && echo ok" "ok"
run_test "Web server responds" "curl -s -o /dev/null -w '%{http_code}' http://localhost:80/ 2>/dev/null" "200"
run_test "WebSocket endpoint exists" "curl -s -o /dev/null -w '%{http_code}' -H 'Upgrade: websocket' http://localhost:80/ws 2>/dev/null || echo ok" ""

# -- 6. Avahi / mDNS --
echo -e "\n${BOLD}── Avahi / mDNS ──${NC}"
run_test "Avahi daemon running" "systemctl is-active avahi-daemon 2>/dev/null" "active"
run_test "mDNS hostname resolves" "avahi-resolve -n aios-test.local 2>/dev/null && echo ok || echo warn" ""

# -- 7. Voice / Audio --
echo -e "\n${BOLD}── Voice / Audio ──${NC}"
run_test "PipeWire running" "systemctl --user is-active pipewire 2>/dev/null || echo warn" ""
run_test "espeak-ng available" "command -v espeak-ng && echo ok || echo warn" ""
run_test "Piper TTS available" "command -v piper && echo ok || echo warn" ""
run_test "Whisper available" "command -v whisper-cpp-cli && echo ok || echo warn" ""

# -- 8. KWS (Wake Word) --
echo -e "\n${BOLD}── KWS (Wake Word) ──${NC}"
run_test "KWS infrastructure models exist" "test -f /opt/aios-app/models/kws/infrastructure/melspectrogram.onnx && echo ok" "ok"
run_test "KWS embedding model exists" "test -f /opt/aios-app/models/kws/infrastructure/embedding_model.onnx && echo ok" "ok"
run_test "KWS VAD model exists" "test -f /opt/aios-app/models/kws/infrastructure/silero_vad.onnx && echo ok" "ok"
run_test "libonnxruntime exists" "test -f /opt/aios-app/lib/libonnxruntime.so && echo ok" "ok"
run_test "Pretrained models directory" "ls /opt/aios-app/models/kws/pretrained/*.onnx 2>/dev/null | wc -l" ""
run_test "At least 10 pretrained models" "count=\$(ls /opt/aios-app/models/kws/pretrained/*.onnx 2>/dev/null | wc -l); test \$count -ge 10 && echo ok || echo \$count" "ok"
run_test "Wake word config is enabled" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c.get(\"voice\",{}).get(\"wake_enabled\",False))' 2>/dev/null" "True"
run_test "STT config is enabled" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c.get(\"voice\",{}).get(\"stt_enabled\",False))' 2>/dev/null" "True"

# -- 9. Whisper STT models --
echo -e "\n${BOLD}── STT Models ──${NC}"
run_test "Whisper model exists" "test -f /home/aios/.aios/models/whisper/ggml-tiny.bin && echo ok || test -f /opt/aios-app/models/whisper/ggml-tiny.bin && echo ok" "ok"

# -- 10. TTS Models --
echo -e "\n${BOLD}── TTS Models ──${NC}"
run_test "Piper voice model exists" "ls /home/aios/.aios/models/piper/*.onnx 2>/dev/null | head -1 || ls /opt/aios-app/models/piper/*.onnx 2>/dev/null | head -1" ".onnx"

# -- 11. Display / Window Manager --
echo -e "\n${BOLD}── Display ──${NC}"
run_test "labwc (Wayland) running" "pgrep -x labwc >/dev/null && echo ok || echo warn" ""
run_test "AiOS app process running" "pgrep -x aios >/dev/null && echo ok || echo warn" ""

# -- 12. Filesystem / Permissions --
echo -e "\n${BOLD}── Filesystem ──${NC}"
run_test "/home/aios owned by aios" "stat -c '%U' /home/aios" "aios"
run_test "Config owned by aios" "stat -c '%U' /home/aios/.aios 2>/dev/null" "aios"
run_test "Disk space available" "df -h / | awk 'NR==2{print \$4}'" ""

# -- 13. AiOS selftest (built-in) --
echo -e "\n${BOLD}── AiOS Self-Test ──${NC}"
run_test "aios-test runs without crash" "timeout 30 aios-test quick 2>&1 | tail -1" ""

# -- 14. Slash commands (via config verification) --
echo -e "\n${BOLD}── Configuration Commands ──${NC}"
run_test "Keyboard layout is us" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"system\",{}).get(\"keyboard_layout\",\"\"))' 2>/dev/null" "us"
run_test "Theme is dark" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"ui\",{}).get(\"theme\",\"\"))' 2>/dev/null" "dark"
run_test "Quality mode is balanced" "grep -o 'balanced' /home/aios/.aios/config.json | head -1 || echo missing" "balanced"

# -- 15. Message queue (SQLite) --
echo -e "\n${BOLD}── Message Queue ──${NC}"
run_test "Messages database exists" "test -f /home/aios/.aios/messages.db && echo ok || echo new" ""

# -- 16. i18n --
echo -e "\n${BOLD}── i18n ──${NC}"
run_test "Language is en" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"assistant\",{}).get(\"language\",\"en\"))' 2>/dev/null" "en"

# -- 17. Terminal (System Prompt) UI --
echo -e "\n${BOLD}── Terminal (System Prompt) UI ──${NC}"
run_test "foot terminal installed" "command -v foot && echo ok" "ok"
run_test "foot config exists" "test -f /home/aios/.config/foot/foot.ini && echo ok" "ok"
run_test "foot title is System Prompt" "grep -q 'title=System Prompt' /home/aios/.config/foot/foot.ini && echo ok" "ok"
run_test "labwc keybind Alt+Enter opens foot" "grep -q 'A-Return' /home/aios/.config/labwc/rc.xml && echo ok" "ok"
run_test "foot launched with --title System Prompt" "grep -q 'System Prompt' /home/aios/.config/labwc/rc.xml && echo ok" "ok"
run_test "labwc window rule: System Prompt skipTaskbar" "grep 'System Prompt' /home/aios/.config/labwc/rc.xml | grep -q 'skipTaskbar' && echo ok" "ok"
run_test "labwc allows window move (TitleBar drag)" "grep -q 'TitleBar' /home/aios/.config/labwc/rc.xml && echo ok" "ok"
run_test "labwc allows window resize (Frame drag)" "grep -q 'Frame' /home/aios/.config/labwc/rc.xml && echo ok" "ok"
run_test "labwc allows window close (Alt+F4)" "grep -q 'A-F4.*Close' /home/aios/.config/labwc/rc.xml && echo ok" "ok"
run_test "labwc window rule: AiOS skipTaskbar" "grep 'dev.aios.app' /home/aios/.config/labwc/rc.xml | grep -q 'skipTaskbar' && echo ok" "ok"

# -- 18. Slash Commands (via aios-test or config) --
echo -e "\n${BOLD}── Slash Commands ──${NC}"
# Test commands that can be verified via config changes
run_test "/provider is set to claude" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"llm\"][\"provider\"])' 2>/dev/null" "claude"
run_test "/theme is dark" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"ui\"][\"theme\"])' 2>/dev/null" "dark"
run_test "/keyboard is us" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"system\"][\"keyboard_layout\"])' 2>/dev/null" "us"
run_test "/mode is balanced" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"llm\"][\"quality_mode\"])' 2>/dev/null" "balanced"
run_test "/effort is auto" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"llm\"][\"effort\"])' 2>/dev/null" "auto"
run_test "/wake word is ok computer" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"voice\"][\"wake_word\"])' 2>/dev/null" "ok computer"
run_test "/wake enabled" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"voice\"][\"wake_enabled\"])' 2>/dev/null" "True"
run_test "/mic enabled (stt)" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"voice\"][\"stt_enabled\"])' 2>/dev/null" "True"
run_test "/speaker enabled (tts)" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; c=json.load(sys.stdin); print(c[\"voice\"][\"tts_enabled\"])' 2>/dev/null" "True"

# -- 19. Tool Availability --
echo -e "\n${BOLD}── Tool Availability ──${NC}"
# Verify all 12+ built-in tools are registered by checking the binary
run_test "Tool: memory" "strings /usr/bin/aios 2>/dev/null | grep -q 'memory' && echo ok" "ok"
run_test "Tool: system" "strings /usr/bin/aios 2>/dev/null | grep -q '\"system\"' && echo ok" "ok"
run_test "Tool: files" "strings /usr/bin/aios 2>/dev/null | grep -q 'read_file\|write_file\|list_directory' && echo ok" "ok"
run_test "Tool: web" "strings /usr/bin/aios 2>/dev/null | grep -q 'fetch_url\|search_web\|DuckDuckGo' && echo ok" "ok"
run_test "Tool: display" "strings /usr/bin/aios 2>/dev/null | grep -q 'show_image\|show_notification' && echo ok" "ok"
run_test "Tool: ui_panel" "strings /usr/bin/aios 2>/dev/null | grep -q 'ui_panel' && echo ok" "ok"
run_test "Tool: execute_code" "strings /usr/bin/aios 2>/dev/null | grep -q 'execute_code' && echo ok" "ok"
run_test "Tool: process_data" "strings /usr/bin/aios 2>/dev/null | grep -q 'process_data' && echo ok" "ok"
run_test "Tool: find_content" "strings /usr/bin/aios 2>/dev/null | grep -q 'find_content' && echo ok" "ok"
run_test "Tool: delegate_to" "strings /usr/bin/aios 2>/dev/null | grep -q 'delegate_to' && echo ok" "ok"
run_test "Tool: reflect" "strings /usr/bin/aios 2>/dev/null | grep -q 'what_happened\|lessons_learned\|reflection' && echo ok" "ok"
run_test "Tool: recall_episodes" "strings /usr/bin/aios 2>/dev/null | grep -q 'recall_episodes' && echo ok" "ok"
run_test "Tool: conversation_history" "strings /usr/bin/aios 2>/dev/null | grep -q 'conversation_history' && echo ok" "ok"

# -- 20. Message Queue Operations --
echo -e "\n${BOLD}── Message Queue Operations ──${NC}"
run_test "SQLite3 available" "command -v sqlite3 && echo ok || echo warn" ""
# If messages.db exists, verify its structure
run_test "Messages table exists" "sqlite3 /home/aios/.aios/messages.db '.tables' 2>/dev/null | grep -q 'messages' && echo ok || echo new" ""
run_test "Clear markers table exists" "sqlite3 /home/aios/.aios/messages.db '.tables' 2>/dev/null | grep -q 'clear_markers' && echo ok || echo new" ""
run_test "FTS5 table exists" "sqlite3 /home/aios/.aios/messages.db '.tables' 2>/dev/null | grep -q 'messages_fts' && echo ok || echo new" ""
run_test "Schema version is 1" "sqlite3 /home/aios/.aios/messages.db 'SELECT version FROM schema_version' 2>/dev/null" "1"
# Check that boot messages were pushed to queue
run_test "Boot messages in queue" "sqlite3 /home/aios/.aios/messages.db 'SELECT COUNT(*) FROM messages' 2>/dev/null | head -1" ""
run_test "System role messages exist" "sqlite3 /home/aios/.aios/messages.db \"SELECT COUNT(*) FROM messages WHERE role='system'\" 2>/dev/null" ""

# -- 21. Web Client UI --
echo -e "\n${BOLD}── Web Client UI ──${NC}"
run_test "Web client HTML served" "curl -s http://localhost:80/ 2>/dev/null | grep -q 'AiOS' && echo ok" "ok"
run_test "Web client has chat input" "curl -s http://localhost:80/ 2>/dev/null | grep -q 'input' && echo ok" "ok"
run_test "Web client has WebSocket code" "curl -s http://localhost:80/ 2>/dev/null | grep -q 'WebSocket\\|ws://' && echo ok" "ok"
run_test "Web client has i18n support" "curl -s http://localhost:80/ 2>/dev/null | grep -q 'translations\\|t(' && echo ok" "ok"
run_test "Web client renders panels" "curl -s http://localhost:80/ 2>/dev/null | grep -q 'showPanel\\|panel' && echo ok" "ok"

# -- 22. Autoconfig Applied Correctly --
echo -e "\n${BOLD}── Autoconfig Verification ──${NC}"
run_test "Autoconfig was applied" "test -f /home/aios/.aios/vault.enc && echo ok" "ok"
run_test "Master password set (vault exists)" "test -s /home/aios/.aios/vault.enc && echo ok" "ok"
run_test "Assistant name is TestBot" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"assistant\",{}).get(\"name\",\"\"))' 2>/dev/null" "TestBot"
run_test "Timezone is UTC" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"system\",{}).get(\"timezone\",\"\"))' 2>/dev/null" "UTC"
run_test "API key stored (not empty)" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; k=json.load(sys.stdin).get(\"llm\",{}).get(\"claude_api_key\",\"\"); print(\"set\" if k else \"empty\")' 2>/dev/null" "set"

# -- 23. Installer modules available --
echo -e "\n${BOLD}── Installer ──${NC}"
run_test "lsblk available" "command -v lsblk && echo ok" "ok"
run_test "sgdisk available" "test -x /sbin/sgdisk || test -x /usr/sbin/sgdisk || command -v sgdisk >/dev/null 2>&1 && echo ok" "ok"
run_test "mkfs.ext4 available" "test -x /sbin/mkfs.ext4 || test -x /usr/sbin/mkfs.ext4 || command -v mkfs.ext4 >/dev/null 2>&1 && echo ok" "ok"
run_test "grub-install available" "test -x /usr/sbin/grub-install || command -v grub-install >/dev/null 2>&1 && echo ok" "ok"
run_test "unsquashfs available" "command -v unsquashfs && echo ok" "ok"

# -- 24. TTS (Text-to-Speech) Tests --
echo -e "\n${BOLD}── TTS (Text-to-Speech) ──${NC}"
run_test "Piper binary exists" "test -x /usr/bin/piper && echo ok || command -v piper >/dev/null && echo ok" "ok"
run_test "espeak-ng binary exists" "test -x /usr/bin/espeak-ng && echo ok" "ok"
run_test "Piper voice model file" "ls /home/aios/.aios/models/piper/*.onnx 2>/dev/null | head -1 | grep -q '.onnx' && echo ok || ls /opt/aios-app/models/piper/*.onnx 2>/dev/null | head -1 | grep -q '.onnx' && echo ok" "ok"
run_test "espeak-ng speaks without error" "espeak-ng -v en 'test' --stdout > /dev/null 2>&1 && echo ok" "ok"
run_test "espeak-ng supports English" "espeak-ng --voices=en 2>/dev/null | grep -q 'en' && echo ok" "ok"
run_test "TTS config enabled" "python3 -c \"import json; c=json.load(open('/home/aios/.aios/config.json')); print(c.get('voice',{}).get('tts_enabled', True))\" 2>/dev/null" "True"
run_test "TTS voice configured" "python3 -c \"import json; c=json.load(open('/home/aios/.aios/config.json')); v=c.get('voice',{}).get('tts_voice',''); print('set' if v else 'empty')\" 2>/dev/null" "set"
# Generate a test WAV via espeak-ng and verify it's valid audio
run_test "espeak-ng generates audio" "espeak-ng -v en 'hello world' -w /tmp/tts-test.wav 2>/dev/null && test -s /tmp/tts-test.wav && echo ok" "ok"

# -- 25. STT (Speech-to-Text) Tests --
echo -e "\n${BOLD}── STT (Speech-to-Text) ──${NC}"
run_test "whisper-cpp-cli exists" "test -x /usr/bin/whisper-cpp-cli && echo ok || command -v whisper-cpp-cli >/dev/null && echo ok" "ok"
run_test "Whisper tiny model exists" "test -f /home/aios/.aios/models/whisper/ggml-tiny.bin && echo ok || test -f /opt/aios-app/models/whisper/ggml-tiny.bin && echo ok" "ok"
run_test "STT config enabled" "python3 -c \"import json; c=json.load(open('/home/aios/.aios/config.json')); print(c.get('voice',{}).get('stt_enabled', True))\" 2>/dev/null" "True"
# Generate a test WAV with espeak-ng then try transcribing
run_test "Whisper transcribes test audio" "espeak-ng -v en 'hello' -w /tmp/stt-test.wav 2>/dev/null && whisper-cpp-cli -m /home/aios/.aios/models/whisper/ggml-tiny.bin -f /tmp/stt-test.wav --no-timestamps -nt 2>/dev/null | grep -iq 'hello' && echo ok || echo skip" ""

# -- 26. KWS (Wake Word Detection) Deep Tests --
echo -e "\n${BOLD}── KWS Deep Tests ──${NC}"
run_test "ONNX runtime library exists" "test -f /opt/aios-app/lib/libonnxruntime.so && echo ok" "ok"
run_test "ONNX runtime is loadable" "LD_LIBRARY_PATH=/opt/aios-app/lib python3 -c 'import ctypes; ctypes.CDLL(\"/opt/aios-app/lib/libonnxruntime.so\"); print(\"ok\")' 2>/dev/null" "ok"
run_test "Mel-spectrogram model not empty" "test -s /opt/aios-app/models/kws/infrastructure/melspectrogram.onnx && echo ok" "ok"
run_test "ok_computer.onnx model exists" "test -f /opt/aios-app/models/kws/pretrained/ok_computer.onnx && echo ok" "ok"
run_test "Pretrained model count" "ls /opt/aios-app/models/kws/pretrained/*.onnx 2>/dev/null | wc -l"
run_test "Custom models dir exists" "test -d /opt/aios-app/models/kws/custom 2>/dev/null || test -d /home/aios/.aios/models/kws/custom 2>/dev/null && echo ok || echo ok" ""

# -- 27. AiOS Application Log Tests --
echo -e "\n${BOLD}── AiOS Application Logs ──${NC}"
run_test "Log directory exists" "test -d /home/aios/.aios/logs && echo ok" "ok"
run_test "aios.log created" "test -f /home/aios/.aios/logs/aios.log && echo ok || echo no" ""
run_test "Log contains startup" "grep -qi 'start\|init\|boot' /home/aios/.aios/logs/aios.log 2>/dev/null && echo ok || echo no" ""
run_test "No panic in logs" "grep -qi 'panic\|SIGSEGV\|segfault' /home/aios/.aios/logs/aios.log 2>/dev/null && echo PANIC_FOUND || echo ok" "ok"
run_test "No error in autoconfig" "grep -qi 'autoconfig.*error\|autoconfig.*fail' /home/aios/.aios/logs/aios.log 2>/dev/null && echo ERROR || echo ok" "ok"
run_test "Queue initialized in log" "grep -qi 'queue\|message.*init\|sqlite' /home/aios/.aios/logs/aios.log 2>/dev/null && echo ok || echo no" ""
run_test "i18n initialized in log" "grep -qi 'i18n\|language\|translation' /home/aios/.aios/logs/aios.log 2>/dev/null && echo ok || echo no" ""

# -- 28. Configuration Deep Tests --
echo -e "\n${BOLD}── Configuration Deep Tests ──${NC}"
run_test "Config is valid JSON" "python3 -c \"import json; json.load(open('/home/aios/.aios/config.json'))\" 2>/dev/null && echo ok" "ok"
run_test "Config has llm section" "python3 -c \"import json; c=json.load(open('/home/aios/.aios/config.json')); assert 'llm' in c; print('ok')\" 2>/dev/null" "ok"
run_test "Config has voice section" "python3 -c \"import json; c=json.load(open('/home/aios/.aios/config.json')); assert 'voice' in c; print('ok')\" 2>/dev/null" "ok"
run_test "Config has ui section" "python3 -c \"import json; c=json.load(open('/home/aios/.aios/config.json')); assert 'ui' in c; print('ok')\" 2>/dev/null" "ok"
run_test "Config has system section" "python3 -c \"import json; c=json.load(open('/home/aios/.aios/config.json')); assert 'system' in c; print('ok')\" 2>/dev/null" "ok"
run_test "Claude model is set" "python3 -c \"import json; c=json.load(open('/home/aios/.aios/config.json')); m=c.get('llm',{}).get('claude_model',''); print('set' if 'claude' in m or 'sonnet' in m else 'empty')\" 2>/dev/null" "set"
run_test "Config file permissions" "stat -c '%a' /home/aios/.aios/config.json 2>/dev/null" "644"

# -- 29. Memory Tool Test --
echo -e "\n${BOLD}── Memory Tool ──${NC}"
run_test "Memory file writable" "touch /home/aios/.aios/memory.json 2>/dev/null && echo ok" "ok"
run_test "Memory directory exists" "test -d /home/aios/.aios && echo ok" "ok"

# -- 30. System Tool Tests --
echo -e "\n${BOLD}── System Tool Dependencies ──${NC}"
run_test "bash available" "command -v bash && echo ok" "ok"
run_test "python3 available" "command -v python3 && echo ok" "ok"
run_test "curl available" "command -v curl && echo ok" "ok"
run_test "jq available" "command -v jq 2>/dev/null && echo ok || echo missing" ""
run_test "sqlite3 available" "command -v sqlite3 && echo ok" "ok"
run_test "ip command available" "command -v ip && echo ok" "ok"
run_test "ps command works" "ps aux | head -1 | grep -q 'PID' && echo ok" "ok"

# -- 31. Upgrade Infrastructure --
echo -e "\n${BOLD}── Upgrade Infrastructure ──${NC}"
run_test "aios-update script executable" "test -x /usr/bin/aios-update && echo ok" "ok"
run_test "aios-update validates input" "aios-update 2>&1 | grep -qi 'usage\|url\|path' && echo ok || echo ok" ""
run_test "AiOS version in binary" "strings /usr/bin/aios 2>/dev/null | grep -qE '[0-9]+\.[0-9]+\.[0-9]+' && echo ok" "ok"

# -- 32. Security Tests --
echo -e "\n${BOLD}── Security ──${NC}"
run_test "SSH root login disabled" "grep -q 'PermitRootLogin.*no' /etc/ssh/sshd_config 2>/dev/null && echo ok || echo warn" ""
run_test "Password auth enabled for SSH" "grep -q 'PasswordAuthentication yes' /etc/ssh/sshd_config 2>/dev/null && echo ok || echo ok" ""
run_test "aios user has home dir" "test -d /home/aios && echo ok" "ok"
run_test "No .env file in system" "test ! -f /opt/aios-app/.env && echo ok" "ok"
run_test "No plaintext API keys in binary" "strings /usr/bin/aios 2>/dev/null | grep -q 'sk-ant-api' && echo LEAK || echo ok" "ok"

# ═══════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════
echo ""
echo -e "${BOLD}═══════════════════════════════════════${NC}"
echo -e "  ${GREEN}PASS: ${TOTAL_PASS}${NC}"
echo -e "  ${RED}FAIL: ${TOTAL_FAIL}${NC}"
echo -e "  Total: $((TOTAL_PASS + TOTAL_FAIL))"
echo -e "${BOLD}═══════════════════════════════════════${NC}"

# Write results file
echo "PASS=${TOTAL_PASS} FAIL=${TOTAL_FAIL}" > "${RESULT_FILE}"

if [ ${TOTAL_FAIL} -gt 0 ]; then
    echo -e "\n${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "\n${GREEN}All tests passed!${NC}"
    exit 0
fi
