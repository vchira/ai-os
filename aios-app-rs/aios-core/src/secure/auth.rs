//! Pluggable authentication framework.
//!
//! [`AuthManager`] holds a set of [`Authenticator`] implementations sorted by
//! priority and supports a "don't ask again" cache.

use std::time::{Duration, Instant};

use crate::error::{AiosError, Result};
use super::vault::Vault;

/// Result of an authentication attempt.
#[derive(Debug, Clone)]
pub struct AuthResult {
    /// Whether authentication succeeded.
    pub success: bool,
    /// Name of the method used (e.g. `"password"`, `"voice"`).
    pub method: String,
    /// If the user chose "don't ask for X minutes", the duration is stored here.
    pub dont_ask_duration: Option<Duration>,
}

/// Trait for pluggable authentication methods.
///
/// Implementations must be `Send + Sync` so the [`AuthManager`] can be shared
/// across threads.
pub trait Authenticator: Send + Sync {
    /// Human-readable name (e.g. `"password"`).
    fn name(&self) -> &str;
    /// Whether this authenticator is currently usable.
    fn is_available(&self) -> bool;
    /// Priority — lower values are preferred. Password should be 100 (always
    /// available as a fallback).
    fn priority(&self) -> u32;
    /// Verify the provided credential against the vault.
    fn verify(&self, credential: &str, vault: &Vault) -> Result<bool>;
}

// ---------------------------------------------------------------------------
// Built-in: PasswordAuthenticator
// ---------------------------------------------------------------------------

/// Authenticates by verifying a password against the [`Vault`]'s master key.
pub struct PasswordAuthenticator;

impl Authenticator for PasswordAuthenticator {
    fn name(&self) -> &str {
        "password"
    }

    fn is_available(&self) -> bool {
        true // always available as the last-resort fallback
    }

    fn priority(&self) -> u32 {
        100
    }

    fn verify(&self, credential: &str, vault: &Vault) -> Result<bool> {
        vault.verify_password(credential)
    }
}

// ---------------------------------------------------------------------------
// AuthManager
// ---------------------------------------------------------------------------

/// Manages registered [`Authenticator`]s and an optional "don't ask" cache.
pub struct AuthManager {
    authenticators: Vec<Box<dyn Authenticator>>,
    /// If `Some`, the user authenticated and chose "don't ask" until this time.
    auth_cache_until: Option<Instant>,
}

impl AuthManager {
    /// Create a new manager with [`PasswordAuthenticator`] pre-registered.
    pub fn new() -> Self {
        let mut mgr = Self {
            authenticators: Vec::new(),
            auth_cache_until: None,
        };
        mgr.register(Box::new(PasswordAuthenticator));
        mgr
    }

    /// Register an additional authenticator. The list is kept sorted by
    /// ascending priority.
    pub fn register(&mut self, auth: Box<dyn Authenticator>) {
        self.authenticators.push(auth);
        self.authenticators.sort_by_key(|a| a.priority());
    }

    /// Names of currently available authentication methods, sorted by priority.
    pub fn available_methods(&self) -> Vec<&str> {
        self.authenticators
            .iter()
            .filter(|a| a.is_available())
            .map(|a| a.name())
            .collect()
    }

    /// Returns `true` if a previous authentication is still cached.
    pub fn is_cached(&self) -> bool {
        self.auth_cache_until
            .map(|until| Instant::now() < until)
            .unwrap_or(false)
    }

    /// Cache an authentication grant for `duration`.
    pub fn cache_auth(&mut self, duration: Duration) {
        self.auth_cache_until = Some(Instant::now() + duration);
    }

    /// Clear the cached authentication.
    pub fn clear_cache(&mut self) {
        self.auth_cache_until = None;
    }

    /// Authenticate using the named `method` with the given `credential`.
    ///
    /// Returns an [`AuthResult`] indicating success or failure. On success the
    /// caller can optionally call [`AuthManager::cache_auth`] with the user's
    /// chosen "don't ask" duration.
    pub fn authenticate(
        &self,
        method: &str,
        credential: &str,
        vault: &Vault,
    ) -> Result<AuthResult> {
        let authenticator = self
            .authenticators
            .iter()
            .find(|a| a.name() == method)
            .ok_or_else(|| {
                AiosError::Secure(format!("unknown authentication method: {method}"))
            })?;

        if !authenticator.is_available() {
            return Err(AiosError::Secure(format!(
                "authentication method '{method}' is not available"
            )));
        }

        let success = authenticator.verify(credential, vault)?;

        Ok(AuthResult {
            success,
            method: method.to_owned(),
            dont_ask_duration: None,
        })
    }
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::vault::Vault;
    use crate::error::Result;
    use std::time::Duration;
    use tempfile::TempDir;

    fn setup_vault(dir: &TempDir) -> Vault {
        let path = dir.path().join("vault.enc");
        let mut vault = Vault::new(path);
        vault.create("testpass").unwrap();
        vault
    }

    #[test]
    fn password_auth_success() {
        let dir = TempDir::new().unwrap();
        let vault = setup_vault(&dir);
        let mgr = AuthManager::new();

        let result = mgr.authenticate("password", "testpass", &vault).unwrap();
        assert!(result.success);
        assert_eq!(result.method, "password");
    }

    #[test]
    fn password_auth_failure() {
        let dir = TempDir::new().unwrap();
        let vault = setup_vault(&dir);
        let mgr = AuthManager::new();

        let result = mgr.authenticate("password", "wrong", &vault).unwrap();
        assert!(!result.success);
    }

    #[test]
    fn unknown_method_errors() {
        let dir = TempDir::new().unwrap();
        let vault = setup_vault(&dir);
        let mgr = AuthManager::new();

        assert!(mgr.authenticate("fingerprint", "", &vault).is_err());
    }

    #[test]
    fn available_methods_includes_password() {
        let mgr = AuthManager::new();
        let methods = mgr.available_methods();
        assert!(methods.contains(&"password"));
    }

    #[test]
    fn cache_auth_and_expiry() {
        let mut mgr = AuthManager::new();
        assert!(!mgr.is_cached());

        mgr.cache_auth(Duration::from_secs(60));
        assert!(mgr.is_cached());

        mgr.clear_cache();
        assert!(!mgr.is_cached());
    }

    #[test]
    fn cache_expires_naturally() {
        let mut mgr = AuthManager::new();
        // Cache for zero seconds — should already be expired.
        mgr.cache_auth(Duration::from_secs(0));
        // Instant::now() >= cache_until, so is_cached should be false.
        assert!(!mgr.is_cached());
    }

    /// A dummy authenticator for testing registration and priority sorting.
    struct DummyAuth;

    impl Authenticator for DummyAuth {
        fn name(&self) -> &str {
            "dummy"
        }
        fn is_available(&self) -> bool {
            true
        }
        fn priority(&self) -> u32 {
            10 // higher priority than password (100)
        }
        fn verify(&self, credential: &str, _vault: &Vault) -> Result<bool> {
            Ok(credential == "magic")
        }
    }

    #[test]
    fn register_custom_authenticator() {
        let dir = TempDir::new().unwrap();
        let vault = setup_vault(&dir);

        let mut mgr = AuthManager::new();
        mgr.register(Box::new(DummyAuth));

        let methods = mgr.available_methods();
        // Dummy has priority 10, should come before password (100).
        assert_eq!(methods[0], "dummy");
        assert_eq!(methods[1], "password");

        let result = mgr.authenticate("dummy", "magic", &vault).unwrap();
        assert!(result.success);

        let result = mgr.authenticate("dummy", "nope", &vault).unwrap();
        assert!(!result.success);
    }
}
