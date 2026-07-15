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
