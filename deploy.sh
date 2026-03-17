#!/bin/bash
# AiOS Deploy — build + push to a remote AiOS machine on the LAN.
#
# Usage:
#   ./deploy.sh                    # build + push to aios.local
#   ./deploy.sh 192.168.1.100      # build + push to specific IP
#   ./deploy.sh --binary-only      # skip build, just push existing binary
#
# Requirements:
#   - Docker (for cross-compiling to Bookworm's GLIBC)
#   - SSH access to the target (default: aios@aios.local, password: aios)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="${SCRIPT_DIR}"
TARGET="${1:-aios.local}"
USER="aios"
IMAGE="aios-builder"
CARGO_CACHE="aios-cargo-cache"

# Check for --binary-only flag
SKIP_BUILD=false
if [ "${TARGET}" = "--binary-only" ]; then
    SKIP_BUILD=true
    TARGET="${2:-aios.local}"
fi

echo "========================================"
echo "  AiOS Deploy to ${TARGET}"
echo "========================================"
echo ""

# ─── Build ───────────────────────────────────────────────────
if [ "${SKIP_BUILD}" = "false" ]; then
    echo "[*] Building AiOS binary (inside Bookworm container)..."

    if ! docker image inspect "${IMAGE}" &>/dev/null; then
        echo "[*] Building builder image (one-time)..."
        docker build -t "${IMAGE}" "${SCRIPT_DIR}/distro"
    fi

    docker volume create "${CARGO_CACHE}" &>/dev/null || true
    touch "${REPO_DIR}/aios-app-rs/aios-gtk/src/main.rs"

    docker run --rm \
        -v "${REPO_DIR}:/work" \
        -v "${CARGO_CACHE}:/root/.cargo/registry" \
        -w /work/aios-app-rs \
        "${IMAGE}" cargo build --release 2>&1

    echo "[*] Binary: $(du -h "${REPO_DIR}/aios-app-rs/target/release/aios" | cut -f1)"
fi

BIN="${REPO_DIR}/aios-app-rs/target/release/aios"
if [ ! -f "${BIN}" ]; then
    echo "ERROR: Binary not found at ${BIN}"
    echo "Run without --binary-only to build first."
    exit 1
fi

# ─── Deploy ──────────────────────────────────────────────────
echo "[*] Pushing binary to ${USER}@${TARGET}..."
scp "${BIN}" "${USER}@${TARGET}:/tmp/aios-new"

echo "[*] Running aios-update on remote machine..."
ssh "${USER}@${TARGET}" "aios-update /tmp/aios-new"

echo ""
echo "========================================"
echo "  Deploy complete!"
echo "  Target: ${TARGET}"
echo "========================================"
