//! Composition root (T022/T035) — the ONLY module that knows concrete adapter types. It wires the
//! adapters into the use cases and exposes them as [`AppState`] for the Tauri command layer.

use std::sync::Arc;

use adapters::anthropic_genre_vibe_classifier::AnthropicGenreVibeClassifier;
use adapters::internal_api_likes_source::InternalApiLikesSource;
use adapters::sqlite_audit_log::SqliteAuditLog;
use adapters::sqlite_crate_repository::SqliteCrateRepository;
use adapters::sqlite_schema::SqliteDatabase;
use adapters::sqlite_settings_repository::SqliteSettingsRepository;
use adapters::sqlite_track_repository::SqliteTrackRepository;
use adapters::system_clock_provider::SystemClockProvider;
use adapters::uuid_id_provider::UuidIdProvider;
use application::audit_recorder::AuditRecorder;
use application::ports::audit_log::AuditLogPort;
use application::ports::crate_repository::CrateRepository;
use application::ports::track_repository::TrackRepository;
use application::use_cases::classify_library::ClassifyLibrary;
use application::use_cases::classify_track::ClassifyTrack;
use application::use_cases::deduplicate_library::DeduplicateLibrary;
use application::use_cases::route_to_triage::RouteToTriage;
use application::use_cases::scan_likes::ScanLikes;

use crate::config::AppConfig;

/// The wired application, shared with Tauri command handlers via managed state.
pub struct AppState {
    /// Scans a profile's likes into the library.
    pub scan: Arc<ScanLikes>,
    /// Classifies + routes the whole library.
    pub classify_library: Arc<ClassifyLibrary>,
    /// Track reads for the UI (crate members, run summary).
    pub tracks: Arc<dyn TrackRepository>,
    /// Crate reads for the UI (crate browsing).
    pub crates: Arc<dyn CrateRepository>,
    /// Audit reads for the UI ("why is this track here?").
    pub audit_log: Arc<dyn AuditLogPort>,
}

impl AppState {
    /// Builds the application from configuration, opening the SQLite database.
    ///
    /// # Errors
    /// A message string if the database cannot be opened or migrated.
    pub fn build(config: &AppConfig) -> Result<Self, String> {
        let database = SqliteDatabase::open(config.database_path())
            .map_err(|e| format!("failed to open database: {e}"))?;

        let ids = Arc::new(UuidIdProvider::new());
        let clock = Arc::new(SystemClockProvider::new());

        let tracks: Arc<dyn TrackRepository> =
            Arc::new(SqliteTrackRepository::new(database.connection()));
        let crates: Arc<dyn CrateRepository> = Arc::new(SqliteCrateRepository::new(
            database.connection(),
            ids.clone(),
        ));
        let audit_log: Arc<dyn AuditLogPort> = Arc::new(SqliteAuditLog::new(database.connection()));
        let settings = Arc::new(SqliteSettingsRepository::new(database.connection()));

        let recorder = Arc::new(AuditRecorder::new(audit_log.clone(), clock, ids.clone()));

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
        let classify = Arc::new(ClassifyTrack::new(
            classifier,
            crates.clone(),
            recorder.clone(),
        ));
        let route = Arc::new(RouteToTriage::new(tracks.clone(), recorder));
        let classify_library = Arc::new(ClassifyLibrary::new(
            tracks.clone(),
            settings,
            classify,
            route,
            ids,
        ));

        Ok(Self {
            scan,
            classify_library,
            tracks,
            crates,
            audit_log,
        })
    }
}
