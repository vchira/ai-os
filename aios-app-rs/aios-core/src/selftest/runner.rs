//! Self-test runner — orchestrates test scenarios and collects results.

use std::fmt;

/// Result of a single test scenario.
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test name.
    pub name: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Details (error message on failure, summary on success).
    pub detail: String,
    /// Whether this test requires user interaction.
    pub interactive: bool,
}

impl fmt::Display for TestResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::types::MessageLevel;

        let (level, label) = if self.passed {
            (MessageLevel::Success, "PASS")
        } else {
            (MessageLevel::Error, "FAIL")
        };
        let tag = if self.interactive { " [interactive]" } else { "" };
        write!(f, "{} [{label}] {}{tag}: {}", level.icon(), self.name, self.detail)
    }
}

/// A test scenario function.
///
/// Receives a [`TestContext`] and returns a [`TestResult`].
/// The context provides access to all subsystems needed for testing.
pub type ScenarioFn = Box<dyn FnOnce(&mut TestContext) -> TestResult>;

/// Context passed to each test scenario.
///
/// Provides access to real subsystems (tool registry, config, channel
/// switcher) so tests exercise the actual code paths.
///
/// Note: not `Send` — the selftest runs on the caller's thread
/// (GTK main thread for Desktop, web handler thread for Web, etc.).
pub struct TestContext {
    /// Callback for displaying a message in the chat on the current channel.
    /// `(role, content)` — role is "system", "user", "assistant", or "selftest".
    pub display: Box<dyn Fn(&str, &str)>,
    /// Callback for showing a panel and collecting input.
    /// Returns `Some(values_json)` or `None` if cancelled/unavailable.
    pub show_panel: Option<Box<dyn Fn(serde_json::Value) -> Option<serde_json::Value>>>,
    /// The channel kind this selftest is running on.
    pub channel_kind: crate::channel::ChannelKind,
}

/// Runs all (or filtered) self-test scenarios.
pub struct SelfTestRunner {
    scenarios: Vec<(String, bool, ScenarioFn)>, // (name, interactive, fn)
}

impl SelfTestRunner {
    /// Create a new runner with all built-in scenarios.
    pub fn new() -> Self {
        let mut runner = Self {
            scenarios: Vec::new(),
        };
        super::scenarios::register_all(&mut runner);
        runner
    }

    /// Register a test scenario.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        interactive: bool,
        f: impl FnOnce(&mut TestContext) -> TestResult + 'static,
    ) {
        self.scenarios.push((name.into(), interactive, Box::new(f)));
    }

    /// Run all scenarios, returning results.
    pub fn run_all(self, ctx_factory: impl Fn() -> TestContext) -> Vec<TestResult> {
        let mut results = Vec::new();
        for (name, interactive, f) in self.scenarios {
            let mut ctx = ctx_factory();
            (ctx.display)("selftest", &format!("Running: {name}..."));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut ctx)));
            match result {
                Ok(r) => results.push(r),
                Err(_) => results.push(TestResult {
                    name,
                    passed: false,
                    detail: "Test panicked!".to_string(),
                    interactive,
                }),
            }
        }
        results
    }

    /// Run only non-interactive scenarios.
    pub fn run_quick(self, ctx_factory: impl Fn() -> TestContext) -> Vec<TestResult> {
        let mut results = Vec::new();
        for (name, interactive, f) in self.scenarios {
            if interactive {
                continue;
            }
            let mut ctx = ctx_factory();
            (ctx.display)("selftest", &format!("Running: {name}..."));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut ctx)));
            match result {
                Ok(r) => results.push(r),
                Err(_) => results.push(TestResult {
                    name,
                    passed: false,
                    detail: "Test panicked!".to_string(),
                    interactive,
                }),
            }
        }
        results
    }

    /// Run only scenarios matching a tag in their name.
    pub fn run_tagged(
        self,
        tag: &str,
        ctx_factory: impl Fn() -> TestContext,
    ) -> Vec<TestResult> {
        let tag_lower = tag.to_lowercase();
        let mut results = Vec::new();
        for (name, interactive, f) in self.scenarios {
            if !name.to_lowercase().contains(&tag_lower) {
                continue;
            }
            let mut ctx = ctx_factory();
            (ctx.display)("selftest", &format!("Running: {name}..."));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut ctx)));
            match result {
                Ok(r) => results.push(r),
                Err(_) => results.push(TestResult {
                    name,
                    passed: false,
                    detail: "Test panicked!".to_string(),
                    interactive,
                }),
            }
        }
        results
    }

    /// Format results into a human-readable report.
    pub fn format_report(results: &[TestResult]) -> String {
        use crate::types::MessageLevel;

        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;

        let header_level = if failed == 0 {
            MessageLevel::Success
        } else {
            MessageLevel::Error
        };

        let mut lines = vec![
            String::new(),
            "========================================".to_string(),
            format!(
                "  {} AiOS Self-Test Report",
                header_level.format(if failed == 0 { "All tests passed" } else { "Some tests failed" })
            ),
            "========================================".to_string(),
            String::new(),
        ];

        for r in results {
            lines.push(r.to_string());
        }

        lines.push(String::new());
        lines.push("----------------------------------------".to_string());
        lines.push(format!(
            "Total: {total}  |  Passed: {passed}  |  Failed: {failed}"
        ));

        if failed == 0 {
            lines.push(MessageLevel::Success.format("All tests passed!"));
        } else {
            lines.push(MessageLevel::Error.format(&format!("{failed} test(s) FAILED.")));
        }
        lines.push("========================================".to_string());

        lines.join("\n")
    }
}

impl Default for SelfTestRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_ctx() -> TestContext {
        TestContext {
            display: Box::new(|_, _| {}),
            show_panel: None,
            channel_kind: crate::channel::ChannelKind::Desktop,
        }
    }

    #[test]
    fn runner_executes_scenarios() {
        let mut runner = SelfTestRunner { scenarios: Vec::new() };
        runner.add("pass_test", false, |_ctx| TestResult {
            name: "pass_test".into(),
            passed: true,
            detail: "ok".into(),
            interactive: false,
        });
        runner.add("fail_test", false, |_ctx| TestResult {
            name: "fail_test".into(),
            passed: false,
            detail: "nope".into(),
            interactive: false,
        });

        let results = runner.run_all(dummy_ctx);
        assert_eq!(results.len(), 2);
        assert!(results[0].passed);
        assert!(!results[1].passed);
    }

    #[test]
    fn runner_catches_panics() {
        let mut runner = SelfTestRunner { scenarios: Vec::new() };
        runner.add("panic_test", false, |_ctx| {
            panic!("boom");
        });

        let results = runner.run_all(dummy_ctx);
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        assert!(results[0].detail.contains("panicked"));
    }

    #[test]
    fn run_quick_skips_interactive() {
        let mut runner = SelfTestRunner { scenarios: Vec::new() };
        runner.add("auto_test", false, |_ctx| TestResult {
            name: "auto_test".into(),
            passed: true,
            detail: "ok".into(),
            interactive: false,
        });
        runner.add("interactive_test", true, |_ctx| TestResult {
            name: "interactive_test".into(),
            passed: true,
            detail: "ok".into(),
            interactive: true,
        });

        let results = runner.run_quick(dummy_ctx);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "auto_test");
    }

    #[test]
    fn format_report_all_pass() {
        let results = vec![
            TestResult { name: "a".into(), passed: true, detail: "ok".into(), interactive: false },
            TestResult { name: "b".into(), passed: true, detail: "ok".into(), interactive: false },
        ];
        let report = SelfTestRunner::format_report(&results);
        assert!(report.contains("All tests passed!"));
        assert!(report.contains("Passed: 2"));
    }

    #[test]
    fn format_report_with_failure() {
        let results = vec![
            TestResult { name: "a".into(), passed: true, detail: "ok".into(), interactive: false },
            TestResult { name: "b".into(), passed: false, detail: "bad".into(), interactive: false },
        ];
        let report = SelfTestRunner::format_report(&results);
        assert!(report.contains("1 test(s) FAILED"));
        assert!(report.contains("[FAIL] b"));
    }
}
