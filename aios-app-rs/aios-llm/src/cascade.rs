//! Cascading model routing with quality-mode-aware escalation.
//!
//! Instead of committing to a single effort level up front, the cascade
//! router can start with a cheaper tier and escalate to a more capable
//! (and expensive) model when quality signals indicate the response is
//! inadequate.
//!
//! ## Quality modes
//!
//! The escalation strategy is governed by [`QualityMode`]:
//!
//! | Mode | Starting effort | Escalation triggers |
//! |------|----------------|---------------------|
//! | **Saver** | Always `Low` | Heuristic: short, hedging, decline, errors, tool error, retry |
//! | **Balanced** | Auto-detected | Reliable only: empty, tool error, user retry |
//! | **Thorough** | Always `High` | Never (already at top tier) |
//!
//! The escalation path is: **Low → Medium → High → (give up)**.

use aios_core::types::{EffortLevel, LlmResponse, QualityMode};

// ---------------------------------------------------------------------------
// CascadeConfig
// ---------------------------------------------------------------------------

/// Configuration for cascading model routing.
#[derive(Debug, Clone)]
pub struct CascadeConfig {
    /// Whether cascading is enabled (default: `true`).
    pub enabled: bool,
    /// Maximum escalation attempts before giving up (default: 2).
    ///
    /// With a max of 2 the full path Low → Medium → High is possible.
    pub max_escalations: usize,
    /// Minimum response length (in characters) to consider "valid"
    /// (used by Saver mode only).
    pub min_valid_length: usize,
}

impl Default for CascadeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_escalations: 2,
            min_valid_length: 10,
        }
    }
}

// ---------------------------------------------------------------------------
// EscalationReason
// ---------------------------------------------------------------------------

/// Reasons why a response should be escalated to a higher tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscalationReason {
    /// Response was too short or empty.
    TooShort,
    /// Model explicitly said it cannot handle the task.
    ModelDeclined,
    /// Response contains repeated error indicators.
    ErrorDetected,
    /// Tool call failed and needs a smarter model.
    ToolCallFailed,
    /// Confidence is low — the model hedged excessively.
    LowConfidence,
    /// User explicitly requested a retry ("try again", "that's wrong", etc.).
    UserRetry,
}

// ---------------------------------------------------------------------------
// CascadeRouter
// ---------------------------------------------------------------------------

/// Stateless helper that decides starting effort, whether to escalate,
/// and to which tier.
pub struct CascadeRouter;

impl CascadeRouter {
    /// Determine the starting effort level based on quality mode.
    ///
    /// - **Saver**: always starts with `Low`.
    /// - **Balanced**: uses the auto-detected effort level.
    /// - **Thorough**: always starts with `High`.
    pub fn starting_effort(mode: QualityMode, auto_detected: EffortLevel) -> EffortLevel {
        match mode {
            QualityMode::Saver => EffortLevel::Low,
            QualityMode::Balanced => auto_detected,
            QualityMode::Thorough => EffortLevel::High,
        }
    }

    /// Determine if a response should be escalated to a higher tier.
    ///
    /// The decision depends on the active [`QualityMode`]:
    ///
    /// - **Saver**: checks heuristics (short, decline, errors, hedging)
    ///   plus tool errors and user retries.
    /// - **Balanced**: only escalates on empty response, tool errors,
    ///   or explicit user retry.
    /// - **Thorough**: never escalates (already at top tier).
    ///
    /// Returns `Some(reason)` when escalation is warranted and `None`
    /// when the response looks acceptable or we are already at `High`.
    pub fn should_escalate(
        response: &LlmResponse,
        mode: QualityMode,
        effort: EffortLevel,
        tool_error_occurred: bool,
        user_requested_retry: bool,
    ) -> Option<EscalationReason> {
        // Thorough mode never escalates — it starts at the top.
        if mode == QualityMode::Thorough {
            return None;
        }

        // Already at the highest tier — nowhere to escalate to.
        if effort == EffortLevel::High {
            return None;
        }

        // -- Signals common to both Saver and Balanced modes --

        // User explicitly asked to retry.
        if user_requested_retry {
            return Some(EscalationReason::UserRetry);
        }

        // Tool call returned an error.
        if tool_error_occurred {
            return Some(EscalationReason::ToolCallFailed);
        }

        // Empty / None response content.
        let content = match &response.content {
            Some(c) if !c.is_empty() => c.as_str(),
            _ => return Some(EscalationReason::TooShort),
        };

        // -- Saver-only heuristic checks --
        if mode == QualityMode::Saver {
            if let Some(reason) = Self::check_saver_heuristics(content) {
                return Some(reason);
            }
        }

        // Balanced: trust the model for everything else.
        None
    }

    /// Get the next effort level in the escalation chain.
    ///
    /// Returns `None` if `current` is already `High`.
    pub fn escalate(current: EffortLevel) -> Option<EffortLevel> {
        match current {
            EffortLevel::Low => Some(EffortLevel::Medium),
            EffortLevel::Medium => Some(EffortLevel::High),
            EffortLevel::High => None,
        }
    }

    /// Detect whether a user message is a retry request.
    ///
    /// Looks for phrases like "try again", "that's wrong", "not right",
    /// "redo", "incorrect", "wrong answer", "bad answer".
    pub fn is_retry_request(message: &str) -> bool {
        let lower = message.to_lowercase();
        let retry_phrases = [
            "try again",
            "that's wrong",
            "that is wrong",
            "not right",
            "redo",
            "incorrect",
            "wrong answer",
            "bad answer",
        ];
        retry_phrases.iter().any(|p| lower.contains(p))
    }

    /// Heuristic quality checks used only in Saver mode.
    fn check_saver_heuristics(content: &str) -> Option<EscalationReason> {
        // Too short — likely confused or incomplete.
        if content.len() < 10 {
            return Some(EscalationReason::TooShort);
        }

        let lower = content.to_lowercase();

        // Model explicitly declined.
        let decline_phrases = [
            "i cannot",
            "i'm not able",
            "i'm unable",
            "i don't have the capability",
        ];
        if decline_phrases.iter().any(|p| lower.contains(p)) {
            return Some(EscalationReason::ModelDeclined);
        }

        // Repeated error indicators.
        let error_count = lower.matches("error").count()
            + lower.matches("failed").count();
        if error_count >= 3 {
            return Some(EscalationReason::ErrorDetected);
        }

        // Excessive hedging — low confidence.
        let hedge_phrases = [
            "i think maybe",
            "i'm not sure but",
            "possibly",
        ];
        let hedge_count: usize = hedge_phrases
            .iter()
            .map(|p| lower.matches(p).count())
            .sum();
        if hedge_count >= 3 {
            return Some(EscalationReason::LowConfidence);
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aios_core::types::{LlmResponse, Usage};

    /// Helper to build a simple text response.
    fn text_response(content: &str) -> LlmResponse {
        LlmResponse {
            content: Some(content.to_string()),
            tool_calls: Vec::new(),
            usage: Usage::default(),
        }
    }

    /// Helper to build a response with no content.
    fn empty_response() -> LlmResponse {
        LlmResponse {
            content: None,
            tool_calls: Vec::new(),
            usage: Usage::default(),
        }
    }

    // -- CascadeConfig defaults -----------------------------------------------

    #[test]
    fn cascade_config_defaults() {
        let cfg = CascadeConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.max_escalations, 2);
        assert_eq!(cfg.min_valid_length, 10);
    }

    // -- starting_effort ------------------------------------------------------

    #[test]
    fn starting_effort_saver_always_low() {
        assert_eq!(
            CascadeRouter::starting_effort(QualityMode::Saver, EffortLevel::Medium),
            EffortLevel::Low,
        );
        assert_eq!(
            CascadeRouter::starting_effort(QualityMode::Saver, EffortLevel::High),
            EffortLevel::Low,
        );
    }

    #[test]
    fn starting_effort_balanced_uses_auto() {
        assert_eq!(
            CascadeRouter::starting_effort(QualityMode::Balanced, EffortLevel::Low),
            EffortLevel::Low,
        );
        assert_eq!(
            CascadeRouter::starting_effort(QualityMode::Balanced, EffortLevel::Medium),
            EffortLevel::Medium,
        );
        assert_eq!(
            CascadeRouter::starting_effort(QualityMode::Balanced, EffortLevel::High),
            EffortLevel::High,
        );
    }

    #[test]
    fn starting_effort_thorough_always_high() {
        assert_eq!(
            CascadeRouter::starting_effort(QualityMode::Thorough, EffortLevel::Low),
            EffortLevel::High,
        );
        assert_eq!(
            CascadeRouter::starting_effort(QualityMode::Thorough, EffortLevel::Medium),
            EffortLevel::High,
        );
    }

    // -- Escalation path ------------------------------------------------------

    #[test]
    fn escalate_low_to_medium() {
        assert_eq!(CascadeRouter::escalate(EffortLevel::Low), Some(EffortLevel::Medium));
    }

    #[test]
    fn escalate_medium_to_high() {
        assert_eq!(CascadeRouter::escalate(EffortLevel::Medium), Some(EffortLevel::High));
    }

    #[test]
    fn escalate_high_gives_none() {
        assert_eq!(CascadeRouter::escalate(EffortLevel::High), None);
    }

    // -- Thorough mode: never escalates ---------------------------------------

    #[test]
    fn thorough_never_escalates() {
        let resp = text_response("short");
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Thorough, EffortLevel::High, false, false
        ).is_none());
    }

    #[test]
    fn thorough_never_escalates_even_on_empty() {
        let resp = empty_response();
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Thorough, EffortLevel::High, true, true
        ).is_none());
    }

    // -- Already at High: never escalates -------------------------------------

    #[test]
    fn no_escalation_at_high_effort_balanced() {
        let resp = empty_response();
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Balanced, EffortLevel::High, false, false
        ).is_none());
    }

    #[test]
    fn no_escalation_at_high_effort_saver() {
        let resp = text_response("ok");
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::High, false, false
        ).is_none());
    }

    // =========================================================================
    // Balanced mode tests
    // =========================================================================

    #[test]
    fn balanced_escalates_on_user_retry() {
        let resp = text_response("Here is my answer which seems fine.");
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Balanced, EffortLevel::Medium, false, true,
        );
        assert_eq!(reason, Some(EscalationReason::UserRetry));
    }

    #[test]
    fn balanced_escalates_on_tool_error() {
        let resp = text_response("Here is my answer which seems fine.");
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Balanced, EffortLevel::Medium, true, false,
        );
        assert_eq!(reason, Some(EscalationReason::ToolCallFailed));
    }

    #[test]
    fn balanced_escalates_on_empty_response() {
        let resp = empty_response();
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Balanced, EffortLevel::Medium, false, false,
        );
        assert_eq!(reason, Some(EscalationReason::TooShort));
    }

    #[test]
    fn balanced_does_not_escalate_on_short_response() {
        let resp = text_response("Hi there");
        // Balanced trusts the model — short responses are fine.
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Balanced, EffortLevel::Medium, false, false
        ).is_none());
    }

    #[test]
    fn balanced_does_not_escalate_on_hedging() {
        let resp = text_response(
            "I think maybe this is correct. I'm not sure but possibly. Possibly yes."
        );
        // Balanced does NOT escalate on hedging.
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Balanced, EffortLevel::Medium, false, false
        ).is_none());
    }

    #[test]
    fn balanced_does_not_escalate_on_model_decline() {
        let resp = text_response("I cannot help with that right now, but here's an alternative.");
        // Balanced does NOT escalate on model decline.
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Balanced, EffortLevel::Medium, false, false
        ).is_none());
    }

    #[test]
    fn balanced_does_not_escalate_on_good_response() {
        let resp = text_response("The capital of France is Paris. It is a beautiful city.");
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Balanced, EffortLevel::Low, false, false
        ).is_none());
    }

    // =========================================================================
    // Saver mode tests
    // =========================================================================

    #[test]
    fn saver_escalates_on_user_retry() {
        let resp = text_response("Here is my answer which seems fine.");
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, true,
        );
        assert_eq!(reason, Some(EscalationReason::UserRetry));
    }

    #[test]
    fn saver_escalates_on_tool_error() {
        let resp = text_response("Here is my answer.");
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, true, false,
        );
        assert_eq!(reason, Some(EscalationReason::ToolCallFailed));
    }

    #[test]
    fn saver_escalates_on_empty_response() {
        let resp = empty_response();
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, false,
        );
        assert_eq!(reason, Some(EscalationReason::TooShort));
    }

    #[test]
    fn saver_escalates_on_short_content() {
        let resp = text_response("ok");
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, false,
        );
        assert_eq!(reason, Some(EscalationReason::TooShort));
    }

    #[test]
    fn saver_escalates_on_model_declined() {
        let resp = text_response("I cannot help with that kind of request at this time.");
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, false,
        );
        assert_eq!(reason, Some(EscalationReason::ModelDeclined));
    }

    #[test]
    fn saver_escalates_on_im_unable() {
        let resp = text_response("I'm unable to process this complex query right now.");
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, false,
        );
        assert_eq!(reason, Some(EscalationReason::ModelDeclined));
    }

    #[test]
    fn saver_escalates_on_repeated_errors() {
        let resp = text_response(
            "The operation error occurred. Another error happened. The command failed too."
        );
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, false,
        );
        assert_eq!(reason, Some(EscalationReason::ErrorDetected));
    }

    #[test]
    fn saver_does_not_escalate_on_single_error() {
        let resp = text_response("There was an error with that command, but I fixed it.");
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, false,
        ).is_none());
    }

    #[test]
    fn saver_escalates_on_excessive_hedging() {
        let resp = text_response(
            "I think maybe this is correct. I'm not sure but it possibly works. \
             Also possibly the format is right."
        );
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, false,
        );
        assert_eq!(reason, Some(EscalationReason::LowConfidence));
    }

    #[test]
    fn saver_no_escalation_on_good_response() {
        let resp = text_response("The capital of France is Paris. It is a beautiful city.");
        assert!(CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, false, false,
        ).is_none());
    }

    // -- is_retry_request -----------------------------------------------------

    #[test]
    fn retry_detected_try_again() {
        assert!(CascadeRouter::is_retry_request("Please try again"));
    }

    #[test]
    fn retry_detected_thats_wrong() {
        assert!(CascadeRouter::is_retry_request("That's wrong, do it properly"));
    }

    #[test]
    fn retry_detected_that_is_wrong() {
        assert!(CascadeRouter::is_retry_request("That is wrong!"));
    }

    #[test]
    fn retry_detected_not_right() {
        assert!(CascadeRouter::is_retry_request("That's not right"));
    }

    #[test]
    fn retry_detected_redo() {
        assert!(CascadeRouter::is_retry_request("Redo the last operation"));
    }

    #[test]
    fn retry_detected_incorrect() {
        assert!(CascadeRouter::is_retry_request("That answer is incorrect"));
    }

    #[test]
    fn retry_detected_wrong_answer() {
        assert!(CascadeRouter::is_retry_request("That was a wrong answer"));
    }

    #[test]
    fn retry_detected_bad_answer() {
        assert!(CascadeRouter::is_retry_request("That's a bad answer"));
    }

    #[test]
    fn retry_not_detected_normal_message() {
        assert!(!CascadeRouter::is_retry_request("What is the capital of France?"));
    }

    #[test]
    fn retry_case_insensitive() {
        assert!(CascadeRouter::is_retry_request("TRY AGAIN please"));
    }

    // -- EscalationReason Debug -----------------------------------------------

    #[test]
    fn escalation_reason_debug_display() {
        let reason = EscalationReason::ToolCallFailed;
        let debug_str = format!("{:?}", reason);
        assert!(debug_str.contains("ToolCallFailed"));
    }

    #[test]
    fn escalation_reason_user_retry_debug() {
        let reason = EscalationReason::UserRetry;
        let debug_str = format!("{:?}", reason);
        assert!(debug_str.contains("UserRetry"));
    }

    // -- CascadeConfig clone --------------------------------------------------

    #[test]
    fn cascade_config_is_cloneable() {
        let cfg = CascadeConfig {
            enabled: false,
            max_escalations: 5,
            min_valid_length: 20,
        };
        let cloned = cfg.clone();
        assert!(!cloned.enabled);
        assert_eq!(cloned.max_escalations, 5);
        assert_eq!(cloned.min_valid_length, 20);
    }

    // -- Priority ordering: user retry > tool error > empty > heuristics ------

    #[test]
    fn saver_user_retry_takes_priority_over_tool_error() {
        let resp = text_response("Some response here.");
        // Both user_requested_retry and tool_error_occurred are true.
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, true, true,
        );
        // User retry is checked first.
        assert_eq!(reason, Some(EscalationReason::UserRetry));
    }

    #[test]
    fn saver_tool_error_takes_priority_over_empty() {
        let resp = empty_response();
        let reason = CascadeRouter::should_escalate(
            &resp, QualityMode::Saver, EffortLevel::Low, true, false,
        );
        // Tool error is checked before content checks.
        assert_eq!(reason, Some(EscalationReason::ToolCallFailed));
    }
}
