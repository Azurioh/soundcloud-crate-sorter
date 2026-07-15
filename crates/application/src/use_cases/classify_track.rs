//! `ClassifyTrack` — picks a genre + confidence + reason for one track and resolves its crate.
//!
//! Policy: prefer the SoundCloud source tag; call the AI classifier ONLY when the genre is missing;
//! flag likely non-music/non-mixable uploads by duration into a dedicated review crate (FR-030).
//! The AI is never used for BPM/key/energy (Principle I) and a classifier failure degrades to a
//! low-confidence result rather than aborting the run (Principle III).

use std::sync::Arc;

use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use domain::classification::{
    ClassificationDecision, ClassificationReason, DecisionId, DecisionSource, GenreSuggestion,
    NewDecision,
};
use domain::confidence::Confidence;
use domain::crate_::{CrateId, CrateOrigin};
use domain::track::Track;
use thiserror::Error;

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audit_log::AuditError;
use crate::ports::clock::ClockPort;
use crate::ports::crate_repository::{CrateRepository, CrateSpec};
use crate::ports::decision_repository::DecisionRepository;
use crate::ports::genre_vibe_classifier::{
    ClassificationInput, GenreCandidate, GenreVibeClassifierPort,
};
use crate::ports::id_provider::IdProvider;
use crate::ports::repo_error::RepoError;

/// How many runner-up genres a triage card offers as alternative chips (FR-015: "a small set").
const MAX_ALTERNATIVES: usize = 3;

/// Confidence assigned when the genre comes straight from the source tag.
const SOURCE_TAG_CONFIDENCE: f32 = 0.9;
/// Confidence assigned when a track is flagged as likely non-music (we are confident it is *not*
/// a mixable track, so it auto-files to the review crate).
const NON_MUSIC_CONFIDENCE: f32 = 0.95;
/// Confidence assigned when no genre can be determined (routes the track to triage).
const FALLBACK_CONFIDENCE: f32 = 0.1;
/// Shortest plausible duration for a mixable track; below this is likely a skit/intro (FR-030).
const MIN_MUSIC_DURATION_MS: u64 = 60_000;
/// Longest plausible duration for a single mixable track; above this is likely a mix/podcast.
const MAX_MUSIC_DURATION_MS: u64 = 1_800_000;
/// Genre bucket for uploads flagged as likely non-music/non-mixable.
const REVIEW_CRATE_GENRE: &str = "Review";
/// Genre bucket when the classifier cannot determine a genre.
const UNKNOWN_CRATE_GENRE: &str = "Unknown";

/// The computed classification for a track (the track's status is not yet updated — routing is
/// `RouteToTriage`'s job).
#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    /// The resolved crate.
    pub crate_id: CrateId,
    /// The confidence in that crate.
    pub confidence: Confidence,
    /// The evidence behind the choice.
    pub reason: ClassificationReason,
    /// Any AI-inferred vibe tags.
    pub vibe_tags: Vec<String>,
    /// Runner-up genres, for the triage card's alternative chips (FR-015).
    pub alternatives: Vec<GenreSuggestion>,
}

/// Failure classifying a track.
#[derive(Debug, Error)]
pub enum ClassifyTrackError {
    /// Resolving/creating the crate failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Recording the decision failed.
    #[error(transparent)]
    Audit(#[from] AuditError),
}

/// The intermediate genre decision, before the crate is resolved.
struct GenreDecision {
    genre: String,
    confidence: Confidence,
    reason: ClassificationReason,
    vibe_tags: Vec<String>,
    alternatives: Vec<GenreSuggestion>,
}

/// The collaborators `ClassifyTrack` needs (grouped per the 2+-params convention).
pub struct ClassifyTrackPorts {
    /// Infers genre/vibe when the source tag cannot answer.
    pub classifier: Arc<dyn GenreVibeClassifierPort>,
    /// Resolves the chosen genre to a crate.
    pub crates: Arc<dyn CrateRepository>,
    /// Stores the decision so triage can rebuild the card later.
    pub decisions: Arc<dyn DecisionRepository>,
    /// Records the observable trail.
    pub audit: Arc<AuditRecorder>,
    /// Stamps the decision time (the only sanctioned time source).
    pub clock: Arc<dyn ClockPort>,
    /// Mints the decision id (the only sanctioned id source).
    pub ids: Arc<dyn IdProvider>,
}

/// Classifies a single track into a crate.
pub struct ClassifyTrack {
    classifier: Arc<dyn GenreVibeClassifierPort>,
    crates: Arc<dyn CrateRepository>,
    decisions: Arc<dyn DecisionRepository>,
    audit: Arc<AuditRecorder>,
    clock: Arc<dyn ClockPort>,
    ids: Arc<dyn IdProvider>,
}

impl ClassifyTrack {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(ports: ClassifyTrackPorts) -> Self {
        Self {
            classifier: ports.classifier,
            crates: ports.crates,
            decisions: ports.decisions,
            audit: ports.audit,
            clock: ports.clock,
            ids: ports.ids,
        }
    }

    /// Classifies `track` under `run_id`, resolving its crate and persisting + auditing the decision.
    ///
    /// # Errors
    /// [`ClassifyTrackError::Repo`] on crate resolution or decision-persistence failure;
    /// [`ClassifyTrackError::Audit`] if the trail cannot be recorded.
    pub async fn execute(
        &self,
        run_id: RunId,
        track: &Track,
    ) -> Result<Classification, ClassifyTrackError> {
        let decision = self.decide_genre(track).await;
        let crate_ = self
            .crates
            .find_or_create(&CrateSpec {
                genre: decision.genre.clone(),
                role: None,
                origin: CrateOrigin::Auto,
            })
            .await?;
        let classification = Classification {
            crate_id: *crate_.id(),
            confidence: decision.confidence,
            reason: decision.reason,
            vibe_tags: decision.vibe_tags,
            alternatives: decision.alternatives,
        };
        self.persist_decision(track, &classification).await?;
        self.record_decision(run_id, track, &decision.genre, &classification)
            .await?;
        Ok(classification)
    }

    /// Appends the typed decision record triage reads back to rebuild the card (FR-015).
    async fn persist_decision(
        &self,
        track: &Track,
        classification: &Classification,
    ) -> Result<(), RepoError> {
        let decision = ClassificationDecision::new(NewDecision {
            id: DecisionId::from_uuid(self.ids.new_id()),
            track_id: *track.id(),
            crate_id: classification.crate_id,
            source: DecisionSource::Auto,
            confidence: Some(classification.confidence),
            reason: classification.reason,
            alternatives: classification.alternatives.clone(),
            decided_at: self.clock.now(),
        });
        self.decisions.record(&decision).await
    }

    /// Determines genre/confidence/reason, calling the AI only when the source tag is absent.
    async fn decide_genre(&self, track: &Track) -> GenreDecision {
        if is_likely_non_music(track.duration_ms()) {
            return GenreDecision {
                genre: REVIEW_CRATE_GENRE.to_owned(),
                confidence: constant_confidence(NON_MUSIC_CONFIDENCE),
                reason: ClassificationReason::LikelyNonMusic,
                vibe_tags: Vec::new(),
                alternatives: Vec::new(),
            };
        }
        if let Some(genre) = track.source_genre() {
            return GenreDecision {
                genre: genre.to_owned(),
                confidence: constant_confidence(SOURCE_TAG_CONFIDENCE),
                reason: ClassificationReason::GenreFromSourceTag,
                vibe_tags: Vec::new(),
                alternatives: Vec::new(),
            };
        }
        self.classify_with_ai(track).await
    }

    /// Asks the AI classifier for a genre, degrading to a low-confidence fallback on any failure.
    async fn classify_with_ai(&self, track: &Track) -> GenreDecision {
        let input = ClassificationInput {
            title: track.title().to_owned(),
            artist: track.artist().to_owned(),
            source_genre: None,
            description: None,
        };
        match self.classifier.classify(&input).await {
            Ok(suggestion) => match split_off_most_confident(suggestion.candidates) {
                Some((chosen, runners_up)) => GenreDecision {
                    genre: chosen.genre,
                    confidence: chosen.confidence,
                    reason: ClassificationReason::GenreFromAi,
                    vibe_tags: suggestion.vibe_tags,
                    alternatives: runners_up,
                },
                // The AI answered but offered no genre.
                None => fallback_decision(ClassificationReason::GenreFromAi),
            },
            // The AI never answered (rate-limit/transport/bad response) — record it as such.
            Err(_) => fallback_decision(ClassificationReason::ClassifierUnavailable),
        }
    }

    /// Records the classification decision as an audit event (the answerable "why").
    async fn record_decision(
        &self,
        run_id: RunId,
        track: &Track,
        genre: &str,
        classification: &Classification,
    ) -> Result<(), AuditError> {
        let mut detail = AuditDetail::new()
            .with("genre", genre)
            .with("crate_id", classification.crate_id.to_string())
            .with(
                "confidence",
                format!("{:.3}", classification.confidence.value()),
            )
            .with("reason", classification.reason.as_str());
        if !classification.vibe_tags.is_empty() {
            detail = detail.with("vibe_tags", classification.vibe_tags.join(","));
        }
        self.audit
            .record(RecordParams {
                run_id,
                track_id: Some(*track.id()),
                stage: PipelineStage::Classify,
                kind: AuditKind::Classification,
                outcome: AuditOutcome::Created,
                detail,
            })
            .await
    }
}

/// Whether a track's duration flags it as likely non-music/non-mixable (FR-030): outside the
/// plausible single-track duration window.
fn is_likely_non_music(duration_ms: u64) -> bool {
    !(MIN_MUSIC_DURATION_MS..=MAX_MUSIC_DURATION_MS).contains(&duration_ms)
}

/// Splits the candidates into the highest-confidence one and up to [`MAX_ALTERNATIVES`] runners-up
/// (confidence-descending), or `None` when the classifier offered none.
///
/// The port documents candidates as "most-confident first", but that ordering is only ever a
/// request made of a model in a prompt — nothing enforces it, and taking the first entry on faith
/// means a reply of `[{Ambient, 0.7}, {Techno, 0.95}]` files the track as Ambient at 0.7: above
/// the default threshold, so auto-filed, silently, into the genre the model ranked second.
/// The use case owns the decision, so it sorts rather than trusts — which also makes the runners-up
/// it hands to triage the genuinely next-best genres, not merely the ones the model listed next.
fn split_off_most_confident(
    candidates: Vec<GenreCandidate>,
) -> Option<(GenreCandidate, Vec<GenreSuggestion>)> {
    let mut ranked = candidates;
    ranked.sort_by(|a, b| b.confidence.value().total_cmp(&a.confidence.value()));
    let mut ranked = ranked.into_iter();
    let chosen = ranked.next()?;
    let runners_up = ranked
        .take(MAX_ALTERNATIVES)
        .map(|candidate| GenreSuggestion {
            genre: candidate.genre,
            confidence: candidate.confidence,
        })
        .collect();
    Some((chosen, runners_up))
}

/// The low-confidence "no genre determined" decision that routes a track to triage. `reason`
/// records *why* no genre was determined (AI answered but empty vs. AI unavailable). It carries no
/// alternatives — there is nothing to suggest, so the triage card falls back to the full picker.
fn fallback_decision(reason: ClassificationReason) -> GenreDecision {
    GenreDecision {
        genre: UNKNOWN_CRATE_GENRE.to_owned(),
        confidence: constant_confidence(FALLBACK_CONFIDENCE),
        reason,
        vibe_tags: Vec::new(),
        alternatives: Vec::new(),
    }
}

/// Wraps a compile-time-valid confidence constant (the value is a known in-range literal).
fn constant_confidence(value: f32) -> Confidence {
    Confidence::new(value).expect("classification confidence constants are in [0.0, 1.0]")
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::track::{LikedTrack, TrackId};
    use uuid::Uuid;

    use crate::ports::genre_vibe_classifier::{GenreCandidate, GenreVibeSuggestion};
    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_crate_repository::InMemoryCrateRepository;
    use crate::testkit::in_memory_decision_repository::InMemoryDecisionRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;
    use crate::testkit::stub_genre_vibe_classifier::StubGenreVibeClassifier;

    fn track(source_genre: Option<&str>, duration_ms: u64) -> Track {
        Track::from_scan(
            TrackId::from_uuid(Uuid::nil()),
            LikedTrack {
                source_track_id: "sc:1".into(),
                title: "Night Drive".into(),
                artist: "Artist".into(),
                source_genre: source_genre.map(str::to_owned),
                duration_ms,
                permalink_url: "https://sc/x".into(),
                artwork_url: None,
            },
        )
    }

    fn suggestion(genre: &str, confidence: f32) -> GenreVibeSuggestion {
        GenreVibeSuggestion {
            candidates: vec![GenreCandidate {
                genre: genre.to_owned(),
                confidence: Confidence::new(confidence).unwrap(),
            }],
            vibe_tags: vec!["dark".into()],
        }
    }

    fn candidate(genre: &str, confidence: f32) -> GenreCandidate {
        GenreCandidate {
            genre: genre.to_owned(),
            confidence: Confidence::new(confidence).unwrap(),
        }
    }

    /// The "most-confident first" ordering is only ever asked of the model in a prompt; nothing
    /// enforces it. Taking the first entry would file this track as Ambient at 0.7 — over the
    /// default 0.6 threshold, so auto-filed into the genre the model ranked second.
    #[test]
    fn picks_the_highest_confidence_candidate_regardless_of_model_ordering() {
        let (picked, _) =
            split_off_most_confident(vec![candidate("Ambient", 0.7), candidate("Techno", 0.95)])
                .expect("a candidate is returned");
        assert_eq!(picked.genre, "Techno");
        assert_eq!(picked.confidence.value(), 0.95);
    }

    /// The runners-up become the triage card's chips, so they must be the genuinely next-best
    /// genres — ranked here, not left in whatever order the model emitted.
    #[test]
    fn runners_up_are_ranked_by_confidence_not_model_order() {
        let (_, alternatives) = split_off_most_confident(vec![
            candidate("Ambient", 0.2),
            candidate("Techno", 0.95),
            candidate("Trance", 0.6),
        ])
        .expect("a candidate is returned");

        let genres: Vec<&str> = alternatives.iter().map(|a| a.genre.as_str()).collect();
        assert_eq!(genres, vec!["Trance", "Ambient"]);
    }

    #[test]
    fn runners_up_are_capped_to_a_small_set() {
        let candidates = vec![
            candidate("A", 0.9),
            candidate("B", 0.8),
            candidate("C", 0.7),
            candidate("D", 0.6),
            candidate("E", 0.5),
            candidate("F", 0.4),
        ];
        let (_, alternatives) = split_off_most_confident(candidates).expect("a candidate");
        assert_eq!(alternatives.len(), MAX_ALTERNATIVES);
    }

    #[test]
    fn no_candidates_yields_none() {
        assert!(split_off_most_confident(vec![]).is_none());
    }

    struct Fixture {
        classify: ClassifyTrack,
        classifier: Arc<StubGenreVibeClassifier>,
        crates: Arc<InMemoryCrateRepository>,
        decisions: Arc<InMemoryDecisionRepository>,
    }

    fn fixture(classifier: StubGenreVibeClassifier) -> Fixture {
        let ids: Arc<SeqIdProvider> = Arc::new(SeqIdProvider::new());
        let crates = Arc::new(InMemoryCrateRepository::new(ids.clone()));
        let decisions = Arc::new(InMemoryDecisionRepository::new());
        let recorder = Arc::new(AuditRecorder::new(
            Arc::new(InMemoryAuditLog::new()),
            Arc::new(FixedClock::at_millis(1_000)),
            Arc::new(SeqIdProvider::new()),
        ));
        let classifier = Arc::new(classifier);
        let classify = ClassifyTrack::new(ClassifyTrackPorts {
            classifier: classifier.clone(),
            crates: crates.clone(),
            decisions: decisions.clone(),
            audit: recorder,
            clock: Arc::new(FixedClock::at_millis(1_000)),
            ids,
        });
        Fixture {
            classify,
            classifier,
            crates,
            decisions,
        }
    }

    fn run() -> RunId {
        RunId::from_uuid(Uuid::nil())
    }

    #[tokio::test]
    async fn uses_source_tag_without_calling_ai() {
        let fx = fixture(StubGenreVibeClassifier::failing());
        let result = fx
            .classify
            .execute(run(), &track(Some("Deep House"), 300_000))
            .await
            .unwrap();

        assert_eq!(result.reason, ClassificationReason::GenreFromSourceTag);
        assert!((result.confidence.value() - SOURCE_TAG_CONFIDENCE).abs() < f32::EPSILON);
        assert_eq!(fx.classifier.call_count(), 0);
        let created = fx.crates.list().await.unwrap();
        assert_eq!(created[0].genre(), "Deep House");
    }

    #[tokio::test]
    async fn calls_ai_only_when_genre_missing() {
        let fx = fixture(StubGenreVibeClassifier::always(suggestion("Techno", 0.72)));
        let result = fx
            .classify
            .execute(run(), &track(None, 300_000))
            .await
            .unwrap();

        assert_eq!(result.reason, ClassificationReason::GenreFromAi);
        assert!((result.confidence.value() - 0.72).abs() < f32::EPSILON);
        assert_eq!(result.vibe_tags, vec!["dark".to_string()]);
        assert_eq!(fx.classifier.call_count(), 1);
    }

    #[tokio::test]
    async fn ai_failure_degrades_to_low_confidence() {
        let fx = fixture(StubGenreVibeClassifier::failing());
        let result = fx
            .classify
            .execute(run(), &track(None, 300_000))
            .await
            .unwrap();

        assert!((result.confidence.value() - FALLBACK_CONFIDENCE).abs() < f32::EPSILON);
        assert_eq!(result.reason, ClassificationReason::ClassifierUnavailable);
        assert_eq!(
            fx.crates.list().await.unwrap()[0].genre(),
            UNKNOWN_CRATE_GENRE
        );
    }

    /// Triage rebuilds its card from the persisted decision, not from the track (which drops its
    /// crate on the way into triage). If `execute` stops recording one, every sub-threshold track
    /// loses its top suggestion and its chips (FR-015).
    #[tokio::test]
    async fn persists_the_decision_with_its_alternatives_for_triage() {
        let fx = fixture(StubGenreVibeClassifier::always(GenreVibeSuggestion {
            candidates: vec![candidate("Techno", 0.4), candidate("Trance", 0.3)],
            vibe_tags: vec!["dark".into()],
        }));
        let track = track(None, 300_000);

        let classification = fx.classify.execute(run(), &track).await.unwrap();

        let stored = fx
            .decisions
            .find_latest_for_track(track.id())
            .await
            .unwrap()
            .expect("a decision was recorded");
        assert_eq!(stored.crate_id(), &classification.crate_id);
        assert_eq!(stored.source(), DecisionSource::Auto);
        assert_eq!(stored.reason(), ClassificationReason::GenreFromAi);
        assert_eq!(
            stored.alternatives().first().map(|a| a.genre.as_str()),
            Some("Trance")
        );
    }

    #[tokio::test]
    async fn short_upload_routed_to_review_crate() {
        let fx = fixture(StubGenreVibeClassifier::failing());
        let result = fx
            .classify
            .execute(run(), &track(Some("House"), 30_000))
            .await
            .unwrap();

        assert_eq!(result.reason, ClassificationReason::LikelyNonMusic);
        assert_eq!(
            fx.crates.list().await.unwrap()[0].genre(),
            REVIEW_CRATE_GENRE
        );
        assert_eq!(fx.classifier.call_count(), 0);
    }
}
