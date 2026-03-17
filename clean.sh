#!/bin/bash
# AiOS — Clean build artifacts
#
# Usage:
#   ./clean.sh          # clean build output (preserves all caches)
#   ./clean.sh --deep   # also purge package + cargo + external caches
#   ./clean.sh --nuke   # delete absolutely everything — full from-scratch rebuild
#
# Cleans:
#   - Rust build artifacts (aios-app-rs/target/)
#   - ISO build directory (distro/build/) — needs Docker since files are root-owned
#   - Docker builder image (forces rebuild with latest Dockerfile)
#   - Python artifacts (.venv, __pycache__, egg-info)
# With --deep:
#   - Docker package cache volume (forces re-downloading all .deb packages)
#   - Cargo registry cache volume (forces re-downloading all crates)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARG="${1:-}"

echo "========================================"
echo "  AiOS — Cleaning all build artifacts"
echo "========================================"
echo ""

# Rust + ISO build dirs (mixed ownership from Docker + local builds)
if [ -d "${SCRIPT_DIR}/aios-app-rs/target" ] || [ -d "${SCRIPT_DIR}/distro/build" ]; then
    echo "[*] Cleaning build directories..."
    if command -v docker &>/dev/null; then
        docker run --rm --privileged -u root -v "${SCRIPT_DIR}:/work" debian:bookworm \
            bash -c '
                umount -lf /work/distro/build/chroot/proc 2>/dev/null
                umount -lf /work/distro/build/chroot/sys 2>/dev/null
                umount -lf /work/distro/build/chroot/dev/pts 2>/dev/null
                rm -rf /work/aios-app-rs/target /work/distro/build
            '
    else
        rm -rf "${SCRIPT_DIR}/aios-app-rs/target" 2>/dev/null || true
        rm -rf "${SCRIPT_DIR}/distro/build" 2>/dev/null || true
        echo "  WARN: some files may remain (needs docker or sudo)"
    fi
fi

# Docker builder image — only remove with --deep or --nuke
# The image contains pre-cached packages, so keeping it avoids re-downloading
if [ "${ARG}" = "--deep" ] || [ "${ARG}" = "--nuke" ]; then
    if docker image inspect aios-builder &>/dev/null 2>&1; then
        echo "[*] Removing Docker builder image..."
        docker rmi aios-builder 2>/dev/null || true
    fi
fi

# Python artifacts
echo "[*] Cleaning Python artifacts..."
rm -rf "${SCRIPT_DIR}/.venv"
rm -rf "${SCRIPT_DIR}/aios-app/build" "${SCRIPT_DIR}/aios-app/dist"
find "${SCRIPT_DIR}" -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
find "${SCRIPT_DIR}" -type d -name "*.egg-info" -exec rm -rf {} + 2>/dev/null || true
find "${SCRIPT_DIR}" -type f -name "*.pyc" -delete 2>/dev/null || true

# Generated files
# _inner_build.sh is a source file, not generated — don't delete it

# Cache cleanup based on level
if [ "${ARG}" = "--nuke" ]; then
    echo "[*] NUKE: Removing all Docker cache volumes..."
    docker volume rm aios-build-cache 2>/dev/null || true
    docker volume rm aios-cargo-cache 2>/dev/null || true
    echo "[*] NUKE: Purging Docker build cache..."
    docker builder prune -af 2>/dev/null || true
    echo "[*] NUKE: Removing VirtualBox VM + disk..."
    if command -v VBoxManage &>/dev/null; then
        VBoxManage controlvm aios-live poweroff 2>/dev/null || true
        sleep 1
        VBoxManage unregistervm aios-live --delete-all 2>/dev/null || true
    fi
    rm -f "${SCRIPT_DIR}/distro/aios-disk.vdi" 2>/dev/null || true
    echo "[*] NUKE: Removing QEMU disk..."
    rm -f "${SCRIPT_DIR}/distro/aios-disk.qcow2" 2>/dev/null || true
    echo "[*] NUKE: Everything deleted. Next build starts completely from scratch."
elif [ "${ARG}" = "--deep" ]; then
    echo "[*] Removing Docker cache volumes (package + cargo + external caches)..."
    docker volume rm aios-build-cache 2>/dev/null || true
    docker volume rm aios-cargo-cache 2>/dev/null || true
else
    echo "[*] Keeping caches (use --deep or --nuke to purge)"
fi

echo ""
echo "Done. Run ./start.sh to rebuild and boot."
