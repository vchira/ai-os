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
        match client.get(&sha256_url).send().await {
            Ok(r) => r
                .text()
                .await
                .ok()
                .and_then(|s| s.split_whitespace().next().map(|h| h.to_string()))
                .unwrap_or_default(),
            Err(_) => String::new(),
        }
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
    use futures::StreamExt;
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

    // ========================================================================
    // Additional comprehensive tests
    // ========================================================================

    #[test]
    fn current_version_has_three_components() {
        let ver = Version::parse(CURRENT_VERSION.trim()).unwrap();
        // semver always has major.minor.patch
        // Just verify they are accessible (they always are with semver::Version)
        let _major = ver.major;
        let _minor = ver.minor;
        let _patch = ver.patch;
    }

    #[test]
    fn current_version_not_empty() {
        assert!(!CURRENT_VERSION.trim().is_empty());
    }

    #[test]
    fn current_version_does_not_start_with_v() {
        // The VERSION file should be plain semver, not "v1.0.0"
        assert!(
            !CURRENT_VERSION.trim().starts_with('v'),
            "CURRENT_VERSION should not start with 'v': {}",
            CURRENT_VERSION
        );
    }

    #[test]
    fn version_file_exists_and_readable() {
        let content = std::fs::read_to_string("../../VERSION");
        assert!(content.is_ok(), "VERSION file should exist at ../../VERSION");
        let content = content.unwrap();
        assert!(!content.trim().is_empty(), "VERSION file should not be empty");
    }

    #[test]
    fn version_file_is_valid_semver() {
        let file = std::fs::read_to_string("../../VERSION").unwrap();
        Version::parse(file.trim()).expect("VERSION file must contain valid semver");
    }

    #[test]
    fn release_info_clone() {
        let info = ReleaseInfo {
            version: "2.0.0".to_string(),
            changelog: "Bug fixes".to_string(),
            binary_url: "https://example.com/aios".to_string(),
            sha256: "abc123".to_string(),
        };
        let cloned = info.clone();
        assert_eq!(info.version, cloned.version);
        assert_eq!(info.changelog, cloned.changelog);
        assert_eq!(info.binary_url, cloned.binary_url);
        assert_eq!(info.sha256, cloned.sha256);
    }

    #[test]
    fn release_info_debug() {
        let info = ReleaseInfo {
            version: "1.0.0".to_string(),
            changelog: "Initial".to_string(),
            binary_url: "https://example.com/aios".to_string(),
            sha256: "deadbeef".to_string(),
        };
        let debug = format!("{info:?}");
        assert!(debug.contains("ReleaseInfo"));
        assert!(debug.contains("1.0.0"));
        assert!(debug.contains("deadbeef"));
    }

    #[test]
    fn binary_path_is_usr_bin_aios() {
        assert_eq!(BINARY_PATH, "/usr/bin/aios");
    }

    #[test]
    fn current_version_parses_without_trim_issues() {
        // Ensure there's no hidden whitespace that breaks parsing
        let trimmed = CURRENT_VERSION.trim();
        let padded = format!("  {}  ", trimmed);
        Version::parse(padded.trim()).expect("trimmed version should still parse");
    }

    // ========================================================================
    // Further edge-case tests
    // ========================================================================

    #[test]
    fn version_file_is_single_line() {
        let content = std::fs::read_to_string("../../VERSION").unwrap();
        let non_empty_lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            non_empty_lines.len(),
            1,
            "VERSION file should contain exactly one non-empty line, found {}",
            non_empty_lines.len()
        );
    }

    #[test]
    fn current_version_major_is_reasonable() {
        let v = Version::parse(CURRENT_VERSION.trim()).unwrap();
        // AiOS is currently at version 1.x — major should be 0, 1, or 2
        assert!(
            v.major <= 10,
            "major version {} seems unreasonably large",
            v.major
        );
    }

    #[test]
    fn release_info_with_empty_fields() {
        let info = ReleaseInfo {
            version: String::new(),
            changelog: String::new(),
            binary_url: String::new(),
            sha256: String::new(),
        };
        assert!(info.version.is_empty());
        assert!(info.changelog.is_empty());
        assert!(info.binary_url.is_empty());
        assert!(info.sha256.is_empty());
    }

    #[test]
    fn binary_tmp_path_is_on_same_filesystem() {
        // BINARY_TMP should be in /usr/bin/ for atomic rename
        assert!(
            BINARY_TMP.starts_with("/usr/bin/"),
            "BINARY_TMP should be in /usr/bin/ for atomic rename, got: {}",
            BINARY_TMP
        );
    }

    #[test]
    fn current_version_env_equals_compile_time_constant() {
        // env!("AIOS_VERSION") is set at compile time by build.rs from ../../VERSION.
        // This test just confirms the constant is accessible and non-empty.
        let v = CURRENT_VERSION;
        assert!(!v.is_empty());
        assert!(v.contains('.'), "version should contain dots: {v}");
    }
}
