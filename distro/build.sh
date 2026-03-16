#!/bin/bash
# AiOS Linux Distribution Builder
# Builds inside a Debian Bookworm Docker container.
#
# Prerequisites: docker
#
# Usage:
#   ./build.sh              # incremental build (full ISO)
#   ./build.sh --code-rebuild       # rebuild Rust binary only + repack ISO (~1 min)
#   ./build.sh --clean      # full clean rebuild
#   ./build.sh --debug      # build with debug logging enabled at boot

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/build"
REPO_DIR="${SCRIPT_DIR}/.."
ARG="${1:-}"
IMAGE="aios-builder"
CACHE_VOL="aios-build-cache"
CARGO_CACHE="aios-cargo-cache"

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
docker volume create "${CARGO_CACHE}" &>/dev/null || true

# Clean if requested
if [ "${ARG}" = "--clean" ] && [ -d "${BUILD_DIR}" ]; then
    echo "[*] Cleaning previous build..."
    docker run --rm -v "${REPO_DIR}:/work" "${IMAGE}" rm -rf /work/distro/build
fi

# ─── Always rebuild the Rust binary ──────────────────────────
# This ensures code changes are always picked up.
echo "[*] Building AiOS Rust binary (inside Bookworm container)..."
docker run --rm \
    -v "${REPO_DIR}:/work" \
    -v "${CARGO_CACHE}:/root/.cargo/registry" \
    -w /work/aios-app-rs \
    "${IMAGE}" cargo build --release 2>&1

AIOS_BIN="${REPO_DIR}/aios-app-rs/target/release/aios"
if [ ! -f "${AIOS_BIN}" ]; then
    echo "ERROR: Rust binary not found at ${AIOS_BIN}"
    exit 1
fi
echo "[*] AiOS binary: $(du -h "${AIOS_BIN}" | cut -f1)"

# ─── --code-rebuild: fast path — just replace binary in existing ISO ─
if [ "${ARG}" = "--code-rebuild" ]; then
    echo "[*] Fast rebuild: replacing binary in existing chroot..."

    if [ ! -d "${BUILD_DIR}/chroot" ]; then
        echo "ERROR: No existing chroot. Run ./build.sh first (without --code-rebuild)."
        exit 1
    fi

    # Copy the new binary into the chroot
    docker run --rm --privileged \
        -v "${REPO_DIR}:/work" \
        -v "${CACHE_VOL}:/cache" \
        "${IMAGE}" bash -c "
            cp /work/aios-app-rs/target/release/aios /work/distro/build/chroot/usr/bin/aios
            chmod +x /work/distro/build/chroot/usr/bin/aios
            echo '[*] Binary replaced in chroot'
            cd /work/distro/build
            lb binary 2>&1
        "

    ISO=$(find "${BUILD_DIR}" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)
    if [ -n "${ISO}" ]; then
        echo ""
        echo "========================================"
        echo "  Fast rebuild complete!"
        echo "  ISO: ${ISO}"
        SIZE=$(du -h "${ISO}" | cut -f1)
        echo "  Size: ${SIZE}"
        echo "========================================"
    else
        echo "WARNING: lb binary failed. Falling back to full rebuild..."
        # Fall through to full build below
        ARG="--fallback"
    fi

    if [ "${ARG}" = "--code-rebuild" ]; then
        echo ""
        echo "ISO ready: ${ISO}"
        exit 0
    fi
fi

# ─── Full ISO build ─────────────────────────────────────────
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
