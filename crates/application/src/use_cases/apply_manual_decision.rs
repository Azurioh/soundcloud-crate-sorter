//! `ApplyManualDecision` — commits one human triage action for one track (T040).
//!
//! Covers every affordance the triage card and the assignment table offer (FR-016): accept the top
//! suggestion, pick an alternative or any existing crate, create a crate on the fly, or defer. Each
//! action persists the track's new status, appends a `Manual` decision so the "why" stays
//! answerable, and records a `from → to` triage audit event (T046, Principle VII).
//!
//! A committed assignment lands the track in `ManuallyDecided`, which every later pass treats as
//! untouchable — that is what makes "never re-present a decided track" true (FR-018, Principle IV).

use std::sync::Arc;

use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use domain::classification::{
    ClassificationDecision, ClassificationReason, DecisionId, DecisionSource, NewDecision,
};
use domain::crate_::{CrateId, CrateOrigin};
use domain::track::{Track, TrackId};
use thiserror::Error;

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audit_log::AuditError;
use crate::ports::clock::ClockPort;
use crate::ports::crate_repository::{CrateRepository, CrateSpec};
use crate::ports::decision_repository::DecisionRepository;
use crate::ports::id_provider::IdProvider;
use crate::ports::repo_error::RepoError;
use crate::ports::track_repository::TrackRepository;

/// What the human chose to do with a queued track (FR-016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageAction {
    /// Take the crate the classifier suggested (read from the track's latest decision).
    AcceptSuggestion,
    /// File into a crate that already exists — an alternative chip or the searchable picker.
    AssignToCrate(CrateId),
    /// File into `genre`, creating that crate on the fly if it does not exist yet.
    CreateCrate {
        /// The genre to file under.
        genre: String,
    },
    /// Put the track back for a later session without deciding.
    Defer,
}

impl TriageAction {
    /// Lowercase token for audit detail.
    const fn as_str(&self) -> &'static str {
        match self {
            Self::AcceptSuggestion => "accept_suggestion",
            Self::AssignToCrate(_) => "assign_to_crate",
            Self::CreateCrate { .. } => "create_crate",
            Self::Defer => "defer",
        }
    }
}

/// The track after a triage action.
#[derive(Debug, Clone)]
pub struct TriagedTrack {
    /// The updated track.
    pub track: Track,
    /// The crate it was filed into (`None` when deferred).
    pub crate_id: Option<CrateId>,
}

/// Failure applying a triage action.
#[derive(Debug, Error)]
pub enum ApplyManualDecisionError {
    /// No track with that id exists.
    #[error("track not found")]
    TrackNotFound,
    /// The chosen crate does not exist.
    #[error("crate not found")]
    CrateNotFound,
    /// The track has no classification decision to accept a suggestion from.
    #[error("no suggestion to accept")]
    NoSuggestion,
    /// Persisting the track, crate, or decision failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Recording the triage event failed.
    #[error(transparent)]
    Audit(#[from] AuditError),
}

/// The collaborators `ApplyManualDecision` needs (grouped per the 2+-params convention).
pub struct ApplyManualDecisionPorts {
    /// Reads and persists the track.
    pub tracks: Arc<dyn TrackRepository>,
    /// Resolves and creates crates.
    pub crates: Arc<dyn CrateRepository>,
    /// Reads the suggestion and appends the manual decision.
    pub decisions: Arc<dyn DecisionRepository>,
    /// Records the observable trail.
    pub audit: Arc<AuditRecorder>,
    /// Stamps the decision time (the only sanctioned time source).
    pub clock: Arc<dyn ClockPort>,
    /// Mints ids (the only sanctioned id source).
    pub ids: Arc<dyn IdProvider>,
}

/// Applies one human triage decision to one track.
pub struct ApplyManualDecision {
    tracks: Arc<dyn TrackRepository>,
    crates: Arc<dyn CrateRepository>,
    decisions: Arc<dyn DecisionRepository>,
    audit: Arc<AuditRecorder>,
    clock: Arc<dyn ClockPort>,
    ids: Arc<dyn IdProvider>,
}

impl ApplyManualDecision {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(ports: ApplyManualDecisionPorts) -> Self {
        Self {
            tracks: ports.tracks,
            crates: ports.crates,
            decisions: ports.decisions,
            audit: ports.audit,
            clock: ports.clock,
            ids: ports.ids,
        }
    }

    /// Applies `action` to the track `track_id` under `run_id`.
    ///
    /// # Errors
    /// [`ApplyManualDecisionError::TrackNotFound`] for an unknown track;
    /// [`ApplyManualDecisionError::CrateNotFound`] when the chosen crate does not exist;
    /// [`ApplyManualDecisionError::NoSuggestion`] when accepting a suggestion the track never had;
    /// [`ApplyManualDecisionError::Repo`] / [`ApplyManualDecisionError::Audit`] on persistence
    /// failure.
    pub async fn execute(
        &self,
        run_id: RunId,
        track_id: &TrackId,
        action: TriageAction,
    ) -> Result<TriagedTrack, ApplyManualDecisionError> {
        let track = self
            .tracks
            .find_by_id(track_id)
            .await?
            .ok_or(ApplyManualDecisionError::TrackNotFound)?;
        let from_crate = track.crate_id().copied();

        let Some(target) = self.resolve_target(&track, &action).await? else {
            return self.defer(run_id, &track, from_crate).await;
        };

        let decided = track.assigned_manual(target);
        self.tracks.upsert(&decided).await?;
        self.record_manual_decision(&decided, target).await?;
        self.record_triage_event(TriageEvent {
            run_id,
            track: &decided,
            action: &action,
            from_crate,
            to_crate: Some(target),
        })
        .await?;
        Ok(TriagedTrack {
            track: decided,
            crate_id: Some(target),
        })
    }

    /// Resolves the crate an action files into, or `None` when the action defers instead.
    async fn resolve_target(
        &self,
        track: &Track,
        action: &TriageAction,
    ) -> Result<Option<CrateId>, ApplyManualDecisionError> {
        match action {
            TriageAction::Defer => Ok(None),
            TriageAction::AcceptSuggestion => self.suggested_crate(track).await.map(Some),
            TriageAction::AssignToCrate(crate_id) => self.existing_crate(*crate_id).await.map(Some),
            TriageAction::CreateCrate { genre } => self.crate_for_genre(genre).await.map(Some),
        }
    }

    /// Reads the crate the classifier suggested from the track's latest decision.
    async fn suggested_crate(&self, track: &Track) -> Result<CrateId, ApplyManualDecisionError> {
        let decision = self
            .decisions
            .find_latest_for_track(track.id())
            .await?
            .ok_or(ApplyManualDecisionError::NoSuggestion)?;
        Ok(*decision.crate_id())
    }

    /// Verifies a crate the user picked still exists, rather than filing into a dangling id.
    async fn existing_crate(&self, crate_id: CrateId) -> Result<CrateId, ApplyManualDecisionError> {
        self.crates
            .find_by_id(&crate_id)
            .await?
            .map(|crate_| *crate_.id())
            .ok_or(ApplyManualDecisionError::CrateNotFound)
    }

    /// Resolves `genre` to a crate, creating it as `Manual` when the human invented it here.
    async fn crate_for_genre(&self, genre: &str) -> Result<CrateId, ApplyManualDecisionError> {
        let crate_ = self
            .crates
            .find_or_create(&CrateSpec {
                genre: genre.to_owned(),
                role: None,
                origin: CrateOrigin::Manual,
            })
            .await?;
        Ok(*crate_.id())
    }

    /// Defers the track: status only, no decision recorded — deferring is explicitly *not* deciding,
    /// so the track must come back to the queue rather than count as answered (FR-016).
    async fn defer(
        &self,
        run_id: RunId,
        track: &Track,
        from_crate: Option<CrateId>,
    ) -> Result<TriagedTrack, ApplyManualDecisionError> {
        let deferred = track.deferred();
        self.tracks.upsert(&deferred).await?;
        self.record_triage_event(TriageEvent {
            run_id,
            track: &deferred,
            action: &TriageAction::Defer,
            from_crate,
            to_crate: None,
        })
        .await?;
        Ok(TriagedTrack {
            track: deferred,
            crate_id: None,
        })
    }

    /// Appends the `Manual` decision — the typed record that keeps "why is this track here?"
    /// answerable after a human overrode the classifier.
    async fn record_manual_decision(
        &self,
        track: &Track,
        crate_id: CrateId,
    ) -> Result<(), RepoError> {
        let decision = ClassificationDecision::new(NewDecision {
            id: DecisionId::from_uuid(self.ids.new_id()),
            track_id: *track.id(),
            crate_id,
            source: DecisionSource::Manual,
            // A human's pick carries no model confidence; fabricating 1.0 here would let a later
            // threshold pass read this as a high-confidence *auto* decision.
            confidence: None,
            reason: ClassificationReason::ManualPick,
            alternatives: Vec::new(),
            decided_at: self.clock.now(),
        });
        self.decisions.record(&decision).await
    }

    /// Records the triage action as `from → to` (data-model: the triage detail shape).
    async fn record_triage_event(&self, event: TriageEvent<'_>) -> Result<(), AuditError> {
        self.audit
            .record(RecordParams {
                run_id: event.run_id,
                track_id: Some(*event.track.id()),
                stage: PipelineStage::Triage,
                kind: AuditKind::TriageAction,
                outcome: triage_outcome(event.action),
                detail: triage_detail(event.action, event.from_crate, event.to_crate),
            })
            .await
    }
}

/// One triage action to audit (grouped per the 2+-params convention).
struct TriageEvent<'a> {
    /// The run this action belongs to.
    run_id: RunId,
    /// The track acted on.
    track: &'a Track,
    /// What the human chose.
    action: &'a TriageAction,
    /// The crate the track sat in before, if any.
    from_crate: Option<CrateId>,
    /// The crate it was filed into, if any.
    to_crate: Option<CrateId>,
}

/// The outcome token for a triage action: deferring leaves the track pending, everything else
/// updates its filing.
const fn triage_outcome(action: &TriageAction) -> AuditOutcome {
    match action {
        TriageAction::Defer => AuditOutcome::Started,
        _ => AuditOutcome::Updated,
    }
}

/// Builds the secret-free triage detail map (action + from → to crate).
fn triage_detail(
    action: &TriageAction,
    from_crate: Option<CrateId>,
    to_crate: Option<CrateId>,
) -> AuditDetail {
    let mut detail = AuditDetail::new().with("action", action.as_str());
    if let Some(from) = from_crate {
        detail = detail.with("from_crate", from.to_string());
    }
    if let Some(to) = to_crate {
        detail = detail.with("to_crate", to.to_string());
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::confidence::Confidence;
    use domain::timestamp::Timestamp;
    use domain::track::{LikedTrack, TrackStatus};
    use uuid::Uuid;

    use crate::ports::audit_log::AuditLogPort;
    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_crate_repository::InMemoryCrateRepository;
    use crate::testkit::in_memory_decision_repository::InMemoryDecisionRepository;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;

    struct Fixture {
        apply: ApplyManualDecision,
        tracks: Arc<InMemoryTrackRepository>,
        crates: Arc<InMemoryCrateRepository>,
        decisions: Arc<InMemoryDecisionRepository>,
        audit: Arc<InMemoryAuditLog>,
    }

    fn fixture() -> Fixture {
        let ids: Arc<SeqIdProvider> = Arc::new(SeqIdProvider::new());
        let tracks = Arc::new(InMemoryTrackRepository::new());
        let crates = Arc::new(InMemoryCrateRepository::new(ids.clone()));
        let decisions = Arc::new(InMemoryDecisionRepository::new());
        let audit = Arc::new(InMemoryAuditLog::new());
        let recorder = Arc::new(AuditRecorder::new(
            audit.clone(),
            Arc::new(FixedClock::at_millis(1_000)),
            ids.clone(),
        ));
        let apply = ApplyManualDecision::new(ApplyManualDecisionPorts {
            tracks: tracks.clone(),
            crates: crates.clone(),
            decisions: decisions.clone(),
            audit: recorder,
            clock: Arc::new(FixedClock::at_millis(1_000)),
            ids,
        });
        Fixture {
            apply,
            tracks,
            crates,
            decisions,
            audit,
        }
    }

    fn track_id() -> TrackId {
        TrackId::from_uuid(Uuid::from_u128(42))
    }

    fn triaged_track() -> Track {
        Track::from_scan(
            track_id(),
            LikedTrack {
                source_track_id: "sc:1".into(),
                title: "Night Drive".into(),
                artist: "Artist".into(),
                source_genre: None,
                duration_ms: 240_000,
                permalink_url: "https://sc/x".into(),
                artwork_url: None,
            },
        )
        .sent_to_triage(Confidence::new(0.2).unwrap())
    }

    fn run() -> RunId {
        RunId::from_uuid(Uuid::nil())
    }

    /// Seeds a suggested crate and the auto decision pointing at it, as `ClassifyTrack` would.
    async fn seed_suggestion(fx: &Fixture, genre: &str) -> CrateId {
        let crate_ = fx
            .crates
            .find_or_create(&CrateSpec {
                genre: genre.to_owned(),
                role: None,
                origin: CrateOrigin::Auto,
            })
            .await
            .unwrap();
        let decision = ClassificationDecision::new(NewDecision {
            id: DecisionId::from_uuid(Uuid::from_u128(7)),
            track_id: track_id(),
            crate_id: *crate_.id(),
            source: DecisionSource::Auto,
            confidence: Some(Confidence::new(0.2).unwrap()),
            reason: ClassificationReason::GenreFromAi,
            alternatives: Vec::new(),
            decided_at: Timestamp::from_millis(500),
        });
        fx.decisions.record(&decision).await.unwrap();
        *crate_.id()
    }

    #[tokio::test]
    async fn accepting_the_suggestion_files_the_track_and_marks_it_decided() {
        let fx = fixture();
        fx.tracks.upsert(&triaged_track()).await.unwrap();
        let suggested = seed_suggestion(&fx, "Techno").await;

        let result = fx
            .apply
            .execute(run(), &track_id(), TriageAction::AcceptSuggestion)
            .await
            .unwrap();

        assert_eq!(result.crate_id, Some(suggested));
        assert_eq!(result.track.status(), TrackStatus::ManuallyDecided);
        assert_eq!(result.track.crate_id(), Some(&suggested));
    }

    /// The whole point of triage: once decided, the track must leave the queue for good (FR-018).
    #[tokio::test]
    async fn a_decided_track_leaves_the_triage_queue() {
        let fx = fixture();
        fx.tracks.upsert(&triaged_track()).await.unwrap();
        seed_suggestion(&fx, "Techno").await;
        assert_eq!(fx.tracks.list_in_triage().await.unwrap().len(), 1);

        fx.apply
            .execute(run(), &track_id(), TriageAction::AcceptSuggestion)
            .await
            .unwrap();

        assert!(
            fx.tracks.list_in_triage().await.unwrap().is_empty(),
            "a decided track must never be re-presented"
        );
    }

    #[tokio::test]
    async fn creating_a_crate_on_the_fly_marks_it_manually_created() {
        let fx = fixture();
        fx.tracks.upsert(&triaged_track()).await.unwrap();

        let result = fx
            .apply
            .execute(
                run(),
                &track_id(),
                TriageAction::CreateCrate {
                    genre: "Breakbeat".into(),
                },
            )
            .await
            .unwrap();

        let created = fx
            .crates
            .find_by_id(&result.crate_id.unwrap())
            .await
            .unwrap()
            .expect("crate created");
        assert_eq!(created.genre(), "Breakbeat");
        assert_eq!(created.created_by(), CrateOrigin::Manual);
    }

    /// Picking an existing auto crate must not rewrite how that crate came to exist.
    #[tokio::test]
    async fn picking_an_existing_crate_preserves_its_original_origin() {
        let fx = fixture();
        fx.tracks.upsert(&triaged_track()).await.unwrap();
        let existing = seed_suggestion(&fx, "House").await;

        fx.apply
            .execute(run(), &track_id(), TriageAction::AssignToCrate(existing))
            .await
            .unwrap();

        let crate_ = fx.crates.find_by_id(&existing).await.unwrap().unwrap();
        assert_eq!(crate_.created_by(), CrateOrigin::Auto);
    }

    #[tokio::test]
    async fn deferring_keeps_the_track_undecided_and_records_no_decision() {
        let fx = fixture();
        fx.tracks.upsert(&triaged_track()).await.unwrap();

        let result = fx
            .apply
            .execute(run(), &track_id(), TriageAction::Defer)
            .await
            .unwrap();

        assert_eq!(result.crate_id, None);
        assert_eq!(result.track.status(), TrackStatus::Deferred);
        assert!(
            fx.decisions.is_empty(),
            "deferring is not deciding — it must not write a decision"
        );
    }

    #[tokio::test]
    async fn a_manual_pick_is_recorded_as_a_manual_decision_without_confidence() {
        let fx = fixture();
        fx.tracks.upsert(&triaged_track()).await.unwrap();
        let suggested = seed_suggestion(&fx, "Techno").await;

        fx.apply
            .execute(run(), &track_id(), TriageAction::AcceptSuggestion)
            .await
            .unwrap();

        let latest = fx
            .decisions
            .find_latest_for_track(&track_id())
            .await
            .unwrap()
            .expect("a manual decision was recorded");
        assert_eq!(latest.source(), DecisionSource::Manual);
        assert_eq!(latest.reason(), ClassificationReason::ManualPick);
        assert_eq!(latest.crate_id(), &suggested);
        assert_eq!(
            latest.confidence(),
            None,
            "a human's pick carries no model confidence"
        );
    }

    /// Principle VII: the trail must say where the track came from and where it went.
    #[tokio::test]
    async fn a_triage_action_is_audited_with_its_from_and_to_crate() {
        let fx = fixture();
        let suggested = seed_suggestion(&fx, "Techno").await;
        let auto_filed = triaged_track().assigned_auto(suggested, Confidence::new(0.2).unwrap());
        fx.tracks.upsert(&auto_filed).await.unwrap();
        let moved_to = fx
            .crates
            .find_or_create(&CrateSpec {
                genre: "Trance".into(),
                role: None,
                origin: CrateOrigin::Auto,
            })
            .await
            .unwrap();

        fx.apply
            .execute(
                run(),
                &track_id(),
                TriageAction::AssignToCrate(*moved_to.id()),
            )
            .await
            .unwrap();

        let events = fx.audit.events_for_track(&track_id()).await.unwrap();
        let triage = events
            .iter()
            .find(|e| e.kind() == AuditKind::TriageAction)
            .expect("a triage event was recorded");
        let detail: Vec<(String, String)> = triage
            .detail()
            .entries()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert!(detail.contains(&("action".to_owned(), "assign_to_crate".to_owned())));
        assert!(detail.contains(&("from_crate".to_owned(), suggested.to_string())));
        assert!(detail.contains(&("to_crate".to_owned(), moved_to.id().to_string())));
    }

    #[tokio::test]
    async fn an_unknown_track_is_reported_not_silently_ignored() {
        let fx = fixture();
        let error = fx
            .apply
            .execute(run(), &track_id(), TriageAction::Defer)
            .await
            .expect_err("unknown track rejected");
        assert!(matches!(error, ApplyManualDecisionError::TrackNotFound));
    }

    #[tokio::test]
    async fn assigning_to_a_dangling_crate_id_is_rejected() {
        let fx = fixture();
        fx.tracks.upsert(&triaged_track()).await.unwrap();

        let error = fx
            .apply
            .execute(
                run(),
                &track_id(),
                TriageAction::AssignToCrate(CrateId::from_uuid(Uuid::from_u128(999))),
            )
            .await
            .expect_err("unknown crate rejected");

        assert!(matches!(error, ApplyManualDecisionError::CrateNotFound));
    }

    /// A track that was never classified has nothing to accept — better a typed error than filing
    /// it into a guessed crate (Principle I: no silent misfiling).
    #[tokio::test]
    async fn accepting_a_suggestion_that_does_not_exist_is_rejected() {
        let fx = fixture();
        fx.tracks.upsert(&triaged_track()).await.unwrap();

        let error = fx
            .apply
            .execute(run(), &track_id(), TriageAction::AcceptSuggestion)
            .await
            .expect_err("missing suggestion rejected");

        assert!(matches!(error, ApplyManualDecisionError::NoSuggestion));
    }
}
