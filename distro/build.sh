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

# ─── Version management ──────────────────────────────────────
# VERSION file format: MAJOR.MINOR.PATCH
# --bump-major: increment major, reset minor+patch
# --bump-minor: increment minor, reset patch
# Patch is ALWAYS incremented on every build
VERSION_FILE="${REPO_DIR}/VERSION"
if [ ! -f "${VERSION_FILE}" ]; then
    echo "2.0.0" > "${VERSION_FILE}"
fi

CURRENT_VERSION=$(cat "${VERSION_FILE}" | tr -d '[:space:]')
IFS='.' read -r V_MAJOR V_MINOR V_PATCH <<< "${CURRENT_VERSION}"

if [ "${ARG}" = "--bump-major" ]; then
    V_MAJOR=$((V_MAJOR + 1))
    V_MINOR=0
    V_PATCH=0
    echo "[*] Bumped major version to ${V_MAJOR}.${V_MINOR}.${V_PATCH}"
    ARG="--clean"  # major bump implies clean build
elif [ "${ARG}" = "--bump-minor" ]; then
    V_MINOR=$((V_MINOR + 1))
    V_PATCH=0
    echo "[*] Bumped minor version to ${V_MAJOR}.${V_MINOR}.${V_PATCH}"
    ARG="--clean"  # minor bump implies clean build
else
    # Auto-increment patch on every build
    V_PATCH=$((V_PATCH + 1))
fi

AIOS_VERSION="${V_MAJOR}.${V_MINOR}.${V_PATCH}"
echo "${AIOS_VERSION}" > "${VERSION_FILE}"
export AIOS_VERSION
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
# Touch main.rs to invalidate cargo's fingerprint cache — otherwise
# cargo inside Docker may skip recompilation if the target/ dir has
# stale fingerprints from a previous Docker build.
echo "[*] Building AiOS Rust binary (inside Bookworm container)..."
touch "${REPO_DIR}/aios-app-rs/aios-gtk/src/main.rs"
docker run --rm \
    -v "${REPO_DIR}:/work" \
    -v "${CARGO_CACHE}:/root/.cargo/registry" \
    -w /work/aios-app-rs \
    -e "AIOS_VERSION=${AIOS_VERSION}" \
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

    # Replace the binary inside the existing ISO directly.
    # We unsquash the filesystem, replace the binary, re-squash, and rebuild the ISO.
    # This avoids live-build's state machine entirely.
    docker run --rm --privileged \
        -v "${REPO_DIR}:/work" \
        -v "${CACHE_VOL}:/cache" \
        "${IMAGE}" bash -c '
            set -e
            cd /work/distro/build

            # Find the existing ISO
            OLD_ISO=$(find . -maxdepth 1 -name "*.iso" -type f | head -1)
            if [ -z "${OLD_ISO}" ]; then
                echo "ERROR: No existing ISO to patch."
                exit 1
            fi

            echo "[*] Patching binary in existing ISO: ${OLD_ISO}"

            # Mount the ISO to extract the squashfs
            mkdir -p /tmp/iso_mount /tmp/iso_repack
            mount -o loop "${OLD_ISO}" /tmp/iso_mount
            cp -a /tmp/iso_mount/. /tmp/iso_repack/
            umount /tmp/iso_mount

            # Unsquash, replace binary, re-squash
            SQFS="/tmp/iso_repack/live/filesystem.squashfs"
            if [ ! -f "${SQFS}" ]; then
                echo "ERROR: squashfs not found in ISO"
                exit 1
            fi

            mkdir -p /tmp/sqfs_root
            unsquashfs -d /tmp/sqfs_root -f "${SQFS}"
            cp /work/aios-app-rs/target/release/aios /tmp/sqfs_root/usr/bin/aios
            chmod +x /tmp/sqfs_root/usr/bin/aios
            echo "[*] Binary replaced in squashfs"

            rm "${SQFS}"
            mksquashfs /tmp/sqfs_root "${SQFS}" -comp xz -Xbcj x86 -b 1M -no-progress
            rm -rf /tmp/sqfs_root
            echo "[*] Squashfs repacked"

            # Rebuild ISO
            rm -f "${OLD_ISO}"
            xorriso -as mkisofs \
                -o "${OLD_ISO}" \
                -isohybrid-mbr /usr/lib/ISOLINUX/isohdpfx.bin \
                -c isolinux/boot.cat \
                -b isolinux/isolinux.bin \
                -no-emul-boot -boot-load-size 4 -boot-info-table \
                /tmp/iso_repack

            rm -rf /tmp/iso_mount /tmp/iso_repack
            echo "[*] ISO rebuilt"
        '

    ISO=$(find "${BUILD_DIR}" -maxdepth 1 -name "*.iso" -type f 2>/dev/null || true)
    if [ -n "${ISO}" ]; then
        SIZE=$(du -h "${ISO}" | cut -f1)
        echo ""
        echo "========================================"
        echo "  Fast rebuild complete!"
        echo "  ISO: ${ISO}"
        echo "  Size: ${SIZE}"
        echo "========================================"
        echo ""
        echo "ISO ready: ${ISO}"
        exit 0
    else
        echo "ERROR: ISO repacking failed."
        exit 1
    fi
fi

# ─── Full ISO build ─────────────────────────────────────────
echo "[*] Building AiOS v${AIOS_VERSION}..."
docker run --rm --privileged \
    -v "${REPO_DIR}:/work" \
    -v "${CACHE_VOL}:/cache" \
    -e "AIOS_VERSION=${AIOS_VERSION}" \
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
