//! The `Track` entity and its lifecycle state machine.

use std::fmt;
use std::path::PathBuf;

use uuid::Uuid;

use crate::camelot_key::CamelotKey;
use crate::confidence::{Confidence, Energy};
use crate::crate_::CrateId;

/// Stable internal identity of a `Track` (distinct from SoundCloud's `source_track_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackId(Uuid);

impl TrackId {
    /// Wraps a raw UUID minted by the application's `IdProvider`.
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Returns the underlying UUID (for persistence at the adapter edge).
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Where a track sits in the classification lifecycle (see the state machine in data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackStatus {
    /// Imported from likes, not yet classified.
    Scanned,
    /// Auto-classified with confidence at or above the threshold.
    AutoClassified,
    /// Confidence below threshold (or ambiguous) — awaiting a manual decision.
    InTriage,
    /// A human assigned this track; a re-scan must never re-present it (FR-018).
    ManuallyDecided,
    /// Deferred during triage; returns to the queue in a later session.
    Deferred,
}

impl TrackStatus {
    /// Lowercase persistence token for this status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scanned => "scanned",
            Self::AutoClassified => "auto_classified",
            Self::InTriage => "in_triage",
            Self::ManuallyDecided => "manually_decided",
            Self::Deferred => "deferred",
        }
    }

    /// Parses a persistence token back into a status.
    ///
    /// # Errors
    /// Returns `None` for an unrecognized token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "scanned" => Some(Self::Scanned),
            "auto_classified" => Some(Self::AutoClassified),
            "in_triage" => Some(Self::InTriage),
            "manually_decided" => Some(Self::ManuallyDecided),
            "deferred" => Some(Self::Deferred),
            _ => None,
        }
    }
}

/// Metadata for one liked track as yielded by `LikesSourcePort` — the input to a scan.
/// Vendor-free: the adapter maps SoundCloud's JSON onto this shape at the edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LikedTrack {
    /// SoundCloud's stable track id (the dedup key).
    pub source_track_id: String,
    /// Track title.
    pub title: String,
    /// Uploader / artist name.
    pub artist: String,
    /// SoundCloud genre tag, if present.
    pub source_genre: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Permalink URL (used later for opt-in download).
    pub permalink_url: String,
    /// Artwork URL, if present.
    pub artwork_url: Option<String>,
}

/// A unique liked item, enriched as the pipeline progresses.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    id: TrackId,
    source_track_id: String,
    title: String,
    artist: String,
    source_genre: Option<String>,
    duration_ms: u64,
    permalink_url: String,
    artwork_url: Option<String>,
    bpm: Option<u16>,
    camelot_key: Option<CamelotKey>,
    energy: Option<Energy>,
    vibe_tags: Vec<String>,
    crate_id: Option<CrateId>,
    confidence: Option<Confidence>,
    status: TrackStatus,
    local_audio_path: Option<PathBuf>,
}

/// The fully-hydrated field set of a `Track`, as reconstructed from persistence. Lets a repository
/// rebuild a `Track` in any lifecycle state without going through the scan/transition path.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRecord {
    /// Internal id.
    pub id: TrackId,
    /// SoundCloud stable id.
    pub source_track_id: String,
    /// Title.
    pub title: String,
    /// Artist.
    pub artist: String,
    /// Source genre tag, if any.
    pub source_genre: Option<String>,
    /// Duration in ms.
    pub duration_ms: u64,
    /// Permalink URL.
    pub permalink_url: String,
    /// Artwork URL, if any.
    pub artwork_url: Option<String>,
    /// BPM, if analyzed.
    pub bpm: Option<u16>,
    /// Camelot key, if analyzed.
    pub camelot_key: Option<CamelotKey>,
    /// Energy, if analyzed.
    pub energy: Option<Energy>,
    /// Vibe tags.
    pub vibe_tags: Vec<String>,
    /// Assigned crate, if any.
    pub crate_id: Option<CrateId>,
    /// Latest confidence, if any.
    pub confidence: Option<Confidence>,
    /// Lifecycle status.
    pub status: TrackStatus,
    /// Local audio path, if downloaded.
    pub local_audio_path: Option<PathBuf>,
}

impl Track {
    /// Rebuilds a track from its persisted record (used only by repository adapters).
    #[must_use]
    pub fn from_record(record: TrackRecord) -> Self {
        Self {
            id: record.id,
            source_track_id: record.source_track_id,
            title: record.title,
            artist: record.artist,
            source_genre: record.source_genre,
            duration_ms: record.duration_ms,
            permalink_url: record.permalink_url,
            artwork_url: record.artwork_url,
            bpm: record.bpm,
            camelot_key: record.camelot_key,
            energy: record.energy,
            vibe_tags: record.vibe_tags,
            crate_id: record.crate_id,
            confidence: record.confidence,
            status: record.status,
            local_audio_path: record.local_audio_path,
        }
    }

    /// Builds a freshly scanned track (status `Scanned`, no classification or audio yet).
    ///
    /// A blank or whitespace-only `source_genre` is normalized to `None`.
    #[must_use]
    pub fn from_scan(id: TrackId, liked: LikedTrack) -> Self {
        Self {
            id,
            source_track_id: liked.source_track_id,
            title: liked.title,
            artist: liked.artist,
            source_genre: normalize_optional(liked.source_genre),
            duration_ms: liked.duration_ms,
            permalink_url: liked.permalink_url,
            artwork_url: liked.artwork_url,
            bpm: None,
            camelot_key: None,
            energy: None,
            vibe_tags: Vec::new(),
            crate_id: None,
            confidence: None,
            status: TrackStatus::Scanned,
            local_audio_path: None,
        }
    }

    /// Returns a copy auto-classified into `crate_id` with `confidence` (status `AutoClassified`).
    #[must_use]
    pub fn assigned_auto(&self, crate_id: CrateId, confidence: Confidence) -> Self {
        Self {
            crate_id: Some(crate_id),
            confidence: Some(confidence),
            status: TrackStatus::AutoClassified,
            ..self.clone()
        }
    }

    /// Returns a copy sent to triage with `confidence` (status `InTriage`, crate suggestion
    /// dropped from the track — it is preserved in the persisted `ClassificationDecision`).
    #[must_use]
    pub fn sent_to_triage(&self, confidence: Confidence) -> Self {
        Self {
            crate_id: None,
            confidence: Some(confidence),
            status: TrackStatus::InTriage,
            ..self.clone()
        }
    }

    /// Returns a copy assigned by a human to `crate_id` (status `ManuallyDecided`).
    #[must_use]
    pub fn assigned_manual(&self, crate_id: CrateId) -> Self {
        Self {
            crate_id: Some(crate_id),
            status: TrackStatus::ManuallyDecided,
            ..self.clone()
        }
    }

    /// Returns a copy deferred out of the current triage session (status `Deferred`).
    #[must_use]
    pub fn deferred(&self) -> Self {
        Self {
            status: TrackStatus::Deferred,
            ..self.clone()
        }
    }

    /// Whether a human has already decided this track (must not be re-presented, FR-018).
    #[must_use]
    pub fn is_manually_decided(&self) -> bool {
        matches!(self.status, TrackStatus::ManuallyDecided)
    }

    // --- Accessors (map to persistence columns at the adapter edge) ---

    /// Internal identity.
    #[must_use]
    pub const fn id(&self) -> &TrackId {
        &self.id
    }

    /// SoundCloud's stable track id (dedup key).
    #[must_use]
    pub fn source_track_id(&self) -> &str {
        &self.source_track_id
    }

    /// Track title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Artist / uploader.
    #[must_use]
    pub fn artist(&self) -> &str {
        &self.artist
    }

    /// SoundCloud genre tag, if present.
    #[must_use]
    pub fn source_genre(&self) -> Option<&str> {
        self.source_genre.as_deref()
    }

    /// Duration in milliseconds.
    #[must_use]
    pub const fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    /// Permalink URL.
    #[must_use]
    pub fn permalink_url(&self) -> &str {
        &self.permalink_url
    }

    /// Artwork URL, if present.
    #[must_use]
    pub fn artwork_url(&self) -> Option<&str> {
        self.artwork_url.as_deref()
    }

    /// Detected BPM (present only after audio analysis).
    #[must_use]
    pub const fn bpm(&self) -> Option<u16> {
        self.bpm
    }

    /// Detected Camelot key (present only after audio analysis).
    #[must_use]
    pub const fn camelot_key(&self) -> Option<CamelotKey> {
        self.camelot_key
    }

    /// Detected energy (present only after audio analysis).
    #[must_use]
    pub const fn energy(&self) -> Option<Energy> {
        self.energy
    }

    /// AI-inferred vibe tags (optional, never required).
    #[must_use]
    pub fn vibe_tags(&self) -> &[String] {
        &self.vibe_tags
    }

    /// Assigned crate, if any (`None` while unclassified or in triage).
    #[must_use]
    pub const fn crate_id(&self) -> Option<&CrateId> {
        self.crate_id.as_ref()
    }

    /// Latest auto-classification confidence, if any.
    #[must_use]
    pub const fn confidence(&self) -> Option<Confidence> {
        self.confidence
    }

    /// Lifecycle status.
    #[must_use]
    pub const fn status(&self) -> TrackStatus {
        self.status
    }

    /// Local audio path, once downloaded (required for local export).
    #[must_use]
    pub fn local_audio_path(&self) -> Option<&PathBuf> {
        self.local_audio_path.as_ref()
    }
}

/// Normalizes an optional string, collapsing blank/whitespace-only values to `None`.
fn normalize_optional(value: Option<String>) -> Option<String> {
    match value {
        Some(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn liked() -> LikedTrack {
        LikedTrack {
            source_track_id: "sc:1".into(),
            title: "Track".into(),
            artist: "Artist".into(),
            source_genre: Some("  ".into()),
            duration_ms: 300_000,
            permalink_url: "https://soundcloud.com/a/track".into(),
            artwork_url: None,
        }
    }

    fn track() -> Track {
        Track::from_scan(TrackId::from_uuid(Uuid::nil()), liked())
    }

    #[test]
    fn from_scan_starts_scanned_and_blanks_whitespace_genre() {
        let t = track();
        assert_eq!(t.status(), TrackStatus::Scanned);
        assert_eq!(t.source_genre(), None);
        assert!(t.crate_id().is_none());
    }

    #[test]
    fn assigned_auto_sets_crate_confidence_and_status() {
        let crate_id = CrateId::from_uuid(Uuid::nil());
        let confidence = Confidence::new(0.9).unwrap();
        let t = track().assigned_auto(crate_id, confidence);
        assert_eq!(t.status(), TrackStatus::AutoClassified);
        assert_eq!(t.crate_id(), Some(&crate_id));
        assert_eq!(t.confidence().map(Confidence::value), Some(0.9));
    }

    #[test]
    fn sent_to_triage_drops_crate_and_marks_in_triage() {
        let t = track().sent_to_triage(Confidence::new(0.2).unwrap());
        assert_eq!(t.status(), TrackStatus::InTriage);
        assert!(t.crate_id().is_none());
    }

    #[test]
    fn manual_decision_flag() {
        let crate_id = CrateId::from_uuid(Uuid::nil());
        assert!(track().assigned_manual(crate_id).is_manually_decided());
        assert!(!track().is_manually_decided());
    }
}
