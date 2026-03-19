#!/bin/bash
# AiOS — Build (if needed) and boot the OS
#
# Usage:
#   ./start.sh                  # boot with QEMU/KVM
#   ./start.sh --vbox           # boot with VirtualBox (mic + speaker)
#   ./start.sh --code-rebuild   # rebuild Rust code only + boot
#   ./start.sh --clean          # clean rebuild (keeps caches)
#   ./start.sh --deep           # clean rebuild + purge all caches
#   ./start.sh --nuke           # scorched earth — delete everything + rebuild
#   DEBUG=1 ./start.sh          # boot with serial logging
#
# Flags can be combined: ./start.sh --clean --vbox

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Parse args: extract flags
USE_VBOX=false
ARG=""
AUTOCONFIG_ARG=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --vbox) USE_VBOX=true; shift ;;
        --autoconfig) AUTOCONFIG_ARG="--autoconfig $2"; shift 2 ;;
        *) ARG="$1"; shift ;;
    esac
done

if [ "${ARG}" = "--nuke" ]; then
    echo "[*] NUKE: deleting everything and rebuilding from scratch..."
    "${SCRIPT_DIR}/clean.sh" --nuke
    "${SCRIPT_DIR}/distro/build.sh" ${AUTOCONFIG_ARG} --clean
elif [ "${ARG}" = "--deep" ]; then
    echo "[*] Deep clean: purging all caches and rebuilding..."
    "${SCRIPT_DIR}/clean.sh" --deep
    "${SCRIPT_DIR}/distro/build.sh" ${AUTOCONFIG_ARG} --clean
elif [ "${ARG}" = "--clean" ]; then
    echo "[*] Clean build (keeping caches)..."
    "${SCRIPT_DIR}/clean.sh"
    "${SCRIPT_DIR}/distro/build.sh" ${AUTOCONFIG_ARG} --clean
elif [ "${ARG}" = "--code-rebuild" ]; then
    echo "[*] Rebuilding Rust code only (fast rebuild)..."
    "${SCRIPT_DIR}/distro/build.sh" ${AUTOCONFIG_ARG} --code-rebuild
elif [ "${ARG}" = "--bump-major" ]; then
    echo "[*] Major version bump + clean build..."
    "${SCRIPT_DIR}/clean.sh"
    "${SCRIPT_DIR}/distro/build.sh" ${AUTOCONFIG_ARG} --bump-major
elif [ "${ARG}" = "--bump-minor" ]; then
    echo "[*] Minor version bump + clean build..."
    "${SCRIPT_DIR}/clean.sh"
    "${SCRIPT_DIR}/distro/build.sh" ${AUTOCONFIG_ARG} --bump-minor
else
    ISO=$(find "${SCRIPT_DIR}/distro/build" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)
    if [ -z "${ISO}" ]; then
        echo "[*] No ISO found — building AiOS..."
        "${SCRIPT_DIR}/distro/build.sh" ${AUTOCONFIG_ARG}
    fi
fi

ISO=$(find "${SCRIPT_DIR}/distro/build" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)
if [ -z "${ISO}" ]; then
    echo "ERROR: ISO build failed."
    exit 1
fi

echo "[*] Booting AiOS: ${ISO}"

if [ "${USE_VBOX}" = true ]; then
    echo "[*] Using VirtualBox (mic + speaker)"
    cd "${SCRIPT_DIR}/distro" && exec ./run-vm-vbox.sh "${ISO}"
else
    if [ "${DEBUG:-}" = "1" ]; then
        echo "[*] Debug mode on — after boot, check /tmp/aios-serial.log"
    fi
    echo "[*] Using QEMU/KVM"
    # Kill any running AiOS QEMU instance first
    pkill -9 -f "qemu-system-x86_64.*aios" 2>/dev/null || true
    virsh --connect qemu:///system destroy aios-live 2>/dev/null || true
    virsh --connect qemu:///system undefine aios-live 2>/dev/null || true
    sleep 1
    cd "${SCRIPT_DIR}/distro" && exec ./run-vm.sh "${ISO}"
fi
