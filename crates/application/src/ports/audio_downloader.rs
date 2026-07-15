//! `AudioDownloaderPort` — opt-in per-track audio download (User Story 4; adapter added in US4).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use thiserror::Error;

use domain::track::Track;

use crate::ports::BoxError;

/// The local file produced by a successful download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedAudio {
    /// Absolute path to the downloaded audio file.
    pub path: PathBuf,
}

/// Failure downloading one track. Per-track failure is a reported skip — never a run-stopper
/// (constitution Principle III).
#[derive(Debug, Error)]
pub enum DownloadError {
    /// The track is deleted, geo-blocked, or otherwise unavailable.
    #[error("track audio is unavailable")]
    Unavailable,
    /// The `yt-dlp` binary is not installed / not on PATH.
    #[error("download tool (yt-dlp) is missing")]
    ToolMissing,
    /// A local I/O failure writing the file.
    #[error("download I/O error")]
    Io {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
}

/// Downloads best-available audio for one track into `dest_dir`.
#[async_trait]
pub trait AudioDownloaderPort: Send + Sync {
    /// Downloads `track`'s audio into `dest_dir`, returning the created file path.
    ///
    /// # Errors
    /// [`DownloadError::Unavailable`] / [`DownloadError::ToolMissing`] / [`DownloadError::Io`].
    async fn download(
        &self,
        track: &Track,
        dest_dir: &Path,
    ) -> Result<DownloadedAudio, DownloadError>;
}
