//! `DownloadAudio` — fetches one track's audio, behind the explicit opt-in gate (User Story 4).
//!
//! Two constitutional rules shape this use case:
//!
//! - **The gate is here, not in the UI (Principle V).** Downloading is off by default and the user
//!   must opt in. A gate that only exists in the webview is advisory — any other caller (a new Tauri
//!   command, a test, a later batch job) would bypass it silently. This use case re-reads the
//!   setting on every call so "off" means no download can happen, full stop.
//! - **A failure is a reported outcome, not an error (Principle III).** A deleted or geo-blocked
//!   track must not abort a 1,000-track run. Only infrastructure failures (the repository, the audit
//!   log) are `Err` — everything a single track can do wrong is a [`DownloadOutcome`].

use std::path::Path;
use std::sync::Arc;

use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use domain::track::Track;
use thiserror::Error;

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audio_downloader::{AudioDownloaderPort, DownloadError};
use crate::ports::audit_log::AuditError;
use crate::ports::repo_error::RepoError;
use crate::ports::settings_repository::SettingsRepository;
use crate::ports::track_repository::TrackRepository;

/// Why a track ended up without freshly downloaded audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The user has not opted in (`download_enabled = false`, the default).
    DownloadDisabled,
    /// The track is deleted, geo-blocked, or otherwise gone.
    Unavailable,
    /// `yt-dlp` is not installed. Environmental, not per-track: every other track will hit this too,
    /// so a batch caller should stop asking rather than repeat it once per track.
    ToolMissing,
    /// A local I/O failure writing the file.
    Io,
}

impl SkipReason {
    /// Lowercase token for audit detail.
    const fn as_str(self) -> &'static str {
        match self {
            Self::DownloadDisabled => "download_disabled",
            Self::Unavailable => "unavailable",
            Self::ToolMissing => "tool_missing",
            Self::Io => "io_error",
        }
    }

    /// Whether this reason will recur for every remaining track, so a batch run should stop the
    /// download stage instead of retrying it per track.
    #[must_use]
    pub fn is_environmental(self) -> bool {
        matches!(self, Self::DownloadDisabled | Self::ToolMissing)
    }
}

/// What happened for one track. Not `Eq`: it carries a `Track`, whose `Confidence` is an `f32`.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadOutcome {
    /// Audio was fetched; the track now carries a local path.
    Downloaded(Track),
    /// The track already had usable local audio — nothing was fetched (Principle IV: re-runs are
    /// incremental).
    AlreadyPresent(Track),
    /// No audio was fetched; the run continues regardless (Principle III).
    Skipped(SkipReason),
}

/// Failure downloading — reserved for infrastructure, never for one track's bad luck.
#[derive(Debug, Error)]
pub enum DownloadAudioError {
    /// Reading settings or persisting the track failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Recording the trail failed.
    #[error(transparent)]
    Audit(#[from] AuditError),
}

/// The collaborators `DownloadAudio` needs (grouped per the 2+-params convention).
pub struct DownloadAudioPorts {
    /// Fetches the audio.
    pub downloader: Arc<dyn AudioDownloaderPort>,
    /// Persists the track's new local path.
    pub tracks: Arc<dyn TrackRepository>,
    /// Holds the opt-in flag — the authority on whether downloading may happen at all.
    pub settings: Arc<dyn SettingsRepository>,
    /// Records the observable trail.
    pub audit: Arc<AuditRecorder>,
}

/// Downloads one track's audio, if the user has opted in.
pub struct DownloadAudio {
    downloader: Arc<dyn AudioDownloaderPort>,
    tracks: Arc<dyn TrackRepository>,
    settings: Arc<dyn SettingsRepository>,
    audit: Arc<AuditRecorder>,
}

impl DownloadAudio {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(ports: DownloadAudioPorts) -> Self {
        Self {
            downloader: ports.downloader,
            tracks: ports.tracks,
            settings: ports.settings,
            audit: ports.audit,
        }
    }

    /// Downloads `track`'s audio into `dest_dir` under `run_id`.
    ///
    /// # Errors
    /// [`DownloadAudioError::Repo`] if settings/persistence fail; [`DownloadAudioError::Audit`] if
    /// the trail cannot be recorded. A track that simply cannot be downloaded is `Ok(Skipped)`.
    pub async fn execute(
        &self,
        run_id: RunId,
        track: &Track,
        dest_dir: &Path,
    ) -> Result<DownloadOutcome, DownloadAudioError> {
        if !self.settings.load().await?.download_enabled() {
            return self.skip(run_id, track, SkipReason::DownloadDisabled).await;
        }
        if has_usable_audio(track) {
            self.record(
                run_id,
                track,
                AuditOutcome::Completed,
                AuditDetail::new().with("result", "already_present"),
            )
            .await?;
            return Ok(DownloadOutcome::AlreadyPresent(track.clone()));
        }

        match self.downloader.download(track, dest_dir).await {
            Ok(audio) => {
                let downloaded = track.downloaded(audio.path);
                self.tracks.upsert(&downloaded).await?;
                self.record(
                    run_id,
                    track,
                    AuditOutcome::Created,
                    // The path is local and user-chosen, never a credential — safe to record, and it
                    // is the whole point of the event ("where did this track's audio go?").
                    AuditDetail::new().with("result", "downloaded"),
                )
                .await?;
                Ok(DownloadOutcome::Downloaded(downloaded))
            }
            Err(error) => self.skip(run_id, track, skip_reason(&error)).await,
        }
    }

    /// Records a skip and returns it as the outcome.
    async fn skip(
        &self,
        run_id: RunId,
        track: &Track,
        reason: SkipReason,
    ) -> Result<DownloadOutcome, DownloadAudioError> {
        self.record(
            run_id,
            track,
            AuditOutcome::Failed,
            AuditDetail::new().with("reason", reason.as_str()),
        )
        .await?;
        Ok(DownloadOutcome::Skipped(reason))
    }

    /// Records one download event.
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
                stage: PipelineStage::Download,
                kind: AuditKind::DownloadResult,
                outcome,
                detail,
            })
            .await
    }
}

/// Whether the track already has audio on disk.
///
/// The recorded path is checked against the filesystem rather than trusted: the library persists
/// across runs, and a user who cleaned out their downloads folder would otherwise leave every track
/// claiming audio it no longer has — and the analyzer failing to decode a file that is not there.
fn has_usable_audio(track: &Track) -> bool {
    track.local_audio_path().is_some_and(|path| path.is_file())
}

/// Maps the port's typed error onto the skip reason reported to the user.
fn skip_reason(error: &DownloadError) -> SkipReason {
    match error {
        DownloadError::Unavailable => SkipReason::Unavailable,
        DownloadError::ToolMissing => SkipReason::ToolMissing,
        DownloadError::Io { .. } => SkipReason::Io,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::settings::{ExportMode, Settings};
    use domain::track::{LikedTrack, TrackId};
    use uuid::Uuid;

    use crate::ports::audit_log::AuditLogPort;
    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::in_memory_settings_repository::InMemorySettingsRepository;
    use crate::testkit::in_memory_track_repository::InMemoryTrackRepository;
    use crate::testkit::seq_id_provider::SeqIdProvider;
    use crate::testkit::stub_audio_downloader::StubAudioDownloader;

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

    struct Fixture {
        download: DownloadAudio,
        downloader: Arc<StubAudioDownloader>,
        tracks: Arc<InMemoryTrackRepository>,
        audit_log: Arc<InMemoryAuditLog>,
        dest: tempfile::TempDir,
    }

    async fn fixture(downloader: StubAudioDownloader, download_enabled: bool) -> Fixture {
        let downloader = Arc::new(downloader);
        let tracks = Arc::new(InMemoryTrackRepository::new());
        let settings = Arc::new(InMemorySettingsRepository::new());
        settings
            .save(&Settings::new(
                Settings::default().confidence_threshold(),
                download_enabled,
                ExportMode::Local,
            ))
            .await
            .expect("seed settings");
        let audit_log = Arc::new(InMemoryAuditLog::new());
        let audit = Arc::new(AuditRecorder::new(
            audit_log.clone(),
            Arc::new(FixedClock::at_millis(1_000)),
            Arc::new(SeqIdProvider::new()),
        ));
        Fixture {
            download: DownloadAudio::new(DownloadAudioPorts {
                downloader: downloader.clone(),
                tracks: tracks.clone(),
                settings,
                audit,
            }),
            downloader,
            tracks,
            audit_log,
            dest: tempfile::tempdir().expect("temp dir"),
        }
    }

    fn run() -> RunId {
        RunId::from_uuid(Uuid::nil())
    }

    /// The gate is the whole of Principle V here: with download off (the default), the downloader
    /// must never be reached — not merely have its result discarded.
    #[tokio::test]
    async fn opt_out_never_reaches_the_downloader() {
        let fx = fixture(StubAudioDownloader::available(), false).await;

        let outcome = fx
            .download
            .execute(run(), &track(), fx.dest.path())
            .await
            .unwrap();

        assert_eq!(
            outcome,
            DownloadOutcome::Skipped(SkipReason::DownloadDisabled)
        );
        assert_eq!(fx.downloader.call_count(), 0);
    }

    #[tokio::test]
    async fn opt_in_downloads_and_persists_the_path() {
        let fx = fixture(StubAudioDownloader::available(), true).await;

        let outcome = fx
            .download
            .execute(run(), &track(), fx.dest.path())
            .await
            .unwrap();

        let DownloadOutcome::Downloaded(downloaded) = outcome else {
            panic!("expected a download, got {outcome:?}");
        };
        assert!(downloaded.local_audio_path().expect("path").is_file());
        let stored = fx
            .tracks
            .find_by_id(track().id())
            .await
            .unwrap()
            .expect("persisted");
        assert!(stored.local_audio_path().is_some());
    }

    /// Principle III: a deleted or geo-blocked track is reported and the run carries on.
    #[tokio::test]
    async fn unavailable_track_is_a_reported_skip_not_an_error() {
        let fx = fixture(StubAudioDownloader::unavailable(), true).await;

        let outcome = fx
            .download
            .execute(run(), &track(), fx.dest.path())
            .await
            .unwrap();

        assert_eq!(outcome, DownloadOutcome::Skipped(SkipReason::Unavailable));
        let events = fx.audit_log.events_for_track(track().id()).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome(), AuditOutcome::Failed);
    }

    /// Principle IV: a second run over an already-downloaded library re-fetches nothing.
    #[tokio::test]
    async fn existing_audio_is_not_downloaded_twice() {
        let fx = fixture(StubAudioDownloader::available(), true).await;

        let first = fx
            .download
            .execute(run(), &track(), fx.dest.path())
            .await
            .unwrap();
        let DownloadOutcome::Downloaded(downloaded) = first else {
            panic!("expected a download");
        };
        let second = fx
            .download
            .execute(run(), &downloaded, fx.dest.path())
            .await
            .unwrap();

        assert!(matches!(second, DownloadOutcome::AlreadyPresent(_)));
        assert_eq!(fx.downloader.call_count(), 1, "the second run re-fetched");
    }

    /// A path recorded for a file the user has since deleted must not count as "already present" —
    /// that would leave the track permanently un-analyzable, its audio silently absent.
    #[tokio::test]
    async fn a_recorded_path_to_a_missing_file_is_re_downloaded() {
        let fx = fixture(StubAudioDownloader::available(), true).await;
        let ghost = track().downloaded(fx.dest.path().join("deleted-by-the-user.mp3"));

        let outcome = fx
            .download
            .execute(run(), &ghost, fx.dest.path())
            .await
            .unwrap();

        assert!(matches!(outcome, DownloadOutcome::Downloaded(_)));
        assert_eq!(fx.downloader.call_count(), 1);
    }

    #[tokio::test]
    async fn missing_tool_is_reported_as_environmental() {
        let fx = fixture(StubAudioDownloader::tool_missing(), true).await;

        let outcome = fx
            .download
            .execute(run(), &track(), fx.dest.path())
            .await
            .unwrap();

        assert_eq!(outcome, DownloadOutcome::Skipped(SkipReason::ToolMissing));
        assert!(SkipReason::ToolMissing.is_environmental());
        assert!(!SkipReason::Unavailable.is_environmental());
    }
}
