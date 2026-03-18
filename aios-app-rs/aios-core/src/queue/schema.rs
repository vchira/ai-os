//! SQLite schema for the message queue.

use rusqlite::Connection;

use crate::Result;

/// Current schema version.
pub const SCHEMA_VERSION: i64 = 1;

/// Create all tables if they don't exist.
pub fn ensure_schema(conn: &Connection) -> Result<()> {
    // Enable WAL mode for concurrent read support.
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| crate::AiosError::Other(format!("Failed to set WAL mode: {e}")))?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS messages (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp    TEXT NOT NULL,
            channel      TEXT NOT NULL,
            role         TEXT NOT NULL,
            source       TEXT,
            content      TEXT,
            level        TEXT,
            tool_calls   TEXT,
            tool_call_id TEXT,
            metadata     TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
        CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel);
        CREATE INDEX IF NOT EXISTS idx_messages_role ON messages(role);

        CREATE TABLE IF NOT EXISTS clear_markers (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            channel   TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_clear_channel ON clear_markers(channel);

        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            content,
            content=messages,
            content_rowid=id
        );

        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );

        INSERT OR IGNORE INTO schema_version
            SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM schema_version);
        ",
    )
    .map_err(|e| crate::AiosError::Other(format!("Failed to create schema: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();

        // Verify tables exist by querying them.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);

        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn schema_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        ensure_schema(&conn).unwrap(); // Should not fail on second call.
    }
}
