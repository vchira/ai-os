//! Conversation history tool — search and browse persistent message history.
//!
//! The AI can use this tool to look up past conversations, search by keyword,
//! and browse messages by date or channel.

use std::sync::Arc;

use aios_core::queue::MessageQueue;
use aios_core::types::ToolResult;
use std::sync::Mutex;

use crate::tool::Tool;

/// Tool for searching and browsing the persistent conversation history.
pub struct ConversationHistoryTool {
    queue: Arc<Mutex<MessageQueue>>,
}

impl ConversationHistoryTool {
    /// Create a new conversation history tool with a reference to the message queue.
    pub fn new(queue: Arc<Mutex<MessageQueue>>) -> Self {
        Self { queue }
    }

    fn format_messages(msgs: &[aios_core::queue::QueuedMessage]) -> String {
        if msgs.is_empty() {
            return "No messages found.".to_string();
        }

        let mut lines = Vec::with_capacity(msgs.len());
        for m in msgs {
            let ts = m.timestamp.format("%Y-%m-%d %H:%M");
            let role = m.role.to_string();
            let channel = m.channel.to_string();
            let content = m.content.as_deref().unwrap_or("[no content]");
            // Truncate long messages for display.
            let display = if content.len() > 200 {
                format!("{}...", &content[..200])
            } else {
                content.to_string()
            };
            lines.push(format!("[{ts}] [{channel}] {role}: {display}"));
        }
        lines.join("\n")
    }
}

impl Tool for ConversationHistoryTool {
    fn name(&self) -> &str {
        "conversation_history"
    }

    fn description(&self) -> &str {
        "Search and browse past conversations from the persistent message queue. \
         Actions: search, recent, on_date, between, from_channel, stats."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "recent", "on_date", "between", "from_channel", "stats"],
                    "description": "The action to perform"
                },
                "query": {
                    "type": "string",
                    "description": "Search text (for 'search' action)"
                },
                "n": {
                    "type": "integer",
                    "description": "Number of messages to return (default: 20)"
                },
                "date": {
                    "type": "string",
                    "description": "Date in YYYY-MM-DD format (for 'on_date' action)"
                },
                "from": {
                    "type": "string",
                    "description": "Start date (for 'between' action)"
                },
                "to": {
                    "type": "string",
                    "description": "End date (for 'between' action)"
                },
                "channel": {
                    "type": "string",
                    "description": "Channel name: desktop, web, signal (for 'from_channel' action)"
                }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> &str {
        "memory"
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args["action"].as_str().unwrap_or("recent");
        let n = args["n"].as_u64().unwrap_or(20) as usize;

        // We need to block on the async RwLock. Since tools run synchronously,
        // use try_read to avoid blocking.
        let queue = match self.queue.try_lock() {
            Ok(q) => q,
            Err(_) => return ToolResult::fail("Message queue is busy, try again"),
        };

        match action {
            "search" => {
                let query = match args["query"].as_str() {
                    Some(q) if !q.is_empty() => q,
                    _ => return ToolResult::fail("'query' parameter is required for search"),
                };
                let msgs = queue.search(query);
                ToolResult::ok(Self::format_messages(&msgs))
            }

            "recent" => {
                let msgs = queue.get_latest(n);
                ToolResult::ok(Self::format_messages(&msgs))
            }

            "on_date" => {
                let date = match args["date"].as_str() {
                    Some(d) => d,
                    None => return ToolResult::fail("'date' parameter required (YYYY-MM-DD)"),
                };
                let msgs = queue.get_on_date(date);
                ToolResult::ok(Self::format_messages(&msgs))
            }

            "between" => {
                let from = match args["from"].as_str() {
                    Some(f) => f,
                    None => return ToolResult::fail("'from' parameter required"),
                };
                let to = match args["to"].as_str() {
                    Some(t) => t,
                    None => return ToolResult::fail("'to' parameter required"),
                };
                let msgs = queue.get_between(from, to);
                ToolResult::ok(Self::format_messages(&msgs))
            }

            "from_channel" => {
                let channel = match args["channel"].as_str() {
                    Some(c) => c,
                    None => return ToolResult::fail("'channel' parameter required"),
                };
                let msgs = queue.get_from_channel(channel, n);
                ToolResult::ok(Self::format_messages(&msgs))
            }

            "stats" => {
                let total = queue.count();
                let output = format!("Total messages: {total}");
                ToolResult::ok(output)
            }

            _ => ToolResult::fail(format!("Unknown action: {action}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aios_core::channel::ChannelKind;
    use aios_core::queue::{MessageQueue, QueuedMessage};
    use aios_core::types::Role;
    use std::sync::{Arc, Mutex};

    /// Create a test tool backed by an in-memory message queue.
    fn test_tool() -> (ConversationHistoryTool, Arc<Mutex<MessageQueue>>) {
        let queue = MessageQueue::open_in_memory().unwrap();
        let queue = Arc::new(Mutex::new(queue));
        let tool = ConversationHistoryTool::new(Arc::clone(&queue));
        (tool, queue)
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

    fn web_msg(content: &str) -> QueuedMessage {
        QueuedMessage {
            role: Role::User,
            channel: ChannelKind::Web,
            source: Some("user".into()),
            content: Some(content.into()),
            ..Default::default()
        }
    }

    #[test]
    fn search_with_no_results() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "search",
            "query": "nonexistent topic"
        }));
        assert!(r.success);
        assert_eq!(r.output, "No messages found.");
    }

    #[test]
    fn recent_returns_latest_messages() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("first"));
            queue.push(user_msg("second"));
            queue.push(user_msg("third"));
        }

        let r = tool.execute(serde_json::json!({
            "action": "recent",
            "n": 2
        }));
        assert!(r.success);
        assert!(r.output.contains("second"));
        assert!(r.output.contains("third"));
        assert!(!r.output.contains("first"));
    }

    #[test]
    fn stats_shows_correct_count() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("one"));
            queue.push(user_msg("two"));
            queue.push(user_msg("three"));
        }

        let r = tool.execute(serde_json::json!({ "action": "stats" }));
        assert!(r.success);
        assert!(r.output.contains("Total messages: 3"));
    }

    #[test]
    fn on_date_with_no_messages() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "on_date",
            "date": "2000-01-01"
        }));
        assert!(r.success);
        assert_eq!(r.output, "No messages found.");
    }

    #[test]
    fn from_channel_filters_correctly() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("desktop message"));
            queue.push(web_msg("web message"));
        }

        let r = tool.execute(serde_json::json!({
            "action": "from_channel",
            "channel": "desktop"
        }));
        assert!(r.success);
        assert!(r.output.contains("desktop message"));
        assert!(!r.output.contains("web message"));
    }

    #[test]
    fn between_date_range() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("today message"));
        }

        // Use a range that covers all time — should find the message.
        let r = tool.execute(serde_json::json!({
            "action": "between",
            "from": "2000-01-01",
            "to": "2099-12-31"
        }));
        assert!(r.success);
        assert!(r.output.contains("today message"));

        // Use a range in the past — should find nothing.
        let r = tool.execute(serde_json::json!({
            "action": "between",
            "from": "1990-01-01",
            "to": "1990-12-31"
        }));
        assert!(r.success);
        assert_eq!(r.output, "No messages found.");
    }

    #[test]
    fn unknown_action_fails() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "destroy_all" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown action"));
    }

    #[test]
    fn search_missing_query_fails() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "search" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("query"));
    }

    #[test]
    fn search_empty_query_fails() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "search",
            "query": ""
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("query"));
    }

    #[test]
    fn on_date_missing_date_fails() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "on_date" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("date"));
    }

    #[test]
    fn between_missing_from_fails() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "between",
            "to": "2026-12-31"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("from"));
    }

    #[test]
    fn between_missing_to_fails() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "between",
            "from": "2026-01-01"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("to"));
    }

    #[test]
    fn from_channel_missing_channel_fails() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "from_channel" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("channel"));
    }

    #[test]
    fn tool_name_and_category() {
        let (tool, _q) = test_tool();
        assert_eq!(tool.name(), "conversation_history");
        assert_eq!(tool.category(), "memory");
    }

    #[test]
    fn recent_empty_queue() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "recent" }));
        assert!(r.success);
        assert_eq!(r.output, "No messages found.");
    }

    #[test]
    fn stats_empty_queue() {
        let (tool, _q) = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "stats" }));
        assert!(r.success);
        assert!(r.output.contains("Total messages: 0"));
    }

    #[test]
    fn format_messages_truncates_long_content() {
        let long_content = "x".repeat(300);
        let msg = QueuedMessage {
            content: Some(long_content),
            ..Default::default()
        };
        let formatted = ConversationHistoryTool::format_messages(&[msg]);
        assert!(formatted.contains("..."));
        // Should not contain the full 300 chars — truncated to 200 + "...".
        assert!(formatted.len() < 350);
    }

    #[test]
    fn search_with_valid_query_returns_results() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("rust programming language"));
            queue.push(user_msg("python is great"));
            queue.push(user_msg("rust is fast"));
        }

        let r = tool.execute(serde_json::json!({
            "action": "search",
            "query": "rust"
        }));
        assert!(r.success);
        assert!(r.output.contains("rust"));
    }

    #[test]
    fn recent_with_default_n() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            for i in 0..25 {
                queue.push(user_msg(&format!("message {i}")));
            }
        }

        // Default n is 20, so only the last 20 messages should be returned.
        let r = tool.execute(serde_json::json!({ "action": "recent" }));
        assert!(r.success);
        // message 0..4 should be excluded; message 5..24 included.
        assert!(r.output.contains("message 24"));
        assert!(r.output.contains("message 5"));
        assert!(!r.output.contains("message 0\n") || !r.output.contains("[desktop] user: message 0\n"));
    }

    #[test]
    fn recent_with_custom_n() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("alpha"));
            queue.push(user_msg("beta"));
            queue.push(user_msg("gamma"));
        }

        let r = tool.execute(serde_json::json!({
            "action": "recent",
            "n": 1
        }));
        assert!(r.success);
        assert!(r.output.contains("gamma"));
        assert!(!r.output.contains("alpha"));
        assert!(!r.output.contains("beta"));
    }

    #[test]
    fn on_date_with_valid_date_returns_messages() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("today's message"));
        }

        // Use today's date to find the message we just pushed.
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let r = tool.execute(serde_json::json!({
            "action": "on_date",
            "date": today
        }));
        assert!(r.success);
        assert!(r.output.contains("today's message"));
    }

    #[test]
    fn on_date_with_invalid_date_returns_empty() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("some message"));
        }

        let r = tool.execute(serde_json::json!({
            "action": "on_date",
            "date": "9999-99-99"
        }));
        assert!(r.success);
        assert_eq!(r.output, "No messages found.");
    }

    #[test]
    fn from_channel_with_web_channel() {
        let (tool, q) = test_tool();
        {
            let mut queue = q.lock().unwrap();
            queue.push(user_msg("desktop only"));
            queue.push(web_msg("web only"));
        }

        let r = tool.execute(serde_json::json!({
            "action": "from_channel",
            "channel": "web"
        }));
        assert!(r.success);
        assert!(r.output.contains("web only"));
        assert!(!r.output.contains("desktop only"));
    }

    #[test]
    fn format_messages_empty_returns_no_messages() {
        let formatted = ConversationHistoryTool::format_messages(&[]);
        assert_eq!(formatted, "No messages found.");
    }

    #[test]
    fn format_messages_no_content_shows_placeholder() {
        let msg = QueuedMessage {
            content: None,
            ..Default::default()
        };
        let formatted = ConversationHistoryTool::format_messages(&[msg]);
        assert!(formatted.contains("[no content]"));
    }
}
