//! `AnalyzeLibrary` — runs the opt-in audio path over the whole library (User Story 4).
//!
//! Per track: download (if opted in) → analyze → re-file by energy role. It composes the existing
//! single-track use cases rather than re-implementing them, so the rules they enforce — the opt-in
//! gate, "a failure is a skip", "never move a human's decision" — hold here by construction.
//!
//! Not in the original task breakdown. `ClassifyLibrary` is the equivalent for the metadata path;
//! the audio path needs its own because the UI drives it as one action (T057) and because the
//! refine step (FR-029) has no home in either single-track use case.
//!
//! **Resumability** (T057) comes from every step already being idempotent: `DownloadAudio` skips a
//! track that has its file, and re-analysis overwrites the same measurements with the same values.
//! Interrupting a run and starting again costs the current track, not the run.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use domain::audit::RunId;
use domain::track::Track;
use thiserror::Error;

use crate::ports::id_provider::IdProvider;
use crate::ports::repo_error::RepoError;
use crate::ports::settings_repository::SettingsRepository;
use crate::ports::track_repository::TrackRepository;
use crate::use_cases::analyze_audio::{AnalyzeAudio, AnalyzeAudioError, AnalyzeOutcome};
use crate::use_cases::classify_track::{ClassifyTrack, ClassifyTrackError};
use crate::use_cases::download_audio::{
    DownloadAudio, DownloadAudioError, DownloadOutcome, SkipReason,
};
use crate::use_cases::route_to_triage::{RouteError, RouteOutcome, RouteToTriage};

/// Counts describing one audio-enrichment run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnalyzeLibrarySummary {
    /// Tracks whose audio was fetched in this run.
    pub downloaded: usize,
    /// Tracks that already had their audio (nothing re-fetched).
    pub already_present: usize,
    /// Tracks that gained BPM/key/energy.
    pub analyzed: usize,
    /// Analyzed tracks re-filed into an energy sub-crate.
    pub refined: usize,
    /// Tracks sent to triage — sub-threshold, or an uncertain tempo (FR-031).
    pub sent_to_triage: usize,
    /// Tracks left alone because a human had already decided them (FR-029).
    pub preserved_manual: usize,
    /// Tracks that ended the run with no audio features, for any reason.
    pub skipped: usize,
    /// Set when the run stopped early because downloading is impossible for every track — the user
    /// has not opted in, or `yt-dlp` is missing. Distinguishes "nothing to do" from "did nothing".
    pub halted: Option<SkipReason>,
}

/// Failure enriching the library — infrastructure only.
#[derive(Debug, Error)]
pub enum AnalyzeLibraryError {
    /// A download step hit infrastructure trouble.
    #[error(transparent)]
    Download(#[from] DownloadAudioError),
    /// An analysis step hit infrastructure trouble.
    #[error(transparent)]
    Analyze(#[from] AnalyzeAudioError),
    /// Re-classifying an analyzed track failed.
    #[error(transparent)]
    Classify(#[from] ClassifyTrackError),
    /// Re-routing an analyzed track failed.
    #[error(transparent)]
    Route(#[from] RouteError),
    /// Reading settings or listing tracks failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// The collaborators `AnalyzeLibrary` needs (grouped per the 2+-params convention).
pub struct AnalyzeLibraryPorts {
    /// Lists the library.
    pub tracks: Arc<dyn TrackRepository>,
    /// Supplies the confidence threshold for re-routing.
    pub settings: Arc<dyn SettingsRepository>,
    /// Fetches audio, behind the opt-in gate.
    pub download: Arc<DownloadAudio>,
    /// Measures BPM/key/energy.
    pub analyze: Arc<AnalyzeAudio>,
    /// Re-files an analyzed track by genre × energy role.
    pub classify: Arc<ClassifyTrack>,
    /// Applies the threshold and the uncertain-tempo rule.
    pub route: Arc<RouteToTriage>,
    /// Mints the run id correlating this run's audit events.
    pub ids: Arc<dyn IdProvider>,
    /// Where downloaded audio is written.
    pub download_dir: PathBuf,
}

/// Runs download → analyze → refine over every track.
pub struct AnalyzeLibrary {
    tracks: Arc<dyn TrackRepository>,
    settings: Arc<dyn SettingsRepository>,
    download: Arc<DownloadAudio>,
    analyze: Arc<AnalyzeAudio>,
    classify: Arc<ClassifyTrack>,
    route: Arc<RouteToTriage>,
    ids: Arc<dyn IdProvider>,
    download_dir: PathBuf,
}

impl AnalyzeLibrary {
    /// Builds the orchestrator from its collaborators.
    #[must_use]
    pub fn new(ports: AnalyzeLibraryPorts) -> Self {
        Self {
            tracks: ports.tracks,
            settings: ports.settings,
            download: ports.download,
            analyze: ports.analyze,
            classify: ports.classify,
            route: ports.route,
            ids: ports.ids,
            download_dir: ports.download_dir,
        }
    }

    /// Where downloaded audio is written — the UI reports it alongside the opt-in gate.
    #[must_use]
    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }

    /// Enriches every track with audio features, returning a summary.
    ///
    /// # Errors
    /// [`AnalyzeLibraryError`] only on infrastructure failure. A track that cannot be downloaded or
    /// analyzed is counted as skipped and the run continues (Principle III).
    pub async fn execute(&self) -> Result<AnalyzeLibrarySummary, AnalyzeLibraryError> {
        let threshold = self.settings.load().await?.confidence_threshold();
        let run_id = RunId::from_uuid(self.ids.new_id());
        let tracks = self.tracks.list_all().await?;

        let mut summary = AnalyzeLibrarySummary::default();
        for track in tracks {
            let Some(with_audio) = self.fetch_audio(run_id, &track, &mut summary).await? else {
                // `halted` set means no later track can fare better — stop rather than repeat the
                // same failure once per track.
                if summary.halted.is_some() {
                    break;
                }
                continue;
            };
            let AnalyzeOutcome::Analyzed(analyzed) =
                self.analyze.execute(run_id, &with_audio).await?
            else {
                summary.skipped += 1;
                continue;
            };
            summary.analyzed += 1;
            self.refine(run_id, &analyzed, threshold, &mut summary)
                .await?;
        }
        Ok(summary)
    }

    /// Runs the download step, returning the track with its audio, or `None` when there is nothing
    /// to analyze.
    async fn fetch_audio(
        &self,
        run_id: RunId,
        track: &Track,
        summary: &mut AnalyzeLibrarySummary,
    ) -> Result<Option<Track>, AnalyzeLibraryError> {
        match self
            .download
            .execute(run_id, track, &self.download_dir)
            .await?
        {
            DownloadOutcome::Downloaded(downloaded) => {
                summary.downloaded += 1;
                Ok(Some(downloaded))
            }
            DownloadOutcome::AlreadyPresent(present) => {
                summary.already_present += 1;
                Ok(Some(present))
            }
            DownloadOutcome::Skipped(reason) => {
                summary.skipped += 1;
                if reason.is_environmental() {
                    summary.halted = Some(reason);
                }
                Ok(None)
            }
        }
    }

    /// Re-files an analyzed track: its measured energy adds the crate's sub-role (FR-029) and its
    /// tempo may send it to triage (FR-031). `RouteToTriage` refuses to touch a track a human has
    /// already decided, which is what makes re-running the audio path safe.
    async fn refine(
        &self,
        run_id: RunId,
        analyzed: &Track,
        threshold: domain::confidence::ConfidenceThreshold,
        summary: &mut AnalyzeLibrarySummary,
    ) -> Result<(), AnalyzeLibraryError> {
        let classification = self.classify.execute(run_id, analyzed).await?;
        let routed = self
            .route
            .execute(run_id, analyzed, &classification, threshold)
            .await?;
        match routed.outcome {
            RouteOutcome::Auto => summary.refined += 1,
            RouteOutcome::Triage => summary.sent_to_triage += 1,
            RouteOutcome::PreservedManual => summary.preserved_manual += 1,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::crate_::{CrateId, EnergyRole};
    use domain::settings::{ExportMode, Settings};
    use domain::track::{LikedTrack, TrackId, TrackStatus};
    use uuid::Uuid;

    use crate::audit_recorder::AuditRecorder;
    use crate::ports::crate_repository::CrateRepository;
    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_crate_repository::InMemoryCrateRepository;
    use crate::testkit::in_memory_decision_repository::InMemoryDecisionRepository;
    use crate::testkit::in_memory_settings_repository::InMemorySettingsRepository;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;
    use crate::testkit::stub_audio_analyzer::StubAudioAnalyzer;
    use crate::testkit::stub_audio_downloader::StubAudioDownloader;
    use crate::testkit::stub_genre_vibe_classifier::StubGenreVibeClassifier;
    use crate::use_cases::classify_track::ClassifyTrackPorts;
    use crate::use_cases::download_audio::DownloadAudioPorts;

    fn track(id_seed: u128, source_id: &str) -> Track {
        Track::from_scan(
            TrackId::from_uuid(Uuid::from_u128(id_seed)),
            LikedTrack {
                source_track_id: source_id.to_owned(),
                title: "t".into(),
                artist: "a".into(),
                source_genre: Some("House".into()),
                duration_ms: 240_000,
                permalink_url: "https://sc/x".into(),
                artwork_url: None,
            },
        )
    }

    struct Fixture {
        library: AnalyzeLibrary,
        tracks: Arc<InMemoryTrackRepository>,
        crates: Arc<InMemoryCrateRepository>,
        downloader: Arc<StubAudioDownloader>,
        analyzer: Arc<StubAudioAnalyzer>,
        _dir: tempfile::TempDir,
    }

    async fn fixture(
        downloader: StubAudioDownloader,
        analyzer: StubAudioAnalyzer,
        download_enabled: bool,
    ) -> Fixture {
        let ids: Arc<SeqIdProvider> = Arc::new(SeqIdProvider::new());
        let tracks = Arc::new(InMemoryTrackRepository::new());
        let crates = Arc::new(InMemoryCrateRepository::new(ids.clone()));
        let settings = Arc::new(InMemorySettingsRepository::new());
        settings
            .save(&Settings::new(
                Settings::default().confidence_threshold(),
                download_enabled,
                ExportMode::Local,
            ))
            .await
            .expect("seed settings");
        let recorder = Arc::new(AuditRecorder::new(
            Arc::new(InMemoryAuditLog::new()),
            Arc::new(FixedClock::at_millis(1_000)),
            ids.clone(),
        ));
        let downloader = Arc::new(downloader);
        let analyzer = Arc::new(analyzer);
        let dir = tempfile::tempdir().expect("temp dir");

        let download = Arc::new(DownloadAudio::new(DownloadAudioPorts {
            downloader: downloader.clone(),
            tracks: tracks.clone(),
            settings: settings.clone(),
            audit: recorder.clone(),
        }));
        let analyze = Arc::new(AnalyzeAudio::new(
            analyzer.clone(),
            tracks.clone(),
            recorder.clone(),
        ));
        let classify = Arc::new(ClassifyTrack::new(ClassifyTrackPorts {
            classifier: Arc::new(StubGenreVibeClassifier::failing()),
            crates: crates.clone(),
            decisions: Arc::new(InMemoryDecisionRepository::new()),
            audit: recorder.clone(),
            clock: Arc::new(FixedClock::at_millis(1_000)),
            ids: ids.clone(),
        }));
        let route = Arc::new(RouteToTriage::new(tracks.clone(), recorder));

        Fixture {
            library: AnalyzeLibrary::new(AnalyzeLibraryPorts {
                tracks: tracks.clone(),
                settings,
                download,
                analyze,
                classify,
                route,
                ids,
                download_dir: dir.path().to_path_buf(),
            }),
            tracks,
            crates,
            downloader,
            analyzer,
            _dir: dir,
        }
    }

    /// The happy path end to end: audio arrives, features land, and the genre-only crate becomes a
    /// genre × energy crate.
    #[tokio::test]
    async fn enriches_and_refiles_the_library() {
        let fx = fixture(
            StubAudioDownloader::available(),
            StubAudioAnalyzer::typical(),
            true,
        )
        .await;
        fx.tracks.upsert(&track(1, "sc:1")).await.unwrap();

        let summary = fx.library.execute().await.unwrap();

        assert_eq!(summary.downloaded, 1);
        assert_eq!(summary.analyzed, 1);
        assert_eq!(summary.refined, 1);
        let stored = fx.tracks.list_all().await.unwrap();
        assert_eq!(stored[0].bpm(), Some(128));
        assert_eq!(stored[0].status(), TrackStatus::AutoClassified);
        let crates = fx.crates.list().await.unwrap();
        assert_eq!(crates[0].energy_role(), Some(EnergyRole::Peak));
    }

    /// Principle V: the whole audio path is off unless the user opted in, and the summary says so
    /// rather than silently reporting a run that did nothing.
    #[tokio::test]
    async fn without_opt_in_nothing_is_downloaded_or_analyzed() {
        let fx = fixture(
            StubAudioDownloader::available(),
            StubAudioAnalyzer::typical(),
            false,
        )
        .await;
        fx.tracks.upsert(&track(1, "sc:1")).await.unwrap();

        let summary = fx.library.execute().await.unwrap();

        assert_eq!(summary.halted, Some(SkipReason::DownloadDisabled));
        assert_eq!(summary.analyzed, 0);
        assert_eq!(fx.downloader.call_count(), 0);
        assert_eq!(fx.analyzer.call_count(), 0);
    }

    /// A missing `yt-dlp` dooms every track, so the run must stop after the first rather than
    /// spawn a doomed subprocess once per track in a 1,000-track library.
    #[tokio::test]
    async fn a_missing_tool_halts_the_run_after_one_attempt() {
        let fx = fixture(
            StubAudioDownloader::tool_missing(),
            StubAudioAnalyzer::typical(),
            true,
        )
        .await;
        for seed in 1..=5u128 {
            fx.tracks
                .upsert(&track(seed, &format!("sc:{seed}")))
                .await
                .unwrap();
        }

        let summary = fx.library.execute().await.unwrap();

        assert_eq!(summary.halted, Some(SkipReason::ToolMissing));
        assert_eq!(fx.downloader.call_count(), 1, "it kept trying");
    }

    /// Principle III: one dead track must not stop the run — the rest still get analyzed.
    #[tokio::test]
    async fn one_unavailable_track_does_not_stop_the_others() {
        let fx = fixture(
            StubAudioDownloader::unavailable(),
            StubAudioAnalyzer::typical(),
            true,
        )
        .await;
        for seed in 1..=3u128 {
            fx.tracks
                .upsert(&track(seed, &format!("sc:{seed}")))
                .await
                .unwrap();
        }

        let summary = fx.library.execute().await.unwrap();

        assert_eq!(summary.halted, None, "unavailable is per-track, not fatal");
        assert_eq!(summary.skipped, 3);
        assert_eq!(fx.downloader.call_count(), 3, "every track was attempted");
    }

    /// FR-031 end to end: an ambiguous tempo lands the track in triage despite a confident genre.
    #[tokio::test]
    async fn an_uncertain_tempo_sends_the_track_to_triage() {
        let fx = fixture(
            StubAudioDownloader::available(),
            StubAudioAnalyzer::uncertain_tempo(),
            true,
        )
        .await;
        fx.tracks.upsert(&track(1, "sc:1")).await.unwrap();

        let summary = fx.library.execute().await.unwrap();

        assert_eq!(summary.sent_to_triage, 1);
        assert_eq!(summary.refined, 0);
        assert_eq!(fx.tracks.list_in_triage().await.unwrap().len(), 1);
    }

    /// FR-029 end to end: the audio path must never re-file a track the user filed themselves.
    #[tokio::test]
    async fn a_human_decided_track_is_analyzed_but_never_moved() {
        let fx = fixture(
            StubAudioDownloader::available(),
            StubAudioAnalyzer::typical(),
            true,
        )
        .await;
        let crate_id = CrateId::from_uuid(Uuid::from_u128(500));
        fx.tracks
            .upsert(&track(1, "sc:1").assigned_manual(crate_id))
            .await
            .unwrap();

        let summary = fx.library.execute().await.unwrap();

        assert_eq!(summary.preserved_manual, 1);
        assert_eq!(summary.refined, 0);
        let stored = fx.tracks.list_all().await.unwrap();
        assert_eq!(stored[0].status(), TrackStatus::ManuallyDecided);
        assert_eq!(
            stored[0].crate_id(),
            Some(&crate_id),
            "the human's crate stands"
        );
        assert_eq!(
            stored[0].bpm(),
            Some(128),
            "it still gained its measurements"
        );
    }

    /// Principle IV: re-running the audio path re-fetches nothing. This is also what makes an
    /// interrupted run resumable — restarting simply skips everything already done.
    #[tokio::test]
    async fn a_second_run_downloads_nothing_again() {
        let fx = fixture(
            StubAudioDownloader::available(),
            StubAudioAnalyzer::typical(),
            true,
        )
        .await;
        fx.tracks.upsert(&track(1, "sc:1")).await.unwrap();

        fx.library.execute().await.unwrap();
        let second = fx.library.execute().await.unwrap();

        assert_eq!(second.downloaded, 0);
        assert_eq!(second.already_present, 1);
        assert_eq!(fx.downloader.call_count(), 1);
        assert_eq!(
            fx.crates.list().await.unwrap().len(),
            1,
            "the re-run must not fork a second crate"
        );
    }
}
