#!/bin/bash
# AiOS VM launcher — VirtualBox with full audio (mic + speaker)
#
# VirtualBox provides native audio passthrough including microphone,
# clipboard sharing, and good Linux guest support.

set -euo pipefail

ISO="${1:-build/live-image-amd64.hybrid.iso}"

if [ ! -f "${ISO}" ]; then
    echo "ISO not found: ${ISO}"
    echo "Run ./build.sh first"
    exit 1
fi

ISO="$(cd "$(dirname "${ISO}")" && pwd)/$(basename "${ISO}")"
VM_NAME="aios-live"
# Store outside build/ (which is root-owned from Docker)
DISK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DISK_IMG="${DISK_DIR}/aios-disk.vdi"
DISK_SIZE=32768  # MB

# ─── Check dependencies ──────────────────────────────────────
if ! command -v VBoxManage &>/dev/null; then
    echo "VirtualBox not installed. Install with:"
    echo "  sudo apt install virtualbox virtualbox-ext-pack"
    exit 1
fi

# ─── Clean up previous VM ────────────────────────────────────
if VBoxManage showvminfo "${VM_NAME}" &>/dev/null; then
    echo "[*] Removing previous ${VM_NAME} VM..."
    VBoxManage controlvm "${VM_NAME}" poweroff 2>/dev/null || true
    sleep 1
    VBoxManage unregistervm "${VM_NAME}" --delete-all 2>/dev/null || true
    sleep 0.5
fi

# ─── Create virtual disk ─────────────────────────────────────
if [ ! -f "${DISK_IMG}" ]; then
    echo "[*] Creating ${DISK_SIZE}MB virtual hard drive: ${DISK_IMG}"
    VBoxManage createmedium disk --filename "${DISK_IMG}" --size ${DISK_SIZE} --format VDI
fi

# ─── Create VM ────────────────────────────────────────────────
echo "[*] Creating AiOS VM..."
VBoxManage createvm --name "${VM_NAME}" --ostype "Debian_64" --register

# CPU + Memory
VBoxManage modifyvm "${VM_NAME}" \
    --cpus 4 \
    --memory 4096 \
    --vram 128 \
    --ioapic on \
    --firmware efi

# Storage: SATA controller with disk + ISO
VBoxManage storagectl "${VM_NAME}" --name "SATA" --add sata --controller IntelAhci --portcount 2
VBoxManage storageattach "${VM_NAME}" --storagectl "SATA" --port 0 --device 0 \
    --type hdd --medium "${DISK_IMG}"
VBoxManage storageattach "${VM_NAME}" --storagectl "SATA" --port 1 --device 0 \
    --type dvddrive --medium "${ISO}"

# Boot order: disk first, DVD as fallback
# Press F12 at boot to get the boot menu and select DVD for install
VBoxManage modifyvm "${VM_NAME}" --boot1 disk --boot2 dvd --boot3 none --boot4 none

# Graphics
VBoxManage modifyvm "${VM_NAME}" --graphicscontroller vmsvga

# Network: NAT with port forwarding for web channel
VBoxManage modifyvm "${VM_NAME}" --nic1 nat
VBoxManage modifyvm "${VM_NAME}" --natpf1 "web,tcp,,8080,,80"
VBoxManage modifyvm "${VM_NAME}" --natpf1 "ssh,tcp,,2222,,22"

# Audio: PulseAudio host driver with input + output enabled
VBoxManage modifyvm "${VM_NAME}" \
    --audio-driver pulse \
    --audio-controller hda \
    --audio-in on \
    --audio-out on

# Clipboard + drag-and-drop
VBoxManage modifyvm "${VM_NAME}" --clipboard-mode bidirectional
VBoxManage modifyvm "${VM_NAME}" --drag-and-drop bidirectional

# USB 2.0 controller
VBoxManage modifyvm "${VM_NAME}" --usbehci on 2>/dev/null || \
    VBoxManage modifyvm "${VM_NAME}" --usbohci on 2>/dev/null || true

echo "[*] Starting AiOS VM..."
echo "[*] ISO: ${ISO}"
echo "[*] Disk: ${DISK_IMG}"
echo "[*] Audio: PulseAudio (mic + speaker)"
echo "[*] Network: NAT (web: localhost:8080, ssh: localhost:2222)"

# ─── Launch VM ────────────────────────────────────────────────
VBoxManage startvm "${VM_NAME}" --type gui

echo "[*] VM started"
echo ""
echo "    Web channel: http://localhost:8080"
echo "    SSH:         ssh -p 2222 aios@localhost"
echo ""
echo "    To eject ISO after install:"
echo "      VBoxManage storageattach ${VM_NAME} --storagectl SATA --port 1 --device 0 --medium emptydrive"
