//! `AdjustThreshold` — previews and commits a change to the confidence cutoff (FR-013).
//!
//! `preview` answers "what would this threshold do?" without touching a thing, so the user sees the
//! auto-versus-manual split *before* committing. `apply` persists the new threshold and re-routes
//! the library through the same rule.
//!
//! Re-routing replays each track's stored classification decision against the new cutoff rather than
//! re-classifying it: the decision is what the classifier concluded, and that conclusion does not
//! change because the user moved a slider. It also means no AI call and no network on a slider drag.
//! `ManuallyDecided` tracks are never re-evaluated (FR-018, Principle IV) — [`RouteToTriage`] owns
//! that guarantee, and both paths here route through it.

use std::sync::Arc;

use domain::audit::RunId;
use domain::confidence::ConfidenceThreshold;
use domain::track::Track;
use thiserror::Error;

use crate::ports::decision_repository::DecisionRepository;
use crate::ports::id_provider::IdProvider;
use crate::ports::repo_error::RepoError;
use crate::ports::settings_repository::SettingsRepository;
use crate::ports::track_repository::TrackRepository;
use crate::use_cases::classify_track::Classification;
use crate::use_cases::route_to_triage::{RouteError, RouteOutcome, RouteToTriage};

/// How a candidate threshold would divide the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThresholdSplit {
    /// Tracks that would be filed automatically.
    pub auto: usize,
    /// Tracks that would go to the manual triage queue.
    pub manual: usize,
    /// Tracks a human already decided — untouched by any threshold change (FR-018).
    pub preserved: usize,
}

/// Failure previewing or applying a threshold.
#[derive(Debug, Error)]
pub enum AdjustThresholdError {
    /// Reading tracks/decisions or saving settings failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Re-routing a track failed.
    #[error(transparent)]
    Route(#[from] RouteError),
}

/// The collaborators `AdjustThreshold` needs (grouped per the 2+-params convention).
pub struct AdjustThresholdPorts {
    /// Reads the library.
    pub tracks: Arc<dyn TrackRepository>,
    /// Loads and saves the threshold.
    pub settings: Arc<dyn SettingsRepository>,
    /// Supplies each track's stored classification to replay.
    pub decisions: Arc<dyn DecisionRepository>,
    /// Applies the auto-versus-triage rule (and preserves manual decisions).
    pub route: Arc<RouteToTriage>,
    /// Mints the run id correlating one re-evaluation.
    pub ids: Arc<dyn IdProvider>,
}

/// Previews and commits confidence-threshold changes.
pub struct AdjustThreshold {
    tracks: Arc<dyn TrackRepository>,
    settings: Arc<dyn SettingsRepository>,
    decisions: Arc<dyn DecisionRepository>,
    route: Arc<RouteToTriage>,
    ids: Arc<dyn IdProvider>,
}

impl AdjustThreshold {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(ports: AdjustThresholdPorts) -> Self {
        Self {
            tracks: ports.tracks,
            settings: ports.settings,
            decisions: ports.decisions,
            route: ports.route,
            ids: ports.ids,
        }
    }

    /// Returns the split `threshold` would produce, changing nothing (FR-013).
    ///
    /// # Errors
    /// [`AdjustThresholdError::Repo`] when the library cannot be read.
    pub async fn preview(
        &self,
        threshold: ConfidenceThreshold,
    ) -> Result<ThresholdSplit, AdjustThresholdError> {
        let tracks = self.tracks.list_all().await?;
        Ok(split_for(&tracks, threshold))
    }

    /// Persists `threshold` and re-routes every track that is not `ManuallyDecided`.
    ///
    /// # Errors
    /// [`AdjustThresholdError::Repo`] on persistence failure; [`AdjustThresholdError::Route`] when a
    /// track cannot be re-routed.
    pub async fn apply(
        &self,
        threshold: ConfidenceThreshold,
    ) -> Result<ThresholdSplit, AdjustThresholdError> {
        let settings = self.settings.load().await?;
        self.settings
            .save(&settings.with_confidence_threshold(threshold))
            .await?;

        let run_id = RunId::from_uuid(self.ids.new_id());
        let tracks = self.tracks.list_all().await?;
        let mut split = ThresholdSplit {
            auto: 0,
            manual: 0,
            preserved: 0,
        };
        for track in tracks {
            match self.reroute(run_id, &track, threshold).await? {
                Some(RouteOutcome::Auto) => split.auto += 1,
                Some(RouteOutcome::Triage) => split.manual += 1,
                Some(RouteOutcome::PreservedManual) => split.preserved += 1,
                None => {}
            }
        }
        Ok(split)
    }

    /// Re-routes one track against `threshold`, or `None` when it has no replayable decision (a
    /// track scanned but never classified, or one whose latest decision is a human's pick and so
    /// carries no confidence to compare).
    async fn reroute(
        &self,
        run_id: RunId,
        track: &Track,
        threshold: ConfidenceThreshold,
    ) -> Result<Option<RouteOutcome>, AdjustThresholdError> {
        let Some(decision) = self.decisions.find_latest_for_track(track.id()).await? else {
            return Ok(None);
        };
        let Some(confidence) = decision.confidence() else {
            return Ok(None);
        };
        let classification = Classification {
            crate_id: *decision.crate_id(),
            confidence,
            reason: decision.reason(),
            vibe_tags: track.vibe_tags().to_vec(),
            alternatives: decision.alternatives().to_vec(),
        };
        let routed = self
            .route
            .execute(run_id, track, &classification, threshold)
            .await?;
        Ok(Some(routed.outcome))
    }
}

/// Counts how `threshold` would divide `tracks`, using each track's latest recorded confidence.
///
/// Mirrors [`RouteToTriage`]: a `ManuallyDecided` track is preserved rather than counted on either
/// side, and a track with no confidence yet (scanned, never classified) is not predicted at all.
fn split_for(tracks: &[Track], threshold: ConfidenceThreshold) -> ThresholdSplit {
    let mut split = ThresholdSplit {
        auto: 0,
        manual: 0,
        preserved: 0,
    };
    for track in tracks {
        if track.is_manually_decided() {
            split.preserved += 1;
            continue;
        }
        let Some(confidence) = track.confidence() else {
            continue;
        };
        if confidence.meets(threshold) {
            split.auto += 1;
        } else {
            split.manual += 1;
        }
    }
    split
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::confidence::Confidence;
    use domain::crate_::{CrateId, CrateOrigin};
    use domain::track::{LikedTrack, TrackId, TrackStatus};
    use uuid::Uuid;

    use crate::audit_recorder::AuditRecorder;
    use crate::ports::crate_repository::{CrateRepository, CrateSpec};
    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_crate_repository::InMemoryCrateRepository;
    use crate::testkit::in_memory_decision_repository::InMemoryDecisionRepository;
    use crate::testkit::in_memory_settings_repository::InMemorySettingsRepository;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;

    use domain::classification::{
        ClassificationDecision, ClassificationReason, DecisionId, DecisionSource, NewDecision,
    };
    use domain::timestamp::Timestamp;

    struct Fixture {
        adjust: AdjustThreshold,
        tracks: Arc<InMemoryTrackRepository>,
        crates: Arc<InMemoryCrateRepository>,
        decisions: Arc<InMemoryDecisionRepository>,
        settings: Arc<InMemorySettingsRepository>,
    }

    fn fixture() -> Fixture {
        let ids: Arc<SeqIdProvider> = Arc::new(SeqIdProvider::new());
        let tracks = Arc::new(InMemoryTrackRepository::new());
        let crates = Arc::new(InMemoryCrateRepository::new(ids.clone()));
        let decisions = Arc::new(InMemoryDecisionRepository::new());
        let settings = Arc::new(InMemorySettingsRepository::new());
        let recorder = Arc::new(AuditRecorder::new(
            Arc::new(InMemoryAuditLog::new()),
            Arc::new(FixedClock::at_millis(1_000)),
            ids.clone(),
        ));
        let route = Arc::new(RouteToTriage::new(tracks.clone(), recorder));
        let adjust = AdjustThreshold::new(AdjustThresholdPorts {
            tracks: tracks.clone(),
            settings: settings.clone(),
            decisions: decisions.clone(),
            route,
            ids,
        });
        Fixture {
            adjust,
            tracks,
            crates,
            decisions,
            settings,
        }
    }

    fn track(seed: u128) -> Track {
        Track::from_scan(
            TrackId::from_uuid(Uuid::from_u128(seed)),
            LikedTrack {
                source_track_id: format!("sc:{seed}"),
                title: "t".into(),
                artist: "a".into(),
                source_genre: Some("House".into()),
                duration_ms: 240_000,
                permalink_url: "https://sc/x".into(),
                artwork_url: None,
            },
        )
    }

    fn threshold(value: f32) -> ConfidenceThreshold {
        ConfidenceThreshold::new(value).unwrap()
    }

    fn confidence(value: f32) -> Confidence {
        Confidence::new(value).unwrap()
    }

    /// Seeds a track already classified at `score`, with the auto decision behind it.
    async fn seed_classified(fx: &Fixture, seed: u128, score: f32) -> CrateId {
        let crate_ = fx
            .crates
            .find_or_create(&CrateSpec {
                genre: "House".into(),
                role: None,
                origin: CrateOrigin::Auto,
            })
            .await
            .unwrap();
        let track = track(seed).assigned_auto(*crate_.id(), confidence(score));
        fx.tracks.upsert(&track).await.unwrap();
        fx.decisions
            .record(&ClassificationDecision::new(NewDecision {
                id: DecisionId::from_uuid(Uuid::from_u128(seed + 500)),
                track_id: *track.id(),
                crate_id: *crate_.id(),
                source: DecisionSource::Auto,
                confidence: Some(confidence(score)),
                reason: ClassificationReason::GenreFromSourceTag,
                alternatives: Vec::new(),
                decided_at: Timestamp::from_millis(500),
            }))
            .await
            .unwrap();
        *crate_.id()
    }

    #[tokio::test]
    async fn preview_splits_on_the_candidate_threshold_without_touching_anything() {
        let fx = fixture();
        seed_classified(&fx, 1, 0.9).await;
        seed_classified(&fx, 2, 0.5).await;

        let strict = fx.adjust.preview(threshold(0.8)).await.unwrap();
        assert_eq!(strict.auto, 1);
        assert_eq!(strict.manual, 1);

        let lax = fx.adjust.preview(threshold(0.4)).await.unwrap();
        assert_eq!(lax.auto, 2);
        assert_eq!(lax.manual, 0);

        // Preview is a question, not a command.
        assert_eq!(
            fx.settings.load().await.unwrap().confidence_threshold(),
            threshold(0.6),
            "preview must not persist the candidate threshold"
        );
        let untouched_id = TrackId::from_uuid(Uuid::from_u128(2));
        let reloaded = fx.tracks.find_by_id(&untouched_id).await.unwrap().unwrap();
        assert_eq!(
            reloaded.status(),
            TrackStatus::AutoClassified,
            "preview must not re-route tracks"
        );
    }

    #[tokio::test]
    async fn applying_a_stricter_threshold_persists_it_and_reroutes_to_triage() {
        let fx = fixture();
        seed_classified(&fx, 1, 0.9).await;
        seed_classified(&fx, 2, 0.5).await;

        let split = fx.adjust.apply(threshold(0.8)).await.unwrap();

        assert_eq!(split.auto, 1);
        assert_eq!(split.manual, 1);
        assert_eq!(
            fx.settings.load().await.unwrap().confidence_threshold(),
            threshold(0.8)
        );
        let demoted = fx
            .tracks
            .find_by_id(&TrackId::from_uuid(Uuid::from_u128(2)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(demoted.status(), TrackStatus::InTriage);
    }

    /// A lower threshold must pull tracks back out of triage, not merely stop adding to it.
    #[tokio::test]
    async fn applying_a_laxer_threshold_promotes_triaged_tracks_back_to_auto() {
        let fx = fixture();
        let crate_id = seed_classified(&fx, 1, 0.5).await;
        let triaged = track(1).sent_to_triage(confidence(0.5));
        fx.tracks.upsert(&triaged).await.unwrap();

        let split = fx.adjust.apply(threshold(0.4)).await.unwrap();

        assert_eq!(split.auto, 1);
        let promoted = fx.tracks.find_by_id(triaged.id()).await.unwrap().unwrap();
        assert_eq!(promoted.status(), TrackStatus::AutoClassified);
        assert_eq!(
            promoted.crate_id(),
            Some(&crate_id),
            "the replayed decision restores the crate the track lost entering triage"
        );
    }

    /// T047 / FR-018: moving the slider must never undo a human's decision.
    #[tokio::test]
    async fn a_manually_decided_track_survives_any_threshold_change() {
        let fx = fixture();
        let crate_id = seed_classified(&fx, 1, 0.9).await;
        let decided = track(1).assigned_manual(crate_id);
        fx.tracks.upsert(&decided).await.unwrap();

        let split = fx.adjust.apply(threshold(1.0)).await.unwrap();

        assert_eq!(split.preserved, 1);
        assert_eq!(split.manual, 0);
        let reloaded = fx.tracks.find_by_id(decided.id()).await.unwrap().unwrap();
        assert_eq!(reloaded.status(), TrackStatus::ManuallyDecided);
        assert_eq!(reloaded.crate_id(), Some(&crate_id));
    }

    #[tokio::test]
    async fn preview_counts_manually_decided_tracks_as_preserved_not_as_a_side() {
        let fx = fixture();
        let crate_id = seed_classified(&fx, 1, 0.9).await;
        fx.tracks
            .upsert(&track(1).assigned_manual(crate_id))
            .await
            .unwrap();

        let split = fx.adjust.preview(threshold(1.0)).await.unwrap();

        assert_eq!(split.preserved, 1);
        assert_eq!(split.auto, 0);
        assert_eq!(split.manual, 0);
    }

    /// A scanned-but-never-classified track has no confidence to compare — predicting a side for it
    /// would be inventing one.
    #[tokio::test]
    async fn an_unclassified_track_is_not_predicted_on_either_side() {
        let fx = fixture();
        fx.tracks.upsert(&track(1)).await.unwrap();

        let split = fx.adjust.preview(threshold(0.6)).await.unwrap();

        assert_eq!(split.auto, 0);
        assert_eq!(split.manual, 0);
        assert_eq!(split.preserved, 0);
    }
}
