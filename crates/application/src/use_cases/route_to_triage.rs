//! `RouteToTriage` — decides a track's final status from its confidence vs the threshold.
//!
//! At or above the threshold the track is auto-classified into its crate; below it, the track goes
//! to the manual triage queue (data-model state machine). A `ManuallyDecided` track is never
//! re-routed (idempotency, Principle IV) — this makes threshold re-evaluation safe (T047).

use std::sync::Arc;

use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use domain::confidence::ConfidenceThreshold;
use domain::track::{Track, TrackId};
use thiserror::Error;

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audit_log::AuditError;
use crate::ports::repo_error::RepoError;
use crate::ports::track_repository::TrackRepository;
use crate::use_cases::classify_track::Classification;

/// The routing decision for a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteOutcome {
    /// Confidence met the threshold — auto-classified into its crate.
    Auto,
    /// Confidence fell short — sent to the manual triage queue.
    Triage,
    /// The track was already decided by a human and left untouched.
    PreservedManual,
}

impl RouteOutcome {
    /// Lowercase token for audit detail.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto_classified",
            Self::Triage => "in_triage",
            Self::PreservedManual => "preserved_manual",
        }
    }
}

/// A track after routing, plus the decision taken.
#[derive(Debug, Clone)]
pub struct RoutedTrack {
    /// The (possibly updated) track.
    pub track: Track,
    /// What the router decided.
    pub outcome: RouteOutcome,
}

/// Failure routing a track.
#[derive(Debug, Error)]
pub enum RouteError {
    /// Persisting the routed track failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Recording the routing event failed.
    #[error(transparent)]
    Audit(#[from] AuditError),
}

/// Routes a classified track to auto-classification or triage.
pub struct RouteToTriage {
    tracks: Arc<dyn TrackRepository>,
    audit: Arc<AuditRecorder>,
}

impl RouteToTriage {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(tracks: Arc<dyn TrackRepository>, audit: Arc<AuditRecorder>) -> Self {
        Self { tracks, audit }
    }

    /// Routes `track` given its `classification` and the current `threshold`, persisting the result.
    ///
    /// # Errors
    /// [`RouteError::Repo`] on persistence failure; [`RouteError::Audit`] if the trail cannot be
    /// written.
    pub async fn execute(
        &self,
        run_id: RunId,
        track: &Track,
        classification: &Classification,
        threshold: ConfidenceThreshold,
    ) -> Result<RoutedTrack, RouteError> {
        if track.is_manually_decided() {
            return Ok(RoutedTrack {
                track: track.clone(),
                outcome: RouteOutcome::PreservedManual,
            });
        }

        let (routed, outcome) = if classification.confidence.meets(threshold) {
            (
                track.assigned_auto(classification.crate_id, classification.confidence),
                RouteOutcome::Auto,
            )
        } else {
            (
                track.sent_to_triage(classification.confidence),
                RouteOutcome::Triage,
            )
        };

        self.tracks.upsert(&routed).await?;
        let detail = routing_detail(classification, threshold, outcome);
        self.record_routing(run_id, *routed.id(), detail).await?;
        Ok(RoutedTrack {
            track: routed,
            outcome,
        })
    }

    /// Records the routing outcome (status + confidence + threshold) for answerability.
    async fn record_routing(
        &self,
        run_id: RunId,
        track_id: TrackId,
        detail: AuditDetail,
    ) -> Result<(), AuditError> {
        self.audit
            .record(RecordParams {
                run_id,
                track_id: Some(track_id),
                stage: PipelineStage::Classify,
                kind: AuditKind::Classification,
                outcome: AuditOutcome::Completed,
                detail,
            })
            .await
    }
}

/// Builds the secret-free routing-detail map (status + confidence + threshold).
fn routing_detail(
    classification: &Classification,
    threshold: ConfidenceThreshold,
    outcome: RouteOutcome,
) -> AuditDetail {
    AuditDetail::new()
        .with("status", outcome.as_str())
        .with(
            "confidence",
            format!("{:.3}", classification.confidence.value()),
        )
        .with("threshold", format!("{:.3}", threshold.value()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::classification::ClassificationReason;
    use domain::confidence::Confidence;
    use domain::crate_::CrateId;
    use domain::track::{LikedTrack, TrackId, TrackStatus};
    use uuid::Uuid;

    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;

    fn track() -> Track {
        Track::from_scan(
            TrackId::from_uuid(Uuid::from_u128(42)),
            LikedTrack {
                source_track_id: "sc:1".into(),
                title: "t".into(),
                artist: "a".into(),
                source_genre: Some("House".into()),
                duration_ms: 240_000,
                permalink_url: "https://sc/x".into(),
                artwork_url: None,
            },
        )
    }

    fn classification(confidence: f32) -> Classification {
        Classification {
            crate_id: CrateId::from_uuid(Uuid::from_u128(7)),
            confidence: Confidence::new(confidence).unwrap(),
            reason: ClassificationReason::GenreFromSourceTag,
            vibe_tags: Vec::new(),
        }
    }

    fn fixture() -> (RouteToTriage, Arc<InMemoryTrackRepository>) {
        let tracks = Arc::new(InMemoryTrackRepository::new());
        let recorder = Arc::new(AuditRecorder::new(
            Arc::new(InMemoryAuditLog::new()),
            Arc::new(FixedClock::at_millis(1_000)),
            Arc::new(SeqIdProvider::new()),
        ));
        (RouteToTriage::new(tracks.clone(), recorder), tracks)
    }

    fn threshold() -> ConfidenceThreshold {
        ConfidenceThreshold::new(0.6).unwrap()
    }

    #[tokio::test]
    async fn above_threshold_auto_classifies() {
        let (route, tracks) = fixture();
        let routed = route
            .execute(
                RunId::from_uuid(Uuid::nil()),
                &track(),
                &classification(0.9),
                threshold(),
            )
            .await
            .unwrap();

        assert_eq!(routed.outcome, RouteOutcome::Auto);
        assert_eq!(routed.track.status(), TrackStatus::AutoClassified);
        assert!(routed.track.crate_id().is_some());
        assert_eq!(tracks.list_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn below_threshold_sends_to_triage() {
        let (route, _tracks) = fixture();
        let routed = route
            .execute(
                RunId::from_uuid(Uuid::nil()),
                &track(),
                &classification(0.2),
                threshold(),
            )
            .await
            .unwrap();

        assert_eq!(routed.outcome, RouteOutcome::Triage);
        assert_eq!(routed.track.status(), TrackStatus::InTriage);
        assert!(routed.track.crate_id().is_none());
    }

    #[tokio::test]
    async fn manually_decided_track_is_preserved() {
        let (route, _tracks) = fixture();
        let manual = track().assigned_manual(CrateId::from_uuid(Uuid::from_u128(99)));
        let routed = route
            .execute(
                RunId::from_uuid(Uuid::nil()),
                &manual,
                &classification(0.2),
                threshold(),
            )
            .await
            .unwrap();

        assert_eq!(routed.outcome, RouteOutcome::PreservedManual);
        assert_eq!(routed.track.status(), TrackStatus::ManuallyDecided);
    }
}
