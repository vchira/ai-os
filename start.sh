#!/bin/bash
# AiOS — Build (if needed) and boot the OS in QEMU
#
# Usage:
#   ./start.sh              # just boot (rebuild code if source changed)
#   ./start.sh --code-rebuild       # rebuild Rust code only + boot (fast, ~1 min)
#   ./start.sh --clean      # full clean rebuild + boot (slow, ~20 min)
#   DEBUG=1 ./start.sh      # boot with serial logging

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARG="${1:-}"

if [ "${ARG}" = "--clean" ]; then
    echo "[*] Clean build requested..."
    "${SCRIPT_DIR}/clean.sh"
    "${SCRIPT_DIR}/distro/build.sh" --clean
elif [ "${ARG}" = "--code-rebuild" ]; then
    echo "[*] Rebuilding Rust code only (fast rebuild)..."
    "${SCRIPT_DIR}/distro/build.sh" --code-rebuild
elif [ "${ARG}" = "--bump-major" ]; then
    echo "[*] Major version bump + clean build..."
    "${SCRIPT_DIR}/clean.sh"
    "${SCRIPT_DIR}/distro/build.sh" --bump-major
elif [ "${ARG}" = "--bump-minor" ]; then
    echo "[*] Minor version bump + clean build..."
    "${SCRIPT_DIR}/clean.sh"
    "${SCRIPT_DIR}/distro/build.sh" --bump-minor
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

# Clean up stale VM before starting
virsh --connect qemu:///system destroy aios-live 2>/dev/null || true
virsh --connect qemu:///system undefine aios-live 2>/dev/null || true

cd "${SCRIPT_DIR}/distro" && exec ./run-vm.sh "${ISO}"
