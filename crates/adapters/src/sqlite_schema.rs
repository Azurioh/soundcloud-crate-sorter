//! SQLite schema + migrations (T016). One database file holds tracks, crates, classification
//! decisions, the audit log, and settings.
//!
//! Note on classification decisions: an `AuditEvent` records the same facts, so while nothing read
//! decisions back this table was deliberately omitted as duplicated, dead schema. Triage (US2) is
//! the first reader — it rebuilds a card's top suggestion and alternatives (FR-015) for a track that
//! dropped its crate on the way into triage — and reconstructing that from the append-only trail
//! would mean parsing stringly-typed `detail` JSON, making the audit trail load-bearing application
//! state. Decisions therefore get their own typed table; the audit trail keeps its own record for
//! observability (Principle VII).

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::sqlite_support::SharedConnection;

/// DDL applied on open. `IF NOT EXISTS` makes it idempotent across runs.
/// `energy_role` / `vibe_tags` use `''` (not NULL) so UNIQUE and equality behave predictably
/// (SQLite treats NULLs as distinct, which would break `UNIQUE(genre, energy_role)`).
const SCHEMA_SQL: &str = "
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS crates (
    id           TEXT PRIMARY KEY,
    genre        TEXT NOT NULL,
    energy_role  TEXT NOT NULL DEFAULT '',
    created_by   TEXT NOT NULL,
    UNIQUE (genre, energy_role)
);

CREATE TABLE IF NOT EXISTS tracks (
    id               TEXT PRIMARY KEY,
    source_track_id  TEXT NOT NULL UNIQUE,
    title            TEXT NOT NULL,
    artist           TEXT NOT NULL,
    source_genre     TEXT,
    duration_ms      INTEGER NOT NULL,
    permalink_url    TEXT NOT NULL,
    artwork_url      TEXT,
    bpm              INTEGER,
    tempo_ambiguity  TEXT,
    camelot_key      TEXT,
    energy           INTEGER,
    vibe_tags        TEXT NOT NULL DEFAULT '[]',
    crate_id         TEXT REFERENCES crates(id),
    confidence       REAL,
    status           TEXT NOT NULL,
    local_audio_path TEXT
);

CREATE TABLE IF NOT EXISTS classification_decisions (
    id           TEXT PRIMARY KEY,
    track_id     TEXT NOT NULL REFERENCES tracks(id),
    crate_id     TEXT NOT NULL REFERENCES crates(id),
    source       TEXT NOT NULL,
    confidence   REAL,
    reason       TEXT NOT NULL,
    alternatives TEXT NOT NULL DEFAULT '[]',
    decided_at   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_decision_track ON classification_decisions(track_id);

CREATE TABLE IF NOT EXISTS audit_events (
    id          TEXT PRIMARY KEY,
    run_id      TEXT NOT NULL,
    track_id    TEXT,
    stage       TEXT NOT NULL,
    kind        TEXT NOT NULL,
    outcome     TEXT NOT NULL,
    detail      TEXT NOT NULL,
    occurred_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_run   ON audit_events(run_id);
CREATE INDEX IF NOT EXISTS idx_audit_track ON audit_events(track_id);

CREATE TABLE IF NOT EXISTS settings (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    confidence_threshold REAL NOT NULL,
    download_enabled     INTEGER NOT NULL,
    export_mode          TEXT NOT NULL
);
";

/// Columns added to `tracks` after the first schema shipped. `CREATE TABLE IF NOT EXISTS` is a
/// no-op on a database that already exists, so a new column reaches an existing library only via an
/// explicit `ALTER TABLE` — without this, upgrading to the audio path (US4) would leave every
/// pre-existing database missing `tempo_ambiguity` and every query against it failing.
///
/// Each entry is `(column, definition)` and MUST be **additive**: a new nullable column, or one with
/// a default. Never a `DROP`, never a retype — the user's library is irreplaceable (Principle IV).
const TRACK_COLUMN_MIGRATIONS: &[(&str, &str)] = &[("tempo_ambiguity", "TEXT")];

/// The application's SQLite database: owns the shared connection and applies migrations on open.
#[derive(Clone)]
pub struct SqliteDatabase {
    connection: SharedConnection,
}

impl SqliteDatabase {
    /// Opens (creating if needed) the database at `path` and applies the schema.
    ///
    /// # Errors
    /// Returns a `rusqlite::Error` if the file cannot be opened or the schema fails to apply.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    /// Opens an ephemeral in-memory database (for tests) and applies the schema.
    ///
    /// # Errors
    /// Returns a `rusqlite::Error` if the schema fails to apply.
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let connection = Connection::open_in_memory()?;
        Self::from_connection(connection)
    }

    /// Applies the schema to `connection` and wraps it in the shared handle.
    fn from_connection(connection: Connection) -> rusqlite::Result<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        apply_track_column_migrations(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Returns a clone of the shared connection handle for a repository adapter.
    #[must_use]
    pub fn connection(&self) -> SharedConnection {
        Arc::clone(&self.connection)
    }
}

/// Adds any [`TRACK_COLUMN_MIGRATIONS`] column the `tracks` table is missing. Idempotent: a fresh
/// database already has them from the DDL and this does nothing.
fn apply_track_column_migrations(connection: &Connection) -> rusqlite::Result<()> {
    let existing = track_column_names(connection)?;
    for (column, definition) in TRACK_COLUMN_MIGRATIONS {
        if !existing.iter().any(|name| name == column) {
            connection.execute_batch(&format!(
                "ALTER TABLE tracks ADD COLUMN {column} {definition}"
            ))?;
        }
    }
    Ok(())
}

/// Reads the `tracks` table's current column names from SQLite's own catalog.
fn track_column_names(connection: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare("SELECT name FROM pragma_table_info('tracks')")?;
    let mut names = Vec::new();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        names.push(row.get::<_, String>(0)?);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library created before the audio path existed must gain the column rather than break: the
    /// user's tracks and their triage decisions live in that file.
    #[test]
    fn migrates_a_database_created_before_the_column_existed() {
        let connection = Connection::open_in_memory().expect("open");
        // The pre-US4 shape: everything except `tempo_ambiguity`.
        connection
            .execute_batch(
                "CREATE TABLE tracks (
                    id TEXT PRIMARY KEY,
                    source_track_id TEXT NOT NULL UNIQUE,
                    title TEXT NOT NULL,
                    artist TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    permalink_url TEXT NOT NULL,
                    status TEXT NOT NULL
                );
                INSERT INTO tracks VALUES ('t1', 'sc:1', 'Title', 'Artist', 1, 'https://x', 'scanned');",
            )
            .expect("seed legacy schema");

        apply_track_column_migrations(&connection).expect("migrate");

        assert!(track_column_names(&connection)
            .expect("columns")
            .iter()
            .any(|name| name == "tempo_ambiguity"));
        let preserved: String = connection
            .query_row("SELECT title FROM tracks WHERE id = 't1'", [], |row| {
                row.get(0)
            })
            .expect("existing row survives the migration");
        assert_eq!(preserved, "Title");
    }

    /// Opening the same database twice must not try to add the column a second time.
    #[test]
    fn migration_is_idempotent_on_a_current_database() {
        let database = SqliteDatabase::open_in_memory().expect("open");
        let connection = database.connection();
        let guard = connection.lock().expect("lock");

        apply_track_column_migrations(&guard).expect("re-running migrations is a no-op");

        let ambiguity_columns = track_column_names(&guard)
            .expect("columns")
            .iter()
            .filter(|name| *name == "tempo_ambiguity")
            .count();
        assert_eq!(ambiguity_columns, 1);
    }
}
