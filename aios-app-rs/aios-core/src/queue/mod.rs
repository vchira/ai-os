//! Persistent message queue backed by SQLite.
//!
//! All messages flow through this queue. Channels subscribe to events
//! and render messages independently. The queue persists across reboots.

pub mod schema;

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tokio::sync::mpsc;

use crate::channel::ChannelKind;
use crate::types::{MessageLevel, Role};
use crate::Result;

// ---------------------------------------------------------------------------
// QueuedMessage
// ---------------------------------------------------------------------------

/// A message stored in the persistent queue.
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// Auto-incrementing database ID.
    pub id: i64,
    /// When the message was created (UTC).
    pub timestamp: DateTime<Utc>,
    /// Which channel this message is associated with.
    pub channel: ChannelKind,
    /// Role of the message author.
    pub role: Role,
    /// Source identifier (e.g. "claude-sonnet-4", "user", "whisper", "system").
    pub source: Option<String>,
    /// Text content of the message.
    pub content: Option<String>,
    /// Message level for system messages (info, warning, error, success).
    pub level: Option<MessageLevel>,
    /// JSON-serialized tool calls (for assistant messages).
    pub tool_calls: Option<String>,
    /// Tool call ID (for tool result messages).
    pub tool_call_id: Option<String>,
    /// JSON metadata blob (card_type for setup, etc.).
    pub metadata: Option<String>,
}

impl Default for QueuedMessage {
    fn default() -> Self {
        Self {
            id: 0,
            timestamp: Utc::now(),
            channel: ChannelKind::Desktop,
            role: Role::User,
            source: None,
            content: None,
            level: None,
            tool_calls: None,
            tool_call_id: None,
            metadata: None,
        }
    }
}

// Conversion to LLM Message type.
impl From<&QueuedMessage> for crate::types::Message {
    fn from(qm: &QueuedMessage) -> Self {
        crate::types::Message {
            role: qm.role,
            content: qm.content.clone(),
            tool_call_id: qm.tool_call_id.clone(),
            tool_calls: qm
                .tool_calls
                .as_ref()
                .and_then(|j| serde_json::from_str(j).ok())
                .unwrap_or_default(),
        }
    }
}

// ---------------------------------------------------------------------------
// QueueEvent
// ---------------------------------------------------------------------------

/// Events emitted by the queue when its state changes.
#[derive(Debug, Clone)]
pub enum QueueEvent {
    /// A new message was added. Contains the message ID.
    NewMessage(i64),
    /// A clear marker was inserted for the given channel.
    ClearMarker(String),
}

// ---------------------------------------------------------------------------
// MessageQueue
// ---------------------------------------------------------------------------

/// Persistent message store backed by SQLite.
///
/// All messages (user, assistant, system, tool) are stored here.
/// Channels subscribe to events and render independently.
pub struct MessageQueue {
    db: Connection,
    subscribers: Vec<mpsc::UnboundedSender<QueueEvent>>,
}

impl MessageQueue {
    /// Open or create the message database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)
            .map_err(|e| crate::AiosError::Other(format!("Failed to open message DB: {e}")))?;
        schema::ensure_schema(&db)?;
        Ok(Self {
            db,
            subscribers: Vec::new(),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let db = Connection::open_in_memory()
            .map_err(|e| crate::AiosError::Other(format!("Failed to open in-memory DB: {e}")))?;
        schema::ensure_schema(&db)?;
        Ok(Self {
            db,
            subscribers: Vec::new(),
        })
    }

    /// Push a message to the queue. Returns the message ID.
    /// Notifies all subscribers with `QueueEvent::NewMessage(id)`.
    pub fn push(&mut self, msg: QueuedMessage) -> i64 {
        let timestamp = msg.timestamp.to_rfc3339();
        let channel = msg.channel.to_string();
        let role = msg.role.to_string();
        let level = msg.level.map(|l| l.as_str().to_string());

        self.db
            .execute(
                "INSERT INTO messages (timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    timestamp,
                    channel,
                    role,
                    msg.source,
                    msg.content,
                    level,
                    msg.tool_calls,
                    msg.tool_call_id,
                    msg.metadata,
                ],
            )
            .expect("Failed to insert message");

        let id = self.db.last_insert_rowid();

        // Update FTS index.
        if let Some(ref content) = msg.content {
            let _ = self.db.execute(
                "INSERT INTO messages_fts(rowid, content) VALUES (?1, ?2)",
                params![id, content],
            );
        }

        // Notify subscribers (after releasing any implicit locks).
        let event = QueueEvent::NewMessage(id);
        self.subscribers
            .retain(|tx| tx.send(event.clone()).is_ok());

        id
    }

    /// Get the latest N messages (chronological order).
    pub fn get_latest(&self, n: usize) -> Vec<QueuedMessage> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                 FROM messages ORDER BY id DESC LIMIT ?1",
            )
            .expect("Failed to prepare get_latest");

        let rows = stmt
            .query_map(params![n as i64], row_to_message)
            .expect("Failed to query messages");

        let mut msgs: Vec<QueuedMessage> = rows.filter_map(|r| r.ok()).collect();
        msgs.reverse(); // Oldest first for display.
        msgs
    }

    /// Get N messages before the given ID (for scroll-back). Returns oldest-first.
    pub fn get_before(&self, before_id: i64, n: usize) -> Vec<QueuedMessage> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                 FROM messages WHERE id < ?1 ORDER BY id DESC LIMIT ?2",
            )
            .expect("Failed to prepare get_before");

        let rows = stmt
            .query_map(params![before_id, n as i64], row_to_message)
            .expect("Failed to query messages");

        let mut msgs: Vec<QueuedMessage> = rows.filter_map(|r| r.ok()).collect();
        msgs.reverse();
        msgs
    }

    /// Get messages after the latest clear marker for a channel.
    /// This is what a channel should display by default.
    pub fn get_after_clear_marker(&self, channel: &str, n: usize) -> Vec<QueuedMessage> {
        // Find the timestamp of the latest clear marker for this channel (or "all").
        let marker_ts: Option<String> = self
            .db
            .query_row(
                "SELECT timestamp FROM clear_markers WHERE channel IN (?1, 'all') ORDER BY id DESC LIMIT 1",
                params![channel],
                |r| r.get(0),
            )
            .ok();

        if let Some(ref ts) = marker_ts {
            let mut stmt = self
                .db
                .prepare(
                    "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                     FROM messages WHERE timestamp > ?1 ORDER BY id DESC LIMIT ?2",
                )
                .expect("Failed to prepare get_after_clear_marker");

            let rows = stmt
                .query_map(params![ts, n as i64], row_to_message)
                .expect("Failed to query messages");

            let mut msgs: Vec<QueuedMessage> = rows.filter_map(|r| r.ok()).collect();
            msgs.reverse();
            msgs
        } else {
            // No clear marker — return latest N.
            self.get_latest(n)
        }
    }

    /// Get recent user/assistant/tool messages for LLM context.
    /// Filters out system messages. Respects clear_all markers.
    pub fn get_llm_context(&self, limit: usize) -> Vec<QueuedMessage> {
        // Find the latest "all" clear marker.
        let after_ts: Option<String> = self
            .db
            .query_row(
                "SELECT timestamp FROM clear_markers WHERE channel = 'all' ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok();

        let mut msgs = if let Some(ref ts) = after_ts {
            let mut stmt = self
                .db
                .prepare(
                    "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                     FROM messages
                     WHERE role IN ('user', 'assistant', 'tool') AND timestamp > ?1
                     ORDER BY id DESC LIMIT ?2",
                )
                .expect("Failed to prepare get_llm_context");
            let rows = stmt
                .query_map(params![ts, limit as i64], row_to_message)
                .expect("Failed to query messages");
            rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
        } else {
            let mut stmt = self
                .db
                .prepare(
                    "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                     FROM messages
                     WHERE role IN ('user', 'assistant', 'tool')
                     ORDER BY id DESC LIMIT ?1",
                )
                .expect("Failed to prepare get_llm_context");
            let rows = stmt
                .query_map(params![limit as i64], row_to_message)
                .expect("Failed to query messages");
            rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
        };

        msgs.reverse();
        msgs
    }

    /// Insert a clear marker for a specific channel.
    pub fn clear(&mut self, channel: &str) {
        let ts = Utc::now().to_rfc3339();
        self.db
            .execute(
                "INSERT INTO clear_markers (timestamp, channel) VALUES (?1, ?2)",
                params![ts, channel],
            )
            .expect("Failed to insert clear marker");

        let event = QueueEvent::ClearMarker(channel.to_string());
        self.subscribers
            .retain(|tx| tx.send(event.clone()).is_ok());
    }

    /// Insert clear markers for ALL channels.
    pub fn clear_all(&mut self) {
        let ts = Utc::now().to_rfc3339();
        self.db
            .execute(
                "INSERT INTO clear_markers (timestamp, channel) VALUES (?1, 'all')",
                params![ts],
            )
            .expect("Failed to insert clear_all marker");

        let event = QueueEvent::ClearMarker("all".to_string());
        self.subscribers
            .retain(|tx| tx.send(event.clone()).is_ok());
    }

    /// Subscribe to queue events. Returns a receiver.
    pub fn subscribe(&mut self) -> mpsc::UnboundedReceiver<QueueEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.push(tx);
        rx
    }

    /// Full-text search across all messages.
    pub fn search(&self, query: &str) -> Vec<QueuedMessage> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT m.id, m.timestamp, m.channel, m.role, m.source, m.content, m.level, m.tool_calls, m.tool_call_id, m.metadata
                 FROM messages m
                 JOIN messages_fts ON messages_fts.rowid = m.id
                 WHERE messages_fts MATCH ?1
                 ORDER BY m.id DESC
                 LIMIT 50",
            )
            .expect("Failed to prepare search");

        let rows = stmt
            .query_map(params![query], row_to_message)
            .expect("Failed to search messages");

        let mut msgs: Vec<QueuedMessage> = rows.filter_map(|r| r.ok()).collect();
        msgs.reverse();
        msgs
    }

    /// Export all messages as JSON.
    pub fn export_json(&self, writer: &mut dyn std::io::Write) -> Result<()> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                 FROM messages ORDER BY id",
            )
            .expect("Failed to prepare export");

        let rows = stmt
            .query_map([], row_to_message)
            .expect("Failed to query messages");

        let msgs: Vec<QueuedMessage> = rows.filter_map(|r| r.ok()).collect();

        // Serialize as JSON array of objects.
        let json_msgs: Vec<serde_json::Value> = msgs
            .iter()
            .map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "timestamp": m.timestamp.to_rfc3339(),
                    "channel": m.channel.to_string(),
                    "role": m.role.to_string(),
                    "source": m.source,
                    "content": m.content,
                    "level": m.level.map(|l| l.to_string()),
                    "tool_calls": m.tool_calls,
                    "tool_call_id": m.tool_call_id,
                    "metadata": m.metadata,
                })
            })
            .collect();

        serde_json::to_writer_pretty(writer, &json_msgs)
            .map_err(|e| crate::AiosError::Other(format!("Failed to export JSON: {e}")))?;

        Ok(())
    }

    /// Get total message count.
    pub fn count(&self) -> i64 {
        self.db
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Get messages on a specific date (YYYY-MM-DD).
    pub fn get_on_date(&self, date: &str) -> Vec<QueuedMessage> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                 FROM messages WHERE timestamp LIKE ?1 || '%' ORDER BY id",
            )
            .expect("Failed to prepare get_on_date");

        let rows = stmt
            .query_map(params![date], row_to_message)
            .expect("Failed to query messages");

        rows.filter_map(|r| r.ok()).collect()
    }

    /// Get messages between two dates (ISO 8601).
    pub fn get_between(&self, from: &str, to: &str) -> Vec<QueuedMessage> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                 FROM messages WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY id",
            )
            .expect("Failed to prepare get_between");

        let rows = stmt
            .query_map(params![from, to], row_to_message)
            .expect("Failed to query messages");

        rows.filter_map(|r| r.ok()).collect()
    }

    /// Get messages from a specific channel.
    pub fn get_from_channel(&self, channel: &str, n: usize) -> Vec<QueuedMessage> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, timestamp, channel, role, source, content, level, tool_calls, tool_call_id, metadata
                 FROM messages WHERE channel = ?1 ORDER BY id DESC LIMIT ?2",
            )
            .expect("Failed to prepare get_from_channel");

        let rows = stmt
            .query_map(params![channel, n as i64], row_to_message)
            .expect("Failed to query messages");

        let mut msgs: Vec<QueuedMessage> = rows.filter_map(|r| r.ok()).collect();
        msgs.reverse();
        msgs
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a SQLite row to a `QueuedMessage`.
fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<QueuedMessage> {
    let timestamp_str: String = row.get(1)?;
    let channel_str: String = row.get(2)?;
    let role_str: String = row.get(3)?;
    let level_str: Option<String> = row.get(6)?;

    let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let channel = ChannelKind::from_str_opt(&channel_str).unwrap_or(ChannelKind::Desktop);

    let role = match role_str.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        "tool" => Role::Tool,
        _ => Role::User,
    };

    let level = level_str.and_then(|s| MessageLevel::from_str_opt(&s));

    Ok(QueuedMessage {
        id: row.get(0)?,
        timestamp,
        channel,
        role,
        source: row.get(4)?,
        content: row.get(5)?,
        level,
        tool_calls: row.get(7)?,
        tool_call_id: row.get(8)?,
        metadata: row.get(9)?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_queue() -> MessageQueue {
        MessageQueue::open_in_memory().unwrap()
    }

    fn user_msg(content: &str) -> QueuedMessage {
        QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
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

    #[test]
    fn push_returns_incrementing_ids() {
        let mut q = test_queue();
        let id1 = q.push(user_msg("hello"));
        let id2 = q.push(user_msg("world"));
        assert!(id2 > id1);
    }

    #[test]
    fn get_latest_returns_correct_order() {
        let mut q = test_queue();
        q.push(user_msg("first"));
        q.push(user_msg("second"));
        q.push(user_msg("third"));

        let msgs = q.get_latest(2);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content.as_deref(), Some("second"));
        assert_eq!(msgs[1].content.as_deref(), Some("third"));
    }

    #[test]
    fn get_before_returns_older_messages() {
        let mut q = test_queue();
        q.push(user_msg("a"));
        q.push(user_msg("b"));
        let id3 = q.push(user_msg("c"));

        let msgs = q.get_before(id3, 10);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content.as_deref(), Some("a"));
        assert_eq!(msgs[1].content.as_deref(), Some("b"));
    }

    #[test]
    fn clear_inserts_marker_and_get_after_clear_marker_respects_it() {
        let mut q = test_queue();
        q.push(user_msg("before clear"));
        q.clear("desktop");
        q.push(user_msg("after clear"));

        let msgs = q.get_after_clear_marker("desktop", 50);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_deref(), Some("after clear"));
    }

    #[test]
    fn clear_all_marker_affects_llm_context() {
        let mut q = test_queue();
        q.push(user_msg("old message"));
        q.push(assistant_msg("old response"));
        q.clear_all();
        q.push(user_msg("new message"));

        let ctx = q.get_llm_context(50);
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].content.as_deref(), Some("new message"));
    }

    #[test]
    fn per_channel_clear_does_not_affect_other_channels() {
        let mut q = test_queue();
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("desktop msg".into()),
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            content: Some("web msg".into()),
            ..Default::default()
        });

        q.clear("desktop");

        // Web should still see its messages.
        let web_msgs = q.get_after_clear_marker("web", 50);
        assert!(web_msgs.iter().any(|m| m.content.as_deref() == Some("web msg")));
    }

    #[test]
    fn get_llm_context_filters_system_messages() {
        let mut q = test_queue();
        q.push(system_msg("boot status"));
        q.push(user_msg("hello"));
        q.push(assistant_msg("hi there"));

        let ctx = q.get_llm_context(50);
        assert_eq!(ctx.len(), 2);
        // System messages should be filtered out.
        assert!(ctx.iter().all(|m| m.role != Role::System));
    }

    #[test]
    fn get_llm_context_includes_tool_messages() {
        let mut q = test_queue();
        q.push(user_msg("check disk"));
        q.push(QueuedMessage {
            role: Role::Tool,
            channel: ChannelKind::Desktop,
            content: Some("disk: 50% used".into()),
            tool_call_id: Some("tc-1".into()),
            ..Default::default()
        });

        let ctx = q.get_llm_context(50);
        assert_eq!(ctx.len(), 2);
        assert!(ctx.iter().any(|m| m.role == Role::Tool));
    }

    #[test]
    fn search_via_fts5() {
        let mut q = test_queue();
        q.push(user_msg("tell me about the weather"));
        q.push(user_msg("what is the capital of France"));
        q.push(assistant_msg("The capital of France is Paris"));

        let results = q.search("France");
        assert!(results.len() >= 2);
    }

    #[test]
    fn subscribe_receives_events_on_push() {
        let mut q = test_queue();
        let mut rx = q.subscribe();

        q.push(user_msg("test"));

        let event = rx.try_recv().unwrap();
        match event {
            QueueEvent::NewMessage(id) => assert!(id > 0),
            _ => panic!("Expected NewMessage event"),
        }
    }

    #[test]
    fn export_json_writes_valid_json() {
        let mut q = test_queue();
        q.push(user_msg("hello"));
        q.push(assistant_msg("hi"));

        let mut buf = Vec::new();
        q.export_json(&mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 2);
    }

    #[test]
    fn queued_message_to_message_conversion() {
        let qm = QueuedMessage {
            role: Role::Assistant,
            content: Some("Hello!".into()),
            tool_calls: Some(
                serde_json::to_string(&vec![crate::types::ToolCall {
                    id: "tc-1".into(),
                    name: "memory".into(),
                    arguments: serde_json::json!({"key": "name"}),
                }])
                .unwrap(),
            ),
            ..Default::default()
        };

        let msg: crate::types::Message = (&qm).into();
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content.as_deref(), Some("Hello!"));
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].name, "memory");
    }

    #[test]
    fn count_tracks_messages() {
        let mut q = test_queue();
        assert_eq!(q.count(), 0);
        q.push(user_msg("one"));
        q.push(user_msg("two"));
        assert_eq!(q.count(), 2);
    }

    #[test]
    fn get_from_channel_filters_correctly() {
        let mut q = test_queue();
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("desktop".into()),
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            content: Some("web".into()),
            ..Default::default()
        });

        let desktop_msgs = q.get_from_channel("desktop", 50);
        assert_eq!(desktop_msgs.len(), 1);
        assert_eq!(desktop_msgs[0].content.as_deref(), Some("desktop"));
    }

    // -----------------------------------------------------------------------
    // Positive cases
    // -----------------------------------------------------------------------

    #[test]
    fn push_stores_all_fields() {
        let mut q = test_queue();
        let tool_calls_json = serde_json::to_string(&vec![crate::types::ToolCall {
            id: "tc-42".into(),
            name: "system".into(),
            arguments: serde_json::json!({"command": "ls"}),
        }])
        .unwrap();

        let ts = Utc::now();
        let msg = QueuedMessage {
            id: 0,
            timestamp: ts,
            channel: ChannelKind::Signal,
            role: Role::Assistant,
            source: Some("claude-sonnet-4".into()),
            content: Some("Here are the files".into()),
            level: Some(MessageLevel::Warning),
            tool_calls: Some(tool_calls_json.clone()),
            tool_call_id: Some("tc-99".into()),
            metadata: Some(r#"{"card_type":"setup"}"#.into()),
        };

        let id = q.push(msg);
        let msgs = q.get_latest(1);
        assert_eq!(msgs.len(), 1);
        let m = &msgs[0];

        assert_eq!(m.id, id);
        assert_eq!(m.channel, ChannelKind::Signal);
        assert_eq!(m.role, Role::Assistant);
        assert_eq!(m.source.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(m.content.as_deref(), Some("Here are the files"));
        assert_eq!(m.level, Some(MessageLevel::Warning));
        assert_eq!(m.tool_calls.as_deref(), Some(tool_calls_json.as_str()));
        assert_eq!(m.tool_call_id.as_deref(), Some("tc-99"));
        assert_eq!(m.metadata.as_deref(), Some(r#"{"card_type":"setup"}"#));
    }

    #[test]
    fn get_latest_returns_empty_on_empty_db() {
        let q = test_queue();
        let msgs = q.get_latest(10);
        assert!(msgs.is_empty());
    }

    #[test]
    fn get_before_with_no_earlier_messages() {
        let mut q = test_queue();
        let id1 = q.push(user_msg("only message"));
        // Asking for messages before the first message should return nothing.
        let msgs = q.get_before(id1, 10);
        assert!(msgs.is_empty());
    }

    #[test]
    fn multiple_subscribers_all_receive_events() {
        let mut q = test_queue();
        let mut rx1 = q.subscribe();
        let mut rx2 = q.subscribe();
        let mut rx3 = q.subscribe();

        let id = q.push(user_msg("broadcast"));

        for rx in [&mut rx1, &mut rx2, &mut rx3] {
            let event = rx.try_recv().expect("subscriber should receive event");
            match event {
                QueueEvent::NewMessage(eid) => assert_eq!(eid, id),
                _ => panic!("Expected NewMessage event"),
            }
        }
    }

    #[test]
    fn clear_marker_independence() {
        let mut q = test_queue();
        // Push messages on both channels.
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("desktop before".into()),
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            content: Some("web before".into()),
            ..Default::default()
        });

        // Clear only desktop.
        q.clear("desktop");

        // Web should still see its messages (no clear marker for web).
        let web_msgs = q.get_after_clear_marker("web", 50);
        assert!(web_msgs.iter().any(|m| m.content.as_deref() == Some("web before")));

        // Desktop should see nothing from before the clear.
        let desktop_msgs = q.get_after_clear_marker("desktop", 50);
        assert!(!desktop_msgs.iter().any(|m| m.content.as_deref() == Some("desktop before")));
    }

    #[test]
    fn clear_all_then_new_messages_visible() {
        let mut q = test_queue();
        q.push(user_msg("old"));
        q.push(assistant_msg("old reply"));
        q.clear_all();

        q.push(user_msg("fresh start"));
        q.push(assistant_msg("fresh reply"));

        let ctx = q.get_llm_context(50);
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].content.as_deref(), Some("fresh start"));
        assert_eq!(ctx[1].content.as_deref(), Some("fresh reply"));
    }

    #[test]
    fn search_returns_empty_for_no_match() {
        let mut q = test_queue();
        q.push(user_msg("hello world"));
        q.push(assistant_msg("greetings"));

        let results = q.search("xyznonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn get_on_date_filters_correctly() {
        let mut q = test_queue();

        // Insert a message with a specific timestamp in the past.
        let old_ts = DateTime::parse_from_rfc3339("2025-01-15T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("january message".into()),
            timestamp: old_ts,
            ..Default::default()
        });

        // Insert a message with today-like timestamp.
        let today_ts = DateTime::parse_from_rfc3339("2026-03-18T14:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("march message".into()),
            timestamp: today_ts,
            ..Default::default()
        });

        let jan_msgs = q.get_on_date("2025-01-15");
        assert_eq!(jan_msgs.len(), 1);
        assert_eq!(jan_msgs[0].content.as_deref(), Some("january message"));

        let mar_msgs = q.get_on_date("2026-03-18");
        assert_eq!(mar_msgs.len(), 1);
        assert_eq!(mar_msgs[0].content.as_deref(), Some("march message"));

        // A date with no messages.
        let empty = q.get_on_date("2099-12-31");
        assert!(empty.is_empty());
    }

    #[test]
    fn get_between_inclusive_range() {
        let mut q = test_queue();

        let ts1 = DateTime::parse_from_rfc3339("2025-06-01T08:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let ts2 = DateTime::parse_from_rfc3339("2025-06-15T12:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let ts3 = DateTime::parse_from_rfc3339("2025-07-01T08:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);

        q.push(QueuedMessage {
            role: Role::User,
            content: Some("june start".into()),
            timestamp: ts1,
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            content: Some("june mid".into()),
            timestamp: ts2,
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            content: Some("july".into()),
            timestamp: ts3,
            ..Default::default()
        });

        // Range covering all of June.
        let june = q.get_between("2025-06-01T00:00:00+00:00", "2025-06-30T23:59:59+00:00");
        assert_eq!(june.len(), 2);
        assert_eq!(june[0].content.as_deref(), Some("june start"));
        assert_eq!(june[1].content.as_deref(), Some("june mid"));

        // Range covering only mid-June onward.
        let later = q.get_between("2025-06-10T00:00:00+00:00", "2025-07-02T00:00:00+00:00");
        assert_eq!(later.len(), 2);
        assert_eq!(later[0].content.as_deref(), Some("june mid"));
        assert_eq!(later[1].content.as_deref(), Some("july"));
    }

    #[test]
    fn get_from_channel_returns_only_that_channel() {
        let mut q = test_queue();

        for (ch, text) in [
            (ChannelKind::Desktop, "desktop msg"),
            (ChannelKind::Web, "web msg"),
            (ChannelKind::Signal, "signal msg"),
        ] {
            q.push(QueuedMessage {
                role: Role::User,
                channel: ch,
                content: Some(text.into()),
                ..Default::default()
            });
        }

        let web_only = q.get_from_channel("web", 50);
        assert_eq!(web_only.len(), 1);
        assert_eq!(web_only[0].content.as_deref(), Some("web msg"));
        assert_eq!(web_only[0].channel, ChannelKind::Web);

        let signal_only = q.get_from_channel("signal", 50);
        assert_eq!(signal_only.len(), 1);
        assert_eq!(signal_only[0].content.as_deref(), Some("signal msg"));
    }

    #[test]
    fn export_json_roundtrip() {
        let mut q = test_queue();
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            source: Some("user".into()),
            content: Some("test export".into()),
            level: Some(MessageLevel::Info),
            metadata: Some(r#"{"foo":"bar"}"#.into()),
            ..Default::default()
        });
        q.push(assistant_msg("response"));

        let mut buf = Vec::new();
        q.export_json(&mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 2);

        // Check first message structure.
        let first = &arr[0];
        assert_eq!(first["role"], "user");
        assert_eq!(first["channel"], "web");
        assert_eq!(first["content"], "test export");
        assert_eq!(first["source"], "user");
        assert_eq!(first["level"], "INFO"); // MessageLevel::Info Display is "INFO"
        assert!(first["id"].is_number());
        assert!(first["timestamp"].is_string());

        // Check second message.
        let second = &arr[1];
        assert_eq!(second["role"], "assistant");
        assert_eq!(second["content"], "response");
    }

    #[test]
    fn get_llm_context_includes_all_channels() {
        let mut q = test_queue();
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("from desktop".into()),
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            content: Some("from web".into()),
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Signal,
            content: Some("from signal".into()),
            ..Default::default()
        });

        let ctx = q.get_llm_context(50);
        assert_eq!(ctx.len(), 3);
        let contents: Vec<&str> = ctx.iter().filter_map(|m| m.content.as_deref()).collect();
        assert!(contents.contains(&"from desktop"));
        assert!(contents.contains(&"from web"));
        assert!(contents.contains(&"from signal"));
    }

    // -----------------------------------------------------------------------
    // Negative / edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn push_with_none_content() {
        let mut q = test_queue();
        let id = q.push(QueuedMessage {
            role: Role::Assistant,
            channel: ChannelKind::Desktop,
            content: None,
            tool_calls: Some(r#"[{"id":"tc-1","name":"memory","arguments":{}}]"#.into()),
            ..Default::default()
        });

        let msgs = q.get_latest(1);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, id);
        assert!(msgs[0].content.is_none());
        assert!(msgs[0].tool_calls.is_some());
    }

    #[test]
    fn search_with_special_characters() {
        let mut q = test_queue();
        // FTS5 treats quotes and apostrophes specially. Push a normal message
        // and search for a term from it, not the special chars themselves.
        q.push(user_msg("what is the weather today"));
        q.push(user_msg("the forecast looks sunny"));

        // Search for a plain keyword that exists.
        let results = q.search("weather");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content.as_deref(), Some("what is the weather today"));

        // Search for a term that doesn't exist.
        let empty = q.search("nonexistentterm");
        assert!(empty.is_empty());
    }

    #[test]
    fn get_latest_with_n_larger_than_db() {
        let mut q = test_queue();
        q.push(user_msg("one"));
        q.push(user_msg("two"));
        q.push(user_msg("three"));

        // Ask for 1000, only 3 exist.
        let msgs = q.get_latest(1000);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content.as_deref(), Some("one"));
        assert_eq!(msgs[1].content.as_deref(), Some("two"));
        assert_eq!(msgs[2].content.as_deref(), Some("three"));
    }

    #[test]
    fn clear_then_clear_again() {
        let mut q = test_queue();
        q.push(user_msg("before first clear"));
        q.clear("desktop");
        q.push(user_msg("between clears"));
        q.clear("desktop");
        q.push(user_msg("after second clear"));

        let msgs = q.get_after_clear_marker("desktop", 50);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_deref(), Some("after second clear"));

        // Double clear_all.
        q.clear_all();
        q.clear_all();
        q.push(user_msg("final"));
        let ctx = q.get_llm_context(50);
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].content.as_deref(), Some("final"));
    }

    #[test]
    fn subscribe_after_messages_exist() {
        let mut q = test_queue();
        // Push messages BEFORE subscribing.
        q.push(user_msg("old1"));
        q.push(user_msg("old2"));

        // Subscribe now.
        let mut rx = q.subscribe();

        // Subscriber should NOT receive old messages.
        assert!(rx.try_recv().is_err());

        // Push a new message — subscriber should get it.
        let new_id = q.push(user_msg("new"));
        let event = rx.try_recv().unwrap();
        match event {
            QueueEvent::NewMessage(id) => assert_eq!(id, new_id),
            _ => panic!("Expected NewMessage"),
        }
    }

    #[test]
    fn concurrent_push_ids_never_duplicate() {
        let mut q = test_queue();
        let mut ids = std::collections::HashSet::new();

        for i in 0..100 {
            let id = q.push(user_msg(&format!("msg {i}")));
            assert!(ids.insert(id), "Duplicate ID found: {id}");
        }

        assert_eq!(ids.len(), 100);
        assert_eq!(q.count(), 100);
    }

    #[test]
    fn queued_message_default_has_sane_values() {
        let msg = QueuedMessage::default();

        assert_eq!(msg.id, 0);
        assert_eq!(msg.channel, ChannelKind::Desktop);
        assert_eq!(msg.role, Role::User);
        assert!(msg.source.is_none());
        assert!(msg.content.is_none());
        assert!(msg.level.is_none());
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
        assert!(msg.metadata.is_none());
        // Timestamp should be roughly "now" (within 1 second).
        let diff = Utc::now() - msg.timestamp;
        assert!(diff.num_seconds().abs() < 2);
    }

    // -----------------------------------------------------------------------
    // From<&QueuedMessage> for Message conversion tests
    // -----------------------------------------------------------------------

    #[test]
    fn conversion_with_tool_calls_json() {
        let tool_calls = vec![
            crate::types::ToolCall {
                id: "tc-1".into(),
                name: "memory".into(),
                arguments: serde_json::json!({"key": "name", "value": "Alice"}),
            },
            crate::types::ToolCall {
                id: "tc-2".into(),
                name: "system".into(),
                arguments: serde_json::json!({"command": "uptime"}),
            },
        ];
        let json = serde_json::to_string(&tool_calls).unwrap();

        let qm = QueuedMessage {
            role: Role::Assistant,
            content: Some("Let me do that".into()),
            tool_calls: Some(json),
            ..Default::default()
        };

        let msg: crate::types::Message = (&qm).into();
        assert_eq!(msg.tool_calls.len(), 2);
        assert_eq!(msg.tool_calls[0].id, "tc-1");
        assert_eq!(msg.tool_calls[0].name, "memory");
        assert_eq!(msg.tool_calls[0].arguments["key"], "name");
        assert_eq!(msg.tool_calls[0].arguments["value"], "Alice");
        assert_eq!(msg.tool_calls[1].id, "tc-2");
        assert_eq!(msg.tool_calls[1].name, "system");
        assert_eq!(msg.tool_calls[1].arguments["command"], "uptime");
    }

    #[test]
    fn conversion_with_invalid_tool_calls_json() {
        let qm = QueuedMessage {
            role: Role::Assistant,
            content: Some("broken".into()),
            tool_calls: Some("this is not valid json {{{".into()),
            ..Default::default()
        };

        let msg: crate::types::Message = (&qm).into();
        // Invalid JSON should default to empty vec.
        assert!(msg.tool_calls.is_empty());
        // Content should still be preserved.
        assert_eq!(msg.content.as_deref(), Some("broken"));
    }

    #[test]
    fn conversion_preserves_role_and_content() {
        // Test each role.
        for (role, content) in [
            (Role::User, Some("hello")),
            (Role::Assistant, Some("hi there")),
            (Role::System, Some("system prompt")),
            (Role::Tool, Some("tool result")),
            (Role::Assistant, None), // assistant with no content (tool-only)
        ] {
            let qm = QueuedMessage {
                role,
                content: content.map(String::from),
                tool_call_id: if role == Role::Tool {
                    Some("tc-99".into())
                } else {
                    None
                },
                ..Default::default()
            };

            let msg: crate::types::Message = (&qm).into();
            assert_eq!(msg.role, role);
            assert_eq!(msg.content.as_deref(), content);
            if role == Role::Tool {
                assert_eq!(msg.tool_call_id.as_deref(), Some("tc-99"));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Additional edge cases, error paths, and stress scenarios
    // -----------------------------------------------------------------------

    #[test]
    fn get_latest_zero_returns_empty() {
        let mut q = test_queue();
        q.push(user_msg("hello"));
        q.push(user_msg("world"));

        let msgs = q.get_latest(0);
        assert!(msgs.is_empty());
    }

    #[test]
    fn get_before_id_zero_returns_empty() {
        let mut q = test_queue();
        q.push(user_msg("a"));
        q.push(user_msg("b"));

        // SQLite auto-increment starts at 1, so nothing has id < 0.
        let msgs = q.get_before(0, 10);
        assert!(msgs.is_empty());
    }

    #[test]
    fn clear_all_affects_all_channels_uniformly() {
        let mut q = test_queue();
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("desktop old".into()),
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            content: Some("web old".into()),
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Signal,
            content: Some("signal old".into()),
            ..Default::default()
        });

        q.clear_all();

        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("desktop new".into()),
            ..Default::default()
        });

        // Every channel's get_after_clear_marker should only see post-clear messages.
        for channel in ["desktop", "web", "signal"] {
            let msgs = q.get_after_clear_marker(channel, 50);
            assert_eq!(msgs.len(), 1, "channel '{channel}' should see 1 message after clear_all");
            assert_eq!(msgs[0].content.as_deref(), Some("desktop new"));
        }
    }

    #[test]
    fn multiple_clear_markers_only_latest_matters() {
        let mut q = test_queue();
        q.push(user_msg("msg 1"));
        q.clear("desktop");
        q.push(user_msg("msg 2"));
        q.clear("desktop");
        q.push(user_msg("msg 3"));
        q.clear("desktop");
        q.push(user_msg("msg 4"));

        let msgs = q.get_after_clear_marker("desktop", 50);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_deref(), Some("msg 4"));
    }

    #[test]
    fn get_after_clear_marker_empty_queue_returns_empty() {
        let q = test_queue();
        let msgs = q.get_after_clear_marker("desktop", 50);
        assert!(msgs.is_empty());
    }

    #[test]
    fn get_after_clear_marker_with_clear_but_no_subsequent_messages() {
        let mut q = test_queue();
        q.push(user_msg("before"));
        q.clear("desktop");

        // No messages after the clear marker.
        let msgs = q.get_after_clear_marker("desktop", 50);
        assert!(msgs.is_empty());
    }

    #[test]
    fn get_llm_context_empty_queue_returns_empty() {
        let q = test_queue();
        let ctx = q.get_llm_context(50);
        assert!(ctx.is_empty());
    }

    #[test]
    fn get_llm_context_respects_clear_all_but_ignores_per_channel_clear() {
        let mut q = test_queue();
        q.push(user_msg("message one"));
        q.push(assistant_msg("response one"));

        // Per-channel clear should NOT affect LLM context (only clear_all does).
        q.clear("desktop");

        q.push(user_msg("message two"));

        let ctx = q.get_llm_context(50);
        // All three user/assistant messages should be present.
        assert_eq!(ctx.len(), 3);
        assert_eq!(ctx[0].content.as_deref(), Some("message one"));
        assert_eq!(ctx[1].content.as_deref(), Some("response one"));
        assert_eq!(ctx[2].content.as_deref(), Some("message two"));

        // Now clear_all — this one DOES affect LLM context.
        q.clear_all();
        q.push(user_msg("after clear_all"));

        let ctx = q.get_llm_context(50);
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].content.as_deref(), Some("after clear_all"));
    }

    #[test]
    fn get_llm_context_excludes_system_with_all_levels() {
        let mut q = test_queue();

        for level in [
            MessageLevel::Info,
            MessageLevel::Success,
            MessageLevel::Warning,
            MessageLevel::Important,
            MessageLevel::Error,
        ] {
            q.push(QueuedMessage {
                role: Role::System,
                channel: ChannelKind::System,
                source: Some("system".into()),
                content: Some(format!("system {}", level.as_str())),
                level: Some(level),
                ..Default::default()
            });
        }

        q.push(user_msg("user question"));
        q.push(assistant_msg("ai answer"));

        let ctx = q.get_llm_context(50);
        assert_eq!(ctx.len(), 2);
        assert!(ctx.iter().all(|m| m.role != Role::System));
    }

    #[test]
    fn get_llm_context_limit_returns_most_recent() {
        let mut q = test_queue();
        for i in 0..20 {
            q.push(user_msg(&format!("msg {i}")));
        }

        let ctx = q.get_llm_context(5);
        assert_eq!(ctx.len(), 5);
        // Should be the 5 most recent in chronological order.
        assert_eq!(ctx[0].content.as_deref(), Some("msg 15"));
        assert_eq!(ctx[1].content.as_deref(), Some("msg 16"));
        assert_eq!(ctx[2].content.as_deref(), Some("msg 17"));
        assert_eq!(ctx[3].content.as_deref(), Some("msg 18"));
        assert_eq!(ctx[4].content.as_deref(), Some("msg 19"));
    }

    #[test]
    fn search_finds_partial_prefix_matches() {
        let mut q = test_queue();
        q.push(user_msg("the configuration is complete"));
        q.push(user_msg("reconfigure the system"));

        // FTS5 prefix query with *.
        let results = q.search("configur*");
        assert!(results.len() >= 1);
    }

    #[test]
    fn export_json_empty_queue_writes_empty_array() {
        let q = test_queue();
        let mut buf = Vec::new();
        q.export_json(&mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[test]
    fn export_json_preserves_all_optional_fields() {
        let mut q = test_queue();
        q.push(QueuedMessage {
            role: Role::Assistant,
            channel: ChannelKind::Web,
            source: Some("claude-sonnet-4".into()),
            content: Some("hello".into()),
            level: Some(MessageLevel::Success),
            tool_calls: Some(r#"[{"id":"tc-1","name":"memory","arguments":{}}]"#.into()),
            tool_call_id: Some("tc-prev".into()),
            metadata: Some(r#"{"key":"value"}"#.into()),
            ..Default::default()
        });

        let mut buf = Vec::new();
        q.export_json(&mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);

        let m = &arr[0];
        assert_eq!(m["role"], "assistant");
        assert_eq!(m["channel"], "web");
        assert_eq!(m["source"], "claude-sonnet-4");
        assert_eq!(m["content"], "hello");
        assert!(!m["level"].is_null());
        assert!(m["tool_calls"].as_str().unwrap().contains("tc-1"));
        assert_eq!(m["tool_call_id"], "tc-prev");
        assert_eq!(m["metadata"], r#"{"key":"value"}"#);
        assert!(m["id"].is_number());
        assert!(m["timestamp"].is_string());
    }

    #[test]
    fn subscriber_dropped_does_not_affect_others() {
        let mut q = test_queue();
        let mut rx1 = q.subscribe();
        let rx2 = q.subscribe();
        let mut rx3 = q.subscribe();

        // Drop subscriber 2.
        drop(rx2);

        // Push should still work; surviving subscribers receive the event.
        let id = q.push(user_msg("still works"));

        for rx in [&mut rx1, &mut rx3] {
            match rx.try_recv().unwrap() {
                QueueEvent::NewMessage(received_id) => assert_eq!(received_id, id),
                _ => panic!("Expected NewMessage event"),
            }
        }

        // Dead subscriber should have been cleaned up.
        assert_eq!(q.subscribers.len(), 2);
    }

    #[test]
    fn clear_event_received_by_subscriber() {
        let mut q = test_queue();
        let mut rx = q.subscribe();

        q.clear("web");

        match rx.try_recv().unwrap() {
            QueueEvent::ClearMarker(channel) => assert_eq!(channel, "web"),
            _ => panic!("Expected ClearMarker event"),
        }
    }

    #[test]
    fn clear_all_event_received_by_subscriber() {
        let mut q = test_queue();
        let mut rx = q.subscribe();

        q.clear_all();

        match rx.try_recv().unwrap() {
            QueueEvent::ClearMarker(channel) => assert_eq!(channel, "all"),
            _ => panic!("Expected ClearMarker event for 'all'"),
        }
    }

    #[test]
    fn get_between_inverted_dates_returns_empty() {
        let mut q = test_queue();
        q.push(user_msg("a message"));

        // from > to — no messages can match.
        let msgs = q.get_between("2099-01-01T00:00:00+00:00", "2000-01-01T00:00:00+00:00");
        assert!(msgs.is_empty());
    }

    #[test]
    fn get_from_channel_unknown_channel_returns_empty() {
        let mut q = test_queue();
        q.push(user_msg("desktop msg"));

        let msgs = q.get_from_channel("nonexistent_channel", 50);
        assert!(msgs.is_empty());
    }

    #[test]
    fn stress_push_1000_get_latest_10() {
        let mut q = test_queue();
        for i in 0..1000 {
            q.push(user_msg(&format!("message {i}")));
        }

        assert_eq!(q.count(), 1000);

        let msgs = q.get_latest(10);
        assert_eq!(msgs.len(), 10);
        // Should be messages 990..999 in chronological order.
        assert_eq!(msgs[0].content.as_deref(), Some("message 990"));
        assert_eq!(msgs[9].content.as_deref(), Some("message 999"));
    }

    #[test]
    fn stress_push_multiple_roles_verify_filtering() {
        let mut q = test_queue();

        for i in 0..100 {
            match i % 4 {
                0 => q.push(user_msg(&format!("user {i}"))),
                1 => q.push(assistant_msg(&format!("assistant {i}"))),
                2 => q.push(system_msg(&format!("system {i}"))),
                3 => q.push(QueuedMessage {
                    role: Role::Tool,
                    channel: ChannelKind::Desktop,
                    content: Some(format!("tool {i}")),
                    tool_call_id: Some(format!("tc-{i}")),
                    ..Default::default()
                }),
                _ => unreachable!(),
            };
        }

        assert_eq!(q.count(), 100);

        // LLM context filters out system messages.
        let ctx = q.get_llm_context(200);
        assert!(ctx.iter().all(|m| m.role != Role::System));
        // 25 user + 25 assistant + 25 tool = 75
        assert_eq!(ctx.len(), 75);
    }

    #[test]
    fn stress_many_clear_markers() {
        let mut q = test_queue();

        // Insert messages interleaved with clear markers every 5 messages.
        for i in 0..50 {
            q.push(user_msg(&format!("msg {i}")));
            if i % 5 == 0 {
                q.clear("desktop");
            }
        }

        // The last clear was at i=45, so messages 46..49 should be visible.
        let msgs = q.get_after_clear_marker("desktop", 100);
        assert_eq!(msgs.len(), 4);
        for (j, m) in msgs.iter().enumerate() {
            assert_eq!(
                m.content.as_deref(),
                Some(format!("msg {}", 46 + j).as_str())
            );
        }
    }

    #[test]
    fn fts_does_not_index_none_content() {
        let mut q = test_queue();
        // Push a message with no content (tool-call only).
        q.push(QueuedMessage {
            role: Role::Assistant,
            content: None,
            tool_calls: Some(r#"[{"id":"tc-1","name":"web","arguments":{}}]"#.into()),
            ..Default::default()
        });
        q.push(user_msg("searchable text here"));

        let results = q.search("searchable");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content.as_deref(), Some("searchable text here"));
    }

    #[test]
    fn dead_subscribers_cleaned_up_on_clear() {
        let mut q = test_queue();
        let rx1 = q.subscribe();
        let _rx2 = q.subscribe();

        assert_eq!(q.subscribers.len(), 2);
        drop(rx1);

        q.clear("desktop");

        // Dead subscriber should be pruned.
        assert_eq!(q.subscribers.len(), 1);
    }

    #[test]
    fn get_after_clear_marker_no_marker_falls_back_to_latest() {
        let mut q = test_queue();
        q.push(user_msg("a"));
        q.push(user_msg("b"));
        q.push(user_msg("c"));

        // No clear marker set — should fall back to get_latest(n).
        let msgs = q.get_after_clear_marker("desktop", 2);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content.as_deref(), Some("b"));
        assert_eq!(msgs[1].content.as_deref(), Some("c"));
    }

    #[test]
    fn unicode_content_roundtrips() {
        let mut q = test_queue();
        q.push(user_msg("Rom\u{00e2}n\u{0103}: Bun\u{0103} ziua!"));
        q.push(user_msg("\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30b9}\u{30c8}"));
        q.push(user_msg("\u{1f600} emoji test \u{1f680}"));

        let msgs = q.get_latest(3);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content.as_deref(), Some("Rom\u{00e2}n\u{0103}: Bun\u{0103} ziua!"));
        assert_eq!(msgs[1].content.as_deref(), Some("\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30b9}\u{30c8}"));
        assert_eq!(msgs[2].content.as_deref(), Some("\u{1f600} emoji test \u{1f680}"));
    }

    #[test]
    fn get_from_channel_respects_limit() {
        let mut q = test_queue();
        for i in 0..20 {
            q.push(QueuedMessage {
                role: Role::User,
                channel: ChannelKind::Signal,
                content: Some(format!("signal {i}")),
                ..Default::default()
            });
        }

        let msgs = q.get_from_channel("signal", 5);
        assert_eq!(msgs.len(), 5);
        // Should be the last 5 messages in chronological order.
        assert_eq!(msgs[0].content.as_deref(), Some("signal 15"));
        assert_eq!(msgs[4].content.as_deref(), Some("signal 19"));
    }

    #[test]
    fn get_before_respects_limit() {
        let mut q = test_queue();
        for i in 0..10 {
            q.push(user_msg(&format!("msg {i}")));
        }
        let id_last = q.push(user_msg("msg 10"));

        // Request only 3 before the last message.
        let msgs = q.get_before(id_last, 3);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content.as_deref(), Some("msg 7"));
        assert_eq!(msgs[1].content.as_deref(), Some("msg 8"));
        assert_eq!(msgs[2].content.as_deref(), Some("msg 9"));
    }

    #[test]
    fn get_on_date_today_finds_messages() {
        let mut q = test_queue();
        q.push(user_msg("today's message"));

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let msgs = q.get_on_date(&today);
        assert!(msgs.len() >= 1);
        assert!(msgs.iter().any(|m| m.content.as_deref() == Some("today's message")));
    }

    #[test]
    fn get_between_wide_range_finds_all() {
        let mut q = test_queue();
        q.push(user_msg("in range"));

        let msgs = q.get_between("2000-01-01T00:00:00+00:00", "2099-12-31T23:59:59+00:00");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_deref(), Some("in range"));
    }

    #[test]
    fn conversion_with_none_tool_calls_gives_empty_vec() {
        let qm = QueuedMessage {
            role: Role::User,
            content: Some("hello".into()),
            tool_calls: None,
            ..Default::default()
        };
        let msg: crate::types::Message = (&qm).into();
        assert!(msg.tool_calls.is_empty());
    }

    #[test]
    fn conversion_preserves_tool_call_id() {
        let qm = QueuedMessage {
            role: Role::Tool,
            content: Some("result data".into()),
            tool_call_id: Some("tc-abc-123".into()),
            ..Default::default()
        };
        let msg: crate::types::Message = (&qm).into();
        assert_eq!(msg.tool_call_id.as_deref(), Some("tc-abc-123"));
    }

    #[test]
    fn search_with_common_prefix_wildcard() {
        let mut q = test_queue();
        q.push(user_msg("alpha test"));
        q.push(user_msg("alphanumeric code"));
        q.push(user_msg("beta test"));

        // FTS5 prefix search: "alpha*" should match "alpha" and "alphanumeric".
        let results = q.search("alpha*");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn get_from_channel_chronological_order() {
        let mut q = test_queue();
        for i in 0..5 {
            q.push(QueuedMessage {
                role: Role::User,
                channel: ChannelKind::Web,
                content: Some(format!("web {i}")),
                ..Default::default()
            });
        }

        let msgs = q.get_from_channel("web", 5);
        assert_eq!(msgs.len(), 5);
        for (i, m) in msgs.iter().enumerate() {
            assert_eq!(m.content.as_deref(), Some(format!("web {i}").as_str()));
        }
    }

    #[test]
    fn count_after_clear_still_includes_all() {
        let mut q = test_queue();
        q.push(user_msg("one"));
        q.push(user_msg("two"));
        q.clear("desktop");
        q.push(user_msg("three"));

        // count() returns total messages in DB, not affected by clear markers.
        assert_eq!(q.count(), 3);
    }

    #[test]
    fn get_llm_context_zero_limit_returns_empty() {
        let mut q = test_queue();
        q.push(user_msg("hello"));
        q.push(assistant_msg("hi"));

        let ctx = q.get_llm_context(0);
        assert!(ctx.is_empty());
    }

    #[test]
    fn stress_search_after_many_inserts() {
        let mut q = test_queue();
        for i in 0..500 {
            q.push(user_msg(&format!("iteration number {i} of the benchmark")));
        }
        // One unique message.
        q.push(user_msg("the unicorn appears at midnight"));

        let results = q.search("unicorn");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content.as_deref(), Some("the unicorn appears at midnight"));
    }

    #[test]
    fn per_channel_clear_web_preserves_desktop_in_get_after_clear_marker() {
        let mut q = test_queue();
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("desktop msg".into()),
            ..Default::default()
        });
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            content: Some("web msg".into()),
            ..Default::default()
        });

        // Clear only web.
        q.clear("web");

        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            content: Some("web new".into()),
            ..Default::default()
        });

        // Web should only see post-clear.
        let web = q.get_after_clear_marker("web", 50);
        assert_eq!(web.len(), 1);
        assert_eq!(web[0].content.as_deref(), Some("web new"));

        // Desktop should see all its messages (no clear marker for desktop).
        let desktop = q.get_after_clear_marker("desktop", 50);
        assert!(desktop.iter().any(|m| m.content.as_deref() == Some("desktop msg")));
    }

    #[test]
    fn multiple_subscribers_clear_all_events() {
        let mut q = test_queue();
        let mut rx1 = q.subscribe();
        let mut rx2 = q.subscribe();

        q.clear_all();

        for rx in [&mut rx1, &mut rx2] {
            match rx.try_recv().unwrap() {
                QueueEvent::ClearMarker(channel) => assert_eq!(channel, "all"),
                _ => panic!("Expected ClearMarker('all') event"),
            }
        }
    }

    #[test]
    fn export_json_none_fields_are_null() {
        let mut q = test_queue();
        q.push(QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Desktop,
            content: Some("minimal".into()),
            source: None,
            level: None,
            tool_calls: None,
            tool_call_id: None,
            metadata: None,
            ..Default::default()
        });

        let mut buf = Vec::new();
        q.export_json(&mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        let m = &json.as_array().unwrap()[0];
        assert!(m["source"].is_null());
        assert!(m["level"].is_null());
        assert!(m["tool_calls"].is_null());
        assert!(m["tool_call_id"].is_null());
        assert!(m["metadata"].is_null());
    }

    #[test]
    fn get_latest_on_empty_returns_empty() {
        let q = test_queue();
        assert!(q.get_latest(100).is_empty());
    }

    #[test]
    fn get_before_large_id_returns_all() {
        let mut q = test_queue();
        q.push(user_msg("a"));
        q.push(user_msg("b"));

        // id=999999 is far beyond anything in the DB.
        let msgs = q.get_before(999999, 100);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content.as_deref(), Some("a"));
        assert_eq!(msgs[1].content.as_deref(), Some("b"));
    }
}
