#!/bin/bash
# Quick QEMU launch for testing the AiOS ISO
#
# Usage: ./run-qemu.sh [path-to-iso]

ISO="${1:-build/live-image-amd64.hybrid.iso}"

if [ ! -f "${ISO}" ]; then
    echo "ISO not found: ${ISO}"
    echo "Run ./build.sh first, or specify path: ./run-qemu.sh path/to/aios.iso"
    exit 1
fi

# Create a disk image for install testing (if not exists)
DISK="build/aios-disk.qcow2"
if [ ! -f "${DISK}" ]; then
    qemu-img create -f qcow2 "${DISK}" 20G
fi

exec qemu-system-x86_64 \
    -enable-kvm \
    -m 4G \
    -smp 4 \
    -cpu host \
    -cdrom "${ISO}" \
    -drive file="${DISK}",format=qcow2 \
    -boot d \
    -device virtio-vga-gl \
    -display sdl,gl=on \
    -device virtio-net-pci,netdev=net0 \
    -netdev user,id=net0,hostfwd=tcp::2222-:22 \
    -device intel-hda \
    -device hda-duplex \
    -usb \
    -device usb-tablet
