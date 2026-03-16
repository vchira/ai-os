//! Built-in self-test scenarios.
//!
//! Each scenario tests a specific subsystem at the lowest level possible,
//! simulating realistic conversation flows.

use crate::channel::{
    ChannelCapabilities, ChannelContext, ChannelKind, ChannelSwitcher,
};
use crate::config::commands::{CommandHandler, CommandResult};
use crate::config::ConfigManager;
use crate::types::{ToolResult, ToolSchema};

use super::runner::{SelfTestRunner, TestContext, TestResult};

/// Register all built-in test scenarios.
pub fn register_all(runner: &mut SelfTestRunner) {
    // -- Config tests --
    runner.add("config: read/write roundtrip", false, test_config_roundtrip);
    runner.add("config: defaults", false, test_config_defaults);

    // -- Command tests --
    runner.add("commands: /help", false, test_cmd_help);
    runner.add("commands: /channel", false, test_cmd_channel);
    runner.add("commands: /info", false, test_cmd_info);
    runner.add("commands: unknown command", false, test_cmd_unknown);

    // -- Channel tests --
    runner.add("channel: kind enum", false, test_channel_kind);
    runner.add("channel: capabilities", false, test_channel_capabilities);
    runner.add("channel: switcher", false, test_channel_switcher);
    runner.add("channel: switcher callback", false, test_channel_switcher_callback);
    runner.add("channel: context clone", false, test_channel_context_clone);

    // -- Tool schema tests --
    runner.add("tools: ToolResult ok/fail", false, test_tool_result);
    runner.add("tools: ToolSchema serialization", false, test_tool_schema);

    // -- Interactive tests --
    runner.add(
        "interactive: ui_panel text input",
        true,
        test_interactive_panel_text,
    );
    runner.add(
        "interactive: ui_panel choice",
        true,
        test_interactive_panel_choice,
    );
}

// ---------------------------------------------------------------------------
// Config tests
// ---------------------------------------------------------------------------

fn test_config_roundtrip(ctx: &mut TestContext) -> TestResult {
    let name = "config: read/write roundtrip";
    let dir = std::env::temp_dir().join(format!("aios_selftest_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("selftest_config.json");

    let res = (|| -> Result<(), String> {
        let mut cfg =
            ConfigManager::with_path(path.clone()).map_err(|e| format!("create: {e}"))?;
        cfg.set("test.key", serde_json::json!("hello")).map_err(|e| format!("set: {e}"))?;
        let val = cfg.get_str("test.key", "");
        if val != "hello" {
            return Err(format!("expected 'hello', got '{val}'"));
        }

        // Reload from disk.
        let cfg2 = ConfigManager::with_path(path).map_err(|e| format!("reload: {e}"))?;
        let val2 = cfg2.get_str("test.key", "");
        if val2 != "hello" {
            return Err(format!("persistence failed: got '{val2}'"));
        }
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&dir);

    {
        use crate::types::MessageLevel;
        match res {
            Ok(()) => {
                (ctx.display)("selftest", &MessageLevel::Success.format(&format!("[PASS] {name}")));
                TestResult { name: name.into(), passed: true, detail: "Config round-trips correctly".into(), interactive: false }
            }
            Err(e) => {
                (ctx.display)("selftest", &MessageLevel::Error.format(&format!("[FAIL] {name}: {e}")));
                TestResult { name: name.into(), passed: false, detail: e, interactive: false }
            }
        }
    }
}

fn test_config_defaults(ctx: &mut TestContext) -> TestResult {
    let name = "config: defaults";
    let dir = std::env::temp_dir().join(format!("aios_selftest_def_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("selftest_def.json");

    let res = (|| -> Result<String, String> {
        let cfg = ConfigManager::with_path(path).map_err(|e| format!("{e}"))?;
        let provider = cfg.get_str("llm.provider", "claude");
        let theme = cfg.get_str("ui.theme", "dark");
        if provider != "claude" || theme != "dark" {
            Err(format!("unexpected defaults: provider={provider}, theme={theme}"))
        } else {
            Ok("Defaults are correct".into())
        }
    })();

    let _ = std::fs::remove_dir_all(&dir);
    make_result(name, res, ctx)
}

// ---------------------------------------------------------------------------
// Command tests
// ---------------------------------------------------------------------------

fn test_cmd_help(ctx: &mut TestContext) -> TestResult {
    let name = "commands: /help";
    let dir = std::env::temp_dir().join(format!("aios_st_cmd_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("cfg.json");

    let res = (|| -> Result<String, String> {
        let mut cfg = ConfigManager::with_path(path).map_err(|e| format!("{e}"))?;
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/help") {
            CommandResult::Response(text) => {
                if text.contains("/key") && text.contains("/provider") && text.contains("/channel") {
                    Ok("Help text contains expected commands".into())
                } else {
                    Err("Help text missing expected commands".into())
                }
            }
            other => Err(format!("Expected Response, got {other:?}")),
        }
    })();

    let _ = std::fs::remove_dir_all(&dir);
    make_result(name, res, ctx)
}

fn test_cmd_channel(ctx: &mut TestContext) -> TestResult {
    let name = "commands: /channel";
    let dir = std::env::temp_dir().join(format!("aios_st_ch_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("cfg.json");

    let res = (|| -> Result<String, String> {
        let mut cfg = ConfigManager::with_path(path).map_err(|e| format!("{e}"))?;
        let mut handler = CommandHandler::new(&mut cfg);

        // Show channel status.
        match handler.execute("/channel") {
            CommandResult::Response(text) => {
                if !text.contains("Web:") || !text.contains("Signal:") {
                    return Err("Channel status missing expected fields".into());
                }
            }
            other => return Err(format!("Expected Response, got {other:?}")),
        }

        // Enable web channel.
        match handler.execute("/channel web on") {
            CommandResult::Response(text) => {
                if !text.contains("enabled") {
                    return Err(format!("Expected 'enabled', got: {text}"));
                }
            }
            other => return Err(format!("Expected Response, got {other:?}")),
        }

        // Verify it's in config.
        if !cfg.get_bool("channels.web.enabled", false) {
            return Err("Web channel not enabled in config".into());
        }

        Ok("/channel command works correctly".into())
    })();

    let _ = std::fs::remove_dir_all(&dir);
    make_result(name, res, ctx)
}

fn test_cmd_info(ctx: &mut TestContext) -> TestResult {
    let name = "commands: /info";
    let dir = std::env::temp_dir().join(format!("aios_st_info_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("cfg.json");

    let res = (|| -> Result<String, String> {
        let mut cfg = ConfigManager::with_path(path).map_err(|e| format!("{e}"))?;
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/info") {
            CommandResult::Response(text) => {
                if text.contains("AiOS") && text.contains("Provider") {
                    Ok("Info displays correctly".into())
                } else {
                    Err(format!("Unexpected info output: {}", &text[..100.min(text.len())]))
                }
            }
            other => Err(format!("Expected Response, got {other:?}")),
        }
    })();

    let _ = std::fs::remove_dir_all(&dir);
    make_result(name, res, ctx)
}

fn test_cmd_unknown(ctx: &mut TestContext) -> TestResult {
    let name = "commands: unknown command";
    let dir = std::env::temp_dir().join(format!("aios_st_unk_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("cfg.json");

    let res = (|| -> Result<String, String> {
        let mut cfg = ConfigManager::with_path(path).map_err(|e| format!("{e}"))?;
        let mut handler = CommandHandler::new(&mut cfg);
        match handler.execute("/nonexistent") {
            CommandResult::Unknown(_) => Ok("Unknown command handled correctly".into()),
            other => Err(format!("Expected Unknown, got {other:?}")),
        }
    })();

    let _ = std::fs::remove_dir_all(&dir);
    make_result(name, res, ctx)
}

// ---------------------------------------------------------------------------
// Channel tests
// ---------------------------------------------------------------------------

fn test_channel_kind(ctx: &mut TestContext) -> TestResult {
    let name = "channel: kind enum";
    let res = (|| -> Result<String, String> {
        // Parse.
        if ChannelKind::from_str_opt("signal") != Some(ChannelKind::Signal) {
            return Err("Failed to parse 'signal'".into());
        }
        if ChannelKind::from_str_opt("DESKTOP") != Some(ChannelKind::Desktop) {
            return Err("Failed to parse 'DESKTOP'".into());
        }
        if ChannelKind::from_str_opt("telegram").is_some() {
            return Err("Should not parse 'telegram'".into());
        }
        // Display.
        if ChannelKind::Web.to_string() != "web" {
            return Err("Display mismatch".into());
        }
        // Serde.
        let json = serde_json::to_string(&ChannelKind::Voice)
            .map_err(|e| format!("serialize: {e}"))?;
        if json != "\"voice\"" {
            return Err(format!("Serialization: expected '\"voice\"', got '{json}'"));
        }
        Ok("ChannelKind works correctly".into())
    })();
    make_result(name, res, ctx)
}

fn test_channel_capabilities(ctx: &mut TestContext) -> TestResult {
    let name = "channel: capabilities";
    let res = (|| -> Result<String, String> {
        let desktop = ChannelCapabilities::desktop();
        if !desktop.rich_panels || !desktop.images || !desktop.password_input {
            return Err("Desktop should have full capabilities".into());
        }

        let signal = ChannelCapabilities::signal();
        if signal.rich_panels || signal.markdown || signal.password_input {
            return Err("Signal should have limited capabilities".into());
        }
        if !signal.images {
            return Err("Signal should support images".into());
        }

        let voice = ChannelCapabilities::voice();
        if voice.images || voice.rich_panels || voice.markdown {
            return Err("Voice should have minimal capabilities".into());
        }

        Ok("Capabilities are correct per channel".into())
    })();
    make_result(name, res, ctx)
}

fn test_channel_switcher(ctx: &mut TestContext) -> TestResult {
    let name = "channel: switcher";
    let res = (|| -> Result<String, String> {
        let sw = ChannelSwitcher::new();
        if sw.active_kind() != ChannelKind::Desktop {
            return Err("Default should be Desktop".into());
        }

        // Can't switch to unregistered.
        if sw.switch_to(ChannelKind::Signal).is_ok() {
            return Err("Should fail for unregistered channel".into());
        }

        sw.register_channel(ChannelKind::Signal, ChannelContext::signal());
        sw.switch_to(ChannelKind::Signal)
            .map_err(|e| format!("Switch failed: {e}"))?;

        if sw.active_kind() != ChannelKind::Signal {
            return Err("Should be Signal after switch".into());
        }

        // Unregister → falls back to Desktop.
        sw.unregister_channel(ChannelKind::Signal);
        if sw.active_kind() != ChannelKind::Desktop {
            return Err("Should fall back to Desktop after unregister".into());
        }

        Ok("Switcher works correctly".into())
    })();
    make_result(name, res, ctx)
}

fn test_channel_switcher_callback(ctx: &mut TestContext) -> TestResult {
    let name = "channel: switcher callback";
    let res = (|| -> Result<String, String> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let sw = ChannelSwitcher::new();
        sw.register_channel(ChannelKind::Web, ChannelContext::web());

        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();
        sw.on_switch(Arc::new(move |old, new| {
            if old == ChannelKind::Desktop && new == ChannelKind::Web {
                f.store(true, Ordering::SeqCst);
            }
        }));

        sw.switch_to(ChannelKind::Web).map_err(|e| format!("{e}"))?;
        if !fired.load(Ordering::SeqCst) {
            return Err("Callback did not fire".into());
        }

        Ok("Switcher callbacks fire correctly".into())
    })();
    make_result(name, res, ctx)
}

fn test_channel_context_clone(ctx: &mut TestContext) -> TestResult {
    let name = "channel: context clone";
    let res = (|| -> Result<String, String> {
        let ctx1 = ChannelContext::signal();
        let ctx2 = ctx1.clone();
        if ctx1.kind != ctx2.kind {
            return Err("Kind mismatch after clone".into());
        }
        if !std::sync::Arc::ptr_eq(&ctx1.capabilities, &ctx2.capabilities) {
            return Err("Capabilities should share Arc".into());
        }
        Ok("Context clone is cheap (Arc shared)".into())
    })();
    make_result(name, res, ctx)
}

// ---------------------------------------------------------------------------
// Tool tests
// ---------------------------------------------------------------------------

fn test_tool_result(ctx: &mut TestContext) -> TestResult {
    let name = "tools: ToolResult ok/fail";
    let res = (|| -> Result<String, String> {
        let ok = ToolResult::ok("success");
        if !ok.success || ok.output != "success" {
            return Err("ToolResult::ok broken".into());
        }

        let fail = ToolResult::fail("error msg");
        if fail.success {
            return Err("ToolResult::fail should not be success".into());
        }
        if fail.error.as_deref() != Some("error msg") {
            return Err("ToolResult::fail error field wrong".into());
        }

        let with_data = ToolResult::ok_with_data("output", serde_json::json!({"key": 42}));
        if !with_data.success {
            return Err("ok_with_data should be success".into());
        }
        if with_data.data.is_none() {
            return Err("ok_with_data should have data".into());
        }

        Ok("ToolResult constructors work correctly".into())
    })();
    make_result(name, res, ctx)
}

fn test_tool_schema(ctx: &mut TestContext) -> TestResult {
    let name = "tools: ToolSchema serialization";
    let res = (|| -> Result<String, String> {
        let schema = ToolSchema {
            name: "test_tool".into(),
            description: "A test tool".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }),
        };

        let json = serde_json::to_string(&schema).map_err(|e| format!("serialize: {e}"))?;
        let back: ToolSchema =
            serde_json::from_str(&json).map_err(|e| format!("deserialize: {e}"))?;

        if back.name != "test_tool" || back.description != "A test tool" {
            return Err("Roundtrip failed".into());
        }

        Ok("ToolSchema serializes/deserializes correctly".into())
    })();
    make_result(name, res, ctx)
}

// ---------------------------------------------------------------------------
// Interactive tests
// ---------------------------------------------------------------------------

fn test_interactive_panel_text(ctx: &mut TestContext) -> TestResult {
    use crate::types::MessageLevel;

    let name = "interactive: ui_panel text input";

    if ctx.show_panel.is_none() {
        (ctx.display)(
            "selftest",
            &MessageLevel::Warning.format(&format!("[SKIP] {name}: no panel callback on this channel")),
        );
        return TestResult {
            name: name.into(),
            passed: true,
            detail: "Skipped (no panel support on this channel)".into(),
            interactive: true,
        };
    }

    (ctx.display)(
        "selftest",
        &MessageLevel::Info.format("A panel will appear asking you to type something. Please fill it in and click Submit."),
    );

    let panel_req = serde_json::json!({
        "title": "Self-Test: Text Input",
        "icon": "dialog-question-symbolic",
        "description": "This is a self-test. Please type anything below and click Submit.",
        "fields": [{
            "id": "user_input",
            "type": "text",
            "label": "Type anything",
            "placeholder": "e.g. hello world",
            "required": true
        }]
    });

    let show = ctx.show_panel.as_ref().unwrap();
    match show(panel_req) {
        Some(values) => {
            let input = values
                .get("user_input")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if input.trim().is_empty() {
                (ctx.display)("selftest", &MessageLevel::Error.format(&format!("[FAIL] {name}: empty input")));
                TestResult {
                    name: name.into(),
                    passed: false,
                    detail: "User submitted empty input".into(),
                    interactive: true,
                }
            } else {
                (ctx.display)(
                    "selftest",
                    &MessageLevel::Success.format(&format!("[PASS] {name}: received '{input}'")),
                );
                TestResult {
                    name: name.into(),
                    passed: true,
                    detail: format!("Received: '{input}'"),
                    interactive: true,
                }
            }
        }
        None => {
            (ctx.display)("selftest", &MessageLevel::Error.format(&format!("[FAIL] {name}: panel cancelled or unavailable")));
            TestResult {
                name: name.into(),
                passed: false,
                detail: "Panel was cancelled or could not be shown".into(),
                interactive: true,
            }
        }
    }
}

fn test_interactive_panel_choice(ctx: &mut TestContext) -> TestResult {
    use crate::types::MessageLevel;

    let name = "interactive: ui_panel choice";

    if ctx.show_panel.is_none() {
        (ctx.display)(
            "selftest",
            &MessageLevel::Warning.format(&format!("[SKIP] {name}: no panel callback on this channel")),
        );
        return TestResult {
            name: name.into(),
            passed: true,
            detail: "Skipped (no panel support on this channel)".into(),
            interactive: true,
        };
    }

    (ctx.display)(
        "selftest",
        &MessageLevel::Info.format("A panel will appear with a choice. Please select one option and click Submit."),
    );

    let panel_req = serde_json::json!({
        "title": "Self-Test: Choice",
        "description": "Pick your favorite color for testing.",
        "fields": [{
            "id": "color",
            "type": "choice",
            "label": "Favorite color",
            "required": true,
            "options": [
                { "value": "red", "label": "Red", "description": "The color of fire" },
                { "value": "blue", "label": "Blue", "description": "The color of sky" },
                { "value": "green", "label": "Green", "description": "The color of nature" }
            ]
        }]
    });

    let show = ctx.show_panel.as_ref().unwrap();
    match show(panel_req) {
        Some(values) => {
            let choice = values
                .get("color")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if ["red", "blue", "green"].contains(&choice) {
                (ctx.display)(
                    "selftest",
                    &MessageLevel::Success.format(&format!("[PASS] {name}: user chose '{choice}'")),
                );
                TestResult {
                    name: name.into(),
                    passed: true,
                    detail: format!("User chose: '{choice}'"),
                    interactive: true,
                }
            } else {
                (ctx.display)("selftest", &MessageLevel::Error.format(&format!("[FAIL] {name}: invalid choice '{choice}'")));
                TestResult {
                    name: name.into(),
                    passed: false,
                    detail: format!("Invalid choice: '{choice}'"),
                    interactive: true,
                }
            }
        }
        None => {
            (ctx.display)("selftest", &MessageLevel::Error.format(&format!("[FAIL] {name}: cancelled")));
            TestResult {
                name: name.into(),
                passed: false,
                detail: "Panel was cancelled".into(),
                interactive: true,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_result(name: &str, res: Result<String, String>, ctx: &mut TestContext) -> TestResult {
    use crate::types::MessageLevel;

    match res {
        Ok(detail) => {
            (ctx.display)("selftest", &MessageLevel::Success.format(&format!("[PASS] {name}")));
            TestResult {
                name: name.into(),
                passed: true,
                detail,
                interactive: false,
            }
        }
        Err(e) => {
            (ctx.display)("selftest", &MessageLevel::Error.format(&format!("[FAIL] {name}: {e}")));
            TestResult {
                name: name.into(),
                passed: false,
                detail: e,
                interactive: false,
            }
        }
    }
}
