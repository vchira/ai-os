#!/bin/bash
# AiOS — Build (if needed) and boot the OS in QEMU
#
# Usage:
#   ./start.sh              # incremental build + boot
#   ./start.sh --clean      # full clean rebuild + boot
#   DEBUG=1 ./start.sh      # boot with serial logging

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARG="${1:-}"

# Build if --clean requested or no ISO exists
if [ "${ARG}" = "--clean" ]; then
    echo "[*] Clean build requested..."
    "${SCRIPT_DIR}/clean.sh"
    "${SCRIPT_DIR}/distro/build.sh" --clean
else
    ISO=$(find "${SCRIPT_DIR}/distro/build" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)
    if [ -z "${ISO}" ]; then
        echo "[*] No ISO found — building AiOS..."
        "${SCRIPT_DIR}/distro/build.sh"
    fi
fi

ISO=$(find "${SCRIPT_DIR}/distro/build" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)
if [ -z "${ISO}" ]; then
    echo "ERROR: ISO build failed."
    exit 1
fi

echo "[*] Booting AiOS: ${ISO}"
if [ "${DEBUG:-}" = "1" ]; then
    echo "[*] Debug mode on — after boot, check /tmp/aios-serial.log"
fi
cd "${SCRIPT_DIR}/distro" && exec ./run-vm.sh "${ISO}"
