//! Hostname validation utility.
//!
//! Shared validation logic for RFC-compliant hostnames used across the
//! selftest, first-boot wizard, and hostname-conflict resolution UI.

/// Check whether `name` is a valid hostname.
///
/// Rules:
/// - 1 to 63 characters long
/// - ASCII alphanumeric characters and hyphens only
/// - Must not start or end with a hyphen
pub fn is_valid_hostname(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_hostnames() {
        assert!(is_valid_hostname("assistant"));
        assert!(is_valid_hostname("my-pc"));
        assert!(is_valid_hostname("jarvis-2"));
        assert!(is_valid_hostname("a"));
        assert!(is_valid_hostname(&"a".repeat(63)));
    }

    #[test]
    fn invalid_hostnames() {
        assert!(!is_valid_hostname(""));
        assert!(!is_valid_hostname("-bad"));
        assert!(!is_valid_hostname("bad-"));
        assert!(!is_valid_hostname("has spaces"));
        assert!(!is_valid_hostname(&"a".repeat(64)));
        assert!(!is_valid_hostname("hello.world"));
        assert!(!is_valid_hostname("under_score"));
    }

    #[test]
    fn edge_cases() {
        // Single character
        assert!(is_valid_hostname("a"));
        assert!(is_valid_hostname("1"));
        assert!(!is_valid_hostname("-"));
        // Exactly 63 chars (max)
        assert!(is_valid_hostname(&"a".repeat(63)));
        // 64 chars (too long)
        assert!(!is_valid_hostname(&"a".repeat(64)));
        // Unicode rejected
        assert!(!is_valid_hostname("hëllo"));
        assert!(!is_valid_hostname("日本語"));
        // Special chars
        assert!(!is_valid_hostname("host@name"));
        assert!(!is_valid_hostname("host:name"));
        assert!(!is_valid_hostname("host/name"));
        // Numbers only
        assert!(is_valid_hostname("123"));
        // Hyphen in middle
        assert!(is_valid_hostname("a-b-c"));
        assert!(!is_valid_hostname("--double--"));
    }
}
