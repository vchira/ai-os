//! End-to-end setup flow tests.
//!
//! Simulates the first-boot setup flow by creating an in-memory queue + config,
//! pushing setup messages, simulating user inputs, and verifying the final state.

use aios_core::channel::ChannelKind;
use aios_core::config::commands::{CommandHandler, CommandResult};
use aios_core::config::ConfigManager;
use aios_core::i18n;
use aios_core::queue::{MessageQueue, QueuedMessage};
use aios_core::types::{MessageLevel, Role};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_msg(content: &str, metadata: Option<&str>) -> QueuedMessage {
    QueuedMessage {
        role: Role::System,
        channel: ChannelKind::Desktop,
        source: Some("setup".into()),
        content: Some(content.into()),
        level: Some(MessageLevel::Info),
        metadata: metadata.map(String::from),
        ..Default::default()
    }
}

fn user_input(content: &str) -> QueuedMessage {
    QueuedMessage {
        role: Role::User,
        channel: ChannelKind::Desktop,
        source: Some("user".into()),
        content: Some(content.into()),
        ..Default::default()
    }
}

fn ai_response(content: &str) -> QueuedMessage {
    QueuedMessage {
        role: Role::Assistant,
        channel: ChannelKind::Desktop,
        source: Some("setup-ai".into()),
        content: Some(content.into()),
        ..Default::default()
    }
}

// ===========================================================================
// 1. Complete first-boot setup flow simulation
// ===========================================================================

#[test]
fn first_boot_setup_flow() {
    i18n::init();
    i18n::set_language("en");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let mut cfg = ConfigManager::with_path(config_path).unwrap();
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Phase 1: Welcome message.
    let welcome = i18n::t("setup.welcome.title");
    q.push(setup_msg(&welcome, Some(r#"{"card_type":"setup","step":"welcome"}"#)));

    // Phase 2: Provider selection — user picks Claude.
    q.push(setup_msg(
        &i18n::t("setup.provider.title"),
        Some(r#"{"card_type":"setup","step":"provider"}"#),
    ));
    q.push(user_input("Claude"));
    cfg.set("llm.provider", serde_json::json!("claude")).unwrap();

    // Phase 3: API key entry.
    q.push(setup_msg(
        "Enter your Claude API key",
        Some(r#"{"card_type":"setup","step":"api_key"}"#),
    ));
    q.push(user_input("sk-ant-test-key-12345"));
    cfg.set("llm.claude_api_key", serde_json::json!("sk-ant-test-key-12345"))
        .unwrap();

    // Phase 4: Master password.
    q.push(setup_msg(
        &i18n::t("setup.password.title"),
        Some(r#"{"card_type":"setup","step":"password"}"#),
    ));
    q.push(user_input("[password entered]"));

    // Phase 5: Confirm password.
    q.push(setup_msg(
        &i18n::t("setup.confirm_password.title"),
        Some(r#"{"card_type":"setup","step":"confirm_password"}"#),
    ));
    q.push(user_input("[password confirmed]"));

    // Phase 6: Setup complete.
    let complete_msg = i18n::t("setup.complete.title");
    q.push(setup_msg(
        &complete_msg,
        Some(r#"{"card_type":"setup","step":"complete"}"#),
    ));

    // AI greeting after setup.
    q.push(ai_response("Hello! I'm your AI assistant. How can I help you today?"));

    // -----------------------------------------------------------------------
    // Assertions
    // -----------------------------------------------------------------------

    // Verify config state.
    assert_eq!(cfg.get_str("llm.provider", ""), "claude");
    assert_eq!(cfg.get_str("llm.claude_api_key", ""), "sk-ant-test-key-12345");

    // Verify queue has all setup messages in correct order.
    let all_msgs = q.get_latest(100);
    assert!(all_msgs.len() >= 10, "Expected at least 10 messages, got {}", all_msgs.len());

    // First message should be welcome.
    assert!(
        all_msgs[0].content.as_deref().unwrap().contains("Welcome"),
        "First message should be welcome, got: {:?}",
        all_msgs[0].content
    );

    // Last message should be from AI.
    let last = all_msgs.last().unwrap();
    assert_eq!(last.role, Role::Assistant);
    assert!(last.content.as_deref().unwrap().contains("Hello"));

    // Verify setup metadata is preserved in queue.
    let setup_messages: Vec<&QueuedMessage> = all_msgs
        .iter()
        .filter(|m| m.metadata.is_some())
        .collect();
    assert!(
        setup_messages.len() >= 5,
        "Expected at least 5 messages with metadata, got {}",
        setup_messages.len()
    );

    // Verify setup steps appear in order.
    let steps: Vec<String> = setup_messages
        .iter()
        .filter_map(|m| {
            let meta: serde_json::Value = serde_json::from_str(m.metadata.as_ref()?).ok()?;
            meta.get("step")?.as_str().map(|s| s.to_string())
        })
        .collect();
    assert_eq!(
        steps,
        vec!["welcome", "provider", "api_key", "password", "confirm_password", "complete"]
    );
}

// ===========================================================================
// 2. Setup with backup provider
// ===========================================================================

#[test]
fn setup_with_backup_provider() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let mut cfg = ConfigManager::with_path(config_path).unwrap();
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Primary provider.
    q.push(user_input("Claude"));
    cfg.set("llm.provider", serde_json::json!("claude")).unwrap();
    cfg.set("llm.claude_api_key", serde_json::json!("sk-ant-primary"))
        .unwrap();

    // User wants a backup.
    q.push(user_input("Yes, add backup"));

    // Backup provider.
    q.push(user_input("OpenAI"));
    cfg.set("llm.openai_api_key", serde_json::json!("sk-openai-backup"))
        .unwrap();

    // Verify both keys are stored.
    assert_eq!(cfg.get_str("llm.provider", ""), "claude");
    assert_eq!(cfg.get_str("llm.claude_api_key", ""), "sk-ant-primary");
    assert_eq!(cfg.get_str("llm.openai_api_key", ""), "sk-openai-backup");

    // Queue should have the user inputs.
    let msgs = q.get_latest(50);
    assert_eq!(msgs.len(), 3);
}

// ===========================================================================
// 3. Setup flow messages appear in queue with correct order
// ===========================================================================

#[test]
fn setup_messages_ordering() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    let steps = [
        ("Welcome to AiOS!", "welcome"),
        ("Choose your AI provider", "provider"),
        ("Enter API key", "api_key"),
        ("Create master password", "password"),
        ("Confirm password", "confirm"),
        ("Setup complete!", "complete"),
    ];

    for (content, step) in &steps {
        q.push(QueuedMessage {
            role: Role::System,
            channel: ChannelKind::Desktop,
            content: Some(content.to_string()),
            metadata: Some(format!(r#"{{"step":"{step}"}}"#)),
            level: Some(MessageLevel::Info),
            ..Default::default()
        });
    }

    let msgs = q.get_latest(100);
    assert_eq!(msgs.len(), steps.len());

    // Verify ordering — IDs should be ascending and content matches.
    for i in 1..msgs.len() {
        assert!(
            msgs[i].id > msgs[i - 1].id,
            "Messages should have ascending IDs: {} > {}",
            msgs[i].id,
            msgs[i - 1].id
        );
    }

    for (i, (content, _)) in steps.iter().enumerate() {
        assert_eq!(
            msgs[i].content.as_deref(),
            Some(*content),
            "Message {i} content mismatch"
        );
    }
}

// ===========================================================================
// 4. Setup with config changes verified via commands
// ===========================================================================

#[test]
fn setup_config_verified_via_commands() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let mut cfg = ConfigManager::with_path(config_path).unwrap();

    // Simulate setup: set provider + key.
    cfg.set("llm.provider", serde_json::json!("claude")).unwrap();
    cfg.set("llm.claude_api_key", serde_json::json!("sk-ant-test"))
        .unwrap();

    // After setup, /info should show the provider.
    let mut handler = CommandHandler::new(&mut cfg);
    match handler.execute("/info") {
        CommandResult::Response(text) => {
            assert!(text.contains("claude") || text.contains("Claude"),
                "Info should mention the provider: {}", text);
        }
        other => panic!("Expected Response, got {other:?}"),
    }

    // /key with no args should show usage.
    let mut handler = CommandHandler::new(&mut cfg);
    match handler.execute("/key") {
        CommandResult::Response(text) => {
            assert!(text.contains("Usage") || text.contains("usage"),
                "/key with no args should show usage: {}", text);
        }
        other => panic!("Expected Response for /key, got {other:?}"),
    }
}

// ===========================================================================
// 5. Setup with locale detection
// ===========================================================================

#[test]
fn setup_with_locale_detection() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let mut cfg = ConfigManager::with_path(config_path).unwrap();
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Simulate country detection — Romania.
    let romania = aios_core::installer::locale::country_by_code("RO")
        .expect("RO should exist");

    // Apply locale defaults from detected country.
    cfg.set("system.keyboard_layout", serde_json::json!(romania.keyboard))
        .unwrap();
    cfg.set("system.timezone", serde_json::json!(romania.timezone))
        .unwrap();
    cfg.set("system.language", serde_json::json!(romania.language))
        .unwrap();

    q.push(setup_msg(
        &format!("Detected country: {} ({})", romania.country_name, romania.country_code),
        Some(r#"{"card_type":"setup","step":"locale"}"#),
    ));

    // Verify config.
    assert_eq!(cfg.get_str("system.keyboard_layout", ""), "ro");
    assert_eq!(cfg.get_str("system.timezone", ""), "Europe/Bucharest");
    assert_eq!(cfg.get_str("system.language", ""), "ro");

    // Verify queue message.
    let msgs = q.get_latest(10);
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].content.as_deref().unwrap().contains("Romania"));
}

// ===========================================================================
// 6. Full E2E: queue + config + commands + locale
// ===========================================================================

#[test]
fn full_e2e_setup_then_commands() {
    i18n::init();
    i18n::set_language("en");

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let mut cfg = ConfigManager::with_path(config_path).unwrap();
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Step 1: Setup — provider + key.
    cfg.set("llm.provider", serde_json::json!("claude")).unwrap();
    cfg.set("llm.claude_api_key", serde_json::json!("sk-ant-e2e-test"))
        .unwrap();

    q.push(setup_msg("Setup complete!", Some(r#"{"step":"complete"}"#)));

    // Step 2: User sends first real message.
    q.push(QueuedMessage {
        role: Role::User,
        channel: ChannelKind::Desktop,
        content: Some("What time is it?".into()),
        ..Default::default()
    });

    q.push(QueuedMessage {
        role: Role::Assistant,
        channel: ChannelKind::Desktop,
        content: Some("It's 2026-03-18 10:00 UTC".into()),
        ..Default::default()
    });

    // Step 3: User changes theme via command.
    {
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/theme light") {
            CommandResult::Response(text) => {
                q.push(QueuedMessage {
                    role: Role::System,
                    channel: ChannelKind::Desktop,
                    content: Some(text),
                    level: Some(MessageLevel::Info),
                    ..Default::default()
                });
            }
            other => panic!("Expected Response, got {other:?}"),
        }
    }
    assert_eq!(cfg.get_str("ui.theme", ""), "light");

    // Step 4: User applies locale.
    let de = aios_core::installer::locale::country_by_code("DE").unwrap();
    cfg.set("system.keyboard_layout", serde_json::json!(de.keyboard))
        .unwrap();

    // Step 5: Verify /info shows everything.
    {
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/info") {
            CommandResult::Response(text) => {
                assert!(text.contains("claude") || text.contains("Claude"));
                assert!(text.contains("light"));
                q.push(QueuedMessage {
                    role: Role::System,
                    channel: ChannelKind::Desktop,
                    content: Some(text),
                    level: Some(MessageLevel::Info),
                    ..Default::default()
                });
            }
            other => panic!("Expected Response, got {other:?}"),
        }
    }

    // Verify queue integrity.
    let all = q.get_latest(100);
    assert!(all.len() >= 5);

    // LLM context should exclude system messages.
    let ctx = q.get_llm_context(100);
    assert!(ctx.iter().all(|m| m.role != Role::System));
    assert!(ctx.len() >= 2); // at least user + assistant
}
