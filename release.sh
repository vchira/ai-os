#!/bin/bash
# AiOS Release Script — build, scan, tag, and publish a GitHub Release.
#
# Usage:
#   ./release.sh alpha      # Publish alpha pre-release
#   ./release.sh beta       # Publish beta pre-release
#   ./release.sh release    # Publish stable release
#   ./release.sh            # Publish stable release (same as "release")
#
# Prerequisites:
#   - gh CLI installed and authenticated (gh auth login)
#   - Docker running
#   - Clean git working tree (no uncommitted changes)
#
# Safety:
#   - Channel validation: VERSION must match the requested channel
#   - Secret scan: aborts if API keys, vault files, or credentials are found
#   - These gates are non-negotiable — the script will not proceed if they fail

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSION_FILE="${SCRIPT_DIR}/VERSION"
CHANNEL_ARG="${1:-release}"
TOTAL_STEPS=8

# ─── Helpers ──────────────────────────────────────────────────

step() {
    local num="$1"
    local msg="$2"
    echo "[${num}/${TOTAL_STEPS}] ${msg}"
}

die() {
    echo ""
    echo "ERROR: $1" >&2
    if [ -n "${2:-}" ]; then
        echo "$2" >&2
    fi
    exit 1
}

# ─── [1/8] Read VERSION and validate channel ─────────────────

if [ ! -f "${VERSION_FILE}" ]; then
    die "VERSION file not found at ${VERSION_FILE}"
fi

VERSION=$(cat "${VERSION_FILE}" | tr -d '[:space:]')

if [ -z "${VERSION}" ]; then
    die "VERSION file is empty"
fi

# Detect what channel the VERSION actually is
IS_ALPHA=false
IS_BETA=false
if [[ "${VERSION}" == *"-alpha"* ]]; then
    IS_ALPHA=true
    ACTUAL_CHANNEL="alpha"
elif [[ "${VERSION}" == *"-beta"* ]]; then
    IS_BETA=true
    ACTUAL_CHANNEL="beta"
else
    ACTUAL_CHANNEL="stable"
fi

# Validate channel argument matches VERSION
case "${CHANNEL_ARG}" in
    alpha)
        if [ "${IS_ALPHA}" != "true" ]; then
            die "Channel mismatch: requested 'alpha' but VERSION is '${VERSION}' (${ACTUAL_CHANNEL})." \
                "Edit VERSION to contain an alpha suffix (e.g., X.Y.Z-alpha.N) before releasing."
        fi
        CHANNEL="alpha"
        ;;
    beta)
        if [ "${IS_BETA}" != "true" ]; then
            die "Channel mismatch: requested 'beta' but VERSION is '${VERSION}' (${ACTUAL_CHANNEL})." \
                "Edit VERSION to contain a beta suffix (e.g., X.Y.Z-beta.N) before releasing."
        fi
        CHANNEL="beta"
        ;;
    release|"")
        if [ "${IS_ALPHA}" = "true" ]; then
            die "Cannot publish a stable release: VERSION is '${VERSION}' (alpha)." \
                "Did you mean: ./release.sh alpha"
        fi
        if [ "${IS_BETA}" = "true" ]; then
            die "Cannot publish a stable release: VERSION is '${VERSION}' (beta)." \
                "Did you mean: ./release.sh beta"
        fi
        CHANNEL="stable"
        ;;
    *)
        die "Unknown channel '${CHANNEL_ARG}'. Use: alpha, beta, or release."
        ;;
esac

step 1 "Channel: ${CHANNEL} (VERSION=${VERSION}) ✓"

# ─── [2/8] Prerequisites ─────────────────────────────────────

PREREQ_FAIL=false

# gh CLI
if ! command -v gh &>/dev/null; then
    echo "  ✗ gh CLI not found. Install: https://cli.github.com/"
    PREREQ_FAIL=true
else
    if ! gh auth status &>/dev/null 2>&1; then
        echo "  ✗ gh CLI not authenticated. Run: gh auth login"
        PREREQ_FAIL=true
    fi
fi

# Docker
if ! command -v docker &>/dev/null; then
    echo "  ✗ docker not found. Install Docker to build the ISO."
    PREREQ_FAIL=true
fi

# Clean git tree
GIT_STATUS=$(git -C "${SCRIPT_DIR}" status --porcelain 2>/dev/null || echo "NOT_A_REPO")
if [ "${GIT_STATUS}" = "NOT_A_REPO" ]; then
    echo "  ✗ Not a git repository."
    PREREQ_FAIL=true
elif [ -n "${GIT_STATUS}" ]; then
    echo "  ✗ Git working tree is dirty. Commit or stash changes first."
    echo "    Uncommitted files:"
    echo "${GIT_STATUS}" | sed 's/^/      /'
    PREREQ_FAIL=true
fi

if [ "${PREREQ_FAIL}" = "true" ]; then
    die "Prerequisites check failed. Fix the issues above and retry."
fi

step 2 "Prerequisites ✓"

# ─── [3/8] Secret scan (HARD GATE) ───────────────────────────

SECRETS_FOUND=false
SECRET_LIST=""

add_secret() {
    SECRETS_FOUND=true
    SECRET_LIST="${SECRET_LIST}\n  ✗ $1"
}

# --- File existence checks ---

# .env with real content (not just comments/empty lines)
if [ -f "${SCRIPT_DIR}/.env" ]; then
    # Strip comments and blank lines — if anything remains, it's real content
    REAL_CONTENT=$(grep -v '^\s*#' "${SCRIPT_DIR}/.env" | grep -v '^\s*$' | grep '=' || true)
    if [ -n "${REAL_CONTENT}" ]; then
        add_secret ".env contains real values (not just comments)"
    fi
fi

# autoconfig-*.json (not *.sample.json)
while IFS= read -r -d '' f; do
    basename_f=$(basename "$f")
    if [[ "${basename_f}" == autoconfig-*.json ]] && [[ "${basename_f}" != *.sample.json ]]; then
        add_secret "Non-sample autoconfig file: ${f#${SCRIPT_DIR}/}"
    fi
done < <(find "${SCRIPT_DIR}" -maxdepth 1 -name "autoconfig-*.json" -not -name "*.sample.json" -print0 2>/dev/null)

# autoconfig.json symlink
if [ -L "${SCRIPT_DIR}/autoconfig.json" ] || [ -f "${SCRIPT_DIR}/autoconfig.json" ]; then
    add_secret "autoconfig.json exists (symlink or file)"
fi

# vault.enc files anywhere
while IFS= read -r -d '' f; do
    add_secret "Vault file: ${f#${SCRIPT_DIR}/}"
done < <(find "${SCRIPT_DIR}" -name "vault.enc" -print0 2>/dev/null)

# --- Pattern scan in distro/ and repo root ---

SCAN_DIRS=()
if [ -d "${SCRIPT_DIR}/distro" ]; then
    SCAN_DIRS+=("${SCRIPT_DIR}/distro")
fi
# Also scan repo root for stray secret files
SCAN_DIRS+=("${SCRIPT_DIR}")

for scan_dir in "${SCAN_DIRS[@]}"; do
    # sk-ant- (Anthropic API keys)
    while IFS= read -r match; do
        if [ -n "${match}" ]; then
            add_secret "Anthropic API key pattern (sk-ant-): ${match}"
        fi
    done < <(grep -r --include='*' -l 'sk-ant-' "${scan_dir}" 2>/dev/null | grep -v '\.sample\.json$' | grep -v 'release\.sh$' | grep -v 'RELEASE\.md$' | grep -v '\.md$' | grep -v '\.git/' || true)

    # sk- followed by 20+ alphanumeric chars (OpenAI keys)
    # Exclude files that legitimately mention the pattern (this script, docs, samples)
    while IFS= read -r match; do
        if [ -n "${match}" ]; then
            add_secret "OpenAI API key pattern (sk-...): ${match}"
        fi
    done < <(grep -rP --include='*' 'sk-[A-Za-z0-9]{20,}' "${scan_dir}" 2>/dev/null | grep -v '\.sample\.json$' | grep -v 'release\.sh$' | grep -v 'RELEASE\.md$' | grep -v '\.md$' | grep -v '\.git/' | grep -v '\[A-Za-z0-9\]' || true)

    # master_password with non-empty quoted value
    while IFS= read -r match; do
        if [ -n "${match}" ]; then
            add_secret "master_password with value: ${match}"
        fi
    done < <(grep -rP --include='*' 'master_password\s*[:=]\s*"[^"]+"' "${scan_dir}" 2>/dev/null | grep -v '\.sample\.json$' | grep -v 'release\.sh$' | grep -v 'RELEASE\.md$' | grep -v '\.md$' | grep -v '\.git/' || true)
done

if [ "${SECRETS_FOUND}" = "true" ]; then
    echo ""
    echo "SECRET SCAN FAILED — release aborted."
    echo ""
    echo "The following secrets or sensitive files were found in the working tree:"
    echo -e "${SECRET_LIST}"
    echo ""
    echo "Remove or exclude these before releasing. This gate is non-negotiable."
    exit 1
fi

step 3 "Secret scan passed ✓"

# ─── [4/8] Build ISO ─────────────────────────────────────────

step 4 "Building ISO via distro/build.sh --no-bump ..."

BUILD_SCRIPT="${SCRIPT_DIR}/distro/build.sh"
if [ ! -x "${BUILD_SCRIPT}" ]; then
    die "Build script not found or not executable: ${BUILD_SCRIPT}"
fi

"${BUILD_SCRIPT}" --no-bump

# Find the produced ISO
BUILD_DIR="${SCRIPT_DIR}/distro/build"
ISO_SRC=$(find "${BUILD_DIR}" -maxdepth 1 -name "*.iso" -type f 2>/dev/null | head -1)
if [ -z "${ISO_SRC}" ]; then
    die "Build completed but no ISO file found in ${BUILD_DIR}/"
fi

step 4 "Build complete ✓"

# ─── [5/8] Rename ISO + generate checksum ────────────────────

ISO_NAME="aios-${VERSION}-amd64.iso"
ISO_DEST="${BUILD_DIR}/${ISO_NAME}"
CHECKSUM_FILE="${ISO_DEST}.sha256"

if [ "${ISO_SRC}" != "${ISO_DEST}" ]; then
    mv "${ISO_SRC}" "${ISO_DEST}"
fi

# Generate SHA256 checksum (filename only, not full path)
(cd "${BUILD_DIR}" && sha256sum "${ISO_NAME}" > "${ISO_NAME}.sha256")

ISO_SIZE=$(du -h "${ISO_DEST}" | cut -f1)
step 5 "ISO ready: ${ISO_NAME} (${ISO_SIZE}) ✓"

# ─── [6/8] Create git tag ────────────────────────────────────

TAG="v${VERSION}"

if git -C "${SCRIPT_DIR}" tag -l "${TAG}" | grep -q "^${TAG}$"; then
    die "Tag '${TAG}' already exists. Delete it first or bump the version."
fi

git -C "${SCRIPT_DIR}" tag -a "${TAG}" -m "AiOS ${VERSION}"

step 6 "Git tag: ${TAG} ✓"

# ─── [7/8] Push tag + create GitHub Release ───────────────────

git -C "${SCRIPT_DIR}" push origin "${TAG}"

PRERELEASE_FLAG=""
if [ "${CHANNEL}" = "alpha" ] || [ "${CHANNEL}" = "beta" ]; then
    PRERELEASE_FLAG="--prerelease"
fi

gh release create "${TAG}" \
    --title "AiOS ${VERSION}" \
    --generate-notes \
    ${PRERELEASE_FLAG} \
    "${ISO_DEST}" \
    "${CHECKSUM_FILE}"

step 7 "GitHub Release published ✓"

# ─── [8/8] Post-release (stable only): bump to next alpha ────

if [ "${CHANNEL}" = "stable" ]; then
    # Parse version to bump patch and set alpha.1
    V_BASE="${VERSION%%-*}"
    IFS='.' read -r V_MAJOR V_MINOR V_PATCH <<< "${V_BASE}"
    V_PATCH=$((V_PATCH + 1))
    NEXT_VERSION="${V_MAJOR}.${V_MINOR}.${V_PATCH}-alpha.1"

    echo "${NEXT_VERSION}" > "${VERSION_FILE}"
    git -C "${SCRIPT_DIR}" add VERSION
    git -C "${SCRIPT_DIR}" commit -m "chore: bump VERSION to ${NEXT_VERSION} after ${VERSION} release"
    git -C "${SCRIPT_DIR}" push origin HEAD

    step 8 "Version bumped to ${NEXT_VERSION} ✓"
else
    step 8 "Post-release: skipped (${CHANNEL} — no version bump needed) ✓"
fi

# ─── Done ─────────────────────────────────────────────────────

echo ""
echo "========================================"
echo "  AiOS ${VERSION} released successfully!"
echo ""
echo "  Tag:      ${TAG}"
echo "  ISO:      ${ISO_NAME} (${ISO_SIZE})"
echo "  Checksum: ${ISO_NAME}.sha256"
echo "  Channel:  ${CHANNEL}"
echo "========================================"
