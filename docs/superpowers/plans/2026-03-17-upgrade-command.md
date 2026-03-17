# /upgrade Command Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `/upgrade` command that checks for a newer AiOS binary on GitHub Releases (or a configurable URL), downloads it, verifies the SHA256 checksum, replaces `/usr/bin/aios`, re-applies `setcap`, and offers a reboot dialog — all without requiring a URL argument from the user.

**Architecture:** A new `aios-core/src/upgrade.rs` module encapsulates version check, download, checksum verification, and binary install as async functions returning `Result`. `CommandResult::Upgrade` is added to the existing enum so the GTK handler in `app.rs` can drive the interactive parts (progress messages, the "Install?" and "Reboot?" confirm dialogs). The current version is compiled in from the `VERSION` file at the project root via `env!("AIOS_VERSION")` set in a `build.rs`; the update URL is stored under `system.update_url` in the config with a sensible GitHub default.

**Tech Stack:** Rust, `reqwest` (already a workspace dependency via `aios-llm`), `sha2` crate (SHA-256), `tokio::fs`, GTK4 / libadwaita confirm dialog, `aios_core::i18n::{t, t_fmt}` (from the i18n plan), `semver` crate for version comparison.

---

### Task 1: Version Constant via build.rs

**Files:**
- Create: `aios-app-rs/aios-core/build.rs`
- No other files changed yet.

- [ ] **Step 1: Create `build.rs` that reads the root `VERSION` file**

  Create `aios-app-rs/aios-core/build.rs`:

  ```rust
  fn main() {
      // Expose the project version as AIOS_VERSION at compile time.
      // Reads ../../VERSION (relative to aios-core/) — the canonical version file.
      let version = std::fs::read_to_string("../../VERSION")
          .unwrap_or_else(|_| "0.0.0".to_string());
      let version = version.trim();
      println!("cargo:rustc-env=AIOS_VERSION={version}");
      // Re-run if the VERSION file changes.
      println!("cargo:rerun-if-changed=../../VERSION");
  }
  ```

- [ ] **Step 2: Verify the env var is available**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-core`

  Confirm compilation succeeds. The constant is accessible anywhere in `aios-core` (and crates that depend on it) as `env!("AIOS_VERSION")`.

---

### Task 2: `system.update_url` Config Default

**Files:**
- Modify: `aios-app-rs/aios-core/src/config/defaults.rs`

- [ ] **Step 1: Add `update_url` under the `system` section**

  In `DEFAULTS_JSON`, inside the `"system"` object, add:

  ```json
  "update_url": "https://api.github.com/repos/aios-dev/aios/releases/latest"
  ```

  The full `"system"` block becomes:

  ```json
  "system": {
      "keyboard_layout": "us",
      "keyboard_variant": "",
      "locale": "en_US.UTF-8",
      "timezone": "",
      "machine_name": "assistant",
      "update_url": "https://api.github.com/repos/aios-dev/aios/releases/latest"
  }
  ```

- [ ] **Step 2: Update the existing `defaults_parse_successfully` test**

  Add the assertion inside the test:

  ```rust
  assert_eq!(v["system"]["update_url"], "https://api.github.com/repos/aios-dev/aios/releases/latest");
  ```

- [ ] **Step 3: Run tests**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test -p aios-core config`

---

### Task 3: `upgrade.rs` Module — Version Check, Download, Install

**Files:**
- Create: `aios-app-rs/aios-core/src/upgrade.rs`
- Modify: `aios-app-rs/aios-core/src/lib.rs` — add `pub mod upgrade;`
- Modify: `aios-app-rs/aios-core/Cargo.toml` — add `reqwest`, `sha2`, `semver`

- [ ] **Step 1: Add dependencies to `aios-core/Cargo.toml`**

  Add inside `[dependencies]`:

  ```toml
  reqwest = { workspace = true }
  sha2 = "0.10"
  semver = "1.0"
  ```

  `reqwest` is already declared in the workspace `Cargo.toml` with `features = ["json", "stream", "blocking"]` — no workspace entry change needed.

- [ ] **Step 2: Create `upgrade.rs`**

  Create `aios-app-rs/aios-core/src/upgrade.rs`:

  ```rust
  //! Self-update logic for AiOS.
  //!
  //! Checks for a newer binary on the configured update URL (GitHub Releases
  //! API by default), downloads it, verifies the SHA-256 checksum, replaces
  //! `/usr/bin/aios`, and re-applies the `CAP_NET_BIND_SERVICE` capability so
  //! the binary can bind to port 80.
  //!
  //! This module is pure async logic — no GTK. The GTK layer in `aios-gtk`
  //! drives the interactive parts (progress display, confirm dialogs).

  use semver::Version;
  use sha2::{Digest, Sha256};

  use crate::error::{AiosError, Result};

  // ---------------------------------------------------------------------------
  // Constants
  // ---------------------------------------------------------------------------

  /// The version compiled into this binary (read from `../../VERSION` at build
  /// time by `build.rs`).
  pub const CURRENT_VERSION: &str = env!("AIOS_VERSION");

  /// The installed binary path on AiOS systems.
  const BINARY_PATH: &str = "/usr/bin/aios";

  /// Temp path used during download (same filesystem as target → atomic rename).
  const BINARY_TMP: &str = "/usr/bin/aios.new";

  // ---------------------------------------------------------------------------
  // Public types
  // ---------------------------------------------------------------------------

  /// Information about an available update fetched from the release server.
  #[derive(Debug, Clone)]
  pub struct ReleaseInfo {
      /// Semantic version string of the available release (e.g. `"2.1.0"`).
      pub version: String,
      /// Human-readable change summary (may be empty if the server omits it).
      pub changelog: String,
      /// Direct URL of the AiOS binary asset.
      pub binary_url: String,
      /// Expected SHA-256 hex digest of the binary (lowercase, 64 chars).
      pub sha256: String,
  }

  // ---------------------------------------------------------------------------
  // Version check
  // ---------------------------------------------------------------------------

  /// Check whether a newer version is available.
  ///
  /// * `update_url` — GitHub Releases API URL or a custom endpoint that returns
  ///   the same JSON schema.
  ///
  /// Returns `Ok(Some(info))` if a newer version is available, `Ok(None)` if
  /// already up to date, or `Err` if the check fails.
  pub async fn check_for_update(update_url: &str) -> Result<Option<ReleaseInfo>> {
      let current = Version::parse(CURRENT_VERSION.trim())
          .map_err(|e| AiosError::Other(format!("Invalid current version: {e}")))?;

      let client = reqwest::Client::builder()
          .user_agent(format!("AiOS/{CURRENT_VERSION}"))
          .timeout(std::time::Duration::from_secs(15))
          .build()
          .map_err(|e| AiosError::Other(format!("HTTP client error: {e}")))?;

      let resp = client
          .get(update_url)
          .header("Accept", "application/vnd.github+json")
          .send()
          .await
          .map_err(|e| AiosError::Other(format!("Update check failed: {e}")))?;

      if !resp.status().is_success() {
          let status = resp.status();
          return Err(AiosError::Other(format!("Update server returned {status}")));
      }

      let json: serde_json::Value = resp
          .json()
          .await
          .map_err(|e| AiosError::Other(format!("Invalid JSON from update server: {e}")))?;

      let tag = json["tag_name"]
          .as_str()
          .unwrap_or("")
          .trim_start_matches('v');

      let latest = Version::parse(tag)
          .map_err(|e| AiosError::Other(format!("Invalid release version '{tag}': {e}")))?;

      if latest <= current {
          return Ok(None);
      }

      // Find the aios binary asset and its companion .sha256 asset.
      let assets = json["assets"].as_array().cloned().unwrap_or_default();
      let binary_url = assets
          .iter()
          .find(|a| a["name"].as_str() == Some("aios"))
          .and_then(|a| a["browser_download_url"].as_str())
          .unwrap_or("")
          .to_string();

      let sha256_url = assets
          .iter()
          .find(|a| a["name"].as_str() == Some("aios.sha256"))
          .and_then(|a| a["browser_download_url"].as_str())
          .unwrap_or("")
          .to_string();

      if binary_url.is_empty() {
          return Err(AiosError::Other(
              "Release has no 'aios' binary asset".to_string(),
          ));
      }

      // Fetch the checksum file.
      let sha256 = if !sha256_url.is_empty() {
          client
              .get(&sha256_url)
              .send()
              .await
              .ok()
              .and_then(|r| r.text().await.ok())
              .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
              .unwrap_or_default()
      } else {
          String::new()
      };

      let changelog = json["body"].as_str().unwrap_or("").to_string();
      // Trim changelog to a reasonable summary (first 3 lines or 300 chars).
      let changelog = changelog
          .lines()
          .take(3)
          .collect::<Vec<_>>()
          .join("\n")
          .chars()
          .take(300)
          .collect();

      Ok(Some(ReleaseInfo {
          version: latest.to_string(),
          changelog,
          binary_url,
          sha256,
      }))
  }

  // ---------------------------------------------------------------------------
  // Download + verify
  // ---------------------------------------------------------------------------

  /// Download the new binary and write it to `BINARY_TMP`.
  ///
  /// `progress_cb` is called with `(bytes_downloaded, total_bytes_or_0)` so
  /// the caller can display progress.
  pub async fn download_binary(
      info: &ReleaseInfo,
      progress_cb: impl Fn(u64, u64) + Send + 'static,
  ) -> Result<()> {
      use tokio::io::AsyncWriteExt;

      let client = reqwest::Client::builder()
          .user_agent(format!("AiOS/{CURRENT_VERSION}"))
          .timeout(std::time::Duration::from_secs(300))
          .build()
          .map_err(|e| AiosError::Other(format!("HTTP client error: {e}")))?;

      let resp = client
          .get(&info.binary_url)
          .send()
          .await
          .map_err(|e| AiosError::Other(format!("Download failed: {e}")))?;

      if !resp.status().is_success() {
          return Err(AiosError::Other(format!(
              "Download server returned {}",
              resp.status()
          )));
      }

      let total = resp.content_length().unwrap_or(0);
      let mut downloaded: u64 = 0;

      let mut file = tokio::fs::File::create(BINARY_TMP)
          .await
          .map_err(|e| AiosError::Other(format!("Cannot write to {BINARY_TMP}: {e}")))?;

      let mut stream = resp.bytes_stream();
      use futures::StreamExt;
      while let Some(chunk) = stream.next().await {
          let chunk = chunk.map_err(|e| AiosError::Other(format!("Download stream error: {e}")))?;
          file.write_all(&chunk)
              .await
              .map_err(|e| AiosError::Other(format!("Write error: {e}")))?;
          downloaded += chunk.len() as u64;
          progress_cb(downloaded, total);
      }
      file.flush()
          .await
          .map_err(|e| AiosError::Other(format!("Flush error: {e}")))?;

      Ok(())
  }

  // ---------------------------------------------------------------------------
  // SHA-256 verification
  // ---------------------------------------------------------------------------

  /// Verify that the downloaded file at `BINARY_TMP` matches the expected SHA-256.
  ///
  /// Returns `Ok(())` if they match, `Err` if they don't or if the file cannot
  /// be read.  If `expected` is empty the check is skipped (returns `Ok(())`).
  pub async fn verify_checksum(expected: &str) -> Result<()> {
      if expected.is_empty() {
          tracing::warn!("No checksum provided — skipping verification");
          return Ok(());
      }

      let data = tokio::fs::read(BINARY_TMP)
          .await
          .map_err(|e| AiosError::Other(format!("Cannot read {BINARY_TMP}: {e}")))?;

      let mut hasher = Sha256::new();
      hasher.update(&data);
      let actual = format!("{:x}", hasher.finalize());

      if actual != expected.to_lowercase() {
          // Remove the corrupted download.
          let _ = tokio::fs::remove_file(BINARY_TMP).await;
          return Err(AiosError::Other(format!(
              "Checksum mismatch!\n  expected: {expected}\n  got:      {actual}"
          )));
      }

      Ok(())
  }

  // ---------------------------------------------------------------------------
  // Install
  // ---------------------------------------------------------------------------

  /// Atomically replace the running binary and re-apply `setcap`.
  ///
  /// Steps:
  /// 1. `chmod +x /usr/bin/aios.new`
  /// 2. `mv /usr/bin/aios.new /usr/bin/aios`  (atomic on same filesystem)
  /// 3. `setcap cap_net_bind_service=+ep /usr/bin/aios`
  ///
  /// Requires that the process runs as root or has `CAP_DAC_OVERRIDE` /
  /// `CAP_FOWNER`.  On an installed AiOS system the binary runs with those
  /// capabilities set at boot by `aios-update`.
  pub async fn install_binary() -> Result<()> {
      // chmod +x
      tokio::process::Command::new("chmod")
          .args(["+x", BINARY_TMP])
          .status()
          .await
          .map_err(|e| AiosError::Other(format!("chmod failed: {e}")))?;

      // Atomic rename (same filesystem: /usr/bin → /usr/bin).
      tokio::fs::rename(BINARY_TMP, BINARY_PATH)
          .await
          .map_err(|e| AiosError::Other(format!("Failed to replace binary: {e}")))?;

      // Re-apply capability so the new binary can bind port 80.
      let setcap_status = tokio::process::Command::new("setcap")
          .args(["cap_net_bind_service=+ep", BINARY_PATH])
          .status()
          .await;

      match setcap_status {
          Ok(s) if s.success() => {}
          Ok(s) => {
              tracing::warn!("setcap exited with status {s} — port 80 binding may fail after reboot");
          }
          Err(e) => {
              tracing::warn!("setcap not available: {e} — port 80 binding may fail after reboot");
          }
      }

      Ok(())
  }

  // ---------------------------------------------------------------------------
  // Reboot
  // ---------------------------------------------------------------------------

  /// Trigger a system reboot via `systemctl reboot`.
  pub async fn reboot() -> Result<()> {
      tokio::process::Command::new("systemctl")
          .arg("reboot")
          .status()
          .await
          .map_err(|e| AiosError::Other(format!("Reboot failed: {e}")))?;
      Ok(())
  }

  // ---------------------------------------------------------------------------
  // Tests
  // ---------------------------------------------------------------------------

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn current_version_is_valid_semver() {
          Version::parse(CURRENT_VERSION.trim()).expect("CURRENT_VERSION must be valid semver");
      }

      #[test]
      fn current_version_matches_version_file() {
          let file = std::fs::read_to_string("../../VERSION").unwrap_or_default();
          assert_eq!(CURRENT_VERSION.trim(), file.trim());
      }
  }
  ```

- [ ] **Step 3: Register the module in `lib.rs`**

  In `aios-app-rs/aios-core/src/lib.rs`, add:

  ```rust
  pub mod upgrade;
  ```

- [ ] **Step 4: Run tests**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test -p aios-core upgrade`

  Both tests should pass (semver valid, version matches file).

---

### Task 4: `CommandResult::Upgrade` and `/upgrade` in `commands.rs`

**Files:**
- Modify: `aios-app-rs/aios-core/src/config/commands.rs`

- [ ] **Step 1: Add `Upgrade` variant to `CommandResult`**

  In the `CommandResult` enum, add after `Update(String)`:

  ```rust
  /// The user typed `/upgrade` — check for and optionally install an update.
  Upgrade,
  ```

- [ ] **Step 2: Register `/upgrade` in `command_list()`**

  Add to the `vec!` in `command_list()`:

  ```rust
  CommandInfo { command: "/upgrade", description: "Check for and install AiOS updates" },
  ```

- [ ] **Step 3: Add the match arm in `execute()`**

  In the `match cmd.as_str()` block, add before the `_` catch-all:

  ```rust
  "/upgrade" => CommandResult::Upgrade,
  ```

- [ ] **Step 4: Add `/upgrade` to the help text in `cmd_help()`**

  Append to the help string (after the `/update` line):

  ```
  /upgrade                    Check for and install AiOS updates
  ```

- [ ] **Step 5: Add i18n keys to `en.json`** (depends on i18n plan Task 1)

  In `aios-app-rs/aios-core/i18n/en.json`, add under the `cmd.*` section:

  ```json
  "cmd.upgrade.description": "Check for and install AiOS updates",
  "cmd.upgrade.checking": "Checking for updates...",
  "cmd.upgrade.up_to_date": "You're running the latest version (v{version})",
  "cmd.upgrade.available": "Update available: v{latest} (current: v{current})",
  "cmd.upgrade.changelog_label": "Changes:",
  "cmd.upgrade.install_prompt": "Install now?",
  "cmd.upgrade.downloading": "Downloading update...",
  "cmd.upgrade.installing": "Installing...",
  "cmd.upgrade.install_ok": "Update installed! Reboot to apply.",
  "cmd.upgrade.reboot_prompt": "Reboot now?",
  "cmd.upgrade.reboot_later": "Reboot skipped. Changes take effect after next restart.",
  "cmd.upgrade.check_failed": "Update check failed: {error}",
  "cmd.upgrade.install_failed": "Install failed: {error}",
  "cmd.upgrade.live_iso": "Upgrade not available on live ISO."
  ```

  > Note: If the i18n plan has not yet been implemented, use plain string literals and add a TODO comment. The step structure assumes i18n is available; replace `t(...)` / `t_fmt(...)` calls with string literals if needed.

- [ ] **Step 6: Add a test for the new variant**

  In the `#[cfg(test)]` section at the bottom of `commands.rs`, add:

  ```rust
  #[test]
  fn upgrade_returns_upgrade_variant() {
      let (_dir, mut cfg) = temp_config();
      let mut handler = CommandHandler::new(&mut cfg);
      assert!(matches!(handler.execute("/upgrade"), CommandResult::Upgrade));
  }
  ```

- [ ] **Step 7: Run tests**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test -p aios-core`

---

### Task 5: GTK Handler in `app.rs`

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs`

The GTK handler drives the interactive upgrade flow. It uses the same `state.rt` Tokio runtime and `chat_view` pattern already used by the `/update` handler.

- [ ] **Step 1: Handle `CommandResult::Upgrade` in `handle_command_result()`**

  Locate the existing `match result` block in `handle_command_result()` (around line 1540). After the `CommandResult::Update(url)` arm, add:

  ```rust
  CommandResult::Upgrade => {
      use aios_core::upgrade;
      use aios_core::types::MessageLevel;

      // Show a "Checking..." message immediately on the GTK thread.
      let msg = if cfg!(feature = "i18n") {
          aios_core::i18n::t("cmd.upgrade.checking")
      } else {
          "Checking for updates...".to_string()
      };
      chat_view.add_message("system", &msg);

      // Pull the update URL from config before dropping the borrow.
      let update_url = s.config.get_str(
          "system.update_url",
          "https://api.github.com/repos/aios-dev/aios/releases/latest",
      );
      let rt = s.rt.clone();
      let chat = chat_view.clone();
      let state_weak = Rc::downgrade(state);
      drop(s);

      rt.spawn(async move {
          // --- Version check ---
          match upgrade::check_for_update(&update_url).await {
              Err(e) => {
                  let msg = format!("Update check failed: {e}");
                  glib::idle_add_once(move || {
                      chat.add_level_message(MessageLevel::Warning, &msg);
                  });
              }
              Ok(None) => {
                  let version = upgrade::CURRENT_VERSION.trim().to_string();
                  let msg = format!("You're running the latest version (v{version})");
                  glib::idle_add_once(move || {
                      chat.add_message("system", &msg);
                  });
              }
              Ok(Some(info)) => {
                  // Show "update available" + ask to install.
                  let current = upgrade::CURRENT_VERSION.trim().to_string();
                  let latest = info.version.clone();
                  let changelog = info.changelog.clone();
                  let info_clone = info.clone();
                  let chat2 = chat.clone();
                  let state_weak2 = state_weak.clone();

                  glib::idle_add_once(move || {
                      let mut lines = vec![
                          format!("Update available: v{latest} (current: v{current})"),
                      ];
                      if !changelog.is_empty() {
                          lines.push(format!("Changes: {changelog}"));
                      }
                      for line in &lines {
                          chat2.add_message("system", line);
                      }

                      // Show "Install update?" confirm dialog.
                      Self::show_upgrade_confirm_dialog(
                          &state_weak2,
                          &chat2,
                          info_clone,
                      );
                  });
              }
          }
      });
      return;
  }
  ```

- [ ] **Step 2: Add `show_upgrade_confirm_dialog()` method**

  Add a private `fn show_upgrade_confirm_dialog(...)` method to the `AiosApp` impl block. It constructs an `adw::AlertDialog` with "Install" and "Skip" responses:

  ```rust
  fn show_upgrade_confirm_dialog(
      state: &std::rc::Weak<RefCell<AiosApp>>,
      chat_view: &ChatView,
      info: aios_core::upgrade::ReleaseInfo,
  ) {
      use adw::prelude::*;

      let dialog = adw::AlertDialog::new(
          Some("Install Update?"),
          Some(&format!("Install AiOS v{}?", info.version)),
      );
      dialog.add_response("skip", "Skip");
      dialog.add_response("install", "Install");
      dialog.set_response_appearance("install", adw::ResponseAppearance::Suggested);
      dialog.set_default_response(Some("skip"));

      let chat = chat_view.clone();
      let state_weak = state.clone();

      dialog.connect_response(None, move |_dlg, response| {
          if response != "install" {
              return;
          }
          Self::run_upgrade(state_weak.clone(), chat.clone(), info.clone());
      });

      // Present over the main window if available.
      if let Some(s) = state.upgrade() {
          if let Ok(s) = s.try_borrow() {
              dialog.present(s.window.as_ref().map(|w| w.upcast_ref::<gtk4::Widget>()));
              return;
          }
      }
      dialog.present(None::<&gtk4::Widget>);
  }
  ```

- [ ] **Step 3: Add `run_upgrade()` method**

  This method drives the download + verify + install sequence, reporting progress via the chat view, then shows the reboot dialog:

  ```rust
  fn run_upgrade(
      state: std::rc::Weak<RefCell<AiosApp>>,
      chat_view: ChatView,
      info: aios_core::upgrade::ReleaseInfo,
  ) {
      use aios_core::upgrade;
      use aios_core::types::MessageLevel;

      let Some(s) = state.upgrade() else { return };
      let rt = s.borrow().rt.clone();
      let chat = chat_view.clone();
      let state_weak = state.clone();

      rt.spawn(async move {
          // --- Download ---
          {
              let c = chat.clone();
              glib::idle_add_once(move || {
                  c.add_message("system", "Downloading update...");
              });
          }

          let c2 = chat.clone();
          let dl_result = upgrade::download_binary(&info, move |downloaded, total| {
              if total > 0 {
                  let pct = downloaded * 100 / total;
                  let c = c2.clone();
                  glib::idle_add_once(move || {
                      c.add_message("system", &format!("Downloading... {pct}%"));
                  });
              }
          })
          .await;

          if let Err(e) = dl_result {
              let msg = format!("Download failed: {e}");
              glib::idle_add_once(move || {
                  chat.add_level_message(MessageLevel::Warning, &msg);
              });
              return;
          }

          // --- Verify checksum ---
          if let Err(e) = upgrade::verify_checksum(&info.sha256).await {
              let msg = format!("Checksum verification failed: {e}");
              let c = chat.clone();
              glib::idle_add_once(move || {
                  c.add_level_message(MessageLevel::Warning, &msg);
              });
              return;
          }

          // --- Install ---
          {
              let c = chat.clone();
              glib::idle_add_once(move || {
                  c.add_message("system", "Installing...");
              });
          }

          if let Err(e) = upgrade::install_binary().await {
              let msg = format!("Install failed: {e}");
              let c = chat.clone();
              glib::idle_add_once(move || {
                  c.add_level_message(MessageLevel::Warning, &msg);
              });
              return;
          }

          // --- Success: offer reboot ---
          let c = chat.clone();
          glib::idle_add_once(move || {
              c.add_message("system", "Update installed! Reboot to apply.");
              Self::show_reboot_dialog(&state_weak, &c);
          });
      });
  }
  ```

- [ ] **Step 4: Add `show_reboot_dialog()` method**

  ```rust
  fn show_reboot_dialog(
      state: &std::rc::Weak<RefCell<AiosApp>>,
      chat_view: &ChatView,
  ) {
      use adw::prelude::*;

      let dialog = adw::AlertDialog::new(
          Some("Reboot Required"),
          Some("Reboot now to apply the update?"),
      );
      dialog.add_response("later", "Later");
      dialog.add_response("reboot", "Reboot");
      dialog.set_response_appearance("reboot", adw::ResponseAppearance::Destructive);
      dialog.set_default_response(Some("reboot"));

      let chat = chat_view.clone();
      let state_weak = state.clone();

      dialog.connect_response(None, move |_dlg, response| {
          if response != "reboot" {
              chat.add_message("system", "Reboot skipped. Changes take effect after next restart.");
              return;
          }
          let Some(s) = state_weak.upgrade() else { return };
          let rt = s.borrow().rt.clone();
          rt.spawn(async {
              let _ = aios_core::upgrade::reboot().await;
          });
      });

      if let Some(s) = state.upgrade() {
          if let Ok(s) = s.try_borrow() {
              dialog.present(s.window.as_ref().map(|w| w.upcast_ref::<gtk4::Widget>()));
              return;
          }
      }
      dialog.present(None::<&gtk4::Widget>);
  }
  ```

- [ ] **Step 5: Verify compilation**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

  Fix any borrow-checker or type errors that arise. The `adw::AlertDialog` API requires `libadwaita >= 1.2`, which is already declared in `Cargo.toml` via `features = ["v1_2"]`.

---

### Task 6: i18n Strings Applied to Upgrade Flow

> **Precondition:** This task requires the i18n framework (Task 1-4 of `2026-03-17-i18n-framework.md`) to be implemented first. If not yet done, skip and keep plain string literals; apply i18n strings as a follow-up.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs`
- Modify: `aios-app-rs/aios-core/src/config/commands.rs`
- Modify: `aios-app-rs/aios-core/i18n/en.json` (done in Task 4 Step 5 above)

- [ ] **Step 1: Replace plain strings in the GTK handler with `t()` / `t_fmt()` calls**

  Add `use aios_core::i18n::{t, t_fmt};` in the relevant scope and replace each literal:

  | Plain literal | i18n call |
  |---|---|
  | `"Checking for updates..."` | `t("cmd.upgrade.checking")` |
  | `"You're running the latest version (v{version})"` | `t_fmt("cmd.upgrade.up_to_date", &[("version", &version)])` |
  | `"Update available: v{latest} (current: v{current})"` | `t_fmt("cmd.upgrade.available", &[("latest", &latest), ("current", &current)])` |
  | `"Changes: ..."` | `t("cmd.upgrade.changelog_label")` |
  | `"Downloading update..."` | `t("cmd.upgrade.downloading")` |
  | `"Installing..."` | `t("cmd.upgrade.installing")` |
  | `"Update installed! Reboot to apply."` | `t("cmd.upgrade.install_ok")` |
  | `"Reboot skipped. ..."` | `t("cmd.upgrade.reboot_later")` |
  | `"Install Update?"` (dialog heading) | `t("cmd.upgrade.install_prompt")` |
  | `"Reboot Required"` (dialog heading) | `t("cmd.upgrade.reboot_prompt")` |

- [ ] **Step 2: Verify compilation**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check -p aios-gtk`

---

### Task 7: Self-Test Scenario

**Files:**
- Modify: `aios-app-rs/aios-core/src/selftest/scenarios.rs`

- [ ] **Step 1: Add upgrade scenarios**

  Add to the scenarios registration block:

  ```rust
  register("upgrade: current version is valid semver", false, |_ctx| {
      let v = semver::Version::parse(aios_core::upgrade::CURRENT_VERSION.trim());
      assert!(v.is_ok(), "CURRENT_VERSION must be valid semver, got {:?}", v);
      TestResult::pass(
          "upgrade: current version is valid semver",
          &format!("Current version: {}", aios_core::upgrade::CURRENT_VERSION.trim()),
      )
  });

  register("upgrade: /upgrade command returns Upgrade variant", false, |_ctx| {
      use aios_core::config::commands::{CommandHandler, CommandResult};
      let dir = tempfile::tempdir().unwrap();
      let path = dir.path().join("config.json");
      let mut cfg = aios_core::config::ConfigManager::with_path(path).unwrap();
      let mut handler = CommandHandler::new(&mut cfg);
      let ok = matches!(handler.execute("/upgrade"), CommandResult::Upgrade);
      assert!(ok);
      TestResult::pass("/upgrade command returns Upgrade variant", "")
  });
  ```

  Add `use semver;` and `use tempfile;` to the scenarios imports (both are already workspace/dev dependencies).

- [ ] **Step 2: Run tests**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test --workspace`

  Expected: all existing tests still pass, new scenarios register cleanly.

---

### Task 8: Final Verification

**Files:** None — verification only.

- [ ] **Step 1: Full workspace test run**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test --workspace`

- [ ] **Step 2: Release build check**

  Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check --release`

- [ ] **Step 3: Verify `/upgrade` appears in `/help` output**

  Write a quick unit test in `commands.rs` tests:

  ```rust
  #[test]
  fn help_contains_upgrade() {
      let (_dir, mut cfg) = temp_config();
      let mut handler = CommandHandler::new(&mut cfg);
      match handler.execute("/help") {
          CommandResult::Response(text) => assert!(text.contains("/upgrade")),
          other => panic!("expected Response, got {other:?}"),
      }
  }
  ```

- [ ] **Step 4: Verify `/upgrade` appears in `command_list()`**

  ```rust
  #[test]
  fn command_list_contains_upgrade() {
      let list = aios_core::config::commands::command_list();
      assert!(list.iter().any(|c| c.command == "/upgrade"));
  }
  ```

- [ ] **Step 5: Commit**
