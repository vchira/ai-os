#!/bin/bash
# AiOS — Build (if needed) and boot the OS in QEMU
#
# Usage:
#   ./start.sh          # build + boot
#   DEBUG=1 ./start.sh  # build + boot with serial logging

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ISO=$(find "${SCRIPT_DIR}/distro/build" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)

if [ -z "${ISO}" ]; then
    echo "[*] No ISO found — building AiOS..."
    "${SCRIPT_DIR}/distro/build.sh"
    ISO=$(find "${SCRIPT_DIR}/distro/build" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)
    if [ -z "${ISO}" ]; then
        echo "ERROR: ISO build failed."
        exit 1
    fi
fi

echo "[*] Booting AiOS: ${ISO}"
if [ "${DEBUG:-}" = "1" ]; then
    echo "[*] Debug mode on — after boot, check /tmp/aios-serial.log"
fi
cd "${SCRIPT_DIR}/distro" && exec ./run-qemu.sh "${ISO}"
