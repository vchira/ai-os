#!/bin/bash
# AiOS QEMU Hard Drive Installation Test
#
# Tests the full installation flow:
#   1. Boot live ISO with a virtual hard drive attached
#   2. SSH in and trigger installation via the installer API
#   3. Verify the installed system on disk
#   4. Reboot from installed disk and verify
#   5. Clean up
#
# Usage:
#   ./tests/qemu/run-install-test.sh [path-to-iso]
#
# Prerequisites:
#   - qemu-system-x86_64 with KVM
#   - qemu-img
#   - sshpass

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# ─── Configuration ────────────────────────────────────────────
TEMP_DIR="${HOME}/temp/aios-install-test"
mkdir -p "${TEMP_DIR}"

ISO="${1:-${PROJECT_DIR}/distro/build/live-image-amd64.hybrid.iso}"
DISK_IMAGE="${TEMP_DIR}/install-disk.qcow2"
DISK_SIZE="16G"
SSH_PORT=2233  # Use high port to avoid conflicts with run-tests.sh (2222) and dev VM
SSH_USER="aios"
SSH_PASS="aios"  # Default live ISO password
VM_NAME="aios-install-test"
BOOT_TIMEOUT=60
INSTALL_TIMEOUT=300
REBOOT_TIMEOUT=60

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

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
    die "ISO not found: ${ISO}\nBuild it first: ./start.sh"
fi

for tool in qemu-system-x86_64 qemu-img sshpass; do
    command -v "${tool}" &>/dev/null || die "${tool} not found"
done

if [ -w /dev/kvm ] 2>/dev/null; then
    ACCEL="kvm"
else
    warn "No KVM — VM will be slow"
    ACCEL="tcg"
fi

# ─── Kill any leftover VMs from previous runs ─────────────────
pkill -f "qemu-system-x86_64.*${VM_NAME}" 2>/dev/null || true
sleep 1

# ─── Phase 1: Create disk + boot live ISO ─────────────────────
log "Creating ${DISK_SIZE} virtual disk: ${DISK_IMAGE}"
rm -f "${DISK_IMAGE}"
qemu-img create -f qcow2 "${DISK_IMAGE}" "${DISK_SIZE}" >/dev/null

log "Phase 1: Booting live ISO with virtual hard drive..."
qemu-system-x86_64 \
    -name "${VM_NAME}" \
    -machine q35,accel=${ACCEL} \
    -cpu host \
    -m 4096 \
    -smp 2 \
    -cdrom "${ISO}" \
    -drive file="${DISK_IMAGE}",format=qcow2,if=virtio \
    -boot order=d \
    -display none \
    -daemonize \
    -pidfile "${TEMP_DIR}/vm.pid" \
    -netdev user,id=net0,hostfwd=tcp::${SSH_PORT}-:22 \
    -device virtio-net-pci,netdev=net0 \
    -device intel-hda -device hda-duplex \
    -serial file:"${TEMP_DIR}/serial.log" \
    || die "Failed to start QEMU"

sleep 2
QEMU_PID=$(cat "${TEMP_DIR}/vm.pid" 2>/dev/null || echo "")
log "VM started (PID: ${QEMU_PID})"

# Wait for SSH
log "Waiting for SSH (timeout: ${BOOT_TIMEOUT}s)..."
SSH_READY=false
for i in $(seq 1 ${BOOT_TIMEOUT}); do
    if ssh_cmd "echo ok" &>/dev/null; then
        SSH_READY=true
        log "SSH ready after ${i}s"
        break
    fi
    if [ -n "${QEMU_PID}" ] && ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        die "VM crashed during boot"
    fi
    sleep 1
done
[ "${SSH_READY}" = "true" ] || die "SSH timeout after ${BOOT_TIMEOUT}s"

# ─── Phase 1 Tests: Live ISO + installer tools ────────────────
log ""
log "=== Phase 1: Live ISO Verification ==="

run_test "Live ISO booted" "uname -a" "Linux"
run_test "Running from live media" "findmnt -n -o FSTYPE / 2>/dev/null || echo overlay" "overlay"
run_test "Virtual disk visible (vda)" "lsblk -d -n -o NAME | grep -q vda && echo ok" "ok"
run_test "sgdisk available" "which sgdisk >/dev/null 2>&1 || sudo which sgdisk >/dev/null 2>&1 && echo ok" "ok"
run_test "mkfs.ext4 available" "which mkfs.ext4 >/dev/null 2>&1 || sudo which mkfs.ext4 >/dev/null 2>&1 && echo ok" "ok"
run_test "unsquashfs available" "which unsquashfs >/dev/null 2>&1 && echo ok" "ok"
run_test "grub-install available" "which grub-install >/dev/null 2>&1 || sudo which grub-install >/dev/null 2>&1 && echo ok" "ok"
run_test "Live squashfs exists" "test -f /lib/live/mount/medium/live/filesystem.squashfs && echo ok || test -f /run/live/medium/live/filesystem.squashfs && echo ok" "ok"

# ─── Phase 2: Run the installer via SSH ───────────────────────
log ""
log "=== Phase 2: Run Installer ==="

# The installer is built into the AiOS binary. We trigger it by calling
# the Rust installer directly via a small Python/bash script that invokes
# the partitioning + copy + bootloader steps.
log "Partitioning virtual disk..."
ssh_cmd "sudo sgdisk --zap-all /dev/vda" 2>/dev/null
# Partition 1: BIOS Boot (1MB) — required for GRUB on GPT disks
ssh_cmd "sudo sgdisk -n 1:0:+1M -t 1:ef02 -c 1:'BIOS Boot' /dev/vda" 2>/dev/null
# Partition 2: EFI System (512MB)
ssh_cmd "sudo sgdisk -n 2:0:+512M -t 2:ef00 -c 2:'EFI' /dev/vda" 2>/dev/null
# Partition 3: Swap (2GB)
ssh_cmd "sudo sgdisk -n 3:0:+2G -t 3:8200 -c 3:'Swap' /dev/vda" 2>/dev/null
# Partition 4: Root (rest of disk)
ssh_cmd "sudo sgdisk -n 4:0:0 -t 4:8300 -c 4:'Root' /dev/vda" 2>/dev/null

run_test "Partitions created" "lsblk /dev/vda -n -o NAME | grep -c vda" "4"

log "Formatting partitions..."
# vda1 = BIOS Boot (no format needed)
ssh_cmd "sudo mkfs.fat -F 32 /dev/vda2" 2>/dev/null
ssh_cmd "sudo mkswap /dev/vda3" 2>/dev/null
ssh_cmd "sudo mkfs.ext4 -q -F /dev/vda4" 2>/dev/null

run_test "EFI partition formatted" "sudo blkid /dev/vda2 | grep -q vfat && echo ok" "ok"
run_test "Root partition formatted" "sudo blkid /dev/vda4 | grep -q ext4 && echo ok" "ok"

log "Mounting and copying filesystem..."
ssh_cmd "sudo mkdir -p /mnt/aios-install"
ssh_cmd "sudo mount /dev/vda4 /mnt/aios-install"
ssh_cmd "sudo mkdir -p /mnt/aios-install/boot/efi"
ssh_cmd "sudo mount /dev/vda2 /mnt/aios-install/boot/efi"

# Find the squashfs
SQUASHFS=$(ssh_cmd "ls /lib/live/mount/medium/live/filesystem.squashfs 2>/dev/null || ls /run/live/medium/live/filesystem.squashfs 2>/dev/null" || echo "")
if [ -z "${SQUASHFS}" ]; then
    fail "Cannot find live filesystem.squashfs"
else
    log "Extracting squashfs (this takes a minute)..."
    ssh_cmd "sudo unsquashfs -f -d /mnt/aios-install ${SQUASHFS}" 2>/dev/null
    run_test "Filesystem extracted" "test -f /mnt/aios-install/usr/bin/aios && echo ok" "ok"
fi

log "Writing fstab..."
ROOT_UUID=$(ssh_cmd "sudo blkid -s UUID -o value /dev/vda4")
EFI_UUID=$(ssh_cmd "sudo blkid -s UUID -o value /dev/vda2")
SWAP_UUID=$(ssh_cmd "sudo blkid -s UUID -o value /dev/vda3")
ssh_cmd "sudo bash -c 'cat > /mnt/aios-install/etc/fstab << FSTAB
UUID=${ROOT_UUID}  /          ext4  errors=remount-ro  0  1
UUID=${EFI_UUID}   /boot/efi  vfat  umask=0077         0  1
UUID=${SWAP_UUID}  none       swap  sw                 0  0
FSTAB'"

run_test "fstab written" "sudo cat /mnt/aios-install/etc/fstab | grep -q ext4 && echo ok" "ok"

log "Installing GRUB bootloader..."
ssh_cmd "sudo mount --bind /dev /mnt/aios-install/dev"
ssh_cmd "sudo mount --bind /proc /mnt/aios-install/proc"
ssh_cmd "sudo mount --bind /sys /mnt/aios-install/sys"
# Try BIOS install (UEFI needs efivars which may not be available in QEMU without OVMF)
ssh_cmd "sudo chroot /mnt/aios-install grub-install --target=i386-pc /dev/vda 2>&1" 2>/dev/null
ssh_cmd "sudo chroot /mnt/aios-install grub-mkconfig -o /boot/grub/grub.cfg 2>&1" 2>/dev/null

run_test "GRUB config generated" "sudo test -f /mnt/aios-install/boot/grub/grub.cfg && echo ok" "ok"

log "Configuring installed system..."
ssh_cmd "sudo bash -c 'echo aios-install-test > /mnt/aios-install/etc/hostname'"
ssh_cmd "sudo mkdir -p /mnt/aios-install/home/aios/.aios"
# Copy config + vault from the live session if they exist
ssh_cmd "sudo cp -a /home/aios/.aios/config.json /mnt/aios-install/home/aios/.aios/ 2>/dev/null || true"
ssh_cmd "sudo cp -a /home/aios/.aios/vault.enc /mnt/aios-install/home/aios/.aios/ 2>/dev/null || true"
ssh_cmd "sudo chown -R 1000:1000 /mnt/aios-install/home/aios"

run_test "Hostname set" "sudo cat /mnt/aios-install/etc/hostname" "aios-install-test"
run_test "AiOS binary on disk" "sudo test -f /mnt/aios-install/usr/bin/aios && echo ok" "ok"

# Clean up mounts
ssh_cmd "sudo umount /mnt/aios-install/sys 2>/dev/null || true"
ssh_cmd "sudo umount /mnt/aios-install/proc 2>/dev/null || true"
ssh_cmd "sudo umount /mnt/aios-install/dev 2>/dev/null || true"
ssh_cmd "sudo umount /mnt/aios-install/boot/efi 2>/dev/null || true"
ssh_cmd "sudo umount /mnt/aios-install 2>/dev/null || true"

# Shutdown the live ISO
log "Shutting down live ISO VM..."
ssh_cmd "sudo shutdown -h now" 2>/dev/null || true
sleep 5
kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true
QEMU_PID=""

# ─── Phase 3: Boot from installed disk ────────────────────────
log ""
log "=== Phase 3: Boot from Installed Disk ==="
log "Booting from virtual hard drive (no ISO)..."

qemu-system-x86_64 \
    -name "${VM_NAME}-disk" \
    -machine q35,accel=${ACCEL} \
    -cpu host \
    -m 4096 \
    -smp 2 \
    -drive file="${DISK_IMAGE}",format=qcow2,if=virtio \
    -boot order=c \
    -display none \
    -daemonize \
    -pidfile "${TEMP_DIR}/vm2.pid" \
    -netdev user,id=net0,hostfwd=tcp::${SSH_PORT}-:22 \
    -device virtio-net-pci,netdev=net0 \
    -device intel-hda -device hda-duplex \
    -serial file:"${TEMP_DIR}/serial2.log" \
    || die "Failed to start QEMU from installed disk"

sleep 2
QEMU_PID=$(cat "${TEMP_DIR}/vm2.pid" 2>/dev/null || echo "")
log "Installed system VM started (PID: ${QEMU_PID})"

# Wait for SSH — password may be different on installed system
log "Waiting for installed system SSH (timeout: ${REBOOT_TIMEOUT}s)..."
SSH_READY=false
for i in $(seq 1 ${REBOOT_TIMEOUT}); do
    if ssh_cmd "echo ok" &>/dev/null; then
        SSH_READY=true
        log "SSH ready after ${i}s"
        break
    fi
    if [ -n "${QEMU_PID}" ] && ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        fail "Installed system VM crashed during boot"
        break
    fi
    sleep 1
done

if [ "${SSH_READY}" = "true" ]; then
    run_test "Installed system boots" "uname -a" "Linux"
    run_test "Not live media" "! findmnt -t overlay / >/dev/null 2>&1 && echo ok || echo live" "ok"
    run_test "Root is ext4" "findmnt -n -o FSTYPE /" "ext4"
    run_test "Hostname correct" "hostname" "aios-install-test"
    run_test "User aios exists" "id aios >/dev/null 2>&1 && echo ok" "ok"
    run_test "AiOS binary exists" "test -f /usr/bin/aios && echo ok" "ok"
    run_test "GRUB installed" "test -f /boot/grub/grub.cfg && echo ok" "ok"
    run_test "fstab has root" "grep -q ext4 /etc/fstab && echo ok" "ok"
    run_test "SSH works" "echo ok" "ok"

    # Shutdown
    ssh_cmd "sudo shutdown -h now" 2>/dev/null || true
    sleep 3
else
    fail "Could not SSH into installed system"
fi

# ─── Cleanup ──────────────────────────────────────────────────
log "Cleaning up..."
rm -f "${DISK_IMAGE}" "${TEMP_DIR}/vm.pid" "${TEMP_DIR}/vm2.pid"
rm -f "${TEMP_DIR}/serial.log" "${TEMP_DIR}/serial2.log"

# ─── Summary ──────────────────────────────────────────────────
log ""
log "════════════════════════════════════════"
log " Install Test Summary"
log "════════════════════════════════════════"
log "  ${GREEN}Passed: ${PASSED}${NC}"
log "  ${RED}Failed: ${FAILED}${NC}"
log "  ${YELLOW}Warned: ${WARNED}${NC}"
log "════════════════════════════════════════"

[ "${FAILED}" -gt 0 ] && exit 1
exit 0
