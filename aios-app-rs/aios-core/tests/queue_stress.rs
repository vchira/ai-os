//! Queue stress and concurrency tests.
//!
//! Tests for high-volume push, subscriber notification, search performance,
//! and clear marker behavior under load.

use aios_core::channel::ChannelKind;
use aios_core::queue::{MessageQueue, QueueEvent, QueuedMessage};
use aios_core::types::{MessageLevel, Role};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn user_msg(content: &str) -> QueuedMessage {
    QueuedMessage {
        role: Role::User,
        channel: ChannelKind::Desktop,
        source: Some("user".into()),
        content: Some(content.into()),
        ..Default::default()
    }
}

fn user_msg_on(content: &str, channel: ChannelKind) -> QueuedMessage {
    QueuedMessage {
        role: Role::User,
        channel,
        source: Some("user".into()),
        content: Some(content.into()),
        ..Default::default()
    }
}

fn assistant_msg(content: &str) -> QueuedMessage {
    QueuedMessage {
        role: Role::Assistant,
        channel: ChannelKind::Desktop,
        source: Some("claude-sonnet-4".into()),
        content: Some(content.into()),
        ..Default::default()
    }
}

// ===========================================================================
// 1. Push 10000 messages, verify count and get_latest
// ===========================================================================

#[test]
fn push_10000_messages_count_and_latest() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    for i in 0..10_000 {
        q.push(user_msg(&format!("Message {i}")));
    }

    // Verify total count.
    assert_eq!(q.count(), 10_000);

    // get_latest(10) should return the last 10 messages.
    let latest = q.get_latest(10);
    assert_eq!(latest.len(), 10);

    // They should be in chronological order (oldest first).
    assert_eq!(latest[0].content.as_deref(), Some("Message 9990"));
    assert_eq!(latest[9].content.as_deref(), Some("Message 9999"));

    // IDs should be ascending.
    for i in 1..latest.len() {
        assert!(latest[i].id > latest[i - 1].id);
    }

    // get_latest(100) should return the last 100.
    let latest_100 = q.get_latest(100);
    assert_eq!(latest_100.len(), 100);
    assert_eq!(latest_100[0].content.as_deref(), Some("Message 9900"));
    assert_eq!(latest_100[99].content.as_deref(), Some("Message 9999"));
}

// ===========================================================================
// 2. Subscribe, push 100 messages, verify all 100 events received
// ===========================================================================

#[test]
fn subscribe_100_events() {
    let mut q = MessageQueue::open_in_memory().unwrap();
    let mut rx = q.subscribe();

    for i in 0..100 {
        q.push(user_msg(&format!("Event msg {i}")));
    }

    let mut received = 0;
    while let Ok(event) = rx.try_recv() {
        match event {
            QueueEvent::NewMessage(id) => {
                assert!(id > 0);
                received += 1;
            }
            _ => panic!("Expected NewMessage event"),
        }
    }

    assert_eq!(received, 100, "Expected 100 events, got {received}");
}

// ===========================================================================
// 3. Multiple subscribers (5), push messages, all receive all events
// ===========================================================================

#[test]
fn five_subscribers_all_receive() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Create 5 subscribers.
    let mut receivers: Vec<_> = (0..5).map(|_| q.subscribe()).collect();

    // Push 50 messages.
    for i in 0..50 {
        q.push(user_msg(&format!("Broadcast {i}")));
    }

    // Each subscriber should receive all 50 events.
    for (sub_idx, rx) in receivers.iter_mut().enumerate() {
        let mut count = 0;
        while let Ok(event) = rx.try_recv() {
            match event {
                QueueEvent::NewMessage(id) => {
                    assert!(id > 0);
                    count += 1;
                }
                _ => panic!("Subscriber {sub_idx}: expected NewMessage"),
            }
        }
        assert_eq!(
            count, 50,
            "Subscriber {sub_idx} received {count} events, expected 50"
        );
    }
}

#[test]
fn subscribers_receive_clear_events() {
    let mut q = MessageQueue::open_in_memory().unwrap();
    let mut rx = q.subscribe();

    q.push(user_msg("before clear"));
    q.clear("desktop");
    q.push(user_msg("after clear"));

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], QueueEvent::NewMessage(_)));
    assert!(matches!(&events[1], QueueEvent::ClearMarker(ch) if ch == "desktop"));
    assert!(matches!(&events[2], QueueEvent::NewMessage(_)));
}

// ===========================================================================
// 4. Search performance: push 1000 messages, search for specific term
// ===========================================================================

#[test]
fn search_1000_messages() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Push 1000 messages with varied content.
    for i in 0..1000 {
        let content = match i % 10 {
            0 => format!("The weather is sunny today, message {i}"),
            1 => format!("Rust programming is wonderful, message {i}"),
            2 => format!("Machine learning trends in 2026, message {i}"),
            3 => format!("Cooking recipe for pasta carbonara, message {i}"),
            4 => format!("Linux kernel development notes, message {i}"),
            5 => format!("JavaScript framework comparison, message {i}"),
            6 => format!("Database optimization techniques, message {i}"),
            7 => format!("Network security best practices, message {i}"),
            8 => format!("GTK4 widget documentation, message {i}"),
            9 => format!("Docker container orchestration, message {i}"),
            _ => unreachable!(),
        };
        q.push(user_msg(&content));
    }

    assert_eq!(q.count(), 1000);

    // Search for "Rust programming".
    let rust_results = q.search("Rust");
    assert!(
        rust_results.len() >= 50,
        "Expected at least 50 'Rust' results, got {}",
        rust_results.len()
    );
    for msg in &rust_results {
        assert!(
            msg.content.as_deref().unwrap().contains("Rust"),
            "Search result should contain 'Rust': {:?}",
            msg.content
        );
    }

    // Search for "pasta".
    let pasta_results = q.search("pasta");
    assert!(
        pasta_results.len() >= 50,
        "Expected at least 50 'pasta' results, got {}",
        pasta_results.len()
    );

    // Search for "Docker".
    let docker_results = q.search("Docker");
    assert!(
        docker_results.len() >= 50,
        "Expected at least 50 'Docker' results, got {}",
        docker_results.len()
    );

    // Search for a term that doesn't exist.
    let no_results = q.search("xyznonexistent");
    assert!(
        no_results.is_empty(),
        "Expected 0 results for nonexistent term, got {}",
        no_results.len()
    );
}

// ===========================================================================
// 5. Clear marker stress: push 50, clear, push 50, clear, push 50
// ===========================================================================

#[test]
fn clear_marker_stress() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Phase 1: push 50.
    for i in 0..50 {
        q.push(user_msg(&format!("phase1-{i}")));
    }
    assert_eq!(q.count(), 50);

    // Clear.
    q.clear("desktop");

    // Desktop should see nothing.
    let visible = q.get_after_clear_marker("desktop", 200);
    assert!(
        visible.is_empty(),
        "Phase 1 clear: expected 0, got {}",
        visible.len()
    );

    // Phase 2: push 50 more.
    for i in 0..50 {
        q.push(user_msg(&format!("phase2-{i}")));
    }
    assert_eq!(q.count(), 100);

    // Desktop should see 50 (phase 2).
    let visible = q.get_after_clear_marker("desktop", 200);
    assert_eq!(visible.len(), 50, "Phase 2: expected 50, got {}", visible.len());
    assert!(visible[0]
        .content
        .as_deref()
        .unwrap()
        .starts_with("phase2"));

    // Clear again.
    q.clear("desktop");

    // Phase 3: push 50 more.
    for i in 0..50 {
        q.push(user_msg(&format!("phase3-{i}")));
    }
    assert_eq!(q.count(), 150);

    // Only phase 3 messages should be visible.
    let visible = q.get_after_clear_marker("desktop", 200);
    assert_eq!(visible.len(), 50, "Phase 3: expected 50, got {}", visible.len());
    assert!(visible[0]
        .content
        .as_deref()
        .unwrap()
        .starts_with("phase3"));
    assert!(visible[49]
        .content
        .as_deref()
        .unwrap()
        .starts_with("phase3"));
}

// ===========================================================================
// 6. Clear_all marker stress
// ===========================================================================

#[test]
fn clear_all_marker_stress() {
    let mut q = MessageQueue::open_in_memory().unwrap();
    let channels = ["desktop", "web", "signal"];

    // Push 30 messages across channels.
    for i in 0..30 {
        let channel = match i % 3 {
            0 => ChannelKind::Desktop,
            1 => ChannelKind::Web,
            _ => ChannelKind::Signal,
        };
        q.push(user_msg_on(&format!("batch1-{i}"), channel));
    }

    // Clear all.
    q.clear_all();

    // Nothing visible on any channel.
    for ch in &channels {
        let visible = q.get_after_clear_marker(ch, 200);
        assert!(
            visible.is_empty(),
            "Channel {ch} should be empty after clear_all, got {}",
            visible.len()
        );
    }

    // LLM context should also be empty.
    let ctx = q.get_llm_context(200);
    assert!(ctx.is_empty());

    // Push new messages.
    for i in 0..10 {
        q.push(user_msg_on(&format!("batch2-{i}"), ChannelKind::Desktop));
        q.push(assistant_msg(&format!("response-{i}")));
    }

    // Desktop should see 20 (10 user + 10 assistant).
    let visible = q.get_after_clear_marker("desktop", 200);
    assert_eq!(visible.len(), 20);

    // LLM context should see 20 (user + assistant, no system).
    let ctx = q.get_llm_context(200);
    assert_eq!(ctx.len(), 20);
}

// ===========================================================================
// 7. Interleaved channel clear markers
// ===========================================================================

#[test]
fn interleaved_channel_clears() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Push messages on desktop and web.
    for i in 0..20 {
        q.push(user_msg_on(&format!("desktop-{i}"), ChannelKind::Desktop));
        q.push(user_msg_on(&format!("web-{i}"), ChannelKind::Web));
    }

    // Clear desktop only.
    q.clear("desktop");

    // Push more on both.
    for i in 20..30 {
        q.push(user_msg_on(&format!("desktop-{i}"), ChannelKind::Desktop));
        q.push(user_msg_on(&format!("web-{i}"), ChannelKind::Web));
    }

    // Desktop should only see post-clear messages (20-29) plus web messages
    // (since clear marker uses timestamp, not channel filter).
    let desktop_visible = q.get_after_clear_marker("desktop", 200);
    assert_eq!(desktop_visible.len(), 20);
    // All should be from after the clear.
    for m in &desktop_visible {
        let content = m.content.as_deref().unwrap();
        let num: i32 = content
            .split('-')
            .last()
            .unwrap()
            .parse()
            .unwrap();
        assert!(num >= 20, "Visible msg {content} should be >= 20");
    }

    // Web should still see all 30 messages (no clear marker for web).
    let web_all = q.get_from_channel("web", 200);
    assert_eq!(web_all.len(), 30);

    // Now clear web.
    q.clear("web");

    // Web should see nothing.
    let web_after = q.get_after_clear_marker("web", 200);
    assert!(
        web_after.is_empty(),
        "Web should be empty after clear, got {}",
        web_after.len()
    );
}

// ===========================================================================
// 8. Push with all fields populated
// ===========================================================================

#[test]
fn push_10000_with_varied_fields() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    let channels = [
        ChannelKind::Desktop,
        ChannelKind::Web,
        ChannelKind::Signal,
        ChannelKind::Voice,
        ChannelKind::System,
    ];
    let roles = [Role::User, Role::Assistant, Role::System, Role::Tool];
    let levels = [
        None,
        Some(MessageLevel::Info),
        Some(MessageLevel::Success),
        Some(MessageLevel::Warning),
        Some(MessageLevel::Error),
    ];

    for i in 0..10_000 {
        let msg = QueuedMessage {
            channel: channels[i % channels.len()],
            role: roles[i % roles.len()],
            source: Some(format!("source-{}", i % 10)),
            content: Some(format!("Content for message {i}")),
            level: levels[i % levels.len()],
            tool_calls: if i % 7 == 0 {
                Some(format!(r#"[{{"id":"tc-{i}","name":"memory","arguments":{{}}}}]"#))
            } else {
                None
            },
            tool_call_id: if i % 11 == 0 {
                Some(format!("tc-{i}"))
            } else {
                None
            },
            metadata: if i % 13 == 0 {
                Some(format!(r#"{{"step":"step-{i}"}}"#))
            } else {
                None
            },
            ..Default::default()
        };
        q.push(msg);
    }

    assert_eq!(q.count(), 10_000);

    // Verify channel filtering.
    let desktop = q.get_from_channel("desktop", 10_000);
    let web = q.get_from_channel("web", 10_000);
    let signal = q.get_from_channel("signal", 10_000);
    let voice = q.get_from_channel("voice", 10_000);
    let system = q.get_from_channel("system", 10_000);

    assert_eq!(desktop.len(), 2000);
    assert_eq!(web.len(), 2000);
    assert_eq!(signal.len(), 2000);
    assert_eq!(voice.len(), 2000);
    assert_eq!(system.len(), 2000);

    // Verify search works on 10k messages.
    let results = q.search("message 500");
    assert!(!results.is_empty());
}

// ===========================================================================
// 9. Subscriber dropped — push should not panic
// ===========================================================================

#[test]
fn subscriber_dropped_does_not_panic() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    // Subscribe then immediately drop.
    {
        let _rx = q.subscribe();
    }

    // Pushing should not panic even though the subscriber was dropped.
    for i in 0..100 {
        q.push(user_msg(&format!("After drop {i}")));
    }

    assert_eq!(q.count(), 100);
}

// ===========================================================================
// 10. get_before with high volume
// ===========================================================================

#[test]
fn get_before_high_volume() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    let mut ids = Vec::new();
    for i in 0..1000 {
        let id = q.push(user_msg(&format!("Msg {i}")));
        ids.push(id);
    }

    // Get 50 messages before the last one.
    let last_id = *ids.last().unwrap();
    let before = q.get_before(last_id, 50);
    assert_eq!(before.len(), 50);

    // They should be the 50 messages right before the last one.
    assert_eq!(before[0].content.as_deref(), Some("Msg 949"));
    assert_eq!(before[49].content.as_deref(), Some("Msg 998"));

    // Get 50 messages before the middle.
    let mid_id = ids[500];
    let before_mid = q.get_before(mid_id, 50);
    assert_eq!(before_mid.len(), 50);
    assert_eq!(before_mid[0].content.as_deref(), Some("Msg 450"));
    assert_eq!(before_mid[49].content.as_deref(), Some("Msg 499"));
}

// ===========================================================================
// 11. LLM context with mixed system/user/assistant/tool messages
// ===========================================================================

#[test]
fn llm_context_filters_system_in_high_volume() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    for i in 0..1000 {
        match i % 4 {
            0 => q.push(user_msg(&format!("User {i}"))),
            1 => q.push(assistant_msg(&format!("Assistant {i}"))),
            2 => q.push(QueuedMessage {
                role: Role::System,
                channel: ChannelKind::System,
                content: Some(format!("System {i}")),
                level: Some(MessageLevel::Info),
                ..Default::default()
            }),
            3 => q.push(QueuedMessage {
                role: Role::Tool,
                channel: ChannelKind::Desktop,
                content: Some(format!("Tool {i}")),
                tool_call_id: Some(format!("tc-{i}")),
                ..Default::default()
            }),
            _ => unreachable!(),
        };
    }

    let ctx = q.get_llm_context(1000);
    // Should contain user, assistant, tool — NOT system.
    assert!(ctx.iter().all(|m| m.role != Role::System));
    // 750 non-system messages out of 1000.
    assert_eq!(ctx.len(), 750);
}

// ===========================================================================
// 12. File-based queue stress
// ===========================================================================

#[test]
fn file_based_queue_1000_messages() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("stress.db");

    {
        let mut q = MessageQueue::open(&db_path).unwrap();
        for i in 0..1000 {
            q.push(user_msg(&format!("Persistent {i}")));
        }
        assert_eq!(q.count(), 1000);
    }

    // Reopen and verify.
    {
        let q = MessageQueue::open(&db_path).unwrap();
        assert_eq!(q.count(), 1000);

        let latest = q.get_latest(10);
        assert_eq!(latest.len(), 10);
        assert_eq!(latest[9].content.as_deref(), Some("Persistent 999"));

        // Search should work after reopen.
        let results = q.search("Persistent 500");
        assert!(!results.is_empty());
    }
}

// ===========================================================================
// 13. Rapid clear/push cycles
// ===========================================================================

#[test]
fn rapid_clear_push_cycles() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    for cycle in 0..100 {
        // Push 10 messages.
        for i in 0..10 {
            q.push(user_msg(&format!("cycle{cycle}-msg{i}")));
        }
        // Clear.
        q.clear("desktop");
    }

    // After 100 cycles: 1000 messages total in DB.
    assert_eq!(q.count(), 1000);

    // But after the last clear, nothing should be visible.
    let visible = q.get_after_clear_marker("desktop", 2000);
    assert!(
        visible.is_empty(),
        "After final clear, expected 0, got {}",
        visible.len()
    );

    // Push a final message.
    q.push(user_msg("final message"));
    let visible = q.get_after_clear_marker("desktop", 2000);
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].content.as_deref(), Some("final message"));
}

// ===========================================================================
// 14. Export JSON with 1000 messages
// ===========================================================================

#[test]
fn export_json_1000() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    for i in 0..1000 {
        q.push(user_msg(&format!("Export {i}")));
    }

    let mut buf = Vec::new();
    q.export_json(&mut buf).unwrap();

    let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1000);

    // Verify first and last.
    assert_eq!(arr[0]["content"].as_str(), Some("Export 0"));
    assert_eq!(arr[999]["content"].as_str(), Some("Export 999"));
}

// ===========================================================================
// 15. get_on_date and get_between with volume
// ===========================================================================

#[test]
fn get_on_date_with_volume() {
    let mut q = MessageQueue::open_in_memory().unwrap();

    // All messages are pushed "now", so they share today's date.
    for i in 0..100 {
        q.push(user_msg(&format!("Today {i}")));
    }

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let on_today = q.get_on_date(&today);
    assert_eq!(on_today.len(), 100);

    // A different date should return nothing.
    let on_yesterday = q.get_on_date("2020-01-01");
    assert!(on_yesterday.is_empty());
}
