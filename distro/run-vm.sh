#!/bin/bash
# AiOS VM launcher — direct QEMU with KVM, audio + clipboard support
#
# Uses QEMU directly (not libvirt) so the process inherits the user's
# full environment — PipeWire/PulseAudio audio works without fighting
# libvirt's sandbox.

set -euo pipefail

ISO="${1:-build/live-image-amd64.hybrid.iso}"

if [ ! -f "${ISO}" ]; then
    echo "ISO not found: ${ISO}"
    echo "Run ./build.sh first"
    exit 1
fi

ISO="$(cd "$(dirname "${ISO}")" && pwd)/$(basename "${ISO}")"

# ─── Virtual hard drive ──────────────────────────────────────
# Store outside build/ (which is root-owned from Docker)
DISK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DISK_IMG="${DISK_DIR}/aios-disk.qcow2"
DISK_SIZE="32G"

if [ ! -f "${DISK_IMG}" ]; then
    echo "[*] Creating ${DISK_SIZE} virtual hard drive: ${DISK_IMG}"
    qemu-img create -f qcow2 "${DISK_IMG}" "${DISK_SIZE}"
fi

# ─── Check dependencies ──────────────────────────────────────
if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "qemu-system-x86_64 not installed. Install with:"
    echo "  sudo apt install qemu-system-x86"
    exit 1
fi

# Check KVM access
if [ ! -w /dev/kvm ]; then
    echo "[!] No write access to /dev/kvm. Add yourself to the kvm group:"
    echo "    sudo usermod -aG kvm $(whoami)"
    echo "    (then log out and back in)"
    exit 1
fi

# ─── Kill previous instance ──────────────────────────────────
# Also clean up any libvirt-managed instance from before.
if command -v virsh &>/dev/null; then
    virsh --connect qemu:///system destroy aios-live 2>/dev/null || true
    virsh --connect qemu:///system undefine aios-live 2>/dev/null || true
fi
# Kill any leftover direct QEMU instance
pkill -f "qemu-system-x86_64.*aios-live" 2>/dev/null || true
sleep 0.5

# ─── Audio backend ────────────────────────────────────────────
# QEMU inherits our environment, so PipeWire/PulseAudio just works.
AUDIO_ARGS=""
if [ -S "/run/user/$(id -u)/pipewire-0" ]; then
    echo "[*] Audio: PipeWire (mic + speaker)"
    AUDIO_ARGS="-audiodev pipewire,id=audio0,in.stream-name=aios-mic,out.stream-name=aios-speaker -device intel-hda -device hda-duplex,audiodev=audio0"
elif [ -S "/run/user/$(id -u)/pulse/native" ]; then
    echo "[*] Audio: PulseAudio (mic + speaker)"
    AUDIO_ARGS="-audiodev pa,id=audio0,server=/run/user/$(id -u)/pulse/native -device intel-hda -device hda-duplex,audiodev=audio0"
else
    echo "[*] Audio: none (no PipeWire or PulseAudio detected)"
    AUDIO_ARGS="-device intel-hda -device hda-duplex"
fi

# ─── Network: TAP via bridge (if available) or user-mode ──────
NET_ARGS=""
if ip link show virbr0 &>/dev/null; then
    # Create a TAP device for bridged networking
    TAP_NAME="aios-tap0"
    # Try to use the helper for rootless TAP
    QEMU_BRIDGE_HELPER="/usr/lib/qemu/qemu-bridge-helper"
    if [ -x "${QEMU_BRIDGE_HELPER}" ]; then
        NET_ARGS="-netdev bridge,id=net0,br=virbr0,helper=${QEMU_BRIDGE_HELPER} -device virtio-net-pci,netdev=net0"
    else
        echo "[!] No qemu-bridge-helper — using user-mode networking (no LAN access)"
        NET_ARGS="-netdev user,id=net0,hostfwd=tcp::8080-:80 -device virtio-net-pci,netdev=net0"
    fi
else
    echo "[!] No virbr0 bridge — using user-mode networking"
    NET_ARGS="-netdev user,id=net0,hostfwd=tcp::8080-:80 -device virtio-net-pci,netdev=net0"
fi

# ─── SPICE for clipboard + display ───────────────────────────
SPICE_PORT=5900
SPICE_ARGS="-spice port=${SPICE_PORT},addr=127.0.0.1,disable-ticketing=on"
SPICE_ARGS+=" -device virtio-serial-pci"
SPICE_ARGS+=" -chardev spicevmc,id=vdagent,name=vdagent"
SPICE_ARGS+=" -device virtserialport,chardev=vdagent,name=com.redhat.spice.0"

# ─── Launch QEMU ─────────────────────────────────────────────
echo "[*] Starting AiOS VM..."
echo "[*] ISO: ${ISO}"

qemu-system-x86_64 \
    -name aios-live \
    -machine q35,accel=kvm \
    -cpu host \
    -m 4096 \
    -smp 4 \
    -cdrom "${ISO}" \
    -drive file="${DISK_IMG}",format=qcow2,if=virtio,id=disk0 \
    -boot order=dc \
    -display none \
    -device qxl-vga,ram_size=67108864,vram_size=67108864,vgamem_mb=16 \
    -device qemu-xhci,id=usb \
    -device usb-tablet,bus=usb.0 \
    ${AUDIO_ARGS} \
    ${NET_ARGS} \
    ${SPICE_ARGS} \
    -daemonize

echo "[*] VM started"

# ─── Connect viewer ──────────────────────────────────────────
sleep 10

if command -v remote-viewer &>/dev/null; then
    echo "[*] Connecting: spice://127.0.0.1:${SPICE_PORT}"
    exec remote-viewer "spice://127.0.0.1:${SPICE_PORT}"
elif command -v virt-viewer &>/dev/null; then
    echo "[*] Connecting with virt-viewer..."
    exec virt-viewer "spice://127.0.0.1:${SPICE_PORT}"
else
    echo "[*] Connect manually: remote-viewer spice://127.0.0.1:${SPICE_PORT}"
fi
