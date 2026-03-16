#!/bin/bash
# Quick QEMU launch for testing the AiOS ISO
#
# Usage:
#   ./run-qemu.sh [path-to-iso]
#   DEBUG=1 ./run-qemu.sh [path-to-iso]   # serial console → /tmp/aios-serial.log

ISO="${1:-build/live-image-amd64.hybrid.iso}"

if [ ! -f "${ISO}" ]; then
    echo "ISO not found: ${ISO}"
    echo "Run ./build.sh first"
    exit 1
fi

EXTRA_ARGS=""
if [ "${DEBUG:-}" = "1" ]; then
    LOGFILE="/tmp/aios-serial.log"
    echo "[*] Debug mode: serial output → ${LOGFILE}"
    echo "[*] After boot, check ${LOGFILE} for full boot log"
    EXTRA_ARGS="-serial file:${LOGFILE} -append console=ttyS0,115200"
fi

exec qemu-system-x86_64 \
    -enable-kvm \
    -m 4G \
    -smp 4 \
    -cpu host \
    -cdrom "${ISO}" \
    -boot d \
    -device virtio-vga-gl \
    -display sdl,gl=on \
    -device virtio-net-pci,netdev=net0 \
    -netdev user,id=net0 \
    -device intel-hda \
    -device hda-duplex \
    -usb \
    -device usb-tablet \
    ${EXTRA_ARGS}
