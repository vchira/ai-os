#!/usr/bin/env bash
# push-update.sh — push a new AiOS binary to a running AiOS machine over SSH.
#
# Usage:
#   ./scripts/push-update.sh              # Discover and update (or update assistant.aios.local)
#   ./scripts/push-update.sh jarvis       # Update jarvis.aios.local
#   ./scripts/push-update.sh --all        # Discover and update all machines on the LAN
#
# On first use per host, asks for the SSH password and saves it to ~/.aios-dev/credentials
#
# Dependencies: sshpass, avahi-utils (avahi-browse), openssh-client, cargo

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "${SCRIPT_DIR}")"
BINARY="${PROJECT_DIR}/aios-app-rs/target/release/aios"
AIOS_USER="aios"
CREDENTIALS_FILE="${HOME}/.aios-dev/credentials"
REMOTE_TMP="/tmp/aios-new"
HOSTNAME_ARG="${1:-}"

# --- Dependency check ---

check_deps() {
    local missing=()
    for cmd in sshpass ssh scp cargo avahi-browse; do
        if ! command -v "${cmd}" &>/dev/null; then
            missing+=("${cmd}")
        fi
    done
    if [ ${#missing[@]} -gt 0 ]; then
        echo "ERROR: Missing required tools: ${missing[*]}"
        echo "       Install with: sudo apt install sshpass avahi-utils openssh-client"
        exit 1
    fi
}

# --- Build ---

build_if_needed() {
    if [ ! -f "${BINARY}" ] || find "${PROJECT_DIR}/aios-app-rs" -name "*.rs" -newer "${BINARY}" -print -quit 2>/dev/null | grep -q .; then
        echo "[*] Building release binary..."
        cargo build --manifest-path "${PROJECT_DIR}/aios-app-rs/Cargo.toml" --release
    else
        echo "[*] Binary is up to date, skipping build"
    fi
}

# --- LAN discovery ---

discover_hosts() {
    echo "[*] Scanning LAN for AiOS machines..." >&2
    # avahi-browse -rpt outputs semicolon-delimited lines; filter by mDNS name
    # Matches lines with "Assistant Web Interface" and extracts the hostname field
    avahi-browse -rpt _http._tcp 2>/dev/null \
        | awk -F';' '/Assistant Web Interface/ && $1=="=" {print $7}' \
        | sort -u
}

# --- Credential store ---

get_password() {
    local host="$1"
    mkdir -p "${HOME}/.aios-dev"
    touch "${CREDENTIALS_FILE}"
    chmod 600 "${CREDENTIALS_FILE}"

    local stored
    stored=$(grep -E "^${host}=" "${CREDENTIALS_FILE}" 2>/dev/null | cut -d= -f2- || true)

    if [ -n "${stored}" ]; then
        echo "${stored}"
    else
        read -rsp "Password for ${AIOS_USER}@${host}: " pw
        echo >&2
        echo "${host}=${pw}" >> "${CREDENTIALS_FILE}"
        echo "${pw}"
    fi
}

# --- Push to a single host ---

push_to_host() {
    local host="$1"
    local pw
    pw=$(get_password "${host}")

    echo "[*] Copying binary to ${AIOS_USER}@${host}:${REMOTE_TMP}..."
    SSHPASS="${pw}" sshpass -e scp \
        -o StrictHostKeyChecking=accept-new \
        -o ConnectTimeout=10 \
        "${BINARY}" "${AIOS_USER}@${host}:${REMOTE_TMP}"

    echo "[*] Running aios-update on ${host}..."
    SSHPASS="${pw}" sshpass -e ssh \
        -o StrictHostKeyChecking=accept-new \
        -o ConnectTimeout=10 \
        "${AIOS_USER}@${host}" "sudo aios-update ${REMOTE_TMP}"

    echo "[+] ${host} updated successfully"
}

# --- Main ---

main() {
    check_deps
    build_if_needed

    local targets=()

    if [ "${HOSTNAME_ARG}" = "--all" ]; then
        mapfile -t found < <(discover_hosts)
        if [ ${#found[@]} -eq 0 ]; then
            echo "ERROR: No AiOS machines found on the LAN."
            echo "       Run: ./scripts/push-update.sh <hostname>"
            exit 1
        fi
        echo "Found: ${found[*]}"
        targets=("${found[@]}")

    elif [ -n "${HOSTNAME_ARG}" ]; then
        targets=("${HOSTNAME_ARG}.aios.local")

    else
        # Auto-discover; if multiple found, let user pick
        mapfile -t found < <(discover_hosts)

        if [ ${#found[@]} -eq 0 ]; then
            echo "[*] No machines discovered — falling back to assistant.aios.local"
            targets=("assistant.aios.local")
        elif [ ${#found[@]} -eq 1 ]; then
            targets=("${found[0]}")
        else
            echo "Found AiOS machines:"
            for i in "${!found[@]}"; do
                echo "  $((i+1))) ${found[$i]}"
            done
            echo "  a) all"
            read -rp "Pick a number or 'a': " choice
            if [ "${choice}" = "a" ]; then
                targets=("${found[@]}")
            else
                targets=("${found[$((choice-1))]}")
            fi
        fi
    fi

    local failed=0
    for host in "${targets[@]}"; do
        push_to_host "${host}" || { echo "[!] Failed: ${host}"; failed=$((failed+1)); }
    done

    if [ "${failed}" -gt 0 ]; then
        echo "[!] ${failed} host(s) failed"
        exit 1
    fi

    echo "[✓] Done"
}

main
