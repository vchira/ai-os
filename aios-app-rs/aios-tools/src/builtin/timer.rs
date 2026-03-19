//! Timer tool — set, list, and cancel timers and alarms.
//!
//! Timers are stored in a thread-safe shared list.  When a timer fires,
//! it writes a sentinel file to `/tmp/aios-timer-{id}.fired` so the
//! application layer can poll for completed timers and route the
//! notification through TTS / the active channel.

use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use aios_core::types::ToolResult;
use chrono::{Local, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::tool::Tool;

// ---------------------------------------------------------------------------
// Timer data model
// ---------------------------------------------------------------------------

/// A single timer entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Timer {
    id: u64,
    label: String,
    created_at: String,
    fires_at: String,
    fired: bool,
}

/// A stopwatch entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stopwatch {
    id: u64,
    label: String,
    started_at: String,
    laps: Vec<String>,
    running: bool,
}

/// Shared state holding all active timers, stopwatches, and the next ID counter.
#[derive(Debug, Clone)]
struct TimerStore {
    next_id: u64,
    timers: Vec<Timer>,
    stopwatches: Vec<Stopwatch>,
}

impl TimerStore {
    fn new() -> Self {
        Self {
            next_id: 1,
            timers: Vec::new(),
            stopwatches: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: resolve duration from args
// ---------------------------------------------------------------------------

/// Parse the various duration/time parameters and return the number of
/// seconds until the timer should fire plus a human-readable description.
///
/// Supports three mutually-exclusive modes:
/// - `duration_seconds` — raw seconds
/// - `minutes` — convenience shorthand
/// - `time` — wall-clock time in `HH:MM` (24-hour) format
fn resolve_duration(args: &serde_json::Value) -> Result<(u64, String), String> {
    if let Some(secs) = args.get("duration_seconds").and_then(|v| v.as_u64()) {
        if secs == 0 {
            return Err("duration_seconds must be greater than 0.".into());
        }
        return Ok((secs, format_duration(secs)));
    }

    if let Some(mins) = args.get("minutes").and_then(|v| v.as_u64()) {
        if mins == 0 {
            return Err("minutes must be greater than 0.".into());
        }
        let secs = mins * 60;
        return Ok((secs, format_duration(secs)));
    }

    if let Some(time_str) = args.get("time").and_then(|v| v.as_str()) {
        let target = NaiveTime::parse_from_str(time_str, "%H:%M")
            .map_err(|_| format!("Invalid time format {time_str:?}. Use HH:MM (24-hour)."))?;
        let now = Local::now().time();
        let mut diff = target.signed_duration_since(now).num_seconds();
        if diff <= 0 {
            // Assume the user means tomorrow.
            diff += 86_400;
        }
        #[allow(clippy::cast_sign_loss)]
        let secs = diff as u64;
        return Ok((secs, format!("at {time_str} ({}).", format_duration(secs))));
    }

    Err(
        "Provide one of 'duration_seconds', 'minutes', or 'time' (HH:MM).".into(),
    )
}

/// Format a number of seconds into a human-friendly string like
/// `"5 minutes"`, `"1 hour 30 minutes"`, or `"45 seconds"`.
fn format_duration(total_secs: u64) -> String {
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours} hour{}", if hours == 1 { "" } else { "s" }));
    }
    if minutes > 0 {
        parts.push(format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" }));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{seconds} second{}", if seconds == 1 { "" } else { "s" }));
    }
    parts.join(" ")
}

/// Format remaining seconds for display in the timer list.
fn format_remaining(fires_at: &str) -> String {
    let Ok(target) = chrono::DateTime::parse_from_rfc3339(fires_at) else {
        return "unknown".into();
    };
    let remaining = target.signed_duration_since(Utc::now()).num_seconds();
    if remaining <= 0 {
        return "fired".into();
    }
    #[allow(clippy::cast_sign_loss)]
    format_duration(remaining as u64)
}

// ---------------------------------------------------------------------------
// TimerTool
// ---------------------------------------------------------------------------

/// Timer / alarm tool.
///
/// Supports three actions:
/// - **set** — create a new timer (by duration_seconds, minutes, or wall-clock time)
/// - **list** — show all active (not yet fired) timers with remaining time
/// - **cancel** — cancel an active timer by ID
pub struct TimerTool {
    store: Arc<Mutex<TimerStore>>,
}

impl TimerTool {
    /// Create a new `TimerTool` with its own shared store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(TimerStore::new())),
        }
    }

    /// Create a `TimerTool` backed by an externally-provided store.
    ///
    /// Useful for testing — allows the test to inspect state without
    /// going through the tool interface.
    #[cfg(test)]
    fn with_store(store: Arc<Mutex<TimerStore>>) -> Self {
        Self { store }
    }

    /// Spawn a background thread that sleeps for `duration_secs` and then
    /// marks the timer as fired + writes the sentinel file.
    fn spawn_timer_thread(
        store: Arc<Mutex<TimerStore>>,
        id: u64,
        label: String,
        duration_secs: u64,
    ) {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(duration_secs));

            // Mark as fired in the shared store.
            if let Ok(mut s) = store.lock() {
                if let Some(timer) = s.timers.iter_mut().find(|t| t.id == id) {
                    timer.fired = true;
                }
            }

            // Write sentinel file so the app layer can detect it.
            let path = format!("/tmp/aios-timer-{id}.fired");
            let content = format!("{label}\n");
            let _ = fs::write(&path, content);

            debug!(id, %label, "timer fired");
        });
    }

    /// Handle the `set` action.
    fn action_set(&self, args: &serde_json::Value) -> ToolResult {
        let label = args
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("Timer")
            .to_string();

        let (duration_secs, duration_desc) = match resolve_duration(args) {
            Ok(v) => v,
            Err(e) => return ToolResult::fail(e),
        };

        let mut store = self.store.lock().unwrap();
        let id = store.next_id;
        let fires_at = Utc::now()
            + chrono::Duration::seconds(
                i64::try_from(duration_secs).unwrap_or(i64::MAX),
            );

        let timer = Timer {
            id,
            label: label.clone(),
            created_at: Utc::now().to_rfc3339(),
            fires_at: fires_at.to_rfc3339(),
            fired: false,
        };
        store.next_id += 1;
        store.timers.push(timer);

        // Release the lock before spawning the thread.
        let store_ref = Arc::clone(&self.store);
        drop(store);

        Self::spawn_timer_thread(store_ref, id, label.clone(), duration_secs);

        debug!(id, %label, duration_secs, "timer set");
        ToolResult::ok_with_data(
            format!(
                "Timer #{id} set for {duration_desc}: \"{label}\". I'll let you know when it's done."
            ),
            serde_json::json!({ "id": id, "label": label, "duration_seconds": duration_secs }),
        )
    }

    /// Handle the `list` action.
    fn action_list(&self) -> ToolResult {
        let store = self.store.lock().unwrap();
        let active: Vec<&Timer> = store.timers.iter().filter(|t| !t.fired).collect();

        if active.is_empty() {
            return ToolResult::ok_with_data(
                "(no active timers)".to_string(),
                serde_json::json!({ "timers": [], "count": 0 }),
            );
        }

        let lines: Vec<String> = active
            .iter()
            .map(|t| {
                let remaining = format_remaining(&t.fires_at);
                format!("#{} \"{}\" — {} remaining", t.id, t.label, remaining)
            })
            .collect();

        let data: Vec<serde_json::Value> = active
            .iter()
            .map(|t| serde_json::to_value(t).unwrap_or_default())
            .collect();

        ToolResult::ok_with_data(
            lines.join("\n"),
            serde_json::json!({ "timers": data, "count": active.len() }),
        )
    }

    /// Handle the `cancel` action.
    fn action_cancel(&self, args: &serde_json::Value) -> ToolResult {
        let Some(id) = args.get("id").and_then(|v| v.as_u64()) else {
            return ToolResult::fail("'id' (integer) is required for cancel.");
        };

        let mut store = self.store.lock().unwrap();
        let len_before = store.timers.len();
        store.timers.retain(|t| t.id != id);

        if store.timers.len() == len_before {
            return ToolResult::fail(format!("Timer #{id} not found."));
        }

        // Clean up any sentinel file if it already fired.
        let path = format!("/tmp/aios-timer-{id}.fired");
        let _ = fs::remove_file(path);

        debug!(id, "timer cancelled");
        ToolResult::ok(format!("Cancelled timer #{id}."))
    }

    // --- Stopwatch actions ---

    fn action_stopwatch_start(&self, args: &serde_json::Value) -> ToolResult {
        let label = args.get("label").and_then(|v| v.as_str()).unwrap_or("Stopwatch");
        let mut store = self.store.lock().unwrap();
        let id = store.next_id;
        store.next_id += 1;

        store.stopwatches.push(Stopwatch {
            id,
            label: label.to_string(),
            started_at: Utc::now().to_rfc3339(),
            laps: Vec::new(),
            running: true,
        });

        debug!(id, label, "stopwatch started");
        ToolResult::ok(format!("Stopwatch #{id} started: {label}"))
    }

    fn action_stopwatch_stop(&self, args: &serde_json::Value) -> ToolResult {
        let id = match args.get("id").and_then(|v| v.as_u64()) {
            Some(id) => id,
            None => return ToolResult::fail("'id' is required for stopwatch_stop."),
        };

        let mut store = self.store.lock().unwrap();
        let sw = store.stopwatches.iter_mut().find(|s| s.id == id);
        match sw {
            Some(s) if s.running => {
                s.running = false;
                let started = chrono::DateTime::parse_from_rfc3339(&s.started_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                let elapsed = Utc::now() - started;
                let label = s.label.clone();
                debug!(id, "stopwatch stopped");
                ToolResult::ok(format!(
                    "Stopwatch #{id} stopped: {label}\nElapsed: {}",
                    format_duration(elapsed.num_seconds().max(0) as u64)
                ))
            }
            Some(_) => ToolResult::ok(format!("Stopwatch #{id} is already stopped.")),
            None => ToolResult::fail(format!("Stopwatch #{id} not found.")),
        }
    }

    fn action_stopwatch_lap(&self, args: &serde_json::Value) -> ToolResult {
        let id = match args.get("id").and_then(|v| v.as_u64()) {
            Some(id) => id,
            None => return ToolResult::fail("'id' is required for stopwatch_lap."),
        };

        let mut store = self.store.lock().unwrap();
        let sw = store.stopwatches.iter_mut().find(|s| s.id == id && s.running);
        match sw {
            Some(s) => {
                let started = chrono::DateTime::parse_from_rfc3339(&s.started_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                let elapsed = Utc::now() - started;
                let lap_time = format_duration(elapsed.num_seconds().max(0) as u64);
                s.laps.push(lap_time.clone());
                let lap_num = s.laps.len();
                debug!(id, lap_num, "stopwatch lap");
                ToolResult::ok(format!("Lap #{lap_num}: {lap_time}"))
            }
            None => ToolResult::fail(format!("Running stopwatch #{id} not found.")),
        }
    }

    fn action_stopwatch_status(&self) -> ToolResult {
        let store = self.store.lock().unwrap();
        if store.stopwatches.is_empty() {
            return ToolResult::ok("No stopwatches.");
        }

        let mut lines = Vec::new();
        for sw in &store.stopwatches {
            let started = chrono::DateTime::parse_from_rfc3339(&sw.started_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let elapsed = Utc::now() - started;
            let status = if sw.running { "running" } else { "stopped" };
            lines.push(format!(
                "#{} {} ({}) — {}",
                sw.id,
                sw.label,
                status,
                format_duration(elapsed.num_seconds().max(0) as u64)
            ));
            for (i, lap) in sw.laps.iter().enumerate() {
                lines.push(format!("  Lap {}: {lap}", i + 1));
            }
        }
        ToolResult::ok(lines.join("\n"))
    }
}

impl Tool for TimerTool {
    fn name(&self) -> &str {
        "timer"
    }

    fn description(&self) -> &str {
        "Set timers, alarms, and stopwatches. \
         Supports duration in seconds, minutes, or wall-clock time (HH:MM). \
         Also supports starting, stopping, and lapping a stopwatch."
    }

    fn category(&self) -> &str {
        "system"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["set", "list", "cancel", "stopwatch_start", "stopwatch_stop", "stopwatch_lap", "stopwatch_status"],
                    "description": "The operation: set/list/cancel for timers, stopwatch_start/stop/lap/status for stopwatches."
                },
                "duration_seconds": {
                    "type": "integer",
                    "description": "Timer duration in seconds (for 'set')."
                },
                "minutes": {
                    "type": "integer",
                    "description": "Timer duration in minutes (for 'set'). Convenience alternative to duration_seconds."
                },
                "time": {
                    "type": "string",
                    "description": "Wall-clock time in HH:MM 24-hour format (for 'set'). E.g. '14:30'."
                },
                "label": {
                    "type": "string",
                    "description": "Human-readable label announced when the timer fires (for 'set'). Defaults to 'Timer'."
                },
                "id": {
                    "type": "integer",
                    "description": "The timer ID (required for 'cancel')."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "set" => self.action_set(&args),
            "list" => self.action_list(),
            "cancel" => self.action_cancel(&args),
            "stopwatch_start" => self.action_stopwatch_start(&args),
            "stopwatch_stop" => self.action_stopwatch_stop(&args),
            "stopwatch_lap" => self.action_stopwatch_lap(&args),
            "stopwatch_status" => self.action_stopwatch_status(),
            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. Use: set, list, cancel, stopwatch_start/stop/lap/status."
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tool() -> TimerTool {
        TimerTool::new()
    }

    // -- set --

    #[test]
    fn set_timer_with_duration_seconds() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "set",
            "duration_seconds": 300,
            "label": "Tea is ready!"
        }));
        assert!(r.success);
        assert!(r.output.contains("Tea is ready!"));
        assert!(r.output.contains("#1"));
        assert!(r.output.contains("I'll let you know"));
        let data = r.data.unwrap();
        assert_eq!(data["id"], 1);
        assert_eq!(data["label"], "Tea is ready!");
        assert_eq!(data["duration_seconds"], 300);
    }

    #[test]
    fn set_timer_with_minutes() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "set",
            "minutes": 5,
            "label": "Break over"
        }));
        assert!(r.success);
        assert!(r.output.contains("Break over"));
        assert!(r.output.contains("5 minutes"));
        let data = r.data.unwrap();
        assert_eq!(data["duration_seconds"], 300);
    }

    #[test]
    fn set_timer_with_time() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "set",
            "time": "23:59",
            "label": "Meeting starts"
        }));
        assert!(r.success);
        assert!(r.output.contains("Meeting starts"));
        assert!(r.output.contains("at 23:59"));
        let data = r.data.unwrap();
        assert!(data["duration_seconds"].as_u64().unwrap() > 0);
    }

    #[test]
    fn set_timer_default_label() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "set",
            "duration_seconds": 60
        }));
        assert!(r.success);
        assert!(r.output.contains("Timer"));
        let data = r.data.unwrap();
        assert_eq!(data["label"], "Timer");
    }

    #[test]
    fn set_timer_missing_duration_fails() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "set",
            "label": "Oops"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("duration_seconds"));
    }

    #[test]
    fn set_timer_zero_seconds_fails() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "set",
            "duration_seconds": 0,
            "label": "Zero"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("greater than 0"));
    }

    #[test]
    fn set_timer_zero_minutes_fails() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "set",
            "minutes": 0
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("greater than 0"));
    }

    #[test]
    fn set_timer_invalid_time_format_fails() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({
            "action": "set",
            "time": "not-a-time"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Invalid time format"));
    }

    #[test]
    fn set_multiple_timers_auto_increments_id() {
        let tool = test_tool();
        for i in 1..=3 {
            let r = tool.execute(serde_json::json!({
                "action": "set",
                "duration_seconds": 600,
                "label": format!("Timer {i}")
            }));
            assert!(r.success);
            let data = r.data.unwrap();
            assert_eq!(data["id"], i);
        }
    }

    // -- list --

    #[test]
    fn list_empty() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert_eq!(r.output, "(no active timers)");
        let data = r.data.unwrap();
        assert_eq!(data["count"], 0);
    }

    #[test]
    fn list_active_timers() {
        let tool = test_tool();
        tool.execute(serde_json::json!({
            "action": "set",
            "duration_seconds": 3600,
            "label": "Long timer"
        }));
        tool.execute(serde_json::json!({
            "action": "set",
            "duration_seconds": 600,
            "label": "Short timer"
        }));

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert!(r.output.contains("Long timer"));
        assert!(r.output.contains("Short timer"));
        assert!(r.output.contains("remaining"));
        let data = r.data.unwrap();
        assert_eq!(data["count"], 2);
    }

    #[test]
    fn list_excludes_fired_timers() {
        let store = Arc::new(Mutex::new(TimerStore::new()));
        let tool = TimerTool::with_store(Arc::clone(&store));

        // Manually insert a fired timer.
        {
            let mut s = store.lock().unwrap();
            s.timers.push(Timer {
                id: 1,
                label: "Already done".into(),
                created_at: Utc::now().to_rfc3339(),
                fires_at: Utc::now().to_rfc3339(),
                fired: true,
            });
            s.next_id = 2;
        }

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert_eq!(r.output, "(no active timers)");
        let data = r.data.unwrap();
        assert_eq!(data["count"], 0);
    }

    // -- cancel --

    #[test]
    fn cancel_timer() {
        let tool = test_tool();
        tool.execute(serde_json::json!({
            "action": "set",
            "duration_seconds": 3600,
            "label": "Cancel me"
        }));

        let r = tool.execute(serde_json::json!({ "action": "cancel", "id": 1 }));
        assert!(r.success);
        assert!(r.output.contains("Cancelled"));
        assert!(r.output.contains("#1"));

        // List should be empty now.
        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert_eq!(r.output, "(no active timers)");
    }

    #[test]
    fn cancel_nonexistent_id_fails() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "cancel", "id": 99 }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("not found"));
    }

    #[test]
    fn cancel_missing_id_fails() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "cancel" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("id"));
    }

    #[test]
    fn cancel_does_not_affect_other_timers() {
        let tool = test_tool();
        tool.execute(serde_json::json!({
            "action": "set", "duration_seconds": 3600, "label": "Keep me"
        }));
        tool.execute(serde_json::json!({
            "action": "set", "duration_seconds": 3600, "label": "Remove me"
        }));
        tool.execute(serde_json::json!({
            "action": "set", "duration_seconds": 3600, "label": "Keep me too"
        }));

        tool.execute(serde_json::json!({ "action": "cancel", "id": 2 }));

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert!(r.output.contains("Keep me"));
        assert!(r.output.contains("Keep me too"));
        assert!(!r.output.contains("Remove me"));
        let data = r.data.unwrap();
        assert_eq!(data["count"], 2);
    }

    // -- invalid action --

    #[test]
    fn unknown_action() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({ "action": "explode" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown action"));
        assert!(r.error.as_deref().unwrap().contains("explode"));
    }

    #[test]
    fn missing_action() {
        let tool = test_tool();
        let r = tool.execute(serde_json::json!({}));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown action"));
    }

    // -- tool metadata --

    #[test]
    fn tool_name() {
        let tool = test_tool();
        assert_eq!(tool.name(), "timer");
    }

    #[test]
    fn tool_category() {
        let tool = test_tool();
        assert_eq!(tool.category(), "system");
    }

    #[test]
    fn tool_description_not_empty() {
        let tool = test_tool();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn tool_parameters_valid_schema() {
        let tool = test_tool();
        let params = tool.parameters();
        assert!(params.is_object());
        assert_eq!(params["type"], "object");
        let props = params["properties"].as_object().unwrap();
        assert!(props.contains_key("action"));
        assert!(props.contains_key("duration_seconds"));
        assert!(props.contains_key("minutes"));
        assert!(props.contains_key("time"));
        assert!(props.contains_key("label"));
        assert!(props.contains_key("id"));
    }

    // -- helper functions --

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(45), "45 seconds");
    }

    #[test]
    fn format_duration_minutes_only() {
        assert_eq!(format_duration(300), "5 minutes");
    }

    #[test]
    fn format_duration_hours_and_minutes() {
        assert_eq!(format_duration(5400), "1 hour 30 minutes");
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(0), "0 seconds");
    }

    #[test]
    fn format_duration_singular() {
        assert_eq!(format_duration(3661), "1 hour 1 minute 1 second");
    }

    #[test]
    fn format_duration_hours_only() {
        assert_eq!(format_duration(7200), "2 hours");
    }
}
