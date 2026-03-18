//! Context pruning via recursive summarization.
//!
//! When conversation history grows beyond a configurable token threshold,
//! [`ContextManager`] triggers summarization of older messages into a compact
//! "state memo".  This keeps the context window manageable while preserving
//! essential information from earlier in the conversation.

use aios_core::types::{Message, Role};

/// Default token threshold before pruning triggers.
const DEFAULT_PRUNE_THRESHOLD: usize = 10_000;

/// Default number of estimated tokens to summarize into a memo.
const DEFAULT_PRUNE_WINDOW: usize = 8_000;

/// Default maximum context size (matches typical model limits).
const DEFAULT_MAX_TOKENS: usize = 100_000;

/// Rough estimate: 4 characters per token (conservative average for English).
const CHARS_PER_TOKEN: usize = 4;

/// Manages conversation context length via recursive summarization.
///
/// When the estimated token count of a message list exceeds
/// [`prune_threshold`](Self::prune_threshold), the manager generates a
/// summarization prompt and collapses old messages into a single
/// `[Context Summary]` system message.
pub struct ContextManager {
    /// Maximum tokens the context may contain.
    max_tokens: usize,
    /// Token threshold to start summarization.
    prune_threshold: usize,
    /// How many estimated tokens of old messages to summarize.
    prune_window: usize,
}

impl ContextManager {
    /// Create a new context manager with the given maximum token budget.
    ///
    /// Uses default values for `prune_threshold` (10,000) and
    /// `prune_window` (8,000).
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            prune_threshold: DEFAULT_PRUNE_THRESHOLD,
            prune_window: DEFAULT_PRUNE_WINDOW,
        }
    }

    /// Create a context manager with fully custom parameters.
    pub fn with_config(max_tokens: usize, prune_threshold: usize, prune_window: usize) -> Self {
        Self {
            max_tokens,
            prune_threshold,
            prune_window,
        }
    }

    /// Return the maximum token budget.
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// Return the pruning threshold (summarization starts when exceeded).
    pub fn prune_threshold(&self) -> usize {
        self.prune_threshold
    }

    /// Return the prune window size (how many tokens of old messages to summarize).
    pub fn prune_window(&self) -> usize {
        self.prune_window
    }

    /// Estimate the token count for a list of messages.
    ///
    /// Uses a rough heuristic: 4 characters = 1 token.  This is not precise
    /// but is fast and sufficient for deciding when to prune.
    pub fn estimate_tokens(messages: &[Message]) -> usize {
        let total_chars: usize = messages
            .iter()
            .map(|m| {
                let content_len = m.content.as_deref().map_or(0, |c| c.len());
                let tool_calls_len: usize = m
                    .tool_calls
                    .iter()
                    .map(|tc| tc.name.len() + tc.arguments.to_string().len())
                    .sum();
                content_len + tool_calls_len
            })
            .sum();
        total_chars / CHARS_PER_TOKEN
    }

    /// Check if pruning is needed for the given message list.
    pub fn needs_pruning(&self, messages: &[Message]) -> bool {
        Self::estimate_tokens(messages) > self.prune_threshold
    }

    /// Prune the conversation by replacing old messages with a summary.
    ///
    /// The `summary` parameter is the LLM-generated summary of the old
    /// messages.  This method finds the split point (messages whose
    /// cumulative tokens fit in `prune_window`), replaces them with a
    /// single `[Context Summary]` system message, and returns the new
    /// message list.
    ///
    /// If the summary is empty, the old messages are simply dropped.
    pub fn prune(&self, messages: &[Message], summary: &str) -> Vec<Message> {
        let mut cumulative_tokens: usize = 0;
        let mut split_index: usize = 0;

        for (i, msg) in messages.iter().enumerate() {
            let msg_tokens = Self::estimate_tokens(&[msg.clone()]);
            cumulative_tokens += msg_tokens;
            if cumulative_tokens >= self.prune_window {
                split_index = i + 1;
                break;
            }
            split_index = i + 1;
        }

        // Don't prune if we'd remove everything.
        if split_index >= messages.len() {
            return messages.to_vec();
        }

        let mut pruned = Vec::with_capacity(messages.len() - split_index + 1);

        // Insert the summary as a system message.
        if !summary.is_empty() {
            pruned.push(Message::system(format!("[Context Summary] {summary}")));
        }

        // Keep the remaining (newer) messages.
        pruned.extend_from_slice(&messages[split_index..]);

        pruned
    }

    /// Generate the summarization prompt for the LLM.
    ///
    /// This returns a prompt that asks the AI to summarize the conversation
    /// so far into a compact state memo.  The caller should send this prompt
    /// to the LLM and use the response as the `summary` argument to
    /// [`prune()`](Self::prune).
    pub fn summarization_prompt(messages: &[Message]) -> String {
        let mut conversation_text = String::new();

        for msg in messages {
            let role_label = match msg.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };

            if let Some(content) = &msg.content {
                // Truncate very long messages for the summary prompt.
                let truncated: String = content.chars().take(500).collect();
                let suffix = if content.len() > 500 { "..." } else { "" };
                conversation_text.push_str(&format!("{role_label}: {truncated}{suffix}\n"));
            }

            for tc in &msg.tool_calls {
                conversation_text.push_str(&format!(
                    "  [Tool call: {} with {}]\n",
                    tc.name,
                    tc.arguments.to_string().chars().take(200).collect::<String>(),
                ));
            }
        }

        format!(
            "Summarize the following conversation into a concise state memo. \
             Preserve all important facts, decisions, and context that would be \
             needed to continue the conversation naturally. Be concise but complete. \
             Focus on what was discussed, what was decided, and what state the \
             system is in.\n\n\
             --- Conversation ---\n\
             {conversation_text}\
             --- End ---\n\n\
             State memo:"
        )
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_TOKENS)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(ContextManager::estimate_tokens(&[]), 0);
    }

    #[test]
    fn estimate_tokens_basic() {
        let messages = vec![
            Message::user("Hello world"),        // 11 chars -> ~2 tokens
            Message::assistant("Hi there!"),      // 9 chars -> ~2 tokens
        ];
        let tokens = ContextManager::estimate_tokens(&messages);
        assert_eq!(tokens, (11 + 9) / CHARS_PER_TOKEN);
    }

    #[test]
    fn estimate_tokens_with_tool_calls() {
        let messages = vec![Message::assistant_with_tools(
            Some("thinking...".into()),
            vec![aios_core::types::ToolCall {
                id: "tc-1".into(),
                name: "memory".into(),
                arguments: serde_json::json!({"key": "name", "value": "Alice"}),
            }],
        )];
        let tokens = ContextManager::estimate_tokens(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn needs_pruning_below_threshold() {
        let cm = ContextManager::new(100_000);
        let messages = vec![Message::user("short message")];
        assert!(!cm.needs_pruning(&messages));
    }

    #[test]
    fn needs_pruning_above_threshold() {
        let cm = ContextManager::with_config(100_000, 10, 5);
        // Create a message with > 40 chars (10 tokens * 4 chars/token)
        let messages = vec![Message::user("a".repeat(100))];
        assert!(cm.needs_pruning(&messages));
    }

    #[test]
    fn prune_replaces_old_with_summary() {
        // Each message is 100 chars = 25 tokens.
        // prune_window=60 means we summarize up to 60 tokens worth of
        // messages.  After 2 messages (50 tokens) the 3rd crosses 60,
        // so split_index=3, leaving messages[3..] in place.
        let cm = ContextManager::with_config(100_000, 100, 60);

        let messages = vec![
            Message::user("a".repeat(100)),         // 25 tokens, cumul 25
            Message::assistant("b".repeat(100)),     // 25 tokens, cumul 50
            Message::user("c".repeat(100)),          // 25 tokens, cumul 75 >= 60
            Message::assistant("d".repeat(100)),     // kept
            Message::user("recent question"),        // kept
            Message::assistant("recent answer"),     // kept
        ];

        let pruned = cm.prune(&messages, "Summary of earlier conversation.");
        // Should start with the summary message.
        assert_eq!(pruned[0].role, Role::System);
        assert!(pruned[0]
            .content
            .as_deref()
            .unwrap()
            .contains("[Context Summary]"));
        assert!(pruned[0]
            .content
            .as_deref()
            .unwrap()
            .contains("Summary of earlier conversation."));
        // 1 summary + 3 remaining = 4 messages, less than original 6
        assert!(
            pruned.len() < messages.len(),
            "pruned.len()={} should be < messages.len()={}",
            pruned.len(),
            messages.len(),
        );
        assert_eq!(pruned.len(), 4); // summary + messages[3..5]
    }

    #[test]
    fn prune_empty_summary_drops_messages() {
        let cm = ContextManager::with_config(100_000, 100, 20);
        let messages = vec![
            Message::user("a".repeat(100)),
            Message::assistant("recent"),
        ];

        let pruned = cm.prune(&messages, "");
        // No summary message should be inserted.
        assert!(pruned.iter().all(|m| m.role != Role::System));
    }

    #[test]
    fn prune_preserves_all_when_short() {
        let cm = ContextManager::with_config(100_000, 100, 100_000);
        let messages = vec![
            Message::user("hello"),
            Message::assistant("hi"),
        ];

        // Window is larger than the messages, so nothing should be pruned.
        let pruned = cm.prune(&messages, "summary");
        assert_eq!(pruned.len(), messages.len());
    }

    #[test]
    fn summarization_prompt_includes_messages() {
        let messages = vec![
            Message::user("What is the weather?"),
            Message::assistant("I'll check for you."),
        ];
        let prompt = ContextManager::summarization_prompt(&messages);
        assert!(prompt.contains("User: What is the weather?"));
        assert!(prompt.contains("Assistant: I'll check for you."));
        assert!(prompt.contains("State memo:"));
    }

    #[test]
    fn summarization_prompt_truncates_long_messages() {
        let long_msg = "x".repeat(1000);
        let messages = vec![Message::user(&long_msg)];
        let prompt = ContextManager::summarization_prompt(&messages);
        assert!(prompt.contains("..."));
        // The truncated version should be 500 chars + "..."
        assert!(prompt.len() < long_msg.len() + 200);
    }

    #[test]
    fn default_config() {
        let cm = ContextManager::default();
        assert_eq!(cm.max_tokens(), DEFAULT_MAX_TOKENS);
        assert_eq!(cm.prune_threshold(), DEFAULT_PRUNE_THRESHOLD);
        assert_eq!(cm.prune_window(), DEFAULT_PRUNE_WINDOW);
    }

    #[test]
    fn custom_config() {
        let cm = ContextManager::with_config(50_000, 5_000, 3_000);
        assert_eq!(cm.max_tokens(), 50_000);
        assert_eq!(cm.prune_threshold(), 5_000);
        assert_eq!(cm.prune_window(), 3_000);
    }

    // -- Additional tests --

    #[test]
    fn empty_history_returns_empty() {
        let cm = ContextManager::with_config(100_000, 10, 5);
        // Pruning empty messages should return empty.
        let pruned = cm.prune(&[], "some summary");
        assert!(pruned.is_empty());
    }

    #[test]
    fn short_history_not_pruned() {
        let cm = ContextManager::default();
        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi there!"),
            Message::user("How are you?"),
            Message::assistant("I'm doing well."),
        ];

        // Short history should be well under the default prune threshold (10K tokens).
        assert!(!cm.needs_pruning(&messages));

        // Pruning should preserve all messages (prune_window is large).
        let pruned = cm.prune(&messages, "This summary should not appear");
        assert_eq!(pruned.len(), messages.len());
        assert_eq!(
            pruned[0].content.as_deref(),
            messages[0].content.as_deref(),
        );
    }

    #[test]
    fn long_history_gets_summarized() {
        // Use large messages (400 chars = 100 tokens each) and a small prune_window (150).
        // The prune loop will accumulate: msg0=100, msg1=200 >= 150 → split at index 2.
        // Remaining: 5 - 2 = 3 messages + 1 summary = 4 < 5.
        let cm = ContextManager::with_config(100_000, 5, 150);

        let messages = vec![
            Message::user("a".repeat(400)),         // ~100 tokens
            Message::assistant("b".repeat(400)),     // ~100 tokens
            Message::user("c".repeat(400)),          // kept
            Message::assistant("d".repeat(400)),     // kept
            Message::user("recent question here"),   // kept
        ];

        // Total tokens: ~100 * 4 + 5 = ~405 >> 5 threshold.
        assert!(cm.needs_pruning(&messages));

        // Prune with a mock summary.
        let summary = "User discussed topics a, b with the assistant.";
        let pruned = cm.prune(&messages, summary);

        // Should be shorter than original: 1 summary + 3 kept = 4 < 5.
        assert!(
            pruned.len() < messages.len(),
            "Pruned ({}) should be shorter than original ({})",
            pruned.len(),
            messages.len(),
        );

        // First message should be the context summary.
        assert_eq!(pruned[0].role, Role::System);
        let content = pruned[0].content.as_deref().unwrap();
        assert!(content.contains("[Context Summary]"));
        assert!(content.contains(summary));

        // The last (recent) message should still be present.
        let last_pruned = pruned.last().unwrap();
        assert_eq!(last_pruned.role, Role::User);
        assert_eq!(
            last_pruned.content.as_deref().unwrap(),
            "recent question here",
        );
    }

    // -----------------------------------------------------------------------
    // Prune with short history returns unchanged
    // -----------------------------------------------------------------------

    #[test]
    fn prune_short_history_returns_unchanged() {
        let cm = ContextManager::with_config(100_000, 10_000, 8_000);
        let messages = vec![
            Message::user("Hi"),
            Message::assistant("Hello!"),
        ];
        // Total tokens ~4, well below prune threshold.
        assert!(!cm.needs_pruning(&messages));
        let pruned = cm.prune(&messages, "A summary");
        // Messages are shorter than prune_window, so all are returned as-is.
        assert_eq!(pruned.len(), messages.len());
        for (p, m) in pruned.iter().zip(messages.iter()) {
            assert_eq!(p.content.as_deref(), m.content.as_deref());
        }
    }

    // -----------------------------------------------------------------------
    // Prune with long history summarizes (boundary check)
    // -----------------------------------------------------------------------

    #[test]
    fn prune_long_history_replaces_beginning_with_summary() {
        // prune_window = 30 tokens means roughly 120 chars of content.
        let cm = ContextManager::with_config(100_000, 10, 30);
        let messages = vec![
            Message::user("a".repeat(80)),        // ~20 tokens
            Message::assistant("b".repeat(80)),    // ~20 tokens, cumul 40 >= 30
            Message::user("c".repeat(40)),         // kept
            Message::assistant("d".repeat(40)),    // kept
        ];
        let pruned = cm.prune(&messages, "Summary of a and b.");
        // Summary + 2 remaining messages.
        assert!(pruned.len() < messages.len());
        assert_eq!(pruned[0].role, Role::System);
        assert!(pruned[0].content.as_deref().unwrap().contains("Summary of a and b."));
    }

    // -----------------------------------------------------------------------
    // Token counting works correctly
    // -----------------------------------------------------------------------

    #[test]
    fn token_counting_single_message() {
        // 100 chars / 4 = 25 tokens.
        let messages = vec![Message::user("a".repeat(100))];
        assert_eq!(ContextManager::estimate_tokens(&messages), 25);
    }

    #[test]
    fn token_counting_multiple_messages() {
        let messages = vec![
            Message::user("a".repeat(40)),      // 10 tokens
            Message::assistant("b".repeat(60)),  // 15 tokens
        ];
        assert_eq!(ContextManager::estimate_tokens(&messages), 25);
    }

    #[test]
    fn token_counting_empty_content() {
        // A message with no content should contribute 0 tokens.
        let messages = vec![Message {
            role: Role::System,
            content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }];
        assert_eq!(ContextManager::estimate_tokens(&messages), 0);
    }

    // -----------------------------------------------------------------------
    // Empty history returns empty
    // -----------------------------------------------------------------------

    #[test]
    fn empty_history_returns_empty_vec() {
        let cm = ContextManager::default();
        let pruned = cm.prune(&[], "some summary");
        assert!(pruned.is_empty());
    }

    #[test]
    fn empty_history_estimate_tokens_is_zero() {
        assert_eq!(ContextManager::estimate_tokens(&[]), 0);
    }

    #[test]
    fn empty_history_does_not_need_pruning() {
        let cm = ContextManager::default();
        assert!(!cm.needs_pruning(&[]));
    }

    #[test]
    fn summarization_prompt_on_empty_history() {
        let prompt = ContextManager::summarization_prompt(&[]);
        // Should still produce a valid prompt with no conversation lines.
        assert!(prompt.contains("Summarize the following conversation"));
        assert!(prompt.contains("State memo:"));
    }
}
