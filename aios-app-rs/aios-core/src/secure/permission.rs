//! Per-secret permission grants with time-based caching.
//!
//! The [`PermissionManager`] tracks which secrets a caller has been granted
//! access to, optionally for a limited duration ("don't ask for 5 minutes").

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::auth::{AuthManager, AuthResult};

/// A request to access a secret.
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// The vault key (e.g. `"claude_api_key"`).
    pub secret_key: String,
    /// Human-readable label (e.g. `"Claude API Key"`).
    pub label: String,
    /// Why the secret is needed (e.g. `"Send a message to Claude"`).
    pub reason: String,
    /// Whether accessing this secret also requires re-authentication.
    pub requires_auth: bool,
}

/// The outcome of a permission check.
#[derive(Debug, Clone)]
pub struct PermissionGrant {
    /// Whether access was allowed.
    pub allowed: bool,
    /// If authentication was required, the result is included here.
    pub auth_result: Option<AuthResult>,
    /// If the user chose "don't ask permission for X minutes".
    pub dont_ask_permission_duration: Option<Duration>,
}

/// Manages per-secret permission grants and caches them by key.
pub struct PermissionManager {
    /// `key` -> instant until which permission is granted.
    permission_cache: HashMap<String, Instant>,
}

impl PermissionManager {
    /// Create a new, empty permission manager.
    pub fn new() -> Self {
        Self {
            permission_cache: HashMap::new(),
        }
    }

    /// Returns `true` if a valid (non-expired) permission grant exists for `key`.
    pub fn is_permitted(&self, key: &str) -> bool {
        self.permission_cache
            .get(key)
            .map(|&until| Instant::now() < until)
            .unwrap_or(false)
    }

    /// Cache a permission grant for `key` lasting `duration`.
    pub fn cache_permission(&mut self, key: &str, duration: Duration) {
        self.permission_cache
            .insert(key.to_owned(), Instant::now() + duration);
    }

    /// Clear all cached permissions.
    pub fn clear_cache(&mut self) {
        self.permission_cache.clear();
    }

    /// Clear the cached permission for a single `key`.
    pub fn clear_permission(&mut self, key: &str) {
        self.permission_cache.remove(key);
    }

    /// Returns `true` if the given `key` does **not** have a cached grant
    /// (i.e. the user needs to be asked for permission).
    pub fn needs_permission(&self, key: &str) -> bool {
        !self.is_permitted(key)
    }

    /// Returns `true` if the [`AuthManager`]'s authentication cache has
    /// expired and the user needs to re-authenticate.
    pub fn needs_auth(&self, auth_manager: &AuthManager) -> bool {
        !auth_manager.is_cached()
    }
}

impl Default for PermissionManager {
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
    use super::super::auth::AuthManager;
    use std::time::Duration;

    #[test]
    fn initially_no_permissions() {
        let pm = PermissionManager::new();
        assert!(!pm.is_permitted("some_key"));
        assert!(pm.needs_permission("some_key"));
    }

    #[test]
    fn cache_permission_and_check() {
        let mut pm = PermissionManager::new();
        pm.cache_permission("claude_api_key", Duration::from_secs(300));

        assert!(pm.is_permitted("claude_api_key"));
        assert!(!pm.needs_permission("claude_api_key"));
        // Other keys are unaffected.
        assert!(!pm.is_permitted("openai_api_key"));
    }

    #[test]
    fn clear_single_permission() {
        let mut pm = PermissionManager::new();
        pm.cache_permission("a", Duration::from_secs(300));
        pm.cache_permission("b", Duration::from_secs(300));

        pm.clear_permission("a");
        assert!(!pm.is_permitted("a"));
        assert!(pm.is_permitted("b"));
    }

    #[test]
    fn clear_all_permissions() {
        let mut pm = PermissionManager::new();
        pm.cache_permission("a", Duration::from_secs(300));
        pm.cache_permission("b", Duration::from_secs(300));

        pm.clear_cache();
        assert!(!pm.is_permitted("a"));
        assert!(!pm.is_permitted("b"));
    }

    #[test]
    fn expired_permission_not_valid() {
        let mut pm = PermissionManager::new();
        // Zero-duration grant expires immediately.
        pm.cache_permission("key", Duration::from_secs(0));
        assert!(!pm.is_permitted("key"));
        assert!(pm.needs_permission("key"));
    }

    #[test]
    fn needs_auth_delegates_to_auth_manager() {
        let pm = PermissionManager::new();
        let mut am = AuthManager::new();

        // No cached auth → needs auth.
        assert!(pm.needs_auth(&am));

        am.cache_auth(Duration::from_secs(60));
        assert!(!pm.needs_auth(&am));

        am.clear_cache();
        assert!(pm.needs_auth(&am));
    }
}
