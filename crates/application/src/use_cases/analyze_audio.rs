//! `AnalyzeAudio` — writes one track's deterministic BPM / Camelot key / energy (User Story 4).
//!
//! This use case only *measures*. It deliberately does not re-file the track: refining a crate by
//! energy role is `ClassifyTrack`'s decision and routing an uncertain tempo to triage is
//! `RouteToTriage`'s, both of which owe a `ManuallyDecided` track its human decision (FR-029).
//! Keeping measurement separate from decision is what lets analysis be re-run at any time without
//! any risk of overturning something the user chose.
//!
//! As in [`super::download_audio`], a track that cannot be analyzed is a reported outcome, never an
//! error that ends the run (Principle III).

use std::sync::Arc;

use domain::audio::AudioFeatures;
use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use domain::track::Track;
use thiserror::Error;

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audio_analyzer::{AnalyzeError, AudioAnalyzerPort};
use crate::ports::audit_log::AuditError;
use crate::ports::repo_error::RepoError;
use crate::ports::track_repository::TrackRepository;

/// Why a track carries no audio features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyzeSkipReason {
    /// The track has no downloaded audio — the expected state when download is off (Principle III).
    NoAudio,
    /// The file could not be decoded.
    Decode,
    /// The file holds no analyzable audio (silence).
    Silence,
    /// Decoding worked but analysis failed.
    Analysis,
}

impl AnalyzeSkipReason {
    /// Lowercase token for audit detail.
    const fn as_str(self) -> &'static str {
        match self {
            Self::NoAudio => "no_audio",
            Self::Decode => "decode_failed",
            Self::Silence => "silence",
            Self::Analysis => "analysis_failed",
        }
    }
}

/// What happened for one track. Not `Eq`: it carries a `Track`, whose `Confidence` is an `f32`.
///
/// The track is boxed so a `Skipped` — the common outcome whenever download is off — does not carry
/// a track-sized hole around with it. The allocation is nothing next to the seconds of native
/// analysis that produced the value.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalyzeOutcome {
    /// Features were measured and persisted.
    Analyzed(Box<Track>),
    /// No features; the run continues (Principle III).
    Skipped(AnalyzeSkipReason),
}

/// Failure analyzing — infrastructure only, never one track's bad audio.
#[derive(Debug, Error)]
pub enum AnalyzeAudioError {
    /// Persisting the analyzed track failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Recording the trail failed.
    #[error(transparent)]
    Audit(#[from] AuditError),
}

/// Measures and stores one track's audio features.
pub struct AnalyzeAudio {
    analyzer: Arc<dyn AudioAnalyzerPort>,
    tracks: Arc<dyn TrackRepository>,
    audit: Arc<AuditRecorder>,
}

impl AnalyzeAudio {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(
        analyzer: Arc<dyn AudioAnalyzerPort>,
        tracks: Arc<dyn TrackRepository>,
        audit: Arc<AuditRecorder>,
    ) -> Self {
        Self {
            analyzer,
            tracks,
            audit,
        }
    }

    /// Analyzes `track`'s downloaded audio under `run_id`, persisting the features it finds.
    ///
    /// The analysis itself is CPU-bound and synchronous (see [`AudioAnalyzerPort`]); a caller
    /// running this over a whole library should keep it off a thread it needs for anything else.
    ///
    /// # Errors
    /// [`AnalyzeAudioError::Repo`] on persistence failure; [`AnalyzeAudioError::Audit`] if the trail
    /// cannot be recorded. A track that cannot be analyzed is `Ok(Skipped)`.
    pub async fn execute(
        &self,
        run_id: RunId,
        track: &Track,
    ) -> Result<AnalyzeOutcome, AnalyzeAudioError> {
        let Some(path) = track.local_audio_path() else {
            return self.skip(run_id, track, AnalyzeSkipReason::NoAudio).await;
        };

        match self.analyzer.analyze(path) {
            Ok(features) => {
                let analyzed = track.analyzed(features);
                self.tracks.upsert(&analyzed).await?;
                self.record(
                    run_id,
                    track,
                    AuditOutcome::Updated,
                    features_detail(&features),
                )
                .await?;
                Ok(AnalyzeOutcome::Analyzed(Box::new(analyzed)))
            }
            Err(error) => self.skip(run_id, track, skip_reason(&error)).await,
        }
    }

    /// Records a skip and returns it as the outcome.
    async fn skip(
        &self,
        run_id: RunId,
        track: &Track,
        reason: AnalyzeSkipReason,
    ) -> Result<AnalyzeOutcome, AnalyzeAudioError> {
        let outcome = match reason {
            // "No audio" is the documented, expected degradation, not a failure to flag.
            AnalyzeSkipReason::NoAudio => AuditOutcome::SkippedNoAudio,
            _ => AuditOutcome::Failed,
        };
        self.record(
            run_id,
            track,
            outcome,
            AuditDetail::new().with("reason", reason.as_str()),
        )
        .await?;
        Ok(AnalyzeOutcome::Skipped(reason))
    }

    /// Records one analysis event.
    async fn record(
        &self,
        run_id: RunId,
        track: &Track,
        outcome: AuditOutcome,
        detail: AuditDetail,
    ) -> Result<(), AuditError> {
        self.audit
            .record(RecordParams {
                run_id,
                track_id: Some(*track.id()),
                stage: PipelineStage::Analyze,
                kind: AuditKind::AnalyzeResult,
                outcome,
                detail,
            })
            .await
    }
}

/// Builds the audit detail for a successful analysis — the measured values, so the trail can answer
/// "where did this BPM come from?" and, when the tempo is doubtful, that we knew it was (FR-031).
fn features_detail(features: &AudioFeatures) -> AuditDetail {
    let detail = AuditDetail::new()
        .with("bpm", features.bpm.to_string())
        .with("tempo", features.tempo_ambiguity.as_str())
        .with("energy", features.energy.value().to_string());
    match features.key {
        Some(key) => detail.with("key", key.to_string()),
        // Recorded explicitly: "we looked and found none" is a different fact from "we never looked".
        None => detail.with("key", "none"),
    }
}

/// Maps the port's typed error onto the skip reason reported to the user.
fn skip_reason(error: &AnalyzeError) -> AnalyzeSkipReason {
    match error {
        AnalyzeError::Decode { .. } => AnalyzeSkipReason::Decode,
        AnalyzeError::Analysis { .. } => AnalyzeSkipReason::Analysis,
        AnalyzeError::Silence => AnalyzeSkipReason::Silence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::audio::TempoAmbiguity;
    use domain::confidence::Energy;
    use domain::crate_::CrateId;
    use domain::track::{LikedTrack, TrackId, TrackStatus};
    use uuid::Uuid;

    use crate::ports::audit_log::AuditLogPort;
    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;
    use crate::testkit::stub_audio_analyzer::StubAudioAnalyzer;

    fn track() -> Track {
        Track::from_scan(
            TrackId::from_uuid(Uuid::from_u128(1)),
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

    /// A track that has been through the (opt-in) download stage.
    fn downloaded_track() -> Track {
        track().downloaded("/tmp/does-not-need-to-exist.mp3".into())
    }

    struct Fixture {
        analyze: AnalyzeAudio,
        analyzer: Arc<StubAudioAnalyzer>,
        tracks: Arc<InMemoryTrackRepository>,
        audit_log: Arc<InMemoryAuditLog>,
    }

    fn fixture(analyzer: StubAudioAnalyzer) -> Fixture {
        let analyzer = Arc::new(analyzer);
        let tracks = Arc::new(InMemoryTrackRepository::new());
        let audit_log = Arc::new(InMemoryAuditLog::new());
        let audit = Arc::new(AuditRecorder::new(
            audit_log.clone(),
            Arc::new(FixedClock::at_millis(1_000)),
            Arc::new(SeqIdProvider::new()),
        ));
        Fixture {
            analyze: AnalyzeAudio::new(analyzer.clone(), tracks.clone(), audit),
            analyzer,
            tracks,
            audit_log,
        }
    }

    fn run() -> RunId {
        RunId::from_uuid(Uuid::nil())
    }

    #[tokio::test]
    async fn writes_every_measured_feature_and_persists_them() {
        let fx = fixture(StubAudioAnalyzer::typical());

        let outcome = fx
            .analyze
            .execute(run(), &downloaded_track())
            .await
            .unwrap();

        let AnalyzeOutcome::Analyzed(analyzed) = outcome else {
            panic!("expected an analysis, got {outcome:?}");
        };
        assert_eq!(analyzed.bpm(), Some(128));
        assert_eq!(analyzed.energy().map(Energy::value), Some(80));
        assert_eq!(
            analyzed.camelot_key().map(|k| k.to_string()),
            Some("8B".into())
        );

        let stored = fx
            .tracks
            .find_by_id(track().id())
            .await
            .unwrap()
            .expect("persisted");
        assert_eq!(stored.bpm(), Some(128));
    }

    /// Principle III: with download off, every track reaches here without audio. That is the normal
    /// path, not an error — and the analyzer must not even be consulted.
    #[tokio::test]
    async fn a_track_without_audio_is_skipped_not_failed() {
        let fx = fixture(StubAudioAnalyzer::typical());

        let outcome = fx.analyze.execute(run(), &track()).await.unwrap();

        assert_eq!(outcome, AnalyzeOutcome::Skipped(AnalyzeSkipReason::NoAudio));
        assert_eq!(fx.analyzer.call_count(), 0);
        let events = fx.audit_log.events_for_track(track().id()).await.unwrap();
        assert_eq!(events[0].outcome(), AuditOutcome::SkippedNoAudio);
    }

    #[tokio::test]
    async fn an_undecodable_file_is_a_reported_skip() {
        let fx = fixture(StubAudioAnalyzer::failing());

        let outcome = fx
            .analyze
            .execute(run(), &downloaded_track())
            .await
            .unwrap();

        assert_eq!(outcome, AnalyzeOutcome::Skipped(AnalyzeSkipReason::Decode));
        let events = fx.audit_log.events_for_track(track().id()).await.unwrap();
        assert_eq!(events[0].outcome(), AuditOutcome::Failed);
    }

    #[tokio::test]
    async fn silence_is_reported_as_silence() {
        let fx = fixture(StubAudioAnalyzer::silent());

        let outcome = fx
            .analyze
            .execute(run(), &downloaded_track())
            .await
            .unwrap();

        assert_eq!(outcome, AnalyzeOutcome::Skipped(AnalyzeSkipReason::Silence));
    }

    /// FR-031: the doubt must be recorded, not smoothed over — the BPM is stored, flagged uncertain,
    /// and it is the router's job to act on the flag.
    #[tokio::test]
    async fn an_ambiguous_tempo_is_stored_flagged_uncertain() {
        let fx = fixture(StubAudioAnalyzer::uncertain_tempo());

        let outcome = fx
            .analyze
            .execute(run(), &downloaded_track())
            .await
            .unwrap();

        let AnalyzeOutcome::Analyzed(analyzed) = outcome else {
            panic!("expected an analysis");
        };
        assert!(analyzed.has_uncertain_tempo());
        assert_eq!(
            analyzed.bpm(),
            Some(70),
            "the estimate is kept, not discarded"
        );

        let events = fx.audit_log.events_for_track(track().id()).await.unwrap();
        let recorded: Vec<(String, String)> = events[0]
            .detail()
            .entries()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert!(recorded.contains(&(
            "tempo".to_string(),
            TempoAmbiguity::HalfOrDoubleTime.as_str().to_string()
        )));
    }

    /// Measuring must never overturn a human's decision (FR-029 / Principle IV): the features land,
    /// the crate and the `ManuallyDecided` status do not move.
    #[tokio::test]
    async fn analysis_leaves_a_human_decided_track_where_the_human_put_it() {
        let fx = fixture(StubAudioAnalyzer::typical());
        let crate_id = CrateId::from_uuid(Uuid::from_u128(9));
        let decided = downloaded_track().assigned_manual(crate_id);

        let outcome = fx.analyze.execute(run(), &decided).await.unwrap();

        let AnalyzeOutcome::Analyzed(analyzed) = outcome else {
            panic!("expected an analysis");
        };
        assert_eq!(analyzed.status(), TrackStatus::ManuallyDecided);
        assert_eq!(analyzed.crate_id(), Some(&crate_id));
        assert_eq!(analyzed.bpm(), Some(128), "it is still analyzed");
    }
}
