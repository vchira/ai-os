#!/bin/bash
# AiOS Linux Distribution Builder
# Builds inside a Debian Bookworm Docker container.
#
# Prerequisites: docker
#
# Usage:
#   ./build.sh              # incremental build
#   ./build.sh --clean      # full clean rebuild
#   ./build.sh --debug      # build with debug logging enabled at boot
#   DEBUG=1 ./start.sh      # boot with debug logging

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/build"
REPO_DIR="${SCRIPT_DIR}/.."
ARG="${1:-}"
IMAGE="aios-builder"
CACHE_VOL="aios-build-cache"

echo "========================================"
echo "  AiOS Linux Distribution Builder"
echo "  Base: Debian Bookworm (12)"
echo "========================================"
echo ""

if ! command -v docker &>/dev/null; then
    echo "ERROR: docker is required."
    exit 1
fi

# Build the builder image once
if ! docker image inspect "${IMAGE}" &>/dev/null; then
    echo "[*] Building builder image (one-time)..."
    docker build -t "${IMAGE}" "${SCRIPT_DIR}"
fi

docker volume create "${CACHE_VOL}" &>/dev/null || true

# Clean if requested
if [ "${ARG}" = "--clean" ] && [ -d "${BUILD_DIR}" ]; then
    echo "[*] Cleaning previous build..."
    docker run --rm -v "${REPO_DIR}:/work" "${IMAGE}" rm -rf /work/distro/build
fi

# Build the Rust binary on the host (much faster than inside chroot)
AIOS_BIN="${REPO_DIR}/aios-app-rs/target/release/aios"
if [ ! -f "${AIOS_BIN}" ] || [ "${ARG}" = "--clean" ]; then
    echo "[*] Building AiOS Rust binary..."
    cd "${REPO_DIR}/aios-app-rs" && cargo build --release 2>&1
    cd "${SCRIPT_DIR}"
fi

if [ ! -f "${AIOS_BIN}" ]; then
    echo "ERROR: Rust binary not found at ${AIOS_BIN}"
    echo "  Run: cd aios-app-rs && cargo build --release"
    exit 1
fi
echo "[*] AiOS binary: $(du -h "${AIOS_BIN}" | cut -f1)"

# Run the ISO build inside Docker using _inner_build.sh
docker run --rm --privileged \
    -v "${REPO_DIR}:/work" \
    -v "${CACHE_VOL}:/cache" \
    "${IMAGE}" bash /work/distro/_inner_build.sh

# Check result
ISO=$(find "${BUILD_DIR}" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)
if [ -n "${ISO}" ]; then
    echo ""
    echo "ISO ready: ${ISO}"
else
    echo ""
    echo "Build failed. Check distro/build/build.log"
    exit 1
fi
