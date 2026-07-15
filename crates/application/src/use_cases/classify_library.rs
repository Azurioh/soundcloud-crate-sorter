//! `ClassifyLibrary` — orchestrates `ClassifyTrack` + `RouteToTriage` over every scanned track,
//! using the persisted confidence threshold. Idempotent: already-routed and manually-decided
//! tracks are left untouched (Principle IV), so re-running only processes freshly scanned tracks.

use std::sync::Arc;

use domain::audit::RunId;
use domain::track::TrackStatus;
use thiserror::Error;

use crate::ports::id_provider::IdProvider;
use crate::ports::repo_error::RepoError;
use crate::ports::settings_repository::SettingsRepository;
use crate::ports::track_repository::TrackRepository;
use crate::use_cases::classify_track::{ClassifyTrack, ClassifyTrackError};
use crate::use_cases::route_to_triage::{RouteError, RouteOutcome, RouteToTriage};

/// Counts describing one classify-all run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassifyLibrarySummary {
    /// Tracks auto-classified (confidence met the threshold).
    pub auto_classified: usize,
    /// Tracks routed to the triage queue.
    pub sent_to_triage: usize,
    /// Tracks skipped (already routed or manually decided).
    pub skipped: usize,
}

/// Failure classifying the library.
#[derive(Debug, Error)]
pub enum ClassifyLibraryError {
    /// A per-track classification failed.
    #[error(transparent)]
    Classify(#[from] ClassifyTrackError),
    /// A per-track routing failed.
    #[error(transparent)]
    Route(#[from] RouteError),
    /// Loading settings or listing tracks failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// Classifies and routes every scanned track in the library.
pub struct ClassifyLibrary {
    tracks: Arc<dyn TrackRepository>,
    settings: Arc<dyn SettingsRepository>,
    classify: Arc<ClassifyTrack>,
    route: Arc<RouteToTriage>,
    ids: Arc<dyn IdProvider>,
}

impl ClassifyLibrary {
    /// Builds the orchestrator from its collaborators.
    #[must_use]
    pub fn new(
        tracks: Arc<dyn TrackRepository>,
        settings: Arc<dyn SettingsRepository>,
        classify: Arc<ClassifyTrack>,
        route: Arc<RouteToTriage>,
        ids: Arc<dyn IdProvider>,
    ) -> Self {
        Self {
            tracks,
            settings,
            classify,
            route,
            ids,
        }
    }

    /// Classifies + routes all `Scanned` tracks, returning a summary.
    ///
    /// # Errors
    /// [`ClassifyLibraryError`] if settings/tracks cannot be read or a track fails to classify/route.
    pub async fn execute(&self) -> Result<ClassifyLibrarySummary, ClassifyLibraryError> {
        let threshold = self.settings.load().await?.confidence_threshold();
        let run_id = RunId::from_uuid(self.ids.new_id());
        let tracks = self.tracks.list_all().await?;

        let mut summary = ClassifyLibrarySummary {
            auto_classified: 0,
            sent_to_triage: 0,
            skipped: 0,
        };
        for track in tracks {
            if !matches!(track.status(), TrackStatus::Scanned) {
                summary.skipped += 1;
                continue;
            }
            let classification = self.classify.execute(run_id, &track).await?;
            let routed = self
                .route
                .execute(run_id, &track, &classification, threshold)
                .await?;
            match routed.outcome {
                RouteOutcome::Auto => summary.auto_classified += 1,
                RouteOutcome::Triage => summary.sent_to_triage += 1,
                RouteOutcome::PreservedManual => summary.skipped += 1,
            }
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::track::{LikedTrack, Track, TrackId};
    use uuid::Uuid;

    use crate::audit_recorder::AuditRecorder;
    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_crate_repository::InMemoryCrateRepository;
    use crate::testkit::in_memory_settings_repository::InMemorySettingsRepository;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;
    use crate::testkit::stub_genre_vibe_classifier::StubGenreVibeClassifier;

    fn track(id_seed: u128, source_id: &str, genre: Option<&str>) -> Track {
        Track::from_scan(
            TrackId::from_uuid(Uuid::from_u128(id_seed)),
            LikedTrack {
                source_track_id: source_id.to_owned(),
                title: "t".into(),
                artist: "a".into(),
                source_genre: genre.map(str::to_owned),
                duration_ms: 240_000,
                permalink_url: "https://sc/x".into(),
                artwork_url: None,
            },
        )
    }

    #[tokio::test]
    async fn splits_tracks_into_auto_and_triage() {
        let tracks = Arc::new(InMemoryTrackRepository::new());
        tracks
            .upsert(&track(1, "sc:1", Some("House")))
            .await
            .unwrap(); // 0.9 → auto
        tracks.upsert(&track(2, "sc:2", None)).await.unwrap(); // classifier fails → 0.1 → triage

        let ids: Arc<SeqIdProvider> = Arc::new(SeqIdProvider::new());
        let crates = Arc::new(InMemoryCrateRepository::new(ids.clone()));
        let recorder = Arc::new(AuditRecorder::new(
            Arc::new(InMemoryAuditLog::new()),
            Arc::new(FixedClock::at_millis(1_000)),
            ids.clone(),
        ));
        let classify = Arc::new(ClassifyTrack::new(
            Arc::new(StubGenreVibeClassifier::failing()),
            crates,
            recorder.clone(),
        ));
        let route = Arc::new(RouteToTriage::new(tracks.clone(), recorder));
        let settings = Arc::new(InMemorySettingsRepository::new());

        let library = ClassifyLibrary::new(tracks, settings, classify, route, ids);
        let summary = library.execute().await.unwrap();

        assert_eq!(summary.auto_classified, 1);
        assert_eq!(summary.sent_to_triage, 1);
        assert_eq!(summary.skipped, 0);
    }
}
