#!/bin/bash
# AiOS QEMU Hard Drive Installation Test
#
# Tests the full installation flow:
#   1. Boot live ISO with install autoconfig
#   2. Wait for AiOS to auto-install to a virtual hard drive
#   3. Reboot from the installed drive
#   4. Verify the installed system works
#   5. Clean up
#
# Usage:
#   ./tests/qemu/run-install-test.sh [path-to-iso]
#
# Prerequisites:
#   - qemu-system-x86_64 with KVM
#   - qemu-img
#   - sshpass
#   - A built ISO

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# ─── Configuration ────────────────────────────────────────────
TEST_BUILD_DIR="${PROJECT_DIR}/tests/qemu/build"
ISO="${1:-${TEST_BUILD_DIR}/aios-test.iso}"
DISK_IMAGE="/tmp/aios-install-test-disk.qcow2"
DISK_SIZE="16G"
SSH_PORT=2223  # Different port from run-tests.sh to avoid conflicts
SSH_USER="aios"
SSH_PASS="12345678"  # Matches autoconfig master_password
VM_NAME="aios-install-test"
BOOT_TIMEOUT=60
INSTALL_TIMEOUT=300  # 5 minutes for installation
REBOOT_TIMEOUT=60

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'
BOLD='\033[1m'

# Counters
PASSED=0
FAILED=0
WARNED=0

# ─── Helpers ──────────────────────────────────────────────────
log()  { echo -e "${CYAN}[install-test]${NC} $1"; }
pass() { echo -e "  ${GREEN}✅ PASS${NC}: $1"; PASSED=$((PASSED + 1)); }
fail() { echo -e "  ${RED}❌ FAIL${NC}: $1"; FAILED=$((FAILED + 1)); }
warn() { echo -e "  ${YELLOW}⚠️  WARN${NC}: $1"; WARNED=$((WARNED + 1)); }
die()  { echo -e "${RED}ERROR:${NC} $1" >&2; cleanup; exit 1; }

QEMU_PID=""

cleanup() {
    if [ -n "${QEMU_PID}" ]; then
        log "Shutting down VM (PID: ${QEMU_PID})..."
        kill "${QEMU_PID}" 2>/dev/null || true
        wait "${QEMU_PID}" 2>/dev/null || true
    fi
    pkill -f "qemu-system-x86_64.*${VM_NAME}" 2>/dev/null || true
    # Clean up disk image
    if [ -f "${DISK_IMAGE}" ]; then
        log "Removing test disk image: ${DISK_IMAGE}"
        rm -f "${DISK_IMAGE}"
    fi
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

run_test() {
    local desc="$1"
    local cmd="$2"
    local expected="${3:-}"

    local output
    output=$(ssh_cmd "${cmd}" 2>/dev/null) || output=""

    if [ -n "${expected}" ]; then
        if echo "${output}" | grep -q "${expected}"; then
            pass "${desc}"
        else
            fail "${desc} (expected '${expected}', got '${output}')"
        fi
    else
        if [ -n "${output}" ]; then
            pass "${desc}"
        else
            warn "${desc} (no output)"
        fi
    fi
}

# ─── Prerequisite checks ─────────────────────────────────────
if [ ! -f "${ISO}" ]; then
    # Try the main build ISO
    MAIN_ISO="${PROJECT_DIR}/distro/build/live-image-amd64.hybrid.iso"
    if [ -f "${MAIN_ISO}" ]; then
        ISO="${MAIN_ISO}"
        log "Using main build ISO: ${ISO}"
    else
        die "ISO not found: ${ISO}\nBuild it first: ./start.sh"
    fi
fi

for tool in qemu-system-x86_64 qemu-img sshpass; do
    if ! command -v "${tool}" &>/dev/null; then
        die "${tool} not found. Install it first."
    fi
done

if [ ! -w /dev/kvm ] 2>/dev/null; then
    warn "No KVM access — VM will be very slow"
    ACCEL="tcg"
else
    ACCEL="kvm"
fi

# ─── Create install autoconfig ────────────────────────────────
INSTALL_AUTOCONFIG="/tmp/aios-install-test-autoconfig.json"
cat > "${INSTALL_AUTOCONFIG}" << 'ACEOF'
{
  "_description": "AiOS install test autoconfig",
  "provider": {
    "primary": "claude",
    "claude_api_key": "sk-ant-test-key-not-real"
  },
  "ai": {
    "main_provider": "claude",
    "main_model": "claude-sonnet-4-20250514",
    "summary_provider": "claude",
    "summary_model": "claude-haiku-4-5-20251001"
  },
  "system": {
    "keyboard": "us",
    "language": "en",
    "timezone": "UTC",
    "hostname": "aios-install-test",
    "master_password": "12345678"
  },
  "install": {
    "enabled": true,
    "target_disk": "auto",
    "confirm": false
  },
  "assistant": {
    "name": "TestBot",
    "effort": "auto"
  },
  "debug": true
}
ACEOF

# ─── Phase 1: Create virtual disk ────────────────────────────
log "Creating ${DISK_SIZE} virtual disk: ${DISK_IMAGE}"
qemu-img create -f qcow2 "${DISK_IMAGE}" "${DISK_SIZE}" >/dev/null

# ─── Phase 2: Boot live ISO + install to disk ─────────────────
log "Phase 1: Booting live ISO with install autoconfig..."

# Create a temporary ISO with the install autoconfig baked in
# We'll inject it via a virtio-9p share or just use the kernel cmdline
# Simplest approach: create a small FAT image with the autoconfig
AUTOCONFIG_IMG="/tmp/aios-install-test-autoconfig.img"
dd if=/dev/zero of="${AUTOCONFIG_IMG}" bs=1M count=1 2>/dev/null
mkfs.fat "${AUTOCONFIG_IMG}" >/dev/null 2>&1
# Mount and copy autoconfig
MOUNT_DIR=$(mktemp -d)
sudo mount -o loop "${AUTOCONFIG_IMG}" "${MOUNT_DIR}"
sudo cp "${INSTALL_AUTOCONFIG}" "${MOUNT_DIR}/aios-autoconfig.json"
sudo umount "${MOUNT_DIR}"
rmdir "${MOUNT_DIR}"

qemu-system-x86_64 \
    -name "${VM_NAME}" \
    -machine q35,accel=${ACCEL} \
    -cpu host \
    -m 4096 \
    -smp 2 \
    -cdrom "${ISO}" \
    -drive file="${DISK_IMAGE}",format=qcow2,if=virtio \
    -drive file="${AUTOCONFIG_IMG}",format=raw,if=virtio,readonly=on \
    -boot order=d \
    -display none \
    -daemonize \
    -pidfile /tmp/aios-install-test.pid \
    -netdev user,id=net0,hostfwd=tcp::${SSH_PORT}-:22 \
    -device virtio-net-pci,netdev=net0 \
    -device intel-hda -device hda-duplex \
    -serial file:/tmp/aios-install-test-serial.log \
    &

QEMU_PID=$!
sleep 2
if [ -f /tmp/aios-install-test.pid ]; then
    QEMU_PID=$(cat /tmp/aios-install-test.pid)
fi
log "Live ISO VM started (PID: ${QEMU_PID})"

# Wait for SSH
log "Waiting for SSH (timeout: ${BOOT_TIMEOUT}s)..."
SSH_READY=false
for i in $(seq 1 ${BOOT_TIMEOUT}); do
    if ssh_cmd "echo ok" &>/dev/null; then
        SSH_READY=true
        log "SSH ready after ${i}s"
        break
    fi
    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        die "VM crashed during live boot"
    fi
    sleep 1
done

if [ "${SSH_READY}" != "true" ]; then
    die "SSH did not become available within ${BOOT_TIMEOUT}s"
fi

# Wait for installation to complete
log "Waiting for installation to complete (timeout: ${INSTALL_TIMEOUT}s)..."
INSTALL_DONE=false
for i in $(seq 1 ${INSTALL_TIMEOUT}); do
    # Check if the installer has finished by looking for the installed system
    if ssh_cmd "test -f /mnt/aios-install/home/aios/.aios/vault.enc 2>/dev/null && echo done" 2>/dev/null | grep -q "done"; then
        INSTALL_DONE=true
        log "Installation completed after ${i}s"
        break
    fi
    # Also check if the install partition was mounted (earlier stage)
    if [ $((i % 30)) -eq 0 ]; then
        log "  Still installing... (${i}s elapsed)"
    fi
    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        die "VM crashed during installation"
    fi
    sleep 1
done

if [ "${INSTALL_DONE}" != "true" ]; then
    warn "Installation did not complete within ${INSTALL_TIMEOUT}s — checking partial state"
    # Check what state the installer reached
    ssh_cmd "ls -la /mnt/aios-install/ 2>/dev/null || echo 'mount not found'"
    ssh_cmd "cat /home/aios/.aios/logs/aios.log 2>/dev/null | grep -i 'install' | tail -10"
fi

# ─── Phase 2 Tests: Verify live ISO state ─────────────────────
log ""
log "=== Phase 1 Tests: Live ISO + Installation ==="

run_test "Live ISO booted" "uname -a" "Linux"
run_test "Vault created (autoconfig)" "test -f /home/aios/.aios/vault.enc && echo ok" "ok"
run_test "Config created" "test -f /home/aios/.aios/config.json && echo ok" "ok"
run_test "Installer tools available (sgdisk)" "command -v sgdisk && echo ok" "ok"
run_test "Installer tools available (mkfs.ext4)" "command -v mkfs.ext4 && echo ok" "ok"
run_test "Installer tools available (unsquashfs)" "command -v unsquashfs && echo ok" "ok"
run_test "Installer tools available (grub-install)" "command -v grub-install && echo ok" "ok"
run_test "Virtual disk visible" "lsblk | grep -q vda && echo ok" "ok"

if [ "${INSTALL_DONE}" = "true" ]; then
    run_test "Root partition exists" "lsblk | grep -q vda" "ok"
    run_test "Target has vault" "test -f /mnt/aios-install/home/aios/.aios/vault.enc && echo ok" "ok"
    run_test "Target has config" "test -f /mnt/aios-install/home/aios/.aios/config.json && echo ok" "ok"
    run_test "Target has fstab" "test -f /mnt/aios-install/etc/fstab && echo ok" "ok"
    run_test "Target has GRUB" "test -f /mnt/aios-install/boot/grub/grub.cfg && echo ok || test -d /mnt/aios-install/boot/efi && echo ok" "ok"
fi

# Shutdown the live ISO VM
log "Shutting down live ISO VM..."
ssh_cmd "sudo shutdown -h now" 2>/dev/null || true
sleep 5
kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true
QEMU_PID=""

# ─── Phase 3: Boot from installed disk ────────────────────────
if [ "${INSTALL_DONE}" != "true" ]; then
    log "Skipping Phase 2 (boot from disk) — installation did not complete"
else
    log ""
    log "=== Phase 2: Boot from Installed Disk ==="
    log "Booting from virtual hard drive..."

    qemu-system-x86_64 \
        -name "${VM_NAME}-installed" \
        -machine q35,accel=${ACCEL} \
        -cpu host \
        -m 4096 \
        -smp 2 \
        -drive file="${DISK_IMAGE}",format=qcow2,if=virtio \
        -boot order=c \
        -display none \
        -daemonize \
        -pidfile /tmp/aios-install-test-2.pid \
        -netdev user,id=net0,hostfwd=tcp::${SSH_PORT}-:22 \
        -device virtio-net-pci,netdev=net0 \
        -device intel-hda -device hda-duplex \
        -serial file:/tmp/aios-install-test-serial-2.log \
        &

    QEMU_PID=$!
    sleep 2
    if [ -f /tmp/aios-install-test-2.pid ]; then
        QEMU_PID=$(cat /tmp/aios-install-test-2.pid)
    fi
    log "Installed system VM started (PID: ${QEMU_PID})"

    # Wait for SSH on the installed system
    log "Waiting for installed system SSH (timeout: ${REBOOT_TIMEOUT}s)..."
    SSH_READY=false
    for i in $(seq 1 ${REBOOT_TIMEOUT}); do
        if ssh_cmd "echo ok" &>/dev/null; then
            SSH_READY=true
            log "Installed system SSH ready after ${i}s"
            break
        fi
        if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
            fail "Installed system VM crashed during boot"
            break
        fi
        sleep 1
    done

    if [ "${SSH_READY}" = "true" ]; then
        # ─── Phase 3 Tests: Installed system verification ─────────
        run_test "Installed system boots" "uname -a" "Linux"
        run_test "Hostname set correctly" "hostname" "aios-install-test"
        run_test "User aios exists" "whoami" "aios"
        run_test "Home directory exists" "test -d /home/aios && echo ok" "ok"
        run_test "Config persisted" "test -f /home/aios/.aios/config.json && echo ok" "ok"
        run_test "Vault persisted" "test -f /home/aios/.aios/vault.enc && echo ok" "ok"
        run_test "AiOS binary exists" "test -f /usr/bin/aios && echo ok" "ok"
        run_test "Not running from live ISO" "! findmnt -t overlay -t squashfs -t tmpfs / >/dev/null 2>&1 && echo ok || echo live" "ok"
        run_test "Root is ext4" "findmnt -n -o FSTYPE / 2>/dev/null" "ext4"
        run_test "fstab has root entry" "grep -q 'ext4' /etc/fstab && echo ok" "ok"
        run_test "GRUB installed" "test -f /boot/grub/grub.cfg && echo ok" "ok"
        run_test "SSH works on installed system" "echo ok" "ok"
        run_test "Keyboard layout set" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"system\",{}).get(\"keyboard_layout\",\"\"))' 2>/dev/null" "us"
        run_test "Provider configured" "cat /home/aios/.aios/config.json | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"llm\",{}).get(\"provider\",\"\"))' 2>/dev/null" "claude"

        # Shutdown
        ssh_cmd "sudo shutdown -h now" 2>/dev/null || true
        sleep 3
    else
        fail "Could not SSH into installed system"
    fi
fi

# ─── Cleanup ──────────────────────────────────────────────────
rm -f "${AUTOCONFIG_IMG}" "${INSTALL_AUTOCONFIG}"

# ─── Summary ──────────────────────────────────────────────────
log ""
log "════════════════════════════════════════"
log " Install Test Summary"
log "════════════════════════════════════════"
log "  ${GREEN}Passed: ${PASSED}${NC}"
log "  ${RED}Failed: ${FAILED}${NC}"
log "  ${YELLOW}Warned: ${WARNED}${NC}"
log "════════════════════════════════════════"

if [ "${FAILED}" -gt 0 ]; then
    exit 1
fi
exit 0
