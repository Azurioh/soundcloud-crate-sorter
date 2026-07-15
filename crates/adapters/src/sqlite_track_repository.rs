//! `SqliteTrackRepository` — SQLite-backed `TrackRepository` (T017).
//!
//! Vendor types (rusqlite rows) are mapped to/from domain at the edge: rows are first read into a
//! primitive [`RawTrackRow`], then converted to a `Track` so all domain parsing errors surface as
//! [`RepoError::Serialization`] rather than a `rusqlite::Error`.

use std::path::PathBuf;

use application::ports::repo_error::RepoError;
use application::ports::track_repository::TrackRepository;
use async_trait::async_trait;
use domain::camelot_key::CamelotKey;
use domain::confidence::{Confidence, Energy};
use domain::crate_::CrateId;
use domain::track::{Track, TrackId, TrackRecord, TrackStatus};
use rusqlite::{OptionalExtension, Row};
use uuid::Uuid;

use crate::sqlite_support::{to_repo_error, SharedConnection};

/// The persisted status token for a human-decided track (preserved across upserts).
const MANUALLY_DECIDED_TOKEN: &str = "manually_decided";

/// Column list shared by every SELECT, in the order [`read_raw_row`] expects.
const SELECT_COLUMNS: &str = "id, source_track_id, title, artist, source_genre, duration_ms, \
    permalink_url, artwork_url, bpm, camelot_key, energy, vibe_tags, crate_id, confidence, status, \
    local_audio_path";

/// A track row as raw SQLite primitives, before domain parsing.
struct RawTrackRow {
    id: String,
    source_track_id: String,
    title: String,
    artist: String,
    source_genre: Option<String>,
    duration_ms: i64,
    permalink_url: String,
    artwork_url: Option<String>,
    bpm: Option<i64>,
    camelot_key: Option<String>,
    energy: Option<i64>,
    vibe_tags: String,
    crate_id: Option<String>,
    confidence: Option<f64>,
    status: String,
    local_audio_path: Option<String>,
}

/// SQLite-backed track store.
pub struct SqliteTrackRepository {
    connection: SharedConnection,
}

impl SqliteTrackRepository {
    /// Builds the repository over a shared connection.
    #[must_use]
    pub fn new(connection: SharedConnection) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl TrackRepository for SqliteTrackRepository {
    async fn find_by_source_id(&self, source_id: &str) -> Result<Option<Track>, RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let sql = format!("SELECT {SELECT_COLUMNS} FROM tracks WHERE source_track_id = ?1");
        let raw = conn
            .query_row(&sql, [source_id], read_raw_row)
            .optional()
            .map_err(to_repo_error)?;
        raw.map(raw_to_track).transpose()
    }

    async fn find_by_id(&self, id: &TrackId) -> Result<Option<Track>, RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let sql = format!("SELECT {SELECT_COLUMNS} FROM tracks WHERE id = ?1");
        let raw = conn
            .query_row(&sql, [id.to_string()], read_raw_row)
            .optional()
            .map_err(to_repo_error)?;
        raw.map(raw_to_track).transpose()
    }

    async fn upsert(&self, track: &Track) -> Result<(), RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");

        // Idempotency (Principle IV): never downgrade a human decision on re-classification.
        let existing_status: Option<String> = conn
            .query_row(
                "SELECT status FROM tracks WHERE id = ?1",
                [track.id().to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(to_repo_error)?;
        if existing_status.as_deref() == Some(MANUALLY_DECIDED_TOKEN)
            && !track.is_manually_decided()
        {
            return Ok(());
        }

        conn.execute(UPSERT_SQL, track_params(track)?)
            .map_err(to_repo_error)?;
        Ok(())
    }

    async fn list_all(&self) -> Result<Vec<Track>, RepoError> {
        self.query_tracks(&format!("SELECT {SELECT_COLUMNS} FROM tracks"), [])
    }

    async fn list_in_triage(&self) -> Result<Vec<Track>, RepoError> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM tracks WHERE status = 'in_triage'");
        self.query_tracks(&sql, [])
    }

    async fn list_by_crate(&self, crate_id: &CrateId) -> Result<Vec<Track>, RepoError> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM tracks WHERE crate_id = ?1");
        self.query_tracks(&sql, [crate_id.to_string()])
    }
}

impl SqliteTrackRepository {
    /// Runs a SELECT and maps every row to a `Track`.
    fn query_tracks<P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Vec<Track>, RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let mut stmt = conn.prepare(sql).map_err(to_repo_error)?;
        let rows = stmt
            .query_map(params, read_raw_row)
            .map_err(to_repo_error)?;
        let mut tracks = Vec::new();
        for row in rows {
            let raw = row.map_err(to_repo_error)?;
            tracks.push(raw_to_track(raw)?);
        }
        Ok(tracks)
    }
}

/// The upsert statement (insert or update-on-id-conflict). `source_track_id` UNIQUE still guards
/// against a second internal id claiming an existing source id (surfaces as `Constraint`).
const UPSERT_SQL: &str = "
INSERT INTO tracks (id, source_track_id, title, artist, source_genre, duration_ms, permalink_url,
    artwork_url, bpm, camelot_key, energy, vibe_tags, crate_id, confidence, status, local_audio_path)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
ON CONFLICT(id) DO UPDATE SET
    source_track_id = excluded.source_track_id,
    title           = excluded.title,
    artist          = excluded.artist,
    source_genre    = excluded.source_genre,
    duration_ms     = excluded.duration_ms,
    permalink_url   = excluded.permalink_url,
    artwork_url     = excluded.artwork_url,
    bpm             = excluded.bpm,
    camelot_key     = excluded.camelot_key,
    energy          = excluded.energy,
    vibe_tags       = excluded.vibe_tags,
    crate_id        = excluded.crate_id,
    confidence      = excluded.confidence,
    status          = excluded.status,
    local_audio_path = excluded.local_audio_path
";

/// Builds the positional params for [`UPSERT_SQL`] from a domain track.
fn track_params(
    track: &Track,
) -> Result<rusqlite::ParamsFromIter<Vec<rusqlite::types::Value>>, RepoError> {
    use rusqlite::types::Value;
    let vibe_tags =
        serde_json::to_string(track.vibe_tags()).map_err(|e| RepoError::Serialization {
            source: Box::new(e),
        })?;
    let values: Vec<Value> = vec![
        Value::Text(track.id().to_string()),
        Value::Text(track.source_track_id().to_owned()),
        Value::Text(track.title().to_owned()),
        Value::Text(track.artist().to_owned()),
        optional_text(track.source_genre().map(str::to_owned)),
        Value::Integer(i64::try_from(track.duration_ms()).map_err(serialization)?),
        Value::Text(track.permalink_url().to_owned()),
        optional_text(track.artwork_url().map(str::to_owned)),
        track
            .bpm()
            .map_or(Value::Null, |b| Value::Integer(i64::from(b))),
        track
            .camelot_key()
            .map_or(Value::Null, |k| Value::Text(k.to_string())),
        track
            .energy()
            .map_or(Value::Null, |e| Value::Integer(i64::from(e.value()))),
        Value::Text(vibe_tags),
        track
            .crate_id()
            .map_or(Value::Null, |c| Value::Text(c.to_string())),
        track
            .confidence()
            .map_or(Value::Null, |c| Value::Real(f64::from(c.value()))),
        Value::Text(track.status().as_str().to_owned()),
        optional_text(
            track
                .local_audio_path()
                .map(|p| p.to_string_lossy().into_owned()),
        ),
    ];
    Ok(rusqlite::params_from_iter(values))
}

/// Maps an optional string to a nullable SQLite value.
fn optional_text(value: Option<String>) -> rusqlite::types::Value {
    value.map_or(rusqlite::types::Value::Null, rusqlite::types::Value::Text)
}

/// Reads one row into raw primitives (in [`SELECT_COLUMNS`] order).
fn read_raw_row(row: &Row<'_>) -> rusqlite::Result<RawTrackRow> {
    Ok(RawTrackRow {
        id: row.get(0)?,
        source_track_id: row.get(1)?,
        title: row.get(2)?,
        artist: row.get(3)?,
        source_genre: row.get(4)?,
        duration_ms: row.get(5)?,
        permalink_url: row.get(6)?,
        artwork_url: row.get(7)?,
        bpm: row.get(8)?,
        camelot_key: row.get(9)?,
        energy: row.get(10)?,
        vibe_tags: row.get(11)?,
        crate_id: row.get(12)?,
        confidence: row.get(13)?,
        status: row.get(14)?,
        local_audio_path: row.get(15)?,
    })
}

/// Converts a raw row into a domain `Track`, mapping any parse failure to `Serialization`.
fn raw_to_track(raw: RawTrackRow) -> Result<Track, RepoError> {
    let id = TrackId::from_uuid(parse_uuid(&raw.id)?);
    let camelot_key = raw
        .camelot_key
        .map(|s| CamelotKey::parse(&s).map_err(serialization))
        .transpose()?;
    // `try_from` rather than `unwrap_or(u8::MAX)`: the latter only errored by luck, because 255
    // happens to exceed today's ENERGY_MAX of 100. Raise that ceiling to 255 and a corrupt `999`
    // would silently land as a valid `Energy(255)` — a fabricated value. Propagate instead.
    let energy = raw
        .energy
        .map(|e| {
            let raw_energy = u8::try_from(e).map_err(serialization)?;
            Energy::new(raw_energy).map_err(serialization)
        })
        .transpose()?;
    let confidence = raw
        .confidence
        .map(|c| Confidence::new(c as f32).map_err(serialization))
        .transpose()?;
    let crate_id = raw
        .crate_id
        .map(|s| parse_uuid(&s).map(CrateId::from_uuid))
        .transpose()?;
    let vibe_tags: Vec<String> =
        serde_json::from_str(&raw.vibe_tags).map_err(|e| RepoError::Serialization {
            source: Box::new(e),
        })?;
    let status = TrackStatus::from_token(&raw.status).ok_or_else(|| RepoError::Serialization {
        source: "unknown track status".into(),
    })?;
    // Surface corrupt numerics rather than fabricating plausible values (0 / u16::MAX) from them,
    // consistent with the energy/confidence handling above.
    let duration_ms = u64::try_from(raw.duration_ms).map_err(serialization)?;
    let bpm = raw
        .bpm
        .map(|b| u16::try_from(b).map_err(serialization))
        .transpose()?;

    Ok(Track::from_record(TrackRecord {
        id,
        source_track_id: raw.source_track_id,
        title: raw.title,
        artist: raw.artist,
        source_genre: raw.source_genre,
        duration_ms,
        permalink_url: raw.permalink_url,
        artwork_url: raw.artwork_url,
        bpm,
        camelot_key,
        energy,
        vibe_tags,
        crate_id,
        confidence,
        status,
        local_audio_path: raw.local_audio_path.map(PathBuf::from),
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
