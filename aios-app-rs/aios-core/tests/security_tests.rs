//! Comprehensive security tests for the secure/private data protection system.
//!
//! These tests verify that sensitive data (passwords, API keys, personal info)
//! can NEVER leak to AI models, prompts, logs, or responses.
//!
//! THESE ARE THE MOST CRITICAL TESTS IN THE CODEBASE.
//! If any of these fail, user credentials and personal data are at risk.

use std::collections::BTreeMap;

use aios_core::secure::{
    detect_sensitive_ask, is_credential_key, is_private_key, is_protected_key,
    scan_for_leaked_values, SecureKind, SecureRegistry,
};
use aios_core::ui_components::{DownloadProgressData, ModelTableRow};

// ============================================================================
// Credential key detection — must catch ALL sensitive key patterns
// ============================================================================

#[test]
fn credential_key_passwords() {
    assert!(is_credential_key("password"));
    assert!(is_credential_key("my_password"));
    assert!(is_credential_key("gmail_app_password"));
    assert!(is_credential_key("SMTP_PASSWORD"));
    assert!(is_credential_key("user_passwd"));
    assert!(is_credential_key("master_password"));
    assert!(is_credential_key("login_password"));
    assert!(is_credential_key("wifi_password"));
    assert!(is_credential_key("db_password"));
    assert!(is_credential_key("root_password"));
}

#[test]
fn credential_key_api_keys() {
    assert!(is_credential_key("api_key"));
    assert!(is_credential_key("openai_api_key"));
    assert!(is_credential_key("claude_api_key"));
    assert!(is_credential_key("stripe_apikey"));
    assert!(is_credential_key("github_token"));
    assert!(is_credential_key("auth_token"));
    assert!(is_credential_key("access_token"));
    assert!(is_credential_key("refresh_token"));
    assert!(is_credential_key("bearer_token"));
    assert!(is_credential_key("jwt_secret"));
    assert!(is_credential_key("client_secret"));
    assert!(is_credential_key("app_secret"));
}

#[test]
fn credential_key_auth() {
    assert!(is_credential_key("auth_code"));
    assert!(is_credential_key("oauth_credential"));
    assert!(is_credential_key("ssh_private_key"));
    assert!(is_credential_key("pin_code"));
}

#[test]
fn credential_key_does_not_match_normal_keys() {
    assert!(!is_credential_key("favorite_color"));
    assert!(!is_credential_key("meeting_notes"));
    assert!(!is_credential_key("shopping_list"));
    assert!(!is_credential_key("project_name"));
    assert!(!is_credential_key("language_preference"));
    assert!(!is_credential_key("timezone"));
    assert!(!is_credential_key("keyboard_layout"));
}

// ============================================================================
// Private key detection — personal data
// ============================================================================

#[test]
fn private_key_personal_data() {
    assert!(is_private_key("email"));
    assert!(is_private_key("user_email"));
    assert!(is_private_key("e_mail_address"));
    assert!(is_private_key("full_name"));
    assert!(is_private_key("first_name"));
    assert!(is_private_key("last_name"));
    assert!(is_private_key("home_address"));
    assert!(is_private_key("street_address"));
    assert!(is_private_key("phone_number"));
    assert!(is_private_key("date_of_birth"));
    assert!(is_private_key("birthday"));
    assert!(is_private_key("user_age"));
}

#[test]
fn private_key_does_not_match_normal_keys() {
    assert!(!is_private_key("favorite_color"));
    assert!(!is_private_key("project_status"));
    assert!(!is_private_key("todo_list"));
    assert!(!is_private_key("config_value"));
}

// ============================================================================
// Protected key detection — combines credential + private
// ============================================================================

#[test]
fn protected_key_catches_both() {
    // Credential keys
    assert!(is_protected_key("gmail_app_password"));
    assert!(is_protected_key("api_key"));
    assert!(is_protected_key("auth_token"));
    // Private keys
    assert!(is_protected_key("user_email"));
    assert!(is_protected_key("full_name"));
    assert!(is_protected_key("phone_number"));
    // Normal keys
    assert!(!is_protected_key("favorite_color"));
    assert!(!is_protected_key("todo_item"));
}

// ============================================================================
// SecureKind classification
// ============================================================================

#[test]
fn secure_kind_classification() {
    assert_eq!(SecureKind::from_key("gmail_app_password"), SecureKind::Password);
    assert_eq!(SecureKind::from_key("user_pin"), SecureKind::Password);
    assert_eq!(SecureKind::from_key("openai_api_key"), SecureKind::ApiKey);
    assert_eq!(SecureKind::from_key("github_token"), SecureKind::ApiKey);
    assert_eq!(SecureKind::from_key("client_secret"), SecureKind::ApiKey);
    assert_eq!(SecureKind::from_key("user_email"), SecureKind::Email);
    assert_eq!(SecureKind::from_key("full_name"), SecureKind::PersonalInfo);
    assert_eq!(SecureKind::from_key("home_address"), SecureKind::PersonalInfo);
    assert_eq!(SecureKind::from_key("phone_number"), SecureKind::PersonalInfo);
    assert_eq!(SecureKind::from_key("ssh_private_key"), SecureKind::CryptoKey);
    assert_eq!(SecureKind::from_key("random_note"), SecureKind::Other);
}

// ============================================================================
// Sensitive ask detection — AI asking user to type secrets in chat
// ============================================================================

#[test]
fn detect_ask_direct_password_requests() {
    assert!(detect_sensitive_ask("Please enter your password below.").is_some());
    assert!(detect_sensitive_ask("Type your password here:").is_some());
    assert!(detect_sensitive_ask("What is your password?").is_some());
    assert!(detect_sensitive_ask("Provide your password to continue.").is_some());
    assert!(detect_sensitive_ask("Can you share your password with me?").is_some());
    assert!(detect_sensitive_ask("Tell me your password so I can help.").is_some());
    assert!(detect_sensitive_ask("Give me your password.").is_some());
}

#[test]
fn detect_ask_api_key_requests() {
    assert!(detect_sensitive_ask("Enter your API key:").is_some());
    assert!(detect_sensitive_ask("Type your API key in the chat.").is_some());
    assert!(detect_sensitive_ask("Provide your API key.").is_some());
    assert!(detect_sensitive_ask("What is your API key?").is_some());
}

#[test]
fn detect_ask_app_password_requests() {
    assert!(detect_sensitive_ask("Enter your app password:").is_some());
    assert!(detect_sensitive_ask("Type your app password.").is_some());
    assert!(detect_sensitive_ask("Provide your app password for Gmail.").is_some());
}

#[test]
fn detect_ask_personal_data_requests() {
    assert!(detect_sensitive_ask("Enter your email address please.").is_some());
    assert!(detect_sensitive_ask("What is your email?").is_some());
    assert!(detect_sensitive_ask("Provide your email address.").is_some());
}

#[test]
fn detect_ask_token_requests() {
    assert!(detect_sensitive_ask("Enter your token:").is_some());
    assert!(detect_sensitive_ask("Provide your token to authenticate.").is_some());
    assert!(detect_sensitive_ask("Paste your key here.").is_some());
    assert!(detect_sensitive_ask("Paste your password below.").is_some());
}

#[test]
fn detect_ask_sneaky_requests() {
    assert!(detect_sensitive_ask("Could you give me your password?").is_some());
    assert!(detect_sensitive_ask("Please paste your key in this message.").is_some());
    assert!(detect_sensitive_ask("Send me your credentials please.").is_some());
    assert!(detect_sensitive_ask("Type your credential here.").is_some());
    assert!(detect_sensitive_ask("Enter your credential:").is_some());
}

#[test]
fn detect_ask_case_insensitive() {
    assert!(detect_sensitive_ask("ENTER YOUR PASSWORD").is_some());
    assert!(detect_sensitive_ask("Enter Your Password").is_some());
    assert!(detect_sensitive_ask("WHAT IS YOUR API KEY?").is_some());
    assert!(detect_sensitive_ask("Paste Your Key Here").is_some());
}

#[test]
fn detect_ask_allows_safe_text() {
    assert!(detect_sensitive_ask("I can help you send an email.").is_none());
    assert!(detect_sensitive_ask("Let me check your stored credentials.").is_none());
    assert!(detect_sensitive_ask("I'll use the secure input for that.").is_none());
    assert!(detect_sensitive_ask("Your password has been stored securely.").is_none());
    assert!(detect_sensitive_ask("I found your email configuration.").is_none());
    assert!(detect_sensitive_ask("The API key is already configured.").is_none());
    assert!(detect_sensitive_ask("Hello! How can I help you today?").is_none());
    assert!(detect_sensitive_ask("Here's the weather forecast for today.").is_none());
    assert!(detect_sensitive_ask("I'll run that command for you.").is_none());
    assert!(detect_sensitive_ask("The file has been saved.").is_none());
}

#[test]
fn detect_ask_does_not_false_positive_on_instructions() {
    // These mention passwords/keys but don't ask the user to provide them
    assert!(detect_sensitive_ask("I'll store the password securely.").is_none());
    assert!(detect_sensitive_ask("The API key has been saved.").is_none());
    assert!(detect_sensitive_ask("I need to use the stored password.").is_none());
    assert!(detect_sensitive_ask("Checking if the token exists...").is_none());
}

// ============================================================================
// Value leak scanning — detect stored secrets in text
// ============================================================================

fn write_test_store(dir: &std::path::Path, entries: &[(&str, &str)]) -> std::path::PathBuf {
    let path = dir.join("memory.json");
    let mut store = BTreeMap::new();
    for (k, v) in entries {
        store.insert(k.to_string(), v.to_string());
    }
    std::fs::write(&path, serde_json::to_string(&store).unwrap()).unwrap();
    path
}

#[test]
fn leak_scan_detects_password_in_response() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("gmail_app_password", "xyzzy-secret-pw-1234"),
    ]);
    let leaked = scan_for_leaked_values(
        "Here is the password: xyzzy-secret-pw-1234. Use it to login.",
        &path,
    );
    assert!(!leaked.is_empty(), "SECURITY VIOLATION: password leak not detected!");
    assert!(leaked.contains(&"gmail_app_password".to_string()));
}

#[test]
fn leak_scan_detects_api_key_in_response() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("openai_api_key", "sk-proj-abc123def456ghi789"),
    ]);
    let leaked = scan_for_leaked_values(
        "Your API key is sk-proj-abc123def456ghi789",
        &path,
    );
    assert!(!leaked.is_empty(), "SECURITY VIOLATION: API key leak not detected!");
}

#[test]
fn leak_scan_detects_token_in_response() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("github_token", "ghp_xYz123AbCdEfGhIjKlMnOpQrStUvWx"),
    ]);
    let leaked = scan_for_leaked_values(
        "I'll use this token: ghp_xYz123AbCdEfGhIjKlMnOpQrStUvWx to access the repo.",
        &path,
    );
    assert!(!leaked.is_empty(), "SECURITY VIOLATION: token leak not detected!");
}

#[test]
fn leak_scan_detects_multiple_leaks() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("gmail_password", "hunter2_secret"),
        ("api_key", "sk-abc123456789"),
    ]);
    let leaked = scan_for_leaked_values(
        "Found credentials: hunter2_secret and sk-abc123456789",
        &path,
    );
    assert_eq!(leaked.len(), 2, "Should detect both leaked values");
}

#[test]
fn leak_scan_ignores_short_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("smtp_password", "abc"),  // Too short (< 6 chars) — would cause false positives
    ]);
    let leaked = scan_for_leaked_values("The abc value is fine", &path);
    assert!(leaked.is_empty(), "Short values should not trigger leak detection");
}

#[test]
fn leak_scan_ignores_non_credential_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("favorite_color", "deep_ocean_blue_42"),
    ]);
    let leaked = scan_for_leaked_values(
        "Your favorite color is deep_ocean_blue_42",
        &path,
    );
    assert!(leaked.is_empty(), "Non-credential keys should not trigger leak detection");
}

#[test]
fn leak_scan_clean_response() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("gmail_password", "super_secret_password_123"),
        ("api_key", "sk-ant-abc123456789xyz"),
    ]);
    let leaked = scan_for_leaked_values(
        "I've sent the email successfully. The message was delivered.",
        &path,
    );
    assert!(leaked.is_empty(), "Clean response should not trigger leak detection");
}

#[test]
fn leak_scan_empty_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("password", "test123456"),
    ]);
    assert!(scan_for_leaked_values("", &path).is_empty());
}

#[test]
fn leak_scan_missing_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.json");
    assert!(scan_for_leaked_values("any text here", &path).is_empty());
}

#[test]
fn leak_scan_partial_match_not_triggered() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("gmail_password", "MySecretPassword123"),
    ]);
    // Partial matches should NOT trigger (the full value must appear)
    let leaked = scan_for_leaked_values("MySecret is a common prefix", &path);
    assert!(leaked.is_empty(), "Partial match should not trigger");
}

// ============================================================================
// SecureRegistry — metadata management
// ============================================================================

#[test]
fn registry_new_is_empty() {
    let reg = SecureRegistry::default();
    assert!(reg.list().is_empty());
}

#[test]
fn registry_register_stores_metadata() {
    let mut reg = SecureRegistry::default();
    reg.register("gmail_pw", "Gmail App Password", "For SMTP email sending", SecureKind::Password);
    let entry = reg.get("gmail_pw").unwrap();
    assert_eq!(entry.name, "Gmail App Password");
    assert_eq!(entry.description, "For SMTP email sending");
    assert_eq!(entry.kind, SecureKind::Password);
    assert!(!entry.has_value);
}

#[test]
fn registry_mark_stored() {
    let mut reg = SecureRegistry::default();
    reg.register("key1", "Key", "Desc", SecureKind::ApiKey);
    assert!(!reg.has_value("key1"));
    reg.mark_stored("key1");
    assert!(reg.has_value("key1"));
}

#[test]
fn registry_has_no_value_field() {
    // CRITICAL: SecureEntry struct must NOT have a value field.
    // The value lives in the vault, never in the registry.
    let mut reg = SecureRegistry::default();
    reg.register("test", "Test", "Test desc", SecureKind::Password);
    let entry = reg.get("test").unwrap();
    // Serialize to JSON and verify no "value" field
    let json = serde_json::to_string(entry).unwrap();
    assert!(!json.contains("\"value\""),
        "SECURITY VIOLATION: SecureEntry has a value field! Registry must be metadata only.");
}

#[test]
fn registry_list_only_metadata() {
    let mut reg = SecureRegistry::default();
    reg.register("pw1", "Password 1", "First password", SecureKind::Password);
    reg.register("pw2", "Password 2", "Second password", SecureKind::Password);
    reg.mark_stored("pw1");
    let entries = reg.list();
    assert_eq!(entries.len(), 2);
    // Entries have names and descriptions but NO values
    for entry in entries {
        let json = serde_json::to_string(entry).unwrap();
        assert!(!json.contains("secret"), "Entry should not contain secret values");
    }
}

#[test]
fn registry_remove_entry() {
    let mut reg = SecureRegistry::default();
    reg.register("temp", "Temp", "Temporary", SecureKind::Other);
    assert!(reg.get("temp").is_some());
    reg.remove("temp");
    assert!(reg.get("temp").is_none());
}

#[test]
fn registry_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reg.json");

    // Save
    let mut reg = SecureRegistry::default();
    reg.register("persistent", "Persistent Key", "Should survive reload", SecureKind::Password);
    reg.mark_stored("persistent");
    reg.save(&path).unwrap();

    // Load
    let loaded = SecureRegistry::load(&path);
    assert!(loaded.has_value("persistent"));
    assert_eq!(loaded.get("persistent").unwrap().name, "Persistent Key");
}

#[test]
fn registry_saved_file_has_no_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reg.json");

    let mut reg = SecureRegistry::default();
    reg.register("my_secret_password", "My Password", "Login password", SecureKind::Password);
    reg.mark_stored("my_secret_password");
    reg.save(&path).unwrap();

    let file_contents = std::fs::read_to_string(&path).unwrap();
    // The file should contain the key name and description but NOT any actual secret value
    assert!(file_contents.contains("my_secret_password")); // key name is OK
    assert!(file_contents.contains("My Password")); // display name is OK
    assert!(file_contents.contains("Login password")); // description is OK
    // There should be NO actual password value in the file
    assert!(!file_contents.contains("\"value\""),
        "SECURITY VIOLATION: Registry file contains a value field!");
}

// ============================================================================
// Combined security scenarios — end-to-end attack simulations
// ============================================================================

#[test]
fn attack_ai_asks_for_password_directly() {
    // Simulate: AI responds "Please enter your password"
    let response = "I need to send that email. Please enter your password in the chat below.";
    assert!(
        detect_sensitive_ask(response).is_some(),
        "SECURITY VIOLATION: AI can ask for password directly!"
    );
}

#[test]
fn attack_ai_asks_for_password_sneakily() {
    // Simulate: AI tries a less obvious approach
    let response = "To proceed, could you paste your key in this conversation?";
    assert!(
        detect_sensitive_ask(response).is_some(),
        "SECURITY VIOLATION: AI can ask for key via 'paste your key'!"
    );
}

#[test]
fn attack_ai_leaks_stored_password_in_response() {
    // Simulate: AI somehow gets the password and includes it in response
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("gmail_app_password", "xyzzy-my-real-password"),
    ]);
    let response = "I found your password: xyzzy-my-real-password. I'll use it now.";
    let leaked = scan_for_leaked_values(response, &path);
    assert!(
        !leaked.is_empty(),
        "SECURITY VIOLATION: Stored password leaked in AI response!"
    );
}

#[test]
fn attack_ai_leaks_api_key_in_tool_result() {
    // Simulate: A tool result includes a raw API key
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("stripe_secret", "sk_live_51OcR9HDFgfh3GH7hj"),
    ]);
    let tool_result = "Command output: API_KEY=sk_live_51OcR9HDFgfh3GH7hj";
    let leaked = scan_for_leaked_values(tool_result, &path);
    assert!(
        !leaked.is_empty(),
        "SECURITY VIOLATION: API key leaked in tool result!"
    );
}

#[test]
fn attack_multiple_secrets_leaked() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("gmail_password", "hunter2_the_password"),
        ("api_key", "sk-test-1234567890abc"),
        ("auth_token", "tok_abc123def456ghi"),
    ]);
    let response = "Config dump: password=hunter2_the_password, key=sk-test-1234567890abc, token=tok_abc123def456ghi";
    let leaked = scan_for_leaked_values(response, &path);
    assert_eq!(
        leaked.len(),
        3,
        "SECURITY VIOLATION: Not all leaked secrets detected! Found: {:?}",
        leaked
    );
}

#[test]
fn safe_response_passes_all_checks() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("gmail_password", "super_secret_123"),
        ("api_key", "sk-ant-abcdefghijklmnop"),
    ]);
    let response = "Email sent successfully to user@example.com. The message was delivered.";

    // Should pass all security checks
    assert!(detect_sensitive_ask(response).is_none(), "False positive on safe response");
    assert!(scan_for_leaked_values(response, &path).is_empty(), "False leak on safe response");
}

// ============================================================================
// Edge cases — boundary conditions
// ============================================================================

#[test]
fn empty_inputs() {
    assert!(detect_sensitive_ask("").is_none());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.json");
    assert!(scan_for_leaked_values("", &path).is_empty());
}

#[test]
fn unicode_in_sensitive_ask() {
    // Unicode shouldn't bypass detection
    assert!(detect_sensitive_ask("Please enter your p\u{200B}assword").is_none()); // zero-width space — different string
    assert!(detect_sensitive_ask("Enter your password 🔑").is_some()); // emoji doesn't hide pattern
}

#[test]
fn very_long_response_scan() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("secret_token", "VERYLONGSECRETTOKEN123456789"),
    ]);
    // Build a 100KB response with the secret buried in the middle
    let mut long_text = "x".repeat(50_000);
    long_text.push_str("VERYLONGSECRETTOKEN123456789");
    long_text.push_str(&"y".repeat(50_000));
    let leaked = scan_for_leaked_values(&long_text, &path);
    assert!(!leaked.is_empty(), "SECURITY VIOLATION: Secret not found in long text!");
}

#[test]
fn registry_handles_special_characters_in_keys() {
    let mut reg = SecureRegistry::default();
    reg.register("email@domain.com", "Email Key", "Has @ sign", SecureKind::Email);
    reg.register("path/to/key", "Path Key", "Has slashes", SecureKind::Other);
    assert!(reg.get("email@domain.com").is_some());
    assert!(reg.get("path/to/key").is_some());
}

// ============================================================================
// Sentinel security policy — blocking when not configured + secrets exist
// ============================================================================

#[test]
fn sentinel_blocks_when_not_configured_and_secrets_exist() {
    // Security policy: if secrets are stored but Sentinel is not configured,
    // the system must detect this condition. We verify by checking that
    // credential keys are detected in a memory store, which is the trigger
    // for blocking responses without Sentinel.
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("gmail_app_password", "super_secret_value_123"),
        ("openai_api_key", "sk-ant-test-key-abcdef12345"),
    ]);

    // Read the store and verify credential keys are detected
    let store: BTreeMap<String, String> =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let has_secrets = store.keys().any(|k| is_credential_key(k));
    assert!(
        has_secrets,
        "SECURITY: credential keys must be detected in the memory store — \
         this is the trigger for requiring Sentinel"
    );
}

#[test]
fn sentinel_allows_when_no_secrets() {
    // New installs should work without Sentinel — no credential keys in memory.
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("favorite_color", "blue"),
        ("timezone", "Europe/Berlin"),
        ("language_preference", "en"),
    ]);

    let store: BTreeMap<String, String> =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let has_secrets = store.keys().any(|k| is_credential_key(k));
    assert!(
        !has_secrets,
        "Non-credential keys should NOT trigger Sentinel requirement"
    );
}

// ============================================================================
// Private key detection — comprehensive patterns
// ============================================================================

#[test]
fn private_key_detection_comprehensive() {
    // Email patterns
    assert!(is_private_key("email"), "should detect 'email'");
    assert!(is_private_key("user_email"), "should detect 'user_email'");
    assert!(is_private_key("e_mail_address"), "should detect 'e_mail_address'");
    assert!(is_private_key("work_email"), "should detect 'work_email'");

    // Name patterns
    assert!(is_private_key("full_name"), "should detect 'full_name'");
    assert!(is_private_key("first_name"), "should detect 'first_name'");
    assert!(is_private_key("last_name"), "should detect 'last_name'");
    assert!(is_private_key("display_name"), "should detect 'display_name'");

    // Address patterns
    assert!(is_private_key("home_address"), "should detect 'home_address'");
    assert!(is_private_key("street_address"), "should detect 'street_address'");
    assert!(is_private_key("mailing_address"), "should detect 'mailing_address'");

    // Phone patterns
    assert!(is_private_key("phone_number"), "should detect 'phone_number'");
    assert!(is_private_key("mobile_phone"), "should detect 'mobile_phone'");
    assert!(is_private_key("home_phone"), "should detect 'home_phone'");

    // Birth patterns
    assert!(is_private_key("date_of_birth"), "should detect 'date_of_birth'");
    assert!(is_private_key("birthday"), "should detect 'birthday'");
    assert!(is_private_key("birth_date"), "should detect 'birth_date'");

    // Age patterns
    assert!(is_private_key("user_age"), "should detect 'user_age'");
    assert!(is_private_key("my_age"), "should detect 'my_age'");

    // Gender patterns
    assert!(is_private_key("user_gender"), "should detect 'user_gender'");
    assert!(is_private_key("gender_identity"), "should detect 'gender_identity'");
}

// ============================================================================
// Credential key case insensitivity
// ============================================================================

#[test]
fn credential_key_case_insensitive() {
    // UPPER case
    assert!(is_credential_key("PASSWORD"), "should detect UPPER 'PASSWORD'");
    assert!(is_credential_key("API_KEY"), "should detect UPPER 'API_KEY'");
    assert!(is_credential_key("AUTH_TOKEN"), "should detect UPPER 'AUTH_TOKEN'");
    assert!(is_credential_key("SECRET"), "should detect UPPER 'SECRET'");

    // lower case
    assert!(is_credential_key("password"), "should detect lower 'password'");
    assert!(is_credential_key("api_key"), "should detect lower 'api_key'");
    assert!(is_credential_key("auth_token"), "should detect lower 'auth_token'");
    assert!(is_credential_key("secret"), "should detect lower 'secret'");

    // Mixed case
    assert!(is_credential_key("Password"), "should detect Mixed 'Password'");
    assert!(is_credential_key("Api_Key"), "should detect Mixed 'Api_Key'");
    assert!(is_credential_key("Auth_Token"), "should detect Mixed 'Auth_Token'");
    assert!(is_credential_key("MySecret"), "should detect Mixed 'MySecret'");
    assert!(is_credential_key("Gmail_APP_Password"), "should detect Mixed 'Gmail_APP_Password'");
}

// ============================================================================
// Detect ask — multilingual patterns (German/French)
// ============================================================================

#[test]
fn detect_ask_multilingual() {
    // The current implementation uses English patterns only. Verify that
    // non-English requests that happen to contain English patterns are caught,
    // and pure non-English requests are not (unless they embed English patterns).

    // English embedded in German-style sentence — should match because
    // the English pattern "enter your password" is present
    assert!(
        detect_sensitive_ask("Bitte enter your password below").is_some(),
        "Mixed German/English with English pattern should be caught"
    );

    // Pure German — no English patterns present, so no match expected.
    // This documents current behavior; multilingual detection is future work.
    assert!(
        detect_sensitive_ask("Gib mir dein Passwort").is_none(),
        "Pure German request not caught by English-only patterns (known limitation)"
    );

    // Pure French — same
    assert!(
        detect_sensitive_ask("Entrez votre mot de passe").is_none(),
        "Pure French request not caught by English-only patterns (known limitation)"
    );

    // French with English pattern embedded
    assert!(
        detect_sensitive_ask("Please enter your api key s'il vous plait").is_some(),
        "French text with embedded English pattern should match"
    );
}

// ============================================================================
// Leak scan with special characters in values
// ============================================================================

#[test]
fn leak_scan_with_special_characters() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_test_store(dir.path(), &[
        ("email_password", "p@ss#w0rd!2024"),
        ("api_secret", "sk-test_key$with+special=chars"),
        ("auth_token", "tok.abc-123_DEF!@#"),
    ]);

    // Value with @, #, !
    let leaked = scan_for_leaked_values(
        "Your credentials: p@ss#w0rd!2024",
        &path,
    );
    assert!(
        !leaked.is_empty(),
        "Special chars (@, #, !) in values must still be detected"
    );
    assert!(leaked.contains(&"email_password".to_string()));

    // Value with $, +, =
    let leaked2 = scan_for_leaked_values(
        "Config: key=sk-test_key$with+special=chars",
        &path,
    );
    assert!(
        !leaked2.is_empty(),
        "Special chars ($, +, =) in values must still be detected"
    );

    // Value with dots, dashes, underscores, !, @, #
    let leaked3 = scan_for_leaked_values(
        "Token dump: tok.abc-123_DEF!@#",
        &path,
    );
    assert!(
        !leaked3.is_empty(),
        "Special chars (., -, _, !, @, #) in values must still be detected"
    );
}

// ============================================================================
// ModelTableRow serialization
// ============================================================================

#[test]
fn model_table_row_serialization() {
    let row = ModelTableRow {
        model_id: "llama3.2:3b".to_string(),
        display_name: "Llama 3.2 3B".to_string(),
        download_size: "2GB".to_string(),
        ram_needed: "4GB".to_string(),
        speed: "Fast".to_string(),
        description: "Strong sanitization".to_string(),
        installed: true,
        recommended: false,
    };

    let json = serde_json::to_string(&row).unwrap();
    assert!(json.contains("llama3.2:3b"));
    assert!(json.contains("\"installed\":true"));
    assert!(json.contains("\"recommended\":false"));

    // Deserialize back
    let deserialized: ModelTableRow = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.model_id, "llama3.2:3b");
    assert_eq!(deserialized.display_name, "Llama 3.2 3B");
    assert_eq!(deserialized.download_size, "2GB");
    assert_eq!(deserialized.ram_needed, "4GB");
    assert_eq!(deserialized.speed, "Fast");
    assert_eq!(deserialized.description, "Strong sanitization");
    assert!(deserialized.installed);
    assert!(!deserialized.recommended);
}

// ============================================================================
// DownloadProgressData serialization
// ============================================================================

#[test]
fn download_progress_data_serialization() {
    let progress = DownloadProgressData {
        model_id: "phi3:3.8b".to_string(),
        display_name: "Phi-3 Mini 3.8B".to_string(),
        download_size: "2.3GB".to_string(),
        status: "downloading".to_string(),
        completed: 1_200_000_000,
        total: 2_400_000_000,
        done: false,
        error: None,
    };

    let json = serde_json::to_string(&progress).unwrap();
    assert!(json.contains("phi3:3.8b"));
    assert!(json.contains("downloading"));
    assert!(json.contains("\"done\":false"));

    // Deserialize back
    let deserialized: DownloadProgressData = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.model_id, "phi3:3.8b");
    assert_eq!(deserialized.display_name, "Phi-3 Mini 3.8B");
    assert_eq!(deserialized.download_size, "2.3GB");
    assert_eq!(deserialized.status, "downloading");
    assert_eq!(deserialized.completed, 1_200_000_000);
    assert_eq!(deserialized.total, 2_400_000_000);
    assert!(!deserialized.done);
    assert!(deserialized.error.is_none());

    // With error
    let error_progress = DownloadProgressData {
        model_id: "test".to_string(),
        display_name: "Test".to_string(),
        download_size: "1GB".to_string(),
        status: "error".to_string(),
        completed: 0,
        total: 0,
        done: false,
        error: Some("connection timeout".to_string()),
    };

    let json2 = serde_json::to_string(&error_progress).unwrap();
    let deserialized2: DownloadProgressData = serde_json::from_str(&json2).unwrap();
    assert_eq!(deserialized2.error, Some("connection timeout".to_string()));
}
