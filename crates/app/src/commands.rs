//! Tauri command handlers (T023/T035) and the view DTOs they return (T036/T037).
//!
//! Commands map domain types onto serializable view models at this edge (the frontend never sees a
//! domain type) and map errors onto neutral, secret-free messages (error-design). Read models for
//! crate browsing and the run summary are assembled here from the repositories.

use std::collections::BTreeMap;

use application::use_cases::adjust_threshold::{AdjustThresholdError, ThresholdSplit};
use application::use_cases::analyze_library::{AnalyzeLibraryError, AnalyzeLibrarySummary};
use application::use_cases::apply_manual_decision::{
    ApplyManualDecisionError, TriageAction, TriagedTrack,
};
use application::use_cases::classify_library::{ClassifyLibraryError, ClassifyLibrarySummary};
use application::use_cases::download_audio::SkipReason;
use application::use_cases::resume_deferred::ResumeDeferredError;
use application::use_cases::scan_likes::{ScanError, ScanSummary};
use domain::audit::{AuditEvent, RunId};
use domain::classification::ClassificationDecision;
use domain::confidence::ConfidenceThreshold;
use domain::crate_::{Crate, CrateId};
use domain::settings::Settings;
use domain::track::{Track, TrackId, TrackStatus};
use serde::{Deserialize, Serialize};
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
    /// Whether that BPM is ambiguous with its half/double and must not be trusted unattended
    /// (FR-031). The UI shows the number *and* the doubt — a BPM presented bare would be exactly
    /// the silent mis-tagging the flag exists to prevent.
    pub bpm_uncertain: bool,
    /// Camelot key (e.g. "8A"), if analyzed.
    pub camelot_key: Option<String>,
    /// Energy 0–100, if analyzed.
    pub energy: Option<u8>,
}

/// Result of an audio-enrichment run (US4).
#[derive(Debug, Serialize)]
pub struct AnalyzeResultView {
    /// Tracks whose audio was fetched this run.
    pub downloaded: usize,
    /// Tracks that already had their audio.
    pub already_present: usize,
    /// Tracks that gained BPM/key/energy.
    pub analyzed: usize,
    /// Tracks re-filed into an energy sub-crate.
    pub refined: usize,
    /// Tracks sent to triage (sub-threshold, or an uncertain tempo).
    pub sent_to_triage: usize,
    /// Tracks left where a human had already filed them.
    pub preserved_manual: usize,
    /// Tracks that ended with no audio features.
    pub skipped: usize,
    /// Set when the run stopped early because no track could be downloaded — carries why, so the UI
    /// can say "downloading is off" rather than "0 tracks analyzed".
    pub halted_reason: Option<String>,
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

/// A crate as an assignable option (the triage picker and the alternative chips).
#[derive(Debug, Serialize)]
pub struct CrateOptionView {
    /// Crate id (string).
    pub id: String,
    /// Display name (e.g. "Deep House · Peak").
    pub name: String,
}

/// A runner-up genre offered on a triage card. Carries the **genre, not a crate id** — the crate is
/// created only if the user picks the chip (FR-009).
#[derive(Debug, Serialize)]
pub struct GenreSuggestionView {
    /// The candidate genre.
    pub genre: String,
    /// The confidence the classifier gave it, in `[0.0, 1.0]`.
    pub confidence: f32,
}

/// One card in the triage queue: the track, its preview, the top suggestion and the alternatives
/// (FR-015).
#[derive(Debug, Serialize)]
pub struct TriageCardView {
    /// The queued track.
    pub track: TrackView,
    /// Permalink, used by the card's audio preview.
    pub permalink_url: String,
    /// Artwork URL, if any.
    pub artwork_url: Option<String>,
    /// The crate the classifier suggested — `None` when it never reached one.
    pub suggestion: Option<CrateOptionView>,
    /// Runner-up genres, as chips.
    pub alternatives: Vec<GenreSuggestionView>,
}

/// The outcome of one triage action.
#[derive(Debug, Serialize)]
pub struct TriageResultView {
    /// The track's new status token.
    pub status: String,
    /// The crate it was filed into (`None` when deferred).
    pub crate_id: Option<String>,
}

/// How a candidate threshold would divide the library (FR-013).
#[derive(Debug, Serialize)]
pub struct ThresholdSplitView {
    /// Tracks that would be filed automatically.
    pub auto: usize,
    /// Tracks that would go to triage.
    pub manual: usize,
    /// Tracks a human already decided — never re-evaluated.
    pub preserved: usize,
}

/// The user-adjustable settings the UI shows.
#[derive(Debug, Serialize)]
pub struct SettingsView {
    /// The current confidence cutoff, in `[0.0, 1.0]`.
    pub confidence_threshold: f32,
    /// Whether opt-in audio download is enabled.
    pub download_enabled: bool,
    /// Where downloaded audio would be written — shown at the opt-in gate so the user knows what
    /// turning it on will put on their disk, and where.
    pub download_dir: String,
}

/// A triage action as sent by the frontend. Mirrors the use case's `TriageAction` at the edge so no
/// domain type is named in the webview's payload.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriageActionInput {
    /// Take the suggested crate.
    AcceptSuggestion,
    /// File into an existing crate (alternative chip resolved to a crate, or the picker).
    AssignToCrate {
        /// The chosen crate id.
        crate_id: String,
    },
    /// File into `genre`, creating the crate if needed.
    CreateCrate {
        /// The genre to file under.
        genre: String,
    },
    /// Put the track back for a later session.
    Defer,
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

/// Lists the manual triage queue: each card with its suggestion and alternatives (FR-015).
///
/// # Errors
/// A neutral message string when the repositories cannot be read.
#[tauri::command]
pub async fn list_triage_queue(state: State<'_, AppState>) -> Result<Vec<TriageCardView>, String> {
    let queued = state.tracks.list_in_triage().await.map_err(repo_message)?;
    let mut cards = Vec::with_capacity(queued.len());
    for track in queued {
        let decision = state
            .decisions
            .find_latest_for_track(track.id())
            .await
            .map_err(repo_message)?;
        let suggestion = match &decision {
            Some(decision) => suggested_crate(&state, decision).await?,
            None => None,
        };
        cards.push(triage_card_view(&track, decision.as_ref(), suggestion));
    }
    Ok(cards)
}

/// Applies one triage action to one track (FR-016).
///
/// # Errors
/// A neutral message string when the track/crate is unknown or persistence fails.
#[tauri::command]
pub async fn apply_triage_action(
    state: State<'_, AppState>,
    track_id: String,
    action: TriageActionInput,
) -> Result<TriageResultView, String> {
    let track_id = parse_track_id(&track_id)?;
    let action = parse_action(action)?;
    let result = state
        .apply_manual_decision
        .execute(new_run_id(&state), &track_id, action)
        .await
        .map_err(triage_error_message)?;
    Ok(triage_result_view(&result))
}

/// Lists every crate as an assignable option (the triage picker).
///
/// # Errors
/// A neutral message string when the repositories cannot be read.
#[tauri::command]
pub async fn list_crate_options(
    state: State<'_, AppState>,
) -> Result<Vec<CrateOptionView>, String> {
    let crates = state.crates.list().await.map_err(repo_message)?;
    Ok(crates.iter().map(crate_option_view).collect())
}

/// Returns how many tracks are deferred, awaiting a later triage session.
///
/// # Errors
/// A neutral message string when the repositories cannot be read.
#[tauri::command]
pub async fn count_deferred(state: State<'_, AppState>) -> Result<usize, String> {
    let deferred = state.tracks.list_deferred().await.map_err(repo_message)?;
    Ok(deferred.len())
}

/// Puts every deferred track back on the triage queue.
///
/// # Errors
/// A neutral message string when persistence fails.
#[tauri::command]
pub async fn resume_deferred(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .resume_deferred
        .execute()
        .await
        .map_err(resume_error_message)
}

/// Returns the current settings (the threshold the slider starts from).
///
/// # Errors
/// A neutral message string when the settings cannot be read.
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsView, String> {
    let settings = state.settings.load().await.map_err(repo_message)?;
    Ok(SettingsView {
        confidence_threshold: settings.confidence_threshold().value(),
        download_enabled: settings.download_enabled(),
        download_dir: state.analyze_library.download_dir().display().to_string(),
    })
}

/// Turns opt-in audio download on or off (FR-028, Principle V).
///
/// This is the *record* of the user's choice, not the enforcement of it: `DownloadAudio` re-reads
/// the flag on every track, so flipping this off stops the next download even mid-run.
///
/// # Errors
/// A neutral message string when the setting cannot be saved.
#[tauri::command]
pub async fn set_download_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsView, String> {
    let current = state.settings.load().await.map_err(repo_message)?;
    let updated = Settings::new(
        current.confidence_threshold(),
        enabled,
        current.export_mode(),
    );
    state
        .settings
        .save(&updated)
        .await
        .map_err(|_| "Could not save the download setting.".to_owned())?;
    Ok(SettingsView {
        confidence_threshold: updated.confidence_threshold().value(),
        download_enabled: updated.download_enabled(),
        download_dir: state.analyze_library.download_dir().display().to_string(),
    })
}

/// Runs the opt-in audio path over the library: download → analyze → re-file by energy (US4).
///
/// Runs on a blocking thread: key detection and beat tracking are CPU-bound native work measured in
/// seconds per track, and holding an async worker for that would stall every other command the
/// webview issues — including the one that turns downloading back off.
///
/// # Errors
/// A neutral message string when the library cannot be read or written. A track that cannot be
/// downloaded or analyzed is counted in the summary, not raised (Principle III).
#[tauri::command]
pub async fn analyze_library(state: State<'_, AppState>) -> Result<AnalyzeResultView, String> {
    let library = state.analyze_library.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(library.execute())
    })
    .await
    .map_err(|_| "The analysis run stopped unexpectedly.".to_owned())?
    .map_err(analyze_error_message)?;
    Ok(analyze_result_view(summary))
}

/// Previews the auto-versus-manual split for `threshold` without committing it (FR-013).
///
/// # Errors
/// A neutral message string when the threshold is out of range or the library cannot be read.
#[tauri::command]
pub async fn preview_threshold(
    state: State<'_, AppState>,
    threshold: f32,
) -> Result<ThresholdSplitView, String> {
    let threshold = parse_threshold(threshold)?;
    let split = state
        .adjust_threshold
        .preview(threshold)
        .await
        .map_err(threshold_error_message)?;
    Ok(threshold_split_view(split))
}

/// Commits `threshold` and re-routes the library, preserving manual decisions (FR-013, T047).
///
/// # Errors
/// A neutral message string when the threshold is out of range or persistence fails.
#[tauri::command]
pub async fn update_threshold(
    state: State<'_, AppState>,
    threshold: f32,
) -> Result<ThresholdSplitView, String> {
    let threshold = parse_threshold(threshold)?;
    let split = state
        .adjust_threshold
        .apply(threshold)
        .await
        .map_err(threshold_error_message)?;
    Ok(threshold_split_view(split))
}

/// Mints the run id correlating the audit events of one UI-triggered action.
fn new_run_id(state: &State<'_, AppState>) -> RunId {
    RunId::from_uuid(state.ids.new_id())
}

/// Resolves a decision's chosen crate into a display option for the card's top suggestion.
async fn suggested_crate(
    state: &State<'_, AppState>,
    decision: &ClassificationDecision,
) -> Result<Option<CrateOptionView>, String> {
    let crate_ = state
        .crates
        .find_by_id(decision.crate_id())
        .await
        .map_err(repo_message)?;
    Ok(crate_.as_ref().map(crate_option_view))
}

/// Parses a track id from the frontend.
fn parse_track_id(raw: &str) -> Result<TrackId, String> {
    Uuid::parse_str(raw)
        .map(TrackId::from_uuid)
        .map_err(|_| "Invalid track id.".to_owned())
}

/// Parses and range-checks a threshold from the frontend.
fn parse_threshold(raw: f32) -> Result<ConfidenceThreshold, String> {
    ConfidenceThreshold::new(raw).map_err(|_| "Threshold must be between 0 and 1.".to_owned())
}

/// Maps the frontend's action payload onto the use case's action.
fn parse_action(input: TriageActionInput) -> Result<TriageAction, String> {
    match input {
        TriageActionInput::AcceptSuggestion => Ok(TriageAction::AcceptSuggestion),
        TriageActionInput::Defer => Ok(TriageAction::Defer),
        TriageActionInput::CreateCrate { genre } => validate_genre(genre),
        TriageActionInput::AssignToCrate { crate_id } => Uuid::parse_str(&crate_id)
            .map(|id| TriageAction::AssignToCrate(CrateId::from_uuid(id)))
            .map_err(|_| "Invalid crate id.".to_owned()),
    }
}

/// Rejects a blank crate name before it becomes an unnameable crate the user cannot find again.
fn validate_genre(genre: String) -> Result<TriageAction, String> {
    if genre.trim().is_empty() {
        return Err("A crate needs a name.".to_owned());
    }
    Ok(TriageAction::CreateCrate {
        genre: genre.trim().to_owned(),
    })
}

/// Maps a domain crate onto its assignable option.
fn crate_option_view(crate_: &Crate) -> CrateOptionView {
    CrateOptionView {
        id: crate_.id().to_string(),
        name: crate_.display_name(),
    }
}

/// Maps a queued track plus its latest decision onto a triage card.
fn triage_card_view(
    track: &Track,
    decision: Option<&ClassificationDecision>,
    suggestion: Option<CrateOptionView>,
) -> TriageCardView {
    let alternatives = decision
        .map(|decision| {
            decision
                .alternatives()
                .iter()
                .map(|alternative| GenreSuggestionView {
                    genre: alternative.genre.clone(),
                    confidence: alternative.confidence.value(),
                })
                .collect()
        })
        .unwrap_or_default();
    TriageCardView {
        track: track_view(track),
        permalink_url: track.permalink_url().to_owned(),
        artwork_url: track.artwork_url().map(str::to_owned),
        suggestion,
        alternatives,
    }
}

/// Maps a triage outcome onto its view.
fn triage_result_view(result: &TriagedTrack) -> TriageResultView {
    TriageResultView {
        status: result.track.status().as_str().to_owned(),
        crate_id: result.crate_id.map(|id| id.to_string()),
    }
}

/// Maps a threshold split onto its view.
fn threshold_split_view(split: ThresholdSplit) -> ThresholdSplitView {
    ThresholdSplitView {
        auto: split.auto,
        manual: split.manual,
        preserved: split.preserved,
    }
}

/// Neutral, secret-free message for a triage failure. The three not-found arms are actionable, so
/// they keep their own wording; persistence failures stay generic.
fn triage_error_message(error: ApplyManualDecisionError) -> String {
    match error {
        ApplyManualDecisionError::TrackNotFound => {
            "That track is no longer in the library.".to_owned()
        }
        ApplyManualDecisionError::CrateNotFound => "That crate no longer exists.".to_owned(),
        ApplyManualDecisionError::NoSuggestion => {
            "This track has no suggestion yet — pick a crate instead.".to_owned()
        }
        ApplyManualDecisionError::Repo(_) => "Could not save the decision.".to_owned(),
        ApplyManualDecisionError::Audit(_) => {
            "Could not record the decision in the audit log.".to_owned()
        }
    }
}

/// Neutral, secret-free message for a threshold failure.
fn threshold_error_message(error: AdjustThresholdError) -> String {
    match error {
        AdjustThresholdError::Repo(_) => "Could not read or save library data.".to_owned(),
        AdjustThresholdError::Route(_) => {
            "Could not re-file a track at the new threshold.".to_owned()
        }
    }
}

/// Neutral, secret-free message for a resume failure.
fn resume_error_message(error: ResumeDeferredError) -> String {
    match error {
        ResumeDeferredError::Repo(_) => "Could not restore the deferred tracks.".to_owned(),
        ResumeDeferredError::Audit(_) => "Could not record the resume in the audit log.".to_owned(),
    }
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

/// Maps an audio-enrichment summary onto its view.
fn analyze_result_view(summary: AnalyzeLibrarySummary) -> AnalyzeResultView {
    AnalyzeResultView {
        downloaded: summary.downloaded,
        already_present: summary.already_present,
        analyzed: summary.analyzed,
        refined: summary.refined,
        sent_to_triage: summary.sent_to_triage,
        preserved_manual: summary.preserved_manual,
        skipped: summary.skipped,
        halted_reason: summary.halted.map(halt_reason_message),
    }
}

/// Turns an early stop into something actionable — each of these has a different fix.
fn halt_reason_message(reason: SkipReason) -> String {
    match reason {
        SkipReason::DownloadDisabled => {
            "Audio download is off — turn it on to analyze tracks.".to_owned()
        }
        SkipReason::ToolMissing => {
            "yt-dlp is not installed. Install it (brew install yt-dlp) to download audio."
                .to_owned()
        }
        SkipReason::Unavailable => "The tracks could not be downloaded.".to_owned(),
        SkipReason::Io => "Could not write downloaded audio to disk.".to_owned(),
    }
}

/// Neutral, secret-free message for an audio-enrichment failure.
fn analyze_error_message(error: AnalyzeLibraryError) -> String {
    match error {
        AnalyzeLibraryError::Download(_) => "Could not save downloaded audio.".to_owned(),
        AnalyzeLibraryError::Analyze(_) => "Could not save analysis results.".to_owned(),
        AnalyzeLibraryError::Classify(_) => {
            "Could not re-file a track after analyzing it.".to_owned()
        }
        AnalyzeLibraryError::Route(_) => "Could not route a track after analyzing it.".to_owned(),
        AnalyzeLibraryError::Repo(_) => "Could not read or save library data.".to_owned(),
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
        bpm_uncertain: track.has_uncertain_tempo(),
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
            tempo_ambiguity: None,
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
