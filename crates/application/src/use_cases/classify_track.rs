//! `ClassifyTrack` — picks a genre + confidence + reason for one track and resolves its crate.
//!
//! Policy: prefer the SoundCloud source tag; call the AI classifier ONLY when the genre is missing;
//! flag likely non-music/non-mixable uploads by duration into a dedicated review crate (FR-030).
//! The AI is never used for BPM/key/energy (Principle I) and a classifier failure degrades to a
//! low-confidence result rather than aborting the run (Principle III).

use std::sync::Arc;

use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use domain::classification::ClassificationReason;
use domain::confidence::Confidence;
use domain::crate_::CrateId;
use domain::track::Track;
use thiserror::Error;

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audit_log::AuditError;
use crate::ports::crate_repository::CrateRepository;
use crate::ports::genre_vibe_classifier::{
    ClassificationInput, GenreCandidate, GenreVibeClassifierPort,
};
use crate::ports::repo_error::RepoError;

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

/// The computed classification for a track (not yet persisted — routing is `RouteToTriage`'s job).
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
}

/// Classifies a single track into a crate.
pub struct ClassifyTrack {
    classifier: Arc<dyn GenreVibeClassifierPort>,
    crates: Arc<dyn CrateRepository>,
    audit: Arc<AuditRecorder>,
}

impl ClassifyTrack {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(
        classifier: Arc<dyn GenreVibeClassifierPort>,
        crates: Arc<dyn CrateRepository>,
        audit: Arc<AuditRecorder>,
    ) -> Self {
        Self {
            classifier,
            crates,
            audit,
        }
    }

    /// Classifies `track` under `run_id`, resolving its crate and recording the decision.
    ///
    /// # Errors
    /// [`ClassifyTrackError::Repo`] on crate resolution failure; [`ClassifyTrackError::Audit`] if
    /// the decision cannot be recorded.
    pub async fn execute(
        &self,
        run_id: RunId,
        track: &Track,
    ) -> Result<Classification, ClassifyTrackError> {
        let decision = self.decide_genre(track).await;
        let crate_ = self.crates.find_or_create(&decision.genre, None).await?;
        let classification = Classification {
            crate_id: *crate_.id(),
            confidence: decision.confidence,
            reason: decision.reason,
            vibe_tags: decision.vibe_tags,
        };
        self.record_decision(run_id, track, &decision.genre, &classification)
            .await?;
        Ok(classification)
    }

    /// Determines genre/confidence/reason, calling the AI only when the source tag is absent.
    async fn decide_genre(&self, track: &Track) -> GenreDecision {
        if is_likely_non_music(track.duration_ms()) {
            return GenreDecision {
                genre: REVIEW_CRATE_GENRE.to_owned(),
                confidence: constant_confidence(NON_MUSIC_CONFIDENCE),
                reason: ClassificationReason::LikelyNonMusic,
                vibe_tags: Vec::new(),
            };
        }
        if let Some(genre) = track.source_genre() {
            return GenreDecision {
                genre: genre.to_owned(),
                confidence: constant_confidence(SOURCE_TAG_CONFIDENCE),
                reason: ClassificationReason::GenreFromSourceTag,
                vibe_tags: Vec::new(),
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
            Ok(suggestion) => match most_confident(suggestion.candidates) {
                Some(candidate) => GenreDecision {
                    genre: candidate.genre,
                    confidence: candidate.confidence,
                    reason: ClassificationReason::GenreFromAi,
                    vibe_tags: suggestion.vibe_tags,
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

/// Picks the highest-confidence candidate, or `None` when the classifier offered none.
///
/// The port documents candidates as "most-confident first", but that ordering is only ever a
/// request made of a model in a prompt — nothing enforces it, and taking the first entry on faith
/// means a reply of `[{Ambient, 0.7}, {Techno, 0.95}]` files the track as Ambient at 0.7: above
/// the default threshold, so auto-filed, silently, into the genre the model ranked second.
/// The use case owns the decision, so it selects rather than trusts.
fn most_confident(candidates: Vec<GenreCandidate>) -> Option<GenreCandidate> {
    candidates
        .into_iter()
        .max_by(|a, b| a.confidence.value().total_cmp(&b.confidence.value()))
}

/// The low-confidence "no genre determined" decision that routes a track to triage. `reason`
/// records *why* no genre was determined (AI answered but empty vs. AI unavailable).
fn fallback_decision(reason: ClassificationReason) -> GenreDecision {
    GenreDecision {
        genre: UNKNOWN_CRATE_GENRE.to_owned(),
        confidence: constant_confidence(FALLBACK_CONFIDENCE),
        reason,
        vibe_tags: Vec::new(),
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
        let picked = most_confident(vec![candidate("Ambient", 0.7), candidate("Techno", 0.95)])
            .expect("a candidate is returned");
        assert_eq!(picked.genre, "Techno");
        assert_eq!(picked.confidence.value(), 0.95);
    }

    #[test]
    fn no_candidates_yields_none() {
        assert!(most_confident(vec![]).is_none());
    }

    struct Fixture {
        classify: ClassifyTrack,
        classifier: Arc<StubGenreVibeClassifier>,
        crates: Arc<InMemoryCrateRepository>,
    }

    fn fixture(classifier: StubGenreVibeClassifier) -> Fixture {
        let ids: Arc<SeqIdProvider> = Arc::new(SeqIdProvider::new());
        let crates = Arc::new(InMemoryCrateRepository::new(ids));
        let recorder = Arc::new(AuditRecorder::new(
            Arc::new(InMemoryAuditLog::new()),
            Arc::new(FixedClock::at_millis(1_000)),
            Arc::new(SeqIdProvider::new()),
        ));
        let classifier = Arc::new(classifier);
        let classify = ClassifyTrack::new(classifier.clone(), crates.clone(), recorder);
        Fixture {
            classify,
            classifier,
            crates,
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
