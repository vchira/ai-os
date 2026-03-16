#!/bin/bash
# AiOS VM launcher using virt-manager (libvirt/KVM)
#
# Features: clipboard sharing, audio, USB passthrough — all native via SPICE
#
# Usage:
#   ./run-qemu.sh [path-to-iso]

set -euo pipefail

ISO="${1:-build/live-image-amd64.hybrid.iso}"

if [ ! -f "${ISO}" ]; then
    echo "ISO not found: ${ISO}"
    echo "Run ./build.sh first"
    exit 1
fi

# Use absolute path for libvirt
ISO="$(cd "$(dirname "${ISO}")" && pwd)/$(basename "${ISO}")"
VM_NAME="aios-live"

# Check dependencies
if ! command -v virt-install &>/dev/null; then
    echo "virt-manager not installed. Install with:"
    echo "  sudo apt install virt-manager"
    exit 1
fi

# Make sure libvirtd is running
if ! systemctl is-active --quiet libvirtd 2>/dev/null; then
    echo "[*] Starting libvirtd..."
    sudo systemctl start libvirtd
fi

# Remove existing VM with same name (if any)
if virsh list --all --name 2>/dev/null | grep -q "^${VM_NAME}$"; then
    echo "[*] Removing previous ${VM_NAME} VM..."
    virsh destroy "${VM_NAME}" 2>/dev/null || true
    virsh undefine "${VM_NAME}" --remove-all-storage 2>/dev/null || true
fi

echo "[*] Creating AiOS VM..."
echo "[*] ISO: ${ISO}"
echo "[*] Clipboard, audio, and USB passthrough enabled"

# Create and start the VM
virt-install \
    --name "${VM_NAME}" \
    --ram 4096 \
    --vcpus 4 \
    --cdrom "${ISO}" \
    --os-variant debian12 \
    --graphics spice,listen=none \
    --video qxl \
    --channel spicevmc,target.type=virtio,target.name=com.redhat.spice.0 \
    --sound default \
    --network default \
    --boot cdrom \
    --disk none \
    --noautoconsole

echo "[*] VM started. Opening virt-manager..."
exec virt-manager --connect qemu:///system --show-domain-console "${VM_NAME}"
