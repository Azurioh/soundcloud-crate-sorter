//! `SqliteDecisionRepository` — SQLite-backed `DecisionRepository` (append-only decision history).
//!
//! `alternatives` is stored as a JSON array of `{genre, confidence}` objects; the domain never sees
//! the JSON, which is built and parsed here at the edge. A corrupt row surfaces as
//! `RepoError::Serialization` rather than a fabricated decision (constitution: corrupt persisted
//! data is an error, never a plausible default).

use application::ports::decision_repository::DecisionRepository;
use application::ports::repo_error::RepoError;
use async_trait::async_trait;
use domain::classification::{
    ClassificationDecision, ClassificationReason, DecisionId, DecisionSource, GenreSuggestion,
    NewDecision,
};
use domain::confidence::Confidence;
use domain::crate_::CrateId;
use domain::timestamp::Timestamp;
use domain::track::TrackId;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::sqlite_support::{to_repo_error, SharedConnection};

/// Column list shared by every SELECT, in the order [`read_raw_row`] expects.
const SELECT_COLUMNS: &str =
    "id, track_id, crate_id, source, confidence, reason, alternatives, decided_at";

/// One alternative genre as persisted inside the `alternatives` JSON array.
#[derive(Serialize, Deserialize)]
struct AlternativeJson {
    /// The candidate genre.
    genre: String,
    /// The confidence the classifier gave it.
    confidence: f32,
}

/// A decision row as raw SQLite primitives, before domain parsing.
struct RawDecisionRow {
    id: String,
    track_id: String,
    crate_id: String,
    source: String,
    confidence: Option<f64>,
    reason: String,
    alternatives: String,
    decided_at: i64,
}

/// SQLite-backed classification-decision store.
pub struct SqliteDecisionRepository {
    connection: SharedConnection,
}

impl SqliteDecisionRepository {
    /// Builds the repository over a shared connection.
    #[must_use]
    pub fn new(connection: SharedConnection) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl DecisionRepository for SqliteDecisionRepository {
    async fn record(&self, decision: &ClassificationDecision) -> Result<(), RepoError> {
        let alternatives = serialize_alternatives(decision.alternatives())?;
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO classification_decisions \
             (id, track_id, crate_id, source, confidence, reason, alternatives, decided_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                decision.id().as_uuid().to_string(),
                decision.track_id().as_uuid().to_string(),
                decision.crate_id().as_uuid().to_string(),
                decision.source().as_str(),
                decision.confidence().map(|c| f64::from(c.value())),
                decision.reason().as_str(),
                alternatives,
                decision.decided_at().as_millis(),
            ],
        )
        .map_err(to_repo_error)?;
        Ok(())
    }

    async fn find_latest_for_track(
        &self,
        track_id: &TrackId,
    ) -> Result<Option<ClassificationDecision>, RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let raw = conn
            .query_row(
                // `rowid` breaks ties: a re-classification writes its decision within the same
                // millisecond as the previous one, and SQLite gives no stable order for equal sort
                // keys — without it, "latest" could resolve to the superseded decision. `rowid` is
                // insertion order; `id` would be wrong here (a random v4 UUID sorts arbitrarily).
                &format!(
                    "SELECT {SELECT_COLUMNS} FROM classification_decisions WHERE track_id = ?1 \
                     ORDER BY decided_at DESC, rowid DESC LIMIT 1"
                ),
                [track_id.as_uuid().to_string()],
                read_raw_row,
            )
            .optional()
            .map_err(to_repo_error)?;
        raw.map(raw_to_decision).transpose()
    }
}

/// Serializes the alternatives into the stored JSON array.
fn serialize_alternatives(alternatives: &[GenreSuggestion]) -> Result<String, RepoError> {
    let rows: Vec<AlternativeJson> = alternatives
        .iter()
        .map(|a| AlternativeJson {
            genre: a.genre.clone(),
            confidence: a.confidence.value(),
        })
        .collect();
    serde_json::to_string(&rows).map_err(serialization)
}

/// Parses the stored JSON array back into domain suggestions, rejecting out-of-range confidences.
fn parse_alternatives(json: &str) -> Result<Vec<GenreSuggestion>, RepoError> {
    let rows: Vec<AlternativeJson> = serde_json::from_str(json).map_err(serialization)?;
    let mut suggestions = Vec::with_capacity(rows.len());
    for row in rows {
        suggestions.push(GenreSuggestion {
            genre: row.genre,
            confidence: Confidence::new(row.confidence).map_err(serialization)?,
        });
    }
    Ok(suggestions)
}

/// Reads one row into raw primitives.
fn read_raw_row(row: &Row<'_>) -> rusqlite::Result<RawDecisionRow> {
    Ok(RawDecisionRow {
        id: row.get(0)?,
        track_id: row.get(1)?,
        crate_id: row.get(2)?,
        source: row.get(3)?,
        confidence: row.get(4)?,
        reason: row.get(5)?,
        alternatives: row.get(6)?,
        decided_at: row.get(7)?,
    })
}

/// Converts a raw row into a domain decision, mapping any parse failure to `Serialization`.
fn raw_to_decision(raw: RawDecisionRow) -> Result<ClassificationDecision, RepoError> {
    let confidence = raw
        .confidence
        .map(|value| Confidence::new(value as f32).map_err(serialization))
        .transpose()?;
    let source = DecisionSource::from_token(&raw.source)
        .ok_or_else(|| serialization_msg("unknown decision source"))?;
    let reason = ClassificationReason::from_token(&raw.reason)
        .ok_or_else(|| serialization_msg("unknown classification reason"))?;
    Ok(ClassificationDecision::new(NewDecision {
        id: DecisionId::from_uuid(parse_uuid(&raw.id)?),
        track_id: TrackId::from_uuid(parse_uuid(&raw.track_id)?),
        crate_id: CrateId::from_uuid(parse_uuid(&raw.crate_id)?),
        source,
        confidence,
        reason,
        alternatives: parse_alternatives(&raw.alternatives)?,
        decided_at: Timestamp::from_millis(raw.decided_at),
    }))
}

/// Parses a stored UUID string, mapping failure to `Serialization`.
fn parse_uuid(text: &str) -> Result<Uuid, RepoError> {
    Uuid::parse_str(text).map_err(serialization)
}

/// Wraps any error as a `RepoError::Serialization`.
fn serialization<E: std::error::Error + Send + Sync + 'static>(error: E) -> RepoError {
    RepoError::Serialization {
        source: Box::new(error),
    }
}

/// Builds a `RepoError::Serialization` from a static message.
fn serialization_msg(message: &'static str) -> RepoError {
    RepoError::Serialization {
        source: message.into(),
    }
}
