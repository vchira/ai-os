# Remote Upgrade Script Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create `scripts/push-update.sh` — a developer-side script that discovers AiOS machines on the LAN, builds the release binary if needed, copies it over SSH, and runs the existing `aios-update` script on the target.
**Architecture:** Single bash script; credentials persisted per-host in `~/.aios-dev/credentials` (chmod 600); uses `avahi-browse` for LAN discovery and `sshpass` for non-interactive SSH/SCP; delegates the actual binary swap and service restart to the `aios-update` script already baked into the ISO at `/usr/bin/aios-update`.
**Tech Stack:** bash, sshpass, avahi-browse

---

### Task 1: `scripts/push-update.sh`

**Files:**
- Create: `scripts/push-update.sh`

- [ ] **Step 1: Create the `scripts/` directory and script skeleton**

  Create `scripts/push-update.sh` with the shebang, `set -euo pipefail`, usage/help text, and argument parsing:

  ```bash
  #!/usr/bin/env bash
  # push-update.sh — push a new AiOS binary to a running AiOS machine
  #
  # Usage: ./scripts/push-update.sh [hostname]
  #   hostname  short name (default: assistant) → resolves to <hostname>.aios.local
  #
  # Dependencies: sshpass, avahi-browse (avahi-utils), cargo
  set -euo pipefail

  HOSTNAME_ARG="${1:-}"
  AIOS_USER="aios"
  CREDENTIALS_FILE="${HOME}/.aios-dev/credentials"
  BINARY="aios-app-rs/target/release/aios"
  REMOTE_TMP="/tmp/aios-new"
  ```

- [ ] **Step 2: Implement binary freshness check and build step**

  Check if the release binary is missing or older than any source file under `aios-app-rs/src/`. If stale, run `cargo build --release` from the workspace root:

  ```bash
  build_if_needed() {
      if [ ! -f "${BINARY}" ] || find aios-app-rs -name "*.rs" -newer "${BINARY}" -print -quit | grep -q .; then
          echo "[*] Building release binary..."
          cargo build --manifest-path aios-app-rs/Cargo.toml --release
      else
          echo "[*] Binary is up to date, skipping build"
      fi
  }
  ```

- [ ] **Step 3: Implement LAN discovery via avahi-browse**

  When no hostname argument is given, scan for AiOS machines using:

  ```bash
  discover_hosts() {
      echo "[*] Scanning LAN for AiOS machines..."
      # avahi-browse outputs lines like: hostname [address] port txt
      # The AiOS web channel advertises "_http._tcp" with name "Assistant Web Interface"
      avahi-browse -rt _http._tcp 2>/dev/null \
          | awk '/Assistant Web Interface/{found=1} found && /hostname/{print $2; found=0}'
  }
  ```

  If multiple hosts are found, print a numbered list and prompt the user to pick one or `all`. If none are found, exit with an error suggesting the user pass a hostname explicitly.

- [ ] **Step 4: Implement credential store (read + save)**

  Read the stored password for a given `<hostname>.aios.local` from `~/.aios-dev/credentials`. If not present, prompt interactively (read -s), then append to the file and chmod 600:

  ```bash
  get_password() {
      local host="$1"
      mkdir -p "${HOME}/.aios-dev"
      touch "${CREDENTIALS_FILE}"
      chmod 600 "${CREDENTIALS_FILE}"

      local stored
      stored=$(grep -E "^${host}=" "${CREDENTIALS_FILE}" | cut -d= -f2- || true)

      if [ -n "${stored}" ]; then
          echo "${stored}"
      else
          read -rsp "Password for ${AIOS_USER}@${host}: " pw
          echo
          echo "${host}=${pw}" >> "${CREDENTIALS_FILE}"
          echo "${pw}"
      fi
  }
  ```

- [ ] **Step 5: Implement the push function (SCP + SSH)**

  Given a resolved host and password, copy the binary then invoke `aios-update`:

  ```bash
  push_to_host() {
      local host="$1"
      local password="$2"

      echo "[*] Copying binary to ${AIOS_USER}@${host}:${REMOTE_TMP}..."
      sshpass -p "${password}" scp -o StrictHostKeyChecking=no \
          "${BINARY}" "${AIOS_USER}@${host}:${REMOTE_TMP}"

      echo "[*] Running aios-update on ${host}..."
      sshpass -p "${password}" ssh -o StrictHostKeyChecking=no \
          "${AIOS_USER}@${host}" "sudo aios-update ${REMOTE_TMP}"

      echo "[+] Done: ${host} updated successfully"
  }
  ```

- [ ] **Step 6: Wire the main flow together**

  Call the above functions in order:

  ```bash
  main() {
      build_if_needed

      local targets=()
      if [ -n "${HOSTNAME_ARG}" ]; then
          targets+=("${HOSTNAME_ARG}.aios.local")
      else
          mapfile -t found < <(discover_hosts)
          if [ ${#found[@]} -eq 0 ]; then
              echo "ERROR: No AiOS machines found on the LAN."
              echo "       Run: ./scripts/push-update.sh <hostname>"
              exit 1
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

      for host in "${targets[@]}"; do
          pw=$(get_password "${host}")
          push_to_host "${host}" "${pw}"
      done
  }

  main
  ```

- [ ] **Step 7: Make the script executable**

  ```bash
  chmod +x scripts/push-update.sh
  ```

  Verify with a dry run (no AiOS machine needed):

  ```bash
  bash -n scripts/push-update.sh   # syntax check
  ./scripts/push-update.sh --help 2>&1 || true
  ```
