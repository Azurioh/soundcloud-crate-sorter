//! Composition root (T022/T035) — the ONLY module that knows concrete adapter types. It wires the
//! adapters into the use cases and exposes them as [`AppState`] for the Tauri command layer.

use std::sync::Arc;

use adapters::anthropic_genre_vibe_classifier::AnthropicGenreVibeClassifier;
use adapters::internal_api_likes_source::InternalApiLikesSource;
use adapters::libkeyfinder_aubio_audio_analyzer::LibkeyfinderAubioAudioAnalyzer;
use adapters::sqlite_audit_log::SqliteAuditLog;
use adapters::sqlite_crate_repository::SqliteCrateRepository;
use adapters::sqlite_decision_repository::SqliteDecisionRepository;
use adapters::sqlite_schema::SqliteDatabase;
use adapters::sqlite_settings_repository::SqliteSettingsRepository;
use adapters::sqlite_track_repository::SqliteTrackRepository;
use adapters::system_clock_provider::SystemClockProvider;
use adapters::uuid_id_provider::UuidIdProvider;
use adapters::ytdlp_audio_downloader::YtdlpAudioDownloader;
use application::audit_recorder::AuditRecorder;
use application::ports::audit_log::AuditLogPort;
use application::ports::crate_repository::CrateRepository;
use application::ports::decision_repository::DecisionRepository;
use application::ports::id_provider::IdProvider;
use application::ports::settings_repository::SettingsRepository;
use application::ports::track_repository::TrackRepository;
use application::use_cases::adjust_threshold::{AdjustThreshold, AdjustThresholdPorts};
use application::use_cases::analyze_audio::AnalyzeAudio;
use application::use_cases::analyze_library::{AnalyzeLibrary, AnalyzeLibraryPorts};
use application::use_cases::apply_manual_decision::{
    ApplyManualDecision, ApplyManualDecisionPorts,
};
use application::use_cases::classify_library::ClassifyLibrary;
use application::use_cases::classify_track::{ClassifyTrack, ClassifyTrackPorts};
use application::use_cases::deduplicate_library::DeduplicateLibrary;
use application::use_cases::download_audio::{DownloadAudio, DownloadAudioPorts};
use application::use_cases::resume_deferred::ResumeDeferred;
use application::use_cases::route_to_triage::RouteToTriage;
use application::use_cases::scan_likes::ScanLikes;

use crate::config::AppConfig;

/// The wired application, shared with Tauri command handlers via managed state.
pub struct AppState {
    /// Scans a profile's likes into the library.
    pub scan: Arc<ScanLikes>,
    /// Classifies + routes the whole library.
    pub classify_library: Arc<ClassifyLibrary>,
    /// Commits one human triage decision (US2).
    pub apply_manual_decision: Arc<ApplyManualDecision>,
    /// Previews and commits confidence-threshold changes (US2).
    pub adjust_threshold: Arc<AdjustThreshold>,
    /// Returns deferred tracks to the triage queue (US2).
    pub resume_deferred: Arc<ResumeDeferred>,
    /// Runs the opt-in audio path — download, analyze, refile by energy (US4).
    pub analyze_library: Arc<AnalyzeLibrary>,
    /// Track reads for the UI (crate members, run summary, triage queue).
    pub tracks: Arc<dyn TrackRepository>,
    /// Crate reads for the UI (crate browsing, triage picker).
    pub crates: Arc<dyn CrateRepository>,
    /// Decision reads for the UI (a triage card's suggestion + alternatives).
    pub decisions: Arc<dyn DecisionRepository>,
    /// Settings reads for the UI (the current threshold).
    pub settings: Arc<dyn SettingsRepository>,
    /// Audit reads for the UI ("why is this track here?").
    pub audit_log: Arc<dyn AuditLogPort>,
    /// Mints the run id correlating a UI-triggered action's audit events (the only sanctioned id
    /// source — the command layer must not reach for `Uuid::new_v4`).
    pub ids: Arc<dyn IdProvider>,
}

impl AppState {
    /// Builds the application from configuration, opening the SQLite database.
    ///
    /// # Errors
    /// A message string if the database cannot be opened or migrated.
    pub fn build(config: &AppConfig) -> Result<Self, String> {
        let database = SqliteDatabase::open(config.database_path())
            .map_err(|e| format!("failed to open database: {e}"))?;

        let ids: Arc<dyn IdProvider> = Arc::new(UuidIdProvider::new());
        let clock = Arc::new(SystemClockProvider::new());

        let tracks: Arc<dyn TrackRepository> =
            Arc::new(SqliteTrackRepository::new(database.connection()));
        let crates: Arc<dyn CrateRepository> = Arc::new(SqliteCrateRepository::new(
            database.connection(),
            ids.clone(),
        ));
        let audit_log: Arc<dyn AuditLogPort> = Arc::new(SqliteAuditLog::new(database.connection()));
        let settings: Arc<dyn SettingsRepository> =
            Arc::new(SqliteSettingsRepository::new(database.connection()));
        let decisions: Arc<dyn DecisionRepository> =
            Arc::new(SqliteDecisionRepository::new(database.connection()));

        let recorder = Arc::new(AuditRecorder::new(
            audit_log.clone(),
            clock.clone(),
            ids.clone(),
        ));

        let likes_source = Arc::new(InternalApiLikesSource::new());
        // Constructed even without a key: a classifier error degrades to a low-confidence fallback
        // that routes the track to triage, never a guessed crate. Startup warns when the key is
        // absent, because each classification then costs a rejected API round-trip.
        let classifier = Arc::new(AnthropicGenreVibeClassifier::new(
            config.api_key().unwrap_or_default().to_owned(),
        ));

        let dedup = Arc::new(DeduplicateLibrary::new(recorder.clone()));
        let scan = Arc::new(ScanLikes::new(
            likes_source,
            tracks.clone(),
            ids.clone(),
            dedup,
            recorder.clone(),
        ));
        let classify = Arc::new(ClassifyTrack::new(ClassifyTrackPorts {
            classifier,
            crates: crates.clone(),
            decisions: decisions.clone(),
            audit: recorder.clone(),
            clock: clock.clone(),
            ids: ids.clone(),
        }));
        let route = Arc::new(RouteToTriage::new(tracks.clone(), recorder.clone()));
        let classify_library = Arc::new(ClassifyLibrary::new(
            tracks.clone(),
            settings.clone(),
            classify.clone(),
            route.clone(),
            ids.clone(),
        ));
        let apply_manual_decision = Arc::new(ApplyManualDecision::new(ApplyManualDecisionPorts {
            tracks: tracks.clone(),
            crates: crates.clone(),
            decisions: decisions.clone(),
            audit: recorder.clone(),
            clock,
            ids: ids.clone(),
        }));
        let adjust_threshold = Arc::new(AdjustThreshold::new(AdjustThresholdPorts {
            tracks: tracks.clone(),
            settings: settings.clone(),
            decisions: decisions.clone(),
            route: route.clone(),
            ids: ids.clone(),
        }));
        let resume_deferred = Arc::new(ResumeDeferred::new(
            tracks.clone(),
            recorder.clone(),
            ids.clone(),
        ));

        // The audio path (US4). Constructed unconditionally: the adapters are inert until the user
        // opts in, and `DownloadAudio` re-checks that setting on every call (Principle V).
        let download = Arc::new(DownloadAudio::new(DownloadAudioPorts {
            downloader: Arc::new(YtdlpAudioDownloader::new()),
            tracks: tracks.clone(),
            settings: settings.clone(),
            audit: recorder.clone(),
        }));
        let analyze = Arc::new(AnalyzeAudio::new(
            Arc::new(LibkeyfinderAubioAudioAnalyzer::new()),
            tracks.clone(),
            recorder.clone(),
        ));
        let analyze_library = Arc::new(AnalyzeLibrary::new(AnalyzeLibraryPorts {
            tracks: tracks.clone(),
            settings: settings.clone(),
            download,
            analyze,
            classify,
            route,
            ids: ids.clone(),
            download_dir: config.download_dir().to_path_buf(),
        }));

        Ok(Self {
            scan,
            classify_library,
            apply_manual_decision,
            adjust_threshold,
            resume_deferred,
            analyze_library,
            tracks,
            crates,
            decisions,
            settings,
            audit_log,
            ids,
        })
    }
}
