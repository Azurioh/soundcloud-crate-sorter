//! `ResumeDeferred` — puts every deferred track back on the triage queue (data-model:
//! `Deferred → InTriage`, "next triage session").
//!
//! Deferring is the one triage action that decides nothing, so it needs an explicit way back or the
//! track would sit in `Deferred` forever, invisible to both the queue and the crates — unfinished
//! work the user can no longer see. The user starts the next session on demand rather than on a
//! timer, keeping the queue's contents predictable (Principle V).

use std::sync::Arc;

use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use thiserror::Error;

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audit_log::AuditError;
use crate::ports::id_provider::IdProvider;
use crate::ports::repo_error::RepoError;
use crate::ports::track_repository::TrackRepository;

/// Failure resuming deferred tracks.
#[derive(Debug, Error)]
pub enum ResumeDeferredError {
    /// Reading or persisting tracks failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Recording the resume failed.
    #[error(transparent)]
    Audit(#[from] AuditError),
}

/// Returns deferred tracks to the triage queue.
pub struct ResumeDeferred {
    tracks: Arc<dyn TrackRepository>,
    audit: Arc<AuditRecorder>,
    ids: Arc<dyn IdProvider>,
}

impl ResumeDeferred {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(
        tracks: Arc<dyn TrackRepository>,
        audit: Arc<AuditRecorder>,
        ids: Arc<dyn IdProvider>,
    ) -> Self {
        Self { tracks, audit, ids }
    }

    /// Moves every deferred track back to `InTriage`, returning how many were resumed.
    ///
    /// # Errors
    /// [`ResumeDeferredError::Repo`] on persistence failure; [`ResumeDeferredError::Audit`] if the
    /// trail cannot be written.
    pub async fn execute(&self) -> Result<usize, ResumeDeferredError> {
        let run_id = RunId::from_uuid(self.ids.new_id());
        let deferred = self.tracks.list_deferred().await?;
        for track in &deferred {
            self.tracks.upsert(&track.returned_to_triage()).await?;
            self.audit
                .record(RecordParams {
                    run_id,
                    track_id: Some(*track.id()),
                    stage: PipelineStage::Triage,
                    kind: AuditKind::TriageAction,
                    outcome: AuditOutcome::Updated,
                    detail: AuditDetail::new().with("action", "resume_deferred"),
                })
                .await?;
        }
        Ok(deferred.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::confidence::Confidence;
    use domain::track::{LikedTrack, Track, TrackId, TrackStatus};
    use uuid::Uuid;

    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;

    fn fixture() -> (ResumeDeferred, Arc<InMemoryTrackRepository>) {
        let ids: Arc<SeqIdProvider> = Arc::new(SeqIdProvider::new());
        let tracks = Arc::new(InMemoryTrackRepository::new());
        let recorder = Arc::new(AuditRecorder::new(
            Arc::new(InMemoryAuditLog::new()),
            Arc::new(FixedClock::at_millis(1_000)),
            ids.clone(),
        ));
        (ResumeDeferred::new(tracks.clone(), recorder, ids), tracks)
    }

    fn track(seed: u128) -> Track {
        Track::from_scan(
            TrackId::from_uuid(Uuid::from_u128(seed)),
            LikedTrack {
                source_track_id: format!("sc:{seed}"),
                title: "t".into(),
                artist: "a".into(),
                source_genre: None,
                duration_ms: 240_000,
                permalink_url: "https://sc/x".into(),
                artwork_url: None,
            },
        )
    }

    #[tokio::test]
    async fn deferred_tracks_return_to_the_queue() {
        let (resume, tracks) = fixture();
        let confidence = Confidence::new(0.2).unwrap();
        tracks
            .upsert(&track(1).sent_to_triage(confidence).deferred())
            .await
            .unwrap();
        tracks
            .upsert(&track(2).sent_to_triage(confidence))
            .await
            .unwrap();

        let resumed = resume.execute().await.unwrap();

        assert_eq!(resumed, 1);
        assert_eq!(tracks.list_in_triage().await.unwrap().len(), 2);
        assert!(tracks.list_deferred().await.unwrap().is_empty());
    }

    /// Resuming restores the queue entry without touching what the classifier concluded.
    #[tokio::test]
    async fn resuming_preserves_the_tracks_confidence() {
        let (resume, tracks) = fixture();
        tracks
            .upsert(
                &track(1)
                    .sent_to_triage(Confidence::new(0.35).unwrap())
                    .deferred(),
            )
            .await
            .unwrap();

        resume.execute().await.unwrap();

        let resumed = tracks.list_in_triage().await.unwrap();
        assert_eq!(resumed[0].status(), TrackStatus::InTriage);
        assert_eq!(resumed[0].confidence().map(Confidence::value), Some(0.35));
    }

    #[tokio::test]
    async fn resuming_an_empty_deferred_list_is_a_no_op() {
        let (resume, _tracks) = fixture();
        assert_eq!(resume.execute().await.unwrap(), 0);
    }
}
