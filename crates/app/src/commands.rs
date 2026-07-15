//! Tauri command handlers (T023/T035) and the view DTOs they return (T036/T037).
//!
//! Commands map domain types onto serializable view models at this edge (the frontend never sees a
//! domain type) and map errors onto neutral, secret-free messages (error-design). Read models for
//! crate browsing and the run summary are assembled here from the repositories.

use std::collections::BTreeMap;

use application::use_cases::classify_library::{ClassifyLibraryError, ClassifyLibrarySummary};
use application::use_cases::scan_likes::{ScanError, ScanSummary};
use domain::audit::AuditEvent;
use domain::crate_::Crate;
use domain::track::{Track, TrackId, TrackStatus};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::composition_root::AppState;

/// Result of a scan run.
#[derive(Debug, Serialize)]
pub struct ScanResultView {
    /// Total likes returned (before dedup).
    pub liked_total: usize,
    /// Duplicates collapsed within the batch.
    pub duplicates_collapsed: usize,
    /// New tracks imported this run.
    pub new_tracks: usize,
    /// Unique tracks already present.
    pub already_in_library: usize,
}

/// Result of a classify-all run.
#[derive(Debug, Serialize)]
pub struct ClassifyResultView {
    /// Tracks auto-classified.
    pub auto_classified: usize,
    /// Tracks sent to triage.
    pub sent_to_triage: usize,
    /// Tracks skipped (already routed / manually decided).
    pub skipped: usize,
}

/// A track as shown in the UI.
#[derive(Debug, Serialize)]
pub struct TrackView {
    /// Internal id (string).
    pub id: String,
    /// Title.
    pub title: String,
    /// Artist.
    pub artist: String,
    /// Source genre tag, if any.
    pub source_genre: Option<String>,
    /// Latest confidence in `[0.0, 1.0]`, if classified.
    pub confidence: Option<f32>,
    /// Lifecycle status token.
    pub status: String,
    /// BPM, if analyzed.
    pub bpm: Option<u16>,
    /// Camelot key (e.g. "8A"), if analyzed.
    pub camelot_key: Option<String>,
    /// Energy 0–100, if analyzed.
    pub energy: Option<u8>,
}

/// A crate plus its members.
#[derive(Debug, Serialize)]
pub struct CrateView {
    /// Crate id (string).
    pub id: String,
    /// Display name (e.g. "Deep House · Peak").
    pub name: String,
    /// Primary genre.
    pub genre: String,
    /// Energy role token, if any.
    pub energy_role: Option<String>,
    /// Member tracks.
    pub members: Vec<TrackView>,
}

/// One audit-trail entry for a track ("why is it in this crate?", Principle VII).
#[derive(Debug, Serialize)]
pub struct AuditEventView {
    /// Pipeline stage token.
    pub stage: String,
    /// Event kind token.
    pub kind: String,
    /// Outcome token.
    pub outcome: String,
    /// Structured, secret-free detail.
    pub detail: BTreeMap<String, String>,
    /// When it occurred (Unix ms).
    pub occurred_at: i64,
}

/// Counts of tracks by status, plus the crate count.
#[derive(Debug, Serialize)]
pub struct RunSummaryView {
    /// Total tracks in the library.
    pub total_tracks: usize,
    /// Tracks not yet classified.
    pub scanned: usize,
    /// Auto-classified tracks.
    pub auto_classified: usize,
    /// Tracks awaiting triage.
    pub in_triage: usize,
    /// Human-decided tracks.
    pub manually_decided: usize,
    /// Deferred tracks.
    pub deferred: usize,
    /// Number of crates.
    pub crate_count: usize,
}

/// Scans the likes at `profile_url`.
///
/// # Errors
/// A neutral message string when the profile cannot be read or persistence fails.
#[tauri::command]
pub async fn scan(
    state: State<'_, AppState>,
    profile_url: String,
) -> Result<ScanResultView, String> {
    let summary = state
        .scan
        .execute(&profile_url)
        .await
        .map_err(scan_error_message)?;
    Ok(scan_result_view(summary))
}

/// Classifies + routes every scanned track.
///
/// # Errors
/// A neutral message string when classification or persistence fails.
#[tauri::command]
pub async fn classify_all(state: State<'_, AppState>) -> Result<ClassifyResultView, String> {
    let summary = state
        .classify_library
        .execute()
        .await
        .map_err(classify_error_message)?;
    Ok(classify_result_view(summary))
}

/// Lists crates with their members (crate browsing view).
///
/// # Errors
/// A neutral message string when the repositories cannot be read.
#[tauri::command]
pub async fn list_crates(state: State<'_, AppState>) -> Result<Vec<CrateView>, String> {
    let crates = state.crates.list().await.map_err(repo_message)?;
    let mut views = Vec::new();
    for crate_ in crates {
        let members = state
            .tracks
            .list_by_crate(crate_.id())
            .await
            .map_err(repo_message)?;
        views.push(crate_view(&crate_, &members));
    }
    Ok(views)
}

/// Returns library counts by status plus the crate count (run summary).
///
/// # Errors
/// A neutral message string when the repositories cannot be read.
#[tauri::command]
pub async fn run_summary(state: State<'_, AppState>) -> Result<RunSummaryView, String> {
    let tracks = state.tracks.list_all().await.map_err(repo_message)?;
    let crates = state.crates.list().await.map_err(repo_message)?;
    Ok(run_summary_view(&tracks, crates.len()))
}

/// Returns a track's audit trail — answers "why is this track in this crate?" (Principle VII).
///
/// # Errors
/// A neutral message string when the track id is malformed or the audit log cannot be read.
#[tauri::command]
pub async fn track_audit(
    state: State<'_, AppState>,
    track_id: String,
) -> Result<Vec<AuditEventView>, String> {
    let uuid = Uuid::parse_str(&track_id).map_err(|_| "Invalid track id.".to_owned())?;
    let events = state
        .audit_log
        .events_for_track(&TrackId::from_uuid(uuid))
        .await
        .map_err(|_| "Could not read the audit trail.".to_owned())?;
    Ok(events.iter().map(audit_event_view).collect())
}

/// Maps a scan summary onto its view.
fn scan_result_view(summary: ScanSummary) -> ScanResultView {
    ScanResultView {
        liked_total: summary.liked_total,
        duplicates_collapsed: summary.duplicates_collapsed,
        new_tracks: summary.new_tracks,
        already_in_library: summary.already_in_library,
    }
}

/// Maps a classify summary onto its view.
fn classify_result_view(summary: ClassifyLibrarySummary) -> ClassifyResultView {
    ClassifyResultView {
        auto_classified: summary.auto_classified,
        sent_to_triage: summary.sent_to_triage,
        skipped: summary.skipped,
    }
}

/// Maps a domain track onto its view.
fn track_view(track: &Track) -> TrackView {
    TrackView {
        id: track.id().to_string(),
        title: track.title().to_owned(),
        artist: track.artist().to_owned(),
        source_genre: track.source_genre().map(str::to_owned),
        confidence: track.confidence().map(|c| c.value()),
        status: track.status().as_str().to_owned(),
        bpm: track.bpm(),
        camelot_key: track.camelot_key().map(|k| k.to_string()),
        energy: track.energy().map(|e| e.value()),
    }
}

/// Maps a domain crate + members onto its view.
fn crate_view(crate_: &Crate, members: &[Track]) -> CrateView {
    CrateView {
        id: crate_.id().to_string(),
        name: crate_.display_name(),
        genre: crate_.genre().to_owned(),
        energy_role: crate_.energy_role().map(|r| r.as_str().to_owned()),
        members: members.iter().map(track_view).collect(),
    }
}

/// Maps a domain audit event onto its view.
fn audit_event_view(event: &AuditEvent) -> AuditEventView {
    let detail = event
        .detail()
        .entries()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    AuditEventView {
        stage: event.stage().as_str().to_owned(),
        kind: event.kind().as_str().to_owned(),
        outcome: event.outcome().as_str().to_owned(),
        detail,
        occurred_at: event.occurred_at().as_millis(),
    }
}

/// Builds the run summary by counting tracks per status.
fn run_summary_view(tracks: &[Track], crate_count: usize) -> RunSummaryView {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for track in tracks {
        *counts.entry(track.status().as_str()).or_default() += 1;
    }
    let count = |status: TrackStatus| counts.get(status.as_str()).copied().unwrap_or(0);
    RunSummaryView {
        total_tracks: tracks.len(),
        scanned: count(TrackStatus::Scanned),
        auto_classified: count(TrackStatus::AutoClassified),
        in_triage: count(TrackStatus::InTriage),
        manually_decided: count(TrackStatus::ManuallyDecided),
        deferred: count(TrackStatus::Deferred),
        crate_count,
    }
}

/// Neutral, secret-free message for a scan failure.
fn scan_error_message(error: ScanError) -> String {
    match error {
        ScanError::Source(source) => source.to_string(),
        ScanError::Repo(_) => "Could not save scan results locally.".to_owned(),
        ScanError::Audit(_) => "Could not record the scan in the audit log.".to_owned(),
    }
}

/// Neutral, secret-free message for a classify failure.
fn classify_error_message(error: ClassifyLibraryError) -> String {
    match error {
        ClassifyLibraryError::Classify(_) => {
            "Classification failed for one or more tracks.".to_owned()
        }
        ClassifyLibraryError::Route(_) => {
            "Could not route a track after classifying it.".to_owned()
        }
        ClassifyLibraryError::Repo(_) => "Could not read or save library data.".to_owned(),
    }
}

/// Neutral, secret-free message for a repository failure.
fn repo_message<E>(_error: E) -> String {
    "Could not read library data.".to_owned()
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use application::ports::likes_source::LikesSourceError;
    use domain::track::TrackRecord;

    use super::*;

    /// Stands in for the `reqwest::Error` the likes adapter boxes into `Transport`. reqwest renders
    /// the failing URL in its `Display`, and for this app that URL carries the resolve query with
    /// the scraped `client_id`.
    const SENSITIVE_URL: &str =
        "https://api-v2.soundcloud.com/resolve?url=https://soundcloud.com/dj&client_id=abc123";

    #[derive(Debug)]
    struct UrlBearingError;

    impl fmt::Display for UrlBearingError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "error sending request for url ({SENSITIVE_URL})")
        }
    }

    impl std::error::Error for UrlBearingError {}

    fn track_with_status(status: TrackStatus) -> Track {
        Track::from_record(TrackRecord {
            id: TrackId::from_uuid(Uuid::nil()),
            source_track_id: "sc-1".to_owned(),
            title: "Title".to_owned(),
            artist: "Artist".to_owned(),
            source_genre: None,
            duration_ms: 180_000,
            permalink_url: "https://soundcloud.com/artist/track".to_owned(),
            artwork_url: None,
            bpm: None,
            camelot_key: None,
            energy: None,
            vibe_tags: Vec::new(),
            crate_id: None,
            confidence: None,
            status,
            local_audio_path: None,
        })
    }

    #[test]
    fn run_summary_counts_each_status_independently() {
        let tracks = vec![
            track_with_status(TrackStatus::Scanned),
            track_with_status(TrackStatus::Scanned),
            track_with_status(TrackStatus::InTriage),
            track_with_status(TrackStatus::AutoClassified),
            track_with_status(TrackStatus::ManuallyDecided),
            track_with_status(TrackStatus::Deferred),
        ];

        let view = run_summary_view(&tracks, 3);

        assert_eq!(view.total_tracks, 6);
        assert_eq!(view.scanned, 2);
        assert_eq!(view.in_triage, 1);
        assert_eq!(view.auto_classified, 1);
        assert_eq!(view.manually_decided, 1);
        assert_eq!(view.deferred, 1);
        assert_eq!(view.crate_count, 3);
    }

    #[test]
    fn run_summary_of_an_empty_library_is_all_zeroes() {
        let view = run_summary_view(&[], 0);

        assert_eq!(view.total_tracks, 0);
        assert_eq!(view.scanned, 0);
        assert_eq!(view.in_triage, 0);
        assert_eq!(view.auto_classified, 0);
        assert_eq!(view.manually_decided, 0);
        assert_eq!(view.deferred, 0);
        assert_eq!(view.crate_count, 0);
    }

    /// The `Source` arm forwards `LikesSourceError`'s own `Display`, so this crate's secret-freedom
    /// depends on every variant there keeping a constant message. Interpolating `{source}` into
    /// `Transport` would pipe the resolve URL (and its `client_id`) into the UI; this test is what
    /// catches that edit.
    #[test]
    fn scan_transport_failure_never_renders_the_wrapped_url() {
        let error = ScanError::Source(LikesSourceError::Transport {
            source: Box::new(UrlBearingError),
        });

        let message = scan_error_message(error);

        assert!(
            !message.contains("client_id"),
            "the client_id leaked into a UI message: {message}"
        );
        assert!(
            !message.contains("api-v2.soundcloud.com"),
            "the internal API URL leaked into a UI message: {message}"
        );
        assert!(
            !message.contains("soundcloud.com/dj"),
            "the user's profile URL leaked into a UI message: {message}"
        );
    }

    #[test]
    fn scan_source_failures_stay_actionable() {
        assert_eq!(
            scan_error_message(ScanError::Source(LikesSourceError::ProfileNotFound)),
            "SoundCloud profile not found"
        );
        assert_eq!(
            scan_error_message(ScanError::Source(LikesSourceError::ProfilePrivate)),
            "SoundCloud profile is private"
        );
    }
}
