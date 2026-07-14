//! `SqliteAuditLog` — append-only SQLite-backed `AuditLogPort` (T018).
//!
//! `detail` is stored as a JSON object of string→string (the secret-free `AuditDetail` map).
//! `events_for_track` reconstructs full domain `AuditEvent`s ordered oldest-first, which is what
//! makes "why is this track in this crate?" answerable (Principle VII).

use std::collections::BTreeMap;

use application::ports::audit_log::{AuditError, AuditLogPort};
use async_trait::async_trait;
use domain::audit::{
    AuditDetail, AuditEvent, AuditEventId, AuditKind, AuditOutcome, NewAuditEvent, PipelineStage,
    RunId,
};
use domain::timestamp::Timestamp;
use domain::track::TrackId;
use rusqlite::{params, Row};
use uuid::Uuid;

use crate::sqlite_support::SharedConnection;

/// Column list shared by the reconstruction SELECT.
const SELECT_COLUMNS: &str = "id, run_id, track_id, stage, kind, outcome, detail, occurred_at";

/// An audit row as raw SQLite primitives.
struct RawAuditRow {
    id: String,
    run_id: String,
    track_id: Option<String>,
    stage: String,
    kind: String,
    outcome: String,
    detail: String,
    occurred_at: i64,
}

/// SQLite-backed audit log.
pub struct SqliteAuditLog {
    connection: SharedConnection,
}

impl SqliteAuditLog {
    /// Builds the audit log over a shared connection.
    #[must_use]
    pub fn new(connection: SharedConnection) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl AuditLogPort for SqliteAuditLog {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        let detail_json = serialize_detail(event.detail()).map_err(io)?;
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO audit_events (id, run_id, track_id, stage, kind, outcome, detail, occurred_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.id().as_uuid().to_string(),
                event.run_id().as_uuid().to_string(),
                event.track_id().map(|t| t.as_uuid().to_string()),
                event.stage().as_str(),
                event.kind().as_str(),
                event.outcome().as_str(),
                detail_json,
                event.occurred_at().as_millis(),
            ],
        )
        .map_err(io)?;
        Ok(())
    }

    async fn events_for_track(&self, track_id: &TrackId) -> Result<Vec<AuditEvent>, AuditError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM audit_events WHERE track_id = ?1 ORDER BY occurred_at ASC"
        );
        let mut stmt = conn.prepare(&sql).map_err(io)?;
        let rows = stmt
            .query_map([track_id.as_uuid().to_string()], read_raw_row)
            .map_err(io)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(raw_to_event(row.map_err(io)?)?);
        }
        Ok(events)
    }
}

/// Serializes an `AuditDetail` map into a JSON object of strings.
fn serialize_detail(detail: &AuditDetail) -> Result<String, serde_json::Error> {
    let map: BTreeMap<&str, &str> = detail
        .entries()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    serde_json::to_string(&map)
}

/// Reads one row into raw primitives.
fn read_raw_row(row: &Row<'_>) -> rusqlite::Result<RawAuditRow> {
    Ok(RawAuditRow {
        id: row.get(0)?,
        run_id: row.get(1)?,
        track_id: row.get(2)?,
        stage: row.get(3)?,
        kind: row.get(4)?,
        outcome: row.get(5)?,
        detail: row.get(6)?,
        occurred_at: row.get(7)?,
    })
}

/// Reconstructs a domain `AuditEvent` from a raw row, mapping any parse failure to `Io`.
fn raw_to_event(raw: RawAuditRow) -> Result<AuditEvent, AuditError> {
    let id = AuditEventId::from_uuid(parse_uuid(&raw.id)?);
    let run_id = RunId::from_uuid(parse_uuid(&raw.run_id)?);
    let track_id = raw
        .track_id
        .map(|s| parse_uuid(&s).map(TrackId::from_uuid))
        .transpose()?;
    let stage = PipelineStage::from_token(&raw.stage).ok_or_else(|| io_msg("unknown stage"))?;
    let kind = AuditKind::from_token(&raw.kind).ok_or_else(|| io_msg("unknown kind"))?;
    let outcome =
        AuditOutcome::from_token(&raw.outcome).ok_or_else(|| io_msg("unknown outcome"))?;
    let detail = parse_detail(&raw.detail)?;
    Ok(AuditEvent::new(NewAuditEvent {
        id,
        run_id,
        track_id,
        stage,
        kind,
        outcome,
        detail,
        occurred_at: Timestamp::from_millis(raw.occurred_at),
    }))
}

/// Parses the stored JSON detail object back into an `AuditDetail`.
fn parse_detail(json: &str) -> Result<AuditDetail, AuditError> {
    let map: BTreeMap<String, String> = serde_json::from_str(json).map_err(io)?;
    let mut detail = AuditDetail::new();
    for (key, value) in map {
        detail = detail.with(&key, value);
    }
    Ok(detail)
}

/// Parses a stored UUID string, mapping failure to `Io`.
fn parse_uuid(text: &str) -> Result<Uuid, AuditError> {
    Uuid::parse_str(text).map_err(io)
}

/// Wraps any error as an `AuditError::Io`.
fn io<E: std::error::Error + Send + Sync + 'static>(error: E) -> AuditError {
    AuditError::Io {
        source: Box::new(error),
    }
}

/// Builds an `AuditError::Io` from a static message.
fn io_msg(message: &'static str) -> AuditError {
    AuditError::Io {
        source: message.into(),
    }
}
