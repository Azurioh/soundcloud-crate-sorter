//! `ScanLikes` — resolves a public profile, lists likes, deduplicates the batch, and persists only
//! tracks not already in the library (idempotent incremental scan, Principle IV).

use std::sync::Arc;

use thiserror::Error;

use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use domain::track::{Track, TrackId};

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audit_log::AuditError;
use crate::ports::id_provider::IdProvider;
use crate::ports::likes_source::{LikesSourceError, LikesSourcePort};
use crate::ports::repo_error::RepoError;
use crate::ports::track_repository::TrackRepository;
use crate::use_cases::deduplicate_library::DeduplicateLibrary;

/// Counts describing one scan run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanSummary {
    /// Total likes returned by the source (before dedup).
    pub liked_total: usize,
    /// Duplicates collapsed within this batch.
    pub duplicates_collapsed: usize,
    /// New tracks persisted this run.
    pub new_tracks: usize,
    /// Unique tracks already present from a prior run (left untouched).
    pub already_in_library: usize,
}

/// Failure during a scan.
#[derive(Debug, Error)]
pub enum ScanError {
    /// The likes source failed.
    #[error(transparent)]
    Source(#[from] LikesSourceError),
    /// A repository operation failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Recording an audit event failed.
    #[error(transparent)]
    Audit(#[from] AuditError),
}

/// Scans a user's public likes into the local library.
pub struct ScanLikes {
    source: Arc<dyn LikesSourcePort>,
    tracks: Arc<dyn TrackRepository>,
    ids: Arc<dyn IdProvider>,
    dedup: Arc<DeduplicateLibrary>,
    audit: Arc<AuditRecorder>,
}

impl ScanLikes {
    /// Builds the use case from its ports and collaborators.
    #[must_use]
    pub fn new(
        source: Arc<dyn LikesSourcePort>,
        tracks: Arc<dyn TrackRepository>,
        ids: Arc<dyn IdProvider>,
        dedup: Arc<DeduplicateLibrary>,
        audit: Arc<AuditRecorder>,
    ) -> Self {
        Self {
            source,
            tracks,
            ids,
            dedup,
            audit,
        }
    }

    /// Scans the likes at `profile_url`, persisting new tracks and returning a summary.
    ///
    /// # Errors
    /// [`ScanError::Source`] if the profile cannot be read; [`ScanError::Repo`] on persistence
    /// failure; [`ScanError::Audit`] if the trail cannot be written.
    pub async fn execute(&self, profile_url: &str) -> Result<ScanSummary, ScanError> {
        let run_id = RunId::from_uuid(self.ids.new_id());
        self.record_run(run_id, AuditOutcome::Started, AuditDetail::new())
            .await?;

        let user = self.source.resolve_user(profile_url).await?;
        let liked = self.source.list_likes(&user).await?;
        let liked_total = liked.len();

        let deduped = self.dedup.execute(run_id, liked).await?;

        let mut new_tracks = 0;
        let mut already_in_library = 0;
        for liked_track in deduped.unique {
            match self
                .tracks
                .find_by_source_id(&liked_track.source_track_id)
                .await?
            {
                Some(_) => already_in_library += 1,
                None => {
                    let source_id = liked_track.source_track_id.clone();
                    let track =
                        Track::from_scan(TrackId::from_uuid(self.ids.new_id()), liked_track);
                    self.tracks.upsert(&track).await?;
                    self.record_new_track(run_id, &track, &source_id).await?;
                    new_tracks += 1;
                }
            }
        }

        let summary = ScanSummary {
            liked_total,
            duplicates_collapsed: deduped.duplicates_skipped,
            new_tracks,
            already_in_library,
        };
        self.record_run(run_id, AuditOutcome::Completed, summary_detail(&summary))
            .await?;
        Ok(summary)
    }

    /// Records a run-level lifecycle event for the scan stage.
    async fn record_run(
        &self,
        run_id: RunId,
        outcome: AuditOutcome,
        detail: AuditDetail,
    ) -> Result<(), AuditError> {
        self.audit
            .record(RecordParams {
                run_id,
                track_id: None,
                stage: PipelineStage::Scan,
                kind: AuditKind::RunLifecycle,
                outcome,
                detail,
            })
            .await
    }

    /// Records the origin event for a newly imported track (starts its audit trail).
    async fn record_new_track(
        &self,
        run_id: RunId,
        track: &Track,
        source_track_id: &str,
    ) -> Result<(), AuditError> {
        self.audit
            .record(RecordParams {
                run_id,
                track_id: Some(*track.id()),
                stage: PipelineStage::Scan,
                kind: AuditKind::RunLifecycle,
                outcome: AuditOutcome::Created,
                detail: AuditDetail::new().with("source_track_id", source_track_id),
            })
            .await
    }
}

/// Builds the secret-free detail map for a run-completion event.
fn summary_detail(summary: &ScanSummary) -> AuditDetail {
    AuditDetail::new()
        .with("liked_total", summary.liked_total.to_string())
        .with(
            "duplicates_collapsed",
            summary.duplicates_collapsed.to_string(),
        )
        .with("new_tracks", summary.new_tracks.to_string())
        .with("already_in_library", summary.already_in_library.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::track::{LikedTrack, TrackStatus};

    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;
    use crate::testkit::stub_likes_source::StubLikesSource;

    fn liked(source_id: &str) -> LikedTrack {
        LikedTrack {
            source_track_id: source_id.to_owned(),
            title: "t".into(),
            artist: "a".into(),
            source_genre: Some("House".into()),
            duration_ms: 240_000,
            permalink_url: "https://sc/x".into(),
            artwork_url: None,
        }
    }

    struct Fixture {
        tracks: Arc<InMemoryTrackRepository>,
        scan: ScanLikes,
    }

    fn fixture(source: StubLikesSource) -> Fixture {
        let tracks = Arc::new(InMemoryTrackRepository::new());
        let audit_log = Arc::new(InMemoryAuditLog::new());
        let recorder = Arc::new(AuditRecorder::new(
            audit_log,
            Arc::new(FixedClock::at_millis(1_000)),
            Arc::new(SeqIdProvider::new()),
        ));
        let dedup = Arc::new(DeduplicateLibrary::new(recorder.clone()));
        let scan = ScanLikes::new(
            Arc::new(source),
            tracks.clone(),
            Arc::new(SeqIdProvider::new()),
            dedup,
            recorder,
        );
        Fixture { tracks, scan }
    }

    #[tokio::test]
    async fn scans_dedups_and_persists_new_tracks() {
        let source =
            StubLikesSource::with_likes("u1", vec![liked("sc:1"), liked("sc:2"), liked("sc:1")]);
        let fx = fixture(source);

        let summary = fx.scan.execute("https://soundcloud.com/u1").await.unwrap();

        assert_eq!(summary.liked_total, 3);
        assert_eq!(summary.duplicates_collapsed, 1);
        assert_eq!(summary.new_tracks, 2);
        assert_eq!(summary.already_in_library, 0);

        let stored = fx.tracks.list_all().await.unwrap();
        assert_eq!(stored.len(), 2);
        assert!(stored
            .iter()
            .all(|t| matches!(t.status(), TrackStatus::Scanned)));
    }

    #[tokio::test]
    async fn re_running_imports_nothing_new() {
        let source = StubLikesSource::with_likes("u1", vec![liked("sc:1"), liked("sc:2")]);
        let fx = fixture(source);

        fx.scan.execute("https://soundcloud.com/u1").await.unwrap();
        let second = fx.scan.execute("https://soundcloud.com/u1").await.unwrap();

        assert_eq!(second.new_tracks, 0);
        assert_eq!(second.already_in_library, 2);
        assert_eq!(fx.tracks.list_all().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn missing_profile_is_reported() {
        let fx = fixture(StubLikesSource::profile_not_found());
        let result = fx.scan.execute("https://soundcloud.com/nope").await;
        assert!(matches!(
            result,
            Err(ScanError::Source(LikesSourceError::ProfileNotFound))
        ));
    }
}
