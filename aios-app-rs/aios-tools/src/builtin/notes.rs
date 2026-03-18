//! Notes tool — persistent todo/note/reminder manager for the AI.
//!
//! Data is stored as a single JSON file at `~/.aios/notes.json`.
//! Each note has an auto-incrementing ID, text, optional category,
//! optional due date, creation timestamp, and completion status.

use std::fs;
use std::path::PathBuf;

use aios_core::types::ToolResult;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::tool::Tool;

/// Default storage path: `~/.aios/notes.json`.
fn default_notes_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().join(".aios").join("notes.json"))
        .unwrap_or_else(|| PathBuf::from(".aios/notes.json"))
}

/// A single note/todo/reminder entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Note {
    id: u64,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_date: Option<String>,
    created_at: String,
    completed: bool,
}

/// Top-level JSON structure persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotesStore {
    next_id: u64,
    notes: Vec<Note>,
}

impl NotesStore {
    fn new() -> Self {
        Self {
            next_id: 1,
            notes: Vec::new(),
        }
    }
}

/// Persistent notes/todo manager.
///
/// Supports five actions:
/// - **add** -- create a new note with optional category and due_date
/// - **list** -- list all notes, optionally filtered by category
/// - **done** -- mark a note as completed by ID
/// - **remove** -- delete a note by ID
/// - **search** -- search notes by text substring
pub struct NotesTool {
    storage_path: PathBuf,
}

impl NotesTool {
    /// Create a new `NotesTool`.
    ///
    /// If `storage_path` is `None`, the default `~/.aios/notes.json` is used.
    pub fn new(storage_path: Option<PathBuf>) -> Self {
        Self {
            storage_path: storage_path.unwrap_or_else(default_notes_path),
        }
    }

    /// Load the notes store from disk.  Returns a fresh store if the file
    /// does not exist or is empty.
    fn load(&self) -> Result<NotesStore, String> {
        if !self.storage_path.exists() {
            return Ok(NotesStore::new());
        }
        let text = fs::read_to_string(&self.storage_path)
            .map_err(|e| format!("Failed to read notes file: {e}"))?;
        if text.trim().is_empty() {
            return Ok(NotesStore::new());
        }
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse notes JSON: {e}"))
    }

    /// Save the store back to disk, creating parent directories as needed.
    fn save(&self, store: &NotesStore) -> Result<(), String> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create notes directory: {e}"))?;
        }
        let json = serde_json::to_string_pretty(store)
            .map_err(|e| format!("Failed to serialize notes: {e}"))?;
        fs::write(&self.storage_path, json)
            .map_err(|e| format!("Failed to write notes file: {e}"))?;
        Ok(())
    }

    /// Format a single note as a human-readable line.
    fn format_note(note: &Note) -> String {
        let status = if note.completed { "[x]" } else { "[ ]" };
        let mut line = format!("#{} {} {}", note.id, status, note.text);
        if let Some(ref cat) = note.category {
            line.push_str(&format!("  [{}]", cat));
        }
        if let Some(ref due) = note.due_date {
            line.push_str(&format!("  (due: {})", due));
        }
        line
    }

    /// Format a list of notes as multi-line text.
    fn format_notes(notes: &[&Note]) -> String {
        if notes.is_empty() {
            return "(no notes)".to_string();
        }
        notes
            .iter()
            .map(|n| Self::format_note(n))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Tool for NotesTool {
    fn name(&self) -> &str {
        "notes"
    }

    fn description(&self) -> &str {
        "Manage todos, notes, and reminders. \
         Add notes with optional category and due date, list them, \
         mark as done, remove, or search by text."
    }

    fn category(&self) -> &str {
        "memory"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "list", "done", "remove", "search"],
                    "description": "The notes operation to perform."
                },
                "text": {
                    "type": "string",
                    "description": "The note text (required for 'add')."
                },
                "category": {
                    "type": "string",
                    "description": "Optional category to assign (for 'add') or filter by (for 'list')."
                },
                "due_date": {
                    "type": "string",
                    "description": "Optional due date in YYYY-MM-DD format (for 'add')."
                },
                "id": {
                    "type": "integer",
                    "description": "The note ID (required for 'done' and 'remove')."
                },
                "query": {
                    "type": "string",
                    "description": "Search query (required for 'search')."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        let mut store = match self.load() {
            Ok(s) => s,
            Err(e) => return ToolResult::fail(format!("Failed to load notes: {e}")),
        };

        match action {
            "add" => {
                let Some(text) = args.get("text").and_then(|v| v.as_str()).filter(|t| !t.is_empty()) else {
                    return ToolResult::fail("'text' is required for add.");
                };
                let category = args
                    .get("category")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let due_date = args
                    .get("due_date")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());

                let id = store.next_id;
                let note = Note {
                    id,
                    text: text.to_string(),
                    category: category.clone(),
                    due_date: due_date.clone(),
                    created_at: Utc::now().to_rfc3339(),
                    completed: false,
                };
                store.next_id += 1;
                store.notes.push(note);

                if let Err(e) = self.save(&store) {
                    return ToolResult::fail(e);
                }
                debug!(id, text, "note added");
                ToolResult::ok_with_data(
                    format!("Added note #{id}: {text}"),
                    serde_json::json!({ "id": id, "text": text, "category": category, "due_date": due_date }),
                )
            }

            "list" => {
                let category_filter = args
                    .get("category")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty());

                let filtered: Vec<&Note> = store
                    .notes
                    .iter()
                    .filter(|n| {
                        if let Some(cat) = category_filter {
                            n.category.as_deref() == Some(cat)
                        } else {
                            true
                        }
                    })
                    .collect();

                let output = Self::format_notes(&filtered);
                let data: Vec<serde_json::Value> = filtered
                    .iter()
                    .map(|n| serde_json::to_value(n).unwrap_or_default())
                    .collect();
                ToolResult::ok_with_data(output, serde_json::json!({ "notes": data, "count": filtered.len() }))
            }

            "done" => {
                let Some(id) = args.get("id").and_then(|v| v.as_u64()) else {
                    return ToolResult::fail("'id' (integer) is required for done.");
                };

                let note_idx = store.notes.iter().position(|n| n.id == id);
                let Some(idx) = note_idx else {
                    return ToolResult::fail(format!("Note #{id} not found."));
                };

                if store.notes[idx].completed {
                    return ToolResult::ok(format!("Note #{id} is already completed."));
                }

                store.notes[idx].completed = true;
                let text = store.notes[idx].text.clone();

                if let Err(e) = self.save(&store) {
                    return ToolResult::fail(e);
                }
                debug!(id, "note marked done");
                ToolResult::ok(format!("Marked note #{id} as done: {text}"))
            }

            "remove" => {
                let Some(id) = args.get("id").and_then(|v| v.as_u64()) else {
                    return ToolResult::fail("'id' (integer) is required for remove.");
                };

                let len_before = store.notes.len();
                store.notes.retain(|n| n.id != id);

                if store.notes.len() == len_before {
                    return ToolResult::fail(format!("Note #{id} not found."));
                }

                if let Err(e) = self.save(&store) {
                    return ToolResult::fail(e);
                }
                debug!(id, "note removed");
                ToolResult::ok(format!("Removed note #{id}."))
            }

            "search" => {
                let Some(query) = args.get("query").and_then(|v| v.as_str()).filter(|q| !q.is_empty()) else {
                    return ToolResult::fail("'query' is required for search.");
                };

                let query_lower = query.to_lowercase();
                let matches: Vec<&Note> = store
                    .notes
                    .iter()
                    .filter(|n| {
                        n.text.to_lowercase().contains(&query_lower)
                            || n.category
                                .as_deref()
                                .map(|c| c.to_lowercase().contains(&query_lower))
                                .unwrap_or(false)
                    })
                    .collect();

                let output = Self::format_notes(&matches);
                let data: Vec<serde_json::Value> = matches
                    .iter()
                    .map(|n| serde_json::to_value(n).unwrap_or_default())
                    .collect();
                ToolResult::ok_with_data(
                    output,
                    serde_json::json!({ "notes": data, "count": matches.len(), "query": query }),
                )
            }

            _ => ToolResult::fail(format!(
                "Unknown action {action:?}. Use: add, list, done, remove, search."
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

    fn temp_tool() -> (NotesTool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.json");
        (NotesTool::new(Some(path)), dir)
    }

    // -- add --

    #[test]
    fn add_note_basic() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "add",
            "text": "Buy milk"
        }));
        assert!(r.success);
        assert!(r.output.contains("Buy milk"));
        assert!(r.output.contains("#1"));
        let data = r.data.unwrap();
        assert_eq!(data["id"], 1);
        assert_eq!(data["text"], "Buy milk");
    }

    #[test]
    fn add_note_with_category() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "add",
            "text": "Buy eggs",
            "category": "shopping"
        }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data["category"], "shopping");
    }

    #[test]
    fn add_note_with_due_date() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "add",
            "text": "Submit report",
            "due_date": "2026-03-20"
        }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data["due_date"], "2026-03-20");
    }

    #[test]
    fn add_note_with_all_fields() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "add",
            "text": "Doctor appointment",
            "category": "health",
            "due_date": "2026-04-01"
        }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data["id"], 1);
        assert_eq!(data["text"], "Doctor appointment");
        assert_eq!(data["category"], "health");
        assert_eq!(data["due_date"], "2026-04-01");
    }

    #[test]
    fn add_note_missing_text_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "add"
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("text"));
    }

    #[test]
    fn add_note_empty_text_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({
            "action": "add",
            "text": ""
        }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("text"));
    }

    #[test]
    fn add_multiple_notes_auto_increments_id() {
        let (tool, _dir) = temp_tool();
        for i in 1..=5 {
            let r = tool.execute(serde_json::json!({
                "action": "add",
                "text": format!("Note {i}")
            }));
            assert!(r.success);
            let data = r.data.unwrap();
            assert_eq!(data["id"], i);
        }
    }

    // -- list --

    #[test]
    fn list_empty() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert_eq!(r.output, "(no notes)");
        let data = r.data.unwrap();
        assert_eq!(data["count"], 0);
    }

    #[test]
    fn list_all_notes() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "First" }));
        tool.execute(serde_json::json!({ "action": "add", "text": "Second" }));

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert!(r.output.contains("First"));
        assert!(r.output.contains("Second"));
        let data = r.data.unwrap();
        assert_eq!(data["count"], 2);
    }

    #[test]
    fn list_filter_by_category() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "add", "text": "Buy milk", "category": "shopping"
        }));
        tool.execute(serde_json::json!({
            "action": "add", "text": "Read book", "category": "personal"
        }));
        tool.execute(serde_json::json!({
            "action": "add", "text": "Buy eggs", "category": "shopping"
        }));

        let r = tool.execute(serde_json::json!({
            "action": "list", "category": "shopping"
        }));
        assert!(r.success);
        assert!(r.output.contains("Buy milk"));
        assert!(r.output.contains("Buy eggs"));
        assert!(!r.output.contains("Read book"));
        let data = r.data.unwrap();
        assert_eq!(data["count"], 2);
    }

    #[test]
    fn list_filter_by_nonexistent_category() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "add", "text": "Note 1", "category": "work"
        }));

        let r = tool.execute(serde_json::json!({
            "action": "list", "category": "nonexistent"
        }));
        assert!(r.success);
        assert_eq!(r.output, "(no notes)");
        let data = r.data.unwrap();
        assert_eq!(data["count"], 0);
    }

    #[test]
    fn list_shows_due_dates() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "add",
            "text": "Deadline task",
            "due_date": "2026-12-31"
        }));

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert!(r.output.contains("due: 2026-12-31"));
    }

    #[test]
    fn list_shows_completion_status() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Task A" }));
        tool.execute(serde_json::json!({ "action": "add", "text": "Task B" }));
        tool.execute(serde_json::json!({ "action": "done", "id": 1 }));

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert!(r.output.contains("[x] Task A"));
        assert!(r.output.contains("[ ] Task B"));
    }

    // -- done --

    #[test]
    fn done_marks_note_completed() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Task" }));

        let r = tool.execute(serde_json::json!({ "action": "done", "id": 1 }));
        assert!(r.success);
        assert!(r.output.contains("done"));
        assert!(r.output.contains("Task"));
    }

    #[test]
    fn done_already_completed() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Task" }));
        tool.execute(serde_json::json!({ "action": "done", "id": 1 }));

        let r = tool.execute(serde_json::json!({ "action": "done", "id": 1 }));
        assert!(r.success);
        assert!(r.output.contains("already completed"));
    }

    #[test]
    fn done_nonexistent_id_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "done", "id": 99 }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("not found"));
    }

    #[test]
    fn done_missing_id_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "done" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("id"));
    }

    // -- remove --

    #[test]
    fn remove_note() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Temp note" }));

        let r = tool.execute(serde_json::json!({ "action": "remove", "id": 1 }));
        assert!(r.success);
        assert!(r.output.contains("Removed"));

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert_eq!(r.output, "(no notes)");
    }

    #[test]
    fn remove_nonexistent_id_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "remove", "id": 42 }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("not found"));
    }

    #[test]
    fn remove_missing_id_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "remove" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("id"));
    }

    #[test]
    fn remove_does_not_affect_other_notes() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Keep me" }));
        tool.execute(serde_json::json!({ "action": "add", "text": "Remove me" }));
        tool.execute(serde_json::json!({ "action": "add", "text": "Keep me too" }));

        tool.execute(serde_json::json!({ "action": "remove", "id": 2 }));

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert!(r.output.contains("Keep me"));
        assert!(r.output.contains("Keep me too"));
        assert!(!r.output.contains("Remove me"));
        let data = r.data.unwrap();
        assert_eq!(data["count"], 2);
    }

    // -- search --

    #[test]
    fn search_by_text() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Buy milk" }));
        tool.execute(serde_json::json!({ "action": "add", "text": "Read a book" }));
        tool.execute(serde_json::json!({ "action": "add", "text": "Buy bread" }));

        let r = tool.execute(serde_json::json!({ "action": "search", "query": "buy" }));
        assert!(r.success);
        assert!(r.output.contains("Buy milk"));
        assert!(r.output.contains("Buy bread"));
        assert!(!r.output.contains("Read a book"));
        let data = r.data.unwrap();
        assert_eq!(data["count"], 2);
    }

    #[test]
    fn search_case_insensitive() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Important MEETING" }));

        let r = tool.execute(serde_json::json!({ "action": "search", "query": "meeting" }));
        assert!(r.success);
        assert!(r.output.contains("Important MEETING"));
    }

    #[test]
    fn search_by_category() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "add", "text": "Note 1", "category": "work"
        }));
        tool.execute(serde_json::json!({
            "action": "add", "text": "Note 2", "category": "personal"
        }));

        let r = tool.execute(serde_json::json!({ "action": "search", "query": "work" }));
        assert!(r.success);
        assert!(r.output.contains("Note 1"));
        assert!(!r.output.contains("Note 2"));
    }

    #[test]
    fn search_no_results() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Something" }));

        let r = tool.execute(serde_json::json!({ "action": "search", "query": "zzzzz" }));
        assert!(r.success);
        assert_eq!(r.output, "(no notes)");
        let data = r.data.unwrap();
        assert_eq!(data["count"], 0);
    }

    #[test]
    fn search_missing_query_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "search" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("query"));
    }

    #[test]
    fn search_empty_query_fails() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "search", "query": "" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("query"));
    }

    // -- invalid action --

    #[test]
    fn unknown_action() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({ "action": "explode" }));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown action"));
        assert!(r.error.as_deref().unwrap().contains("explode"));
    }

    #[test]
    fn missing_action() {
        let (tool, _dir) = temp_tool();
        let r = tool.execute(serde_json::json!({}));
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("Unknown action"));
    }

    // -- tool metadata --

    #[test]
    fn tool_name_and_category() {
        let (tool, _dir) = temp_tool();
        assert_eq!(tool.name(), "notes");
        assert_eq!(tool.category(), "memory");
    }

    #[test]
    fn tool_description_not_empty() {
        let (tool, _dir) = temp_tool();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn tool_parameters_valid_schema() {
        let (tool, _dir) = temp_tool();
        let params = tool.parameters();
        assert!(params.is_object());
        assert_eq!(params["type"], "object");
        let props = params["properties"].as_object().unwrap();
        assert!(props.contains_key("action"));
        assert!(props.contains_key("text"));
        assert!(props.contains_key("category"));
        assert!(props.contains_key("due_date"));
        assert!(props.contains_key("id"));
        assert!(props.contains_key("query"));
    }

    // -- persistence --

    #[test]
    fn persistence_across_instances() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.json");

        // First instance: add a note.
        let tool1 = NotesTool::new(Some(path.clone()));
        tool1.execute(serde_json::json!({
            "action": "add", "text": "Persist me"
        }));

        // Second instance: list should find it.
        let tool2 = NotesTool::new(Some(path.clone()));
        let r = tool2.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        assert!(r.output.contains("Persist me"));

        // Third instance: add another note, ID should continue.
        let tool3 = NotesTool::new(Some(path));
        let r = tool3.execute(serde_json::json!({
            "action": "add", "text": "Second note"
        }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data["id"], 2);
    }

    // -- integration-style tests --

    #[test]
    fn full_workflow() {
        let (tool, _dir) = temp_tool();

        // Add several notes.
        tool.execute(serde_json::json!({
            "action": "add", "text": "Buy groceries", "category": "shopping", "due_date": "2026-03-20"
        }));
        tool.execute(serde_json::json!({
            "action": "add", "text": "Write report", "category": "work"
        }));
        tool.execute(serde_json::json!({
            "action": "add", "text": "Call dentist", "category": "health", "due_date": "2026-03-25"
        }));

        // List all.
        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.success);
        let data = r.data.unwrap();
        assert_eq!(data["count"], 3);

        // Complete one.
        let r = tool.execute(serde_json::json!({ "action": "done", "id": 2 }));
        assert!(r.success);

        // List shows [x] for completed.
        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.output.contains("[x] Write report"));

        // Search.
        let r = tool.execute(serde_json::json!({ "action": "search", "query": "dentist" }));
        assert!(r.success);
        assert!(r.output.contains("Call dentist"));

        // Remove.
        let r = tool.execute(serde_json::json!({ "action": "remove", "id": 1 }));
        assert!(r.success);

        // Final list: 2 notes left.
        let r = tool.execute(serde_json::json!({ "action": "list" }));
        let data = r.data.unwrap();
        assert_eq!(data["count"], 2);
    }

    #[test]
    fn id_never_reused_after_remove() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({ "action": "add", "text": "Note A" }));
        tool.execute(serde_json::json!({ "action": "add", "text": "Note B" }));
        tool.execute(serde_json::json!({ "action": "remove", "id": 1 }));

        // Next note should get ID 3, not 1.
        let r = tool.execute(serde_json::json!({ "action": "add", "text": "Note C" }));
        let data = r.data.unwrap();
        assert_eq!(data["id"], 3);
    }

    #[test]
    fn format_includes_category_and_due_date() {
        let (tool, _dir) = temp_tool();
        tool.execute(serde_json::json!({
            "action": "add",
            "text": "Formatted note",
            "category": "test",
            "due_date": "2026-06-15"
        }));

        let r = tool.execute(serde_json::json!({ "action": "list" }));
        assert!(r.output.contains("[test]"));
        assert!(r.output.contains("(due: 2026-06-15)"));
        assert!(r.output.contains("[ ]"));
    }
}
