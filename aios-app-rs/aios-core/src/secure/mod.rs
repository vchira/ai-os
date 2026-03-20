//! Secure storage, authentication, and permission management.
//!
//! This module provides:
//!
//! * [`vault`] — Encrypted secret storage (AES-256-GCM + Argon2id)
//! * [`auth`] — Pluggable authentication framework
//! * [`permission`] — Per-secret permission grants with caching

pub mod auth;
pub mod permission;
pub mod registry;
pub mod vault;

// Re-export the primary public types.
pub use auth::{AuthManager, AuthResult, Authenticator, PasswordAuthenticator};
pub use permission::{PermissionGrant, PermissionManager, PermissionRequest};
pub use registry::{
    SecureEntry, SecureKind, SecureRegistry, detect_sensitive_ask,
    scan_for_leaked_values, is_credential_key, is_private_key, is_protected_key,
};
pub use vault::{SecretEntry, SecretKind, Vault};
