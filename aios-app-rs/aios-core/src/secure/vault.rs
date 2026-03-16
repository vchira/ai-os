//! Encrypted secret storage backed by AES-256-GCM and Argon2id.
//!
//! The [`Vault`] stores secrets as key-value pairs in an encrypted file.
//!
//! File format: `[16 bytes salt][12 bytes nonce][encrypted JSON data]`

use std::collections::HashMap;
use std::path::PathBuf;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::Argon2;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::{AiosError, Result};

/// Size of the Argon2id salt in bytes.
const SALT_LEN: usize = 16;
/// Size of the AES-GCM nonce in bytes.
const NONCE_LEN: usize = 12;
/// Size of the derived master key in bytes.
const KEY_LEN: usize = 32;

/// Classification of stored secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecretKind {
    ApiKey,
    Password,
    Login { username: String },
    PersonalInfo,
    Other,
}

/// A single secret stored in the vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    /// Classification of the secret.
    pub kind: SecretKind,
    /// The secret value itself.
    pub value: String,
    /// Human-readable label, e.g. "Claude API Key".
    pub label: String,
    /// When the entry was created.
    pub created: DateTime<Utc>,
    /// Last time the entry was accessed via [`Vault::get`].
    pub last_accessed: Option<DateTime<Utc>>,
}

/// Encrypted key-value secret store.
///
/// Secrets are encrypted at rest using AES-256-GCM with a key derived from the
/// user's password via Argon2id.
pub struct Vault {
    /// Path to the encrypted vault file (e.g. `~/.aios/vault.enc`).
    path: PathBuf,
    /// Derived master key — `Some` when unlocked, `None` when locked.
    master_key: Option<[u8; KEY_LEN]>,
    /// In-memory secret store, populated after unlock.
    secrets: HashMap<String, SecretEntry>,
}

impl Vault {
    /// Create a new [`Vault`] handle for the given path.
    ///
    /// This does **not** read or create the file — call [`Vault::create`] for
    /// first-time setup or [`Vault::unlock`] to load an existing vault.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            master_key: None,
            secrets: HashMap::new(),
        }
    }

    /// Returns `true` if the vault file exists on disk.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Returns `true` if the vault is currently unlocked (master key loaded).
    pub fn is_unlocked(&self) -> bool {
        self.master_key.is_some()
    }

    // -- First-time setup -------------------------------------------------

    /// Create a new vault file protected by `password`.
    ///
    /// Generates a random salt, derives the master key via Argon2id, and writes
    /// an empty (encrypted) vault to disk.
    pub fn create(&mut self, password: &str) -> Result<()> {
        if self.path.exists() {
            return Err(AiosError::Secure("vault file already exists".into()));
        }

        let salt = random_bytes::<SALT_LEN>();
        let key = derive_key(password, &salt)?;

        self.master_key = Some(key);
        self.secrets.clear();

        self.save_with_salt(&salt)?;

        tracing::info!(path = %self.path.display(), "vault created");
        Ok(())
    }

    // -- Unlock / lock ----------------------------------------------------

    /// Derive the master key from `password` and decrypt the vault contents.
    pub fn unlock(&mut self, password: &str) -> Result<()> {
        let data = std::fs::read(&self.path)
            .map_err(|e| AiosError::Secure(format!("failed to read vault: {e}")))?;

        if data.len() < SALT_LEN + NONCE_LEN {
            return Err(AiosError::Secure("vault file is too short".into()));
        }

        let salt = &data[..SALT_LEN];
        let nonce_bytes = &data[SALT_LEN..SALT_LEN + NONCE_LEN];
        let ciphertext = &data[SALT_LEN + NONCE_LEN..];

        let key = derive_key(password, salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| AiosError::Secure(format!("cipher init failed: {e}")))?;
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| AiosError::Secure("decryption failed — wrong password?".into()))?;

        let secrets: HashMap<String, SecretEntry> = serde_json::from_slice(&plaintext)
            .map_err(|e| AiosError::Secure(format!("corrupt vault data: {e}")))?;

        self.master_key = Some(key);
        self.secrets = secrets;

        tracing::info!(path = %self.path.display(), "vault unlocked");
        Ok(())
    }

    /// Lock the vault: zero the master key and clear secrets from memory.
    pub fn lock(&mut self) {
        if let Some(ref mut key) = self.master_key {
            key.fill(0);
        }
        self.master_key = None;
        self.secrets.clear();
        tracing::info!("vault locked");
    }

    // -- CRUD -------------------------------------------------------------

    /// Retrieve a secret by key, updating its `last_accessed` timestamp.
    pub fn get(&mut self, key: &str) -> Option<&SecretEntry> {
        // Update last_accessed, then return the reference.
        if let Some(entry) = self.secrets.get_mut(key) {
            entry.last_accessed = Some(Utc::now());
        }
        self.secrets.get(key)
    }

    /// Insert or update a secret, then auto-save the vault to disk.
    pub fn set(&mut self, key: &str, entry: SecretEntry) -> Result<()> {
        self.require_unlocked()?;
        self.secrets.insert(key.to_owned(), entry);
        self.save()?;
        Ok(())
    }

    /// Remove a secret by key and auto-save.
    pub fn remove(&mut self, key: &str) -> Result<()> {
        self.require_unlocked()?;
        self.secrets.remove(key);
        self.save()?;
        Ok(())
    }

    /// List all secrets as `(key, kind, label)` tuples.
    pub fn list(&self) -> Vec<(String, SecretKind, String)> {
        self.secrets
            .iter()
            .map(|(k, v)| (k.clone(), v.kind.clone(), v.label.clone()))
            .collect()
    }

    // -- Persistence ------------------------------------------------------

    /// Encrypt the current secrets and write to disk.
    ///
    /// The salt is read back from the existing file header so it remains stable
    /// across saves (only the nonce is regenerated).
    pub fn save(&self) -> Result<()> {
        self.require_unlocked()?;

        // Read the existing salt from the file header.
        let data = std::fs::read(&self.path)
            .map_err(|e| AiosError::Secure(format!("failed to read vault for save: {e}")))?;
        if data.len() < SALT_LEN {
            return Err(AiosError::Secure("vault file too short to read salt".into()));
        }
        let salt = &data[..SALT_LEN];

        self.save_with_salt(salt)
    }

    // -- Internal helpers -------------------------------------------------

    /// Encrypt and write the vault using the given salt.
    fn save_with_salt(&self, salt: &[u8]) -> Result<()> {
        let key = self
            .master_key
            .as_ref()
            .ok_or_else(|| AiosError::Secure("vault is locked".into()))?;

        let plaintext = serde_json::to_vec(&self.secrets)
            .map_err(|e| AiosError::Secure(format!("serialization failed: {e}")))?;

        let nonce_bytes = random_bytes::<NONCE_LEN>();
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| AiosError::Secure(format!("cipher init failed: {e}")))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| AiosError::Secure(format!("encryption failed: {e}")))?;

        // Ensure parent directory exists.
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file_data = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
        file_data.extend_from_slice(salt);
        file_data.extend_from_slice(&nonce_bytes);
        file_data.extend_from_slice(&ciphertext);

        std::fs::write(&self.path, &file_data)
            .map_err(|e| AiosError::Secure(format!("failed to write vault: {e}")))?;

        tracing::debug!(path = %self.path.display(), "vault saved");
        Ok(())
    }

    fn require_unlocked(&self) -> Result<()> {
        if self.master_key.is_none() {
            return Err(AiosError::Secure("vault is locked".into()));
        }
        Ok(())
    }

    /// Verify that `password` derives the same master key that is currently
    /// loaded. Used by [`super::auth::PasswordAuthenticator`].
    pub(crate) fn verify_password(&self, password: &str) -> Result<bool> {
        let data = std::fs::read(&self.path)
            .map_err(|e| AiosError::Secure(format!("failed to read vault: {e}")))?;
        if data.len() < SALT_LEN {
            return Err(AiosError::Secure("vault file too short".into()));
        }
        let salt = &data[..SALT_LEN];
        let candidate = derive_key(password, salt)?;

        match &self.master_key {
            Some(current) => Ok(candidate == *current),
            None => {
                // Vault is locked — attempt a trial decryption.
                if data.len() < SALT_LEN + NONCE_LEN {
                    return Err(AiosError::Secure("vault file too short".into()));
                }
                let nonce_bytes = &data[SALT_LEN..SALT_LEN + NONCE_LEN];
                let ciphertext = &data[SALT_LEN + NONCE_LEN..];

                let cipher = Aes256Gcm::new_from_slice(&candidate)
                    .map_err(|e| AiosError::Secure(format!("cipher init: {e}")))?;
                let nonce = Nonce::from_slice(nonce_bytes);

                Ok(cipher.decrypt(nonce, ciphertext).is_ok())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Free helper functions
// ---------------------------------------------------------------------------

/// Derive a 256-bit key from `password` and `salt` using Argon2id.
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| AiosError::Secure(format!("argon2 key derivation failed: {e}")))?;
    Ok(key)
}

/// Generate `N` cryptographically-secure random bytes.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    rand::rng().fill_bytes(&mut buf);
    buf
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn vault_path(dir: &TempDir) -> PathBuf {
        dir.path().join("vault.enc")
    }

    #[test]
    fn create_and_unlock() {
        let dir = TempDir::new().unwrap();
        let path = vault_path(&dir);

        let mut vault = Vault::new(path.clone());
        assert!(!vault.exists());

        vault.create("hunter2").unwrap();
        assert!(vault.exists());
        assert!(vault.is_unlocked());

        // Lock and re-unlock.
        vault.lock();
        assert!(!vault.is_unlocked());

        vault.unlock("hunter2").unwrap();
        assert!(vault.is_unlocked());
    }

    #[test]
    fn wrong_password_fails() {
        let dir = TempDir::new().unwrap();
        let path = vault_path(&dir);

        let mut vault = Vault::new(path.clone());
        vault.create("correct").unwrap();
        vault.lock();

        let result = vault.unlock("wrong");
        assert!(result.is_err());
    }

    #[test]
    fn set_get_remove() {
        let dir = TempDir::new().unwrap();
        let path = vault_path(&dir);

        let mut vault = Vault::new(path);
        vault.create("pass").unwrap();

        let entry = SecretEntry {
            kind: SecretKind::ApiKey,
            value: "sk-ant-secret".into(),
            label: "Claude API Key".into(),
            created: Utc::now(),
            last_accessed: None,
        };

        vault.set("claude_api_key", entry).unwrap();

        let fetched = vault.get("claude_api_key").unwrap();
        assert_eq!(fetched.value, "sk-ant-secret");
        assert_eq!(fetched.kind, SecretKind::ApiKey);
        assert!(fetched.last_accessed.is_some());

        let listing = vault.list();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].0, "claude_api_key");

        vault.remove("claude_api_key").unwrap();
        assert!(vault.get("claude_api_key").is_none());
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = vault_path(&dir);

        // Create vault and add a secret.
        {
            let mut vault = Vault::new(path.clone());
            vault.create("roundtrip").unwrap();
            vault.set(
                "key1",
                SecretEntry {
                    kind: SecretKind::Password,
                    value: "s3cret".into(),
                    label: "Test Password".into(),
                    created: Utc::now(),
                    last_accessed: None,
                },
            ).unwrap();
            vault.set(
                "key2",
                SecretEntry {
                    kind: SecretKind::Login {
                        username: "alice".into(),
                    },
                    value: "p@ss".into(),
                    label: "Alice Login".into(),
                    created: Utc::now(),
                    last_accessed: None,
                },
            ).unwrap();
        }

        // Open a fresh Vault handle, unlock, verify data survived.
        {
            let mut vault = Vault::new(path);
            vault.unlock("roundtrip").unwrap();
            assert_eq!(vault.list().len(), 2);

            let entry = vault.get("key1").unwrap();
            assert_eq!(entry.value, "s3cret");

            let entry = vault.get("key2").unwrap();
            assert_eq!(entry.label, "Alice Login");
            if let SecretKind::Login { ref username } = entry.kind {
                assert_eq!(username, "alice");
            } else {
                panic!("expected Login kind");
            }
        }
    }

    #[test]
    fn create_twice_fails() {
        let dir = TempDir::new().unwrap();
        let path = vault_path(&dir);

        let mut vault = Vault::new(path);
        vault.create("first").unwrap();
        assert!(vault.create("second").is_err());
    }

    #[test]
    fn operations_while_locked_fail() {
        let dir = TempDir::new().unwrap();
        let path = vault_path(&dir);

        let mut vault = Vault::new(path);
        vault.create("pass").unwrap();
        vault.lock();

        let entry = SecretEntry {
            kind: SecretKind::Other,
            value: "x".into(),
            label: "X".into(),
            created: Utc::now(),
            last_accessed: None,
        };

        assert!(vault.set("k", entry).is_err());
        assert!(vault.remove("k").is_err());
        assert!(vault.save().is_err());
    }

    #[test]
    fn verify_password_while_unlocked() {
        let dir = TempDir::new().unwrap();
        let path = vault_path(&dir);

        let mut vault = Vault::new(path);
        vault.create("mypass").unwrap();

        assert!(vault.verify_password("mypass").unwrap());
        assert!(!vault.verify_password("wrong").unwrap());
    }

    #[test]
    fn verify_password_while_locked() {
        let dir = TempDir::new().unwrap();
        let path = vault_path(&dir);

        let mut vault = Vault::new(path);
        vault.create("mypass").unwrap();
        vault.lock();

        assert!(vault.verify_password("mypass").unwrap());
        assert!(!vault.verify_password("wrong").unwrap());
    }
}
