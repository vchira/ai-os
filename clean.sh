#!/bin/bash
# AiOS — Clean all build artifacts
#
# Usage:
#   ./clean.sh          # clean everything
#
# Cleans:
#   - Rust build artifacts (aios-app-rs/target/)
#   - ISO build directory (distro/build/) — needs Docker since files are root-owned
#   - Docker builder image (forces rebuild with latest Dockerfile)
#   - Python artifacts (.venv, __pycache__, egg-info)
#   - Cargo cache volume (optional, with --deep)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARG="${1:-}"

echo "========================================"
echo "  AiOS — Cleaning all build artifacts"
echo "========================================"
echo ""

# Rust (may be root-owned from Docker build)
if [ -d "${SCRIPT_DIR}/aios-app-rs/target" ]; then
    echo "[*] Cleaning Rust build artifacts..."
    if command -v docker &>/dev/null; then
        docker run --rm -v "${SCRIPT_DIR}:/work" debian:bookworm rm -rf /work/aios-app-rs/target
    else
        rm -rf "${SCRIPT_DIR}/aios-app-rs/target" 2>/dev/null || echo "  WARN: needs sudo or docker to remove target/"
    fi
fi

# ISO build dir (root-owned from Docker)
if [ -d "${SCRIPT_DIR}/distro/build" ]; then
    echo "[*] Cleaning ISO build directory..."
    if command -v docker &>/dev/null; then
        docker run --rm --privileged -v "${SCRIPT_DIR}:/work" debian:bookworm \
            bash -c "umount -lf /work/distro/build/chroot/proc 2>/dev/null; umount -lf /work/distro/build/chroot/sys 2>/dev/null; umount -lf /work/distro/build/chroot/dev/pts 2>/dev/null; rm -rf /work/distro/build"
    else
        rm -rf "${SCRIPT_DIR}/distro/build" 2>/dev/null || echo "  WARN: needs sudo or docker to remove distro/build/"
    fi
fi

# Docker builder image
if docker image inspect aios-builder &>/dev/null 2>&1; then
    echo "[*] Removing Docker builder image..."
    docker rmi aios-builder 2>/dev/null || true
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

# Always remove Docker cache volumes (ensures truly clean builds)
echo "[*] Removing Docker cache volumes..."
docker volume rm aios-build-cache 2>/dev/null || true
docker volume rm aios-cargo-cache 2>/dev/null || true

echo ""
echo "Done. Run ./start.sh to rebuild and boot."
