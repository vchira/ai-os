//! Integration tests for aios-core — cross-module interactions.
//!
//! These tests exercise the boundaries between modules:
//! Queue + Channel, Queue + Config, Queue + i18n, Queue + Types,
//! queue lifecycle, schema migration, and installer locale + config.

use chrono::Utc;

use aios_core::channel::ChannelKind;
use aios_core::config::ConfigManager;
use aios_core::i18n;
use aios_core::installer::locale;
use aios_core::queue::{MessageQueue, QueuedMessage};
use aios_core::types::{Message, MessageLevel, Role};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn user_msg(content: &str, channel: ChannelKind) -> QueuedMessage {
    QueuedMessage {
        role: Role::User,
        channel,
        source: Some("user".into()),
        content: Some(content.into()),
        ..Default::default()
    }
}

fn assistant_msg(content: &str, channel: ChannelKind) -> QueuedMessage {
    QueuedMessage {
        role: Role::Assistant,
        channel,
        source: Some("claude-sonnet-4".into()),
        content: Some(content.into()),
        ..Default::default()
    }
}

fn system_msg(content: &str) -> QueuedMessage {
    QueuedMessage {
        role: Role::System,
        channel: ChannelKind::System,
        source: Some("system".into()),
        content: Some(content.into()),
        level: Some(MessageLevel::Info),
        ..Default::default()
    }
}

// ===========================================================================
// 1. Queue + Channel: push from different channels, verify filtering
// ===========================================================================

#[test]
fn queue_channel_filtering() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Push messages from four different channels.
    q.push(user_msg("Hello from desktop", ChannelKind::Desktop));
    q.push(user_msg("Hello from web", ChannelKind::Web));
    q.push(user_msg("Hello from signal", ChannelKind::Signal));
    q.push(user_msg("Hello from voice", ChannelKind::Voice));

    // Filter by channel.
    let desktop = q.get_from_channel("desktop", 50);
    assert_eq!(desktop.len(), 1);
    assert_eq!(desktop[0].content.as_deref(), Some("Hello from desktop"));
    assert_eq!(desktop[0].channel, ChannelKind::Desktop);

    let web = q.get_from_channel("web", 50);
    assert_eq!(web.len(), 1);
    assert_eq!(web[0].content.as_deref(), Some("Hello from web"));
    assert_eq!(web[0].channel, ChannelKind::Web);

    let signal = q.get_from_channel("signal", 50);
    assert_eq!(signal.len(), 1);
    assert_eq!(signal[0].content.as_deref(), Some("Hello from signal"));

    let voice = q.get_from_channel("voice", 50);
    assert_eq!(voice.len(), 1);
    assert_eq!(voice[0].content.as_deref(), Some("Hello from voice"));

    // get_latest should return all four.
    let all = q.get_latest(50);
    assert_eq!(all.len(), 4);
}

#[test]
fn queue_channel_mixed_roles_filtering() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    q.push(user_msg("user desktop", ChannelKind::Desktop));
    q.push(assistant_msg("ai desktop", ChannelKind::Desktop));
    q.push(user_msg("user web", ChannelKind::Web));
    q.push(assistant_msg("ai web", ChannelKind::Web));

    let desktop = q.get_from_channel("desktop", 50);
    assert_eq!(desktop.len(), 2);
    assert_eq!(desktop[0].role, Role::User);
    assert_eq!(desktop[1].role, Role::Assistant);

    let web = q.get_from_channel("web", 50);
    assert_eq!(web.len(), 2);
    assert_eq!(web[0].role, Role::User);
    assert_eq!(web[1].role, Role::Assistant);
}

// ===========================================================================
// 2. Queue + Config: create queue, push messages, clear, verify
// ===========================================================================

#[test]
fn queue_config_interaction() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    // Create config and set a value.
    let mut cfg = ConfigManager::with_path(config_path.clone()).unwrap();
    cfg.set("llm.provider", serde_json::json!("openai")).unwrap();
    assert_eq!(cfg.get_str("llm.provider", ""), "openai");

    // Create an in-memory queue and push messages referencing the config.
    let mut q = MessageQueue::open_in_memory().unwrap();
    let provider = cfg.get_str("llm.provider", "claude");
    q.push(QueuedMessage {
        role: Role::System,
        channel: ChannelKind::System,
        content: Some(format!("Active provider: {provider}")),
        level: Some(MessageLevel::Info),
        ..Default::default()
    });

    let msgs = q.get_latest(10);
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].content.as_deref().unwrap().contains("openai"));

    // Clear and verify.
    q.clear("system");
    let after_clear = q.get_after_clear_marker("system", 50);
    assert!(after_clear.is_empty());
}

// ===========================================================================
// 3. Queue + i18n: push messages with translated content
// ===========================================================================

#[test]
fn queue_i18n_translated_content() {
    i18n::init();
    i18n::set_language("en");

    let mut q = MessageQueue::open_in_memory().unwrap();

    // Push a message with i18n-translated content.
    let welcome = i18n::t("setup.welcome.title");
    q.push(QueuedMessage {
        role: Role::System,
        channel: ChannelKind::Desktop,
        content: Some(welcome.clone()),
        level: Some(MessageLevel::Info),
        ..Default::default()
    });

    // Push a German-translated message.
    i18n::set_language("de");
    let role_you = i18n::t("chat.role.you");
    q.push(QueuedMessage {
        role: Role::User,
        channel: ChannelKind::Desktop,
        content: Some(format!("{role_you}: Hallo")),
        ..Default::default()
    });
    i18n::set_language("en"); // reset

    // Verify the English welcome message can be found via search.
    let results = q.search("Welcome");
    assert!(
        results.iter().any(|m| m.content.as_deref().unwrap().contains("Welcome")),
        "Search should find the Welcome message"
    );

    // Verify all messages are retrievable.
    let all = q.get_latest(50);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].content.as_deref(), Some(welcome.as_str()));
}

#[test]
fn queue_i18n_interpolated_content_search() {
    i18n::init();
    i18n::set_language("en");

    let mut q = MessageQueue::open_in_memory().unwrap();

    let msg = i18n::t_fmt("setup.backup.yes", &[("provider", "Claude")]);
    q.push(QueuedMessage {
        role: Role::Assistant,
        channel: ChannelKind::Desktop,
        content: Some(msg.clone()),
        ..Default::default()
    });

    let results = q.search("Claude");
    assert!(!results.is_empty(), "Should find the interpolated message containing 'Claude'");
    assert_eq!(results[0].content.as_deref(), Some(msg.as_str()));
}

// ===========================================================================
// 4. Queue + Types: push QueuedMessage, convert via From trait
// ===========================================================================

#[test]
fn queued_message_to_message_preserves_all_fields() {
    let tool_calls_json = serde_json::to_string(&vec![aios_core::types::ToolCall {
        id: "tc-42".into(),
        name: "memory".into(),
        arguments: serde_json::json!({"action": "memorize", "key": "name", "value": "Alice"}),
    }])
    .unwrap();

    let qm = QueuedMessage {
        id: 99,
        timestamp: Utc::now(),
        channel: ChannelKind::Signal,
        role: Role::Assistant,
        source: Some("claude-sonnet-4".into()),
        content: Some("I'll remember that.".into()),
        level: Some(MessageLevel::Success),
        tool_calls: Some(tool_calls_json.clone()),
        tool_call_id: Some("parent-tc-1".into()),
        metadata: Some(r#"{"card_type":"setup"}"#.into()),
    };

    // Convert via From trait.
    let msg: Message = (&qm).into();

    // Verify all fields that transfer.
    assert_eq!(msg.role, Role::Assistant);
    assert_eq!(msg.content.as_deref(), Some("I'll remember that."));
    assert_eq!(msg.tool_call_id.as_deref(), Some("parent-tc-1"));
    assert_eq!(msg.tool_calls.len(), 1);
    assert_eq!(msg.tool_calls[0].id, "tc-42");
    assert_eq!(msg.tool_calls[0].name, "memory");
    assert_eq!(msg.tool_calls[0].arguments["key"], "name");
}

#[test]
fn queued_message_to_message_no_tool_calls() {
    let qm = QueuedMessage {
        role: Role::User,
        content: Some("Hello".into()),
        tool_calls: None,
        ..Default::default()
    };

    let msg: Message = (&qm).into();
    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content.as_deref(), Some("Hello"));
    assert!(msg.tool_calls.is_empty());
    assert!(msg.tool_call_id.is_none());
}

#[test]
fn queued_message_to_message_malformed_tool_calls_json() {
    let qm = QueuedMessage {
        role: Role::Assistant,
        content: Some("response".into()),
        tool_calls: Some("not valid json".into()),
        ..Default::default()
    };

    // Should gracefully handle malformed JSON by defaulting to empty.
    let msg: Message = (&qm).into();
    assert_eq!(msg.role, Role::Assistant);
    assert!(msg.tool_calls.is_empty());
}

// ===========================================================================
// 5. Queue lifecycle: push 100, clear desktop, verify web, clear_all
// ===========================================================================

#[test]
fn queue_lifecycle_100_messages() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Push 100 messages: 50 desktop, 50 web.
    for i in 0..50 {
        q.push(user_msg(&format!("desktop-{i}"), ChannelKind::Desktop));
        q.push(user_msg(&format!("web-{i}"), ChannelKind::Web));
    }

    assert_eq!(q.count(), 100);

    // Clear desktop only.
    q.clear("desktop");

    // Desktop should see nothing from before.
    let desktop_after = q.get_after_clear_marker("desktop", 200);
    assert!(
        desktop_after.is_empty(),
        "Desktop should be empty after clear, got {}",
        desktop_after.len()
    );

    // Web should still see its 50 messages.
    let web_after = q.get_from_channel("web", 200);
    assert_eq!(web_after.len(), 50);

    // Push new messages after clear.
    q.push(user_msg("desktop-new", ChannelKind::Desktop));
    q.push(user_msg("web-new", ChannelKind::Web));

    // Desktop after clear marker should only see the new one.
    let desktop_visible = q.get_after_clear_marker("desktop", 200);
    assert_eq!(desktop_visible.len(), 2); // desktop-new + web-new (both after clear)

    // clear_all.
    q.clear_all();

    // Nothing should be visible after clear_all for any channel.
    let desktop_final = q.get_after_clear_marker("desktop", 200);
    assert!(desktop_final.is_empty());
    let web_final = q.get_after_clear_marker("web", 200);
    assert!(web_final.is_empty());

    // LLM context should also be empty.
    let ctx = q.get_llm_context(200);
    assert!(ctx.is_empty());

    // But total count is still 102 (messages exist, just hidden by markers).
    assert_eq!(q.count(), 102);
}

// ===========================================================================
// 6. Schema migration: open, verify version, close, reopen
// ===========================================================================

#[test]
fn schema_migration_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("messages.db");

    // Open, push some data, close.
    {
        let mut q = MessageQueue::open(&db_path).unwrap();
        q.push(user_msg("persistent message", ChannelKind::Desktop));
        assert_eq!(q.count(), 1);
    }

    // Reopen and verify data persists.
    {
        let q = MessageQueue::open(&db_path).unwrap();
        assert_eq!(q.count(), 1);
        let msgs = q.get_latest(10);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_deref(), Some("persistent message"));
        assert_eq!(msgs[0].channel, ChannelKind::Desktop);
        assert_eq!(msgs[0].role, Role::User);
    }
}

#[test]
fn schema_migration_reopen_preserves_fts() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("messages.db");

    // Open, push searchable data.
    {
        let mut q = MessageQueue::open(&db_path).unwrap();
        q.push(user_msg("The quick brown fox", ChannelKind::Desktop));
        q.push(user_msg("jumps over the lazy dog", ChannelKind::Desktop));
    }

    // Reopen and verify FTS works.
    {
        let q = MessageQueue::open(&db_path).unwrap();
        let results = q.search("fox");
        assert_eq!(results.len(), 1);
        assert!(results[0].content.as_deref().unwrap().contains("fox"));

        let results = q.search("lazy");
        assert_eq!(results.len(), 1);
        assert!(results[0].content.as_deref().unwrap().contains("lazy"));
    }
}

#[test]
fn schema_migration_reopen_preserves_clear_markers() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("messages.db");

    // Open, push, clear, push more.
    {
        let mut q = MessageQueue::open(&db_path).unwrap();
        q.push(user_msg("old message", ChannelKind::Desktop));
        q.clear("desktop");
        q.push(user_msg("new message", ChannelKind::Desktop));
    }

    // Reopen and verify clear marker is respected.
    {
        let q = MessageQueue::open(&db_path).unwrap();
        let visible = q.get_after_clear_marker("desktop", 50);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].content.as_deref(), Some("new message"));
    }
}

// ===========================================================================
// 7. Installer locale + config: detect country, apply defaults
// ===========================================================================

#[test]
fn installer_locale_country_to_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    // Look up Germany.
    let germany = locale::country_by_code("DE").expect("DE should exist");
    assert_eq!(germany.country_name, "Germany");
    assert_eq!(germany.language, "de");
    assert_eq!(germany.keyboard, "de");
    assert_eq!(germany.timezone, "Europe/Berlin");
    assert!(germany.time_format_24h);

    // Apply to config.
    let mut cfg = ConfigManager::with_path(config_path).unwrap();
    cfg.set("system.keyboard_layout", serde_json::json!(germany.keyboard))
        .unwrap();
    cfg.set("system.timezone", serde_json::json!(germany.timezone))
        .unwrap();
    cfg.set("system.language", serde_json::json!(germany.language))
        .unwrap();
    cfg.set(
        "system.time_format_24h",
        serde_json::json!(germany.time_format_24h),
    )
    .unwrap();

    // Verify.
    assert_eq!(cfg.get_str("system.keyboard_layout", ""), "de");
    assert_eq!(cfg.get_str("system.timezone", ""), "Europe/Berlin");
    assert_eq!(cfg.get_str("system.language", ""), "de");
    assert!(cfg.get_bool("system.time_format_24h", false));
}

#[test]
fn installer_locale_us_defaults() {
    let us = locale::country_by_code("US").expect("US should exist");
    assert_eq!(us.keyboard, "us");
    assert_eq!(us.language, "en");
    assert!(!us.time_format_24h);
    assert!(us.timezone.contains("America"));
}

#[test]
fn installer_locale_japan_applies_to_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    let japan = locale::country_by_code("JP").expect("JP should exist");
    let mut cfg = ConfigManager::with_path(config_path).unwrap();
    cfg.set("system.keyboard_layout", serde_json::json!(japan.keyboard))
        .unwrap();
    cfg.set("system.timezone", serde_json::json!(japan.timezone))
        .unwrap();

    assert_eq!(cfg.get_str("system.keyboard_layout", ""), "jp");
    assert_eq!(cfg.get_str("system.timezone", ""), "Asia/Tokyo");
}

#[test]
fn installer_locale_all_countries_have_valid_config_values() {
    let dir = tempfile::tempdir().unwrap();

    for (i, country) in locale::all_countries().iter().enumerate() {
        let config_path = dir.path().join(format!("config_{i}.json"));
        let mut cfg = ConfigManager::with_path(config_path).unwrap();
        cfg.set("system.keyboard_layout", serde_json::json!(country.keyboard))
            .unwrap();
        cfg.set("system.timezone", serde_json::json!(country.timezone))
            .unwrap();
        cfg.set("system.language", serde_json::json!(country.language))
            .unwrap();

        assert_eq!(
            cfg.get_str("system.keyboard_layout", ""),
            country.keyboard,
            "Keyboard mismatch for {}",
            country.country_code
        );
        assert_eq!(
            cfg.get_str("system.timezone", ""),
            country.timezone,
            "Timezone mismatch for {}",
            country.country_code
        );
        assert_eq!(
            cfg.get_str("system.language", ""),
            country.language,
            "Language mismatch for {}",
            country.country_code
        );
    }
}

// ===========================================================================
// Additional cross-module tests
// ===========================================================================

#[test]
fn queue_message_level_roundtrip_through_db() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    let levels = [
        MessageLevel::Info,
        MessageLevel::Success,
        MessageLevel::Warning,
        MessageLevel::Important,
        MessageLevel::Error,
    ];

    for level in &levels {
        q.push(QueuedMessage {
            role: Role::System,
            channel: ChannelKind::System,
            content: Some(format!("Level: {}", level.label())),
            level: Some(*level),
            ..Default::default()
        });
    }

    let msgs = q.get_latest(10);
    assert_eq!(msgs.len(), 5);
    for (i, level) in levels.iter().enumerate() {
        assert_eq!(
            msgs[i].level,
            Some(*level),
            "Level mismatch at index {i}: expected {:?}, got {:?}",
            level,
            msgs[i].level
        );
    }
}

#[test]
fn queue_export_json_with_all_channels() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    q.push(user_msg("desktop msg", ChannelKind::Desktop));
    q.push(user_msg("web msg", ChannelKind::Web));
    q.push(user_msg("signal msg", ChannelKind::Signal));
    q.push(user_msg("voice msg", ChannelKind::Voice));
    q.push(system_msg("system boot"));

    let mut buf = Vec::new();
    q.export_json(&mut buf).unwrap();

    let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 5);

    // Verify channel names are preserved as strings.
    let channels: Vec<&str> = arr
        .iter()
        .map(|m| m["channel"].as_str().unwrap())
        .collect();
    assert!(channels.contains(&"desktop"));
    assert!(channels.contains(&"web"));
    assert!(channels.contains(&"signal"));
    assert!(channels.contains(&"voice"));
    assert!(channels.contains(&"system"));
}

#[test]
fn queue_llm_context_respects_clear_all_across_channels() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    q.push(user_msg("old desktop", ChannelKind::Desktop));
    q.push(assistant_msg("old response", ChannelKind::Desktop));
    q.push(user_msg("old web", ChannelKind::Web));

    q.clear_all();

    q.push(user_msg("new desktop", ChannelKind::Desktop));
    q.push(assistant_msg("new response", ChannelKind::Desktop));

    let ctx = q.get_llm_context(100);
    assert_eq!(ctx.len(), 2);
    assert!(ctx.iter().all(|m| m
        .content
        .as_deref()
        .unwrap()
        .starts_with("new")));
}
