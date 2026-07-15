//! Stub `AudioDownloaderPort` — produces a local file with no network and no `yt-dlp`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;

use domain::track::Track;

use crate::ports::audio_downloader::{AudioDownloaderPort, DownloadError, DownloadedAudio};

/// The bytes the stub writes. Not decodable audio — the downloader's contract is "a file appears at
/// the returned path"; making sense of its contents is `AudioAnalyzerPort`'s job.
const STUB_AUDIO_BYTES: &[u8] = b"stub audio";

/// What the stub does when asked to download.
enum Behavior {
    /// Write a placeholder file and return its path.
    Succeed,
    /// Fail the way a deleted or geo-blocked track does.
    Unavailable,
    /// Fail the way a missing `yt-dlp` binary does.
    ToolMissing,
}

/// An `AudioDownloaderPort` that writes a placeholder file instead of fetching audio, and counts
/// calls so a test can prove the opt-in gate actually gates (Principle V).
pub struct StubAudioDownloader {
    behavior: Behavior,
    calls: Mutex<usize>,
}

impl StubAudioDownloader {
    /// Builds a stub that writes a placeholder file for every track.
    #[must_use]
    pub fn available() -> Self {
        Self::with(Behavior::Succeed)
    }

    /// Builds a stub that reports every track as unavailable (deleted/geo-blocked).
    #[must_use]
    pub fn unavailable() -> Self {
        Self::with(Behavior::Unavailable)
    }

    /// Builds a stub that reports `yt-dlp` as missing.
    #[must_use]
    pub fn tool_missing() -> Self {
        Self::with(Behavior::ToolMissing)
    }

    /// Number of times `download` was invoked — `0` proves the opt-in gate held.
    #[must_use]
    pub fn call_count(&self) -> usize {
        *self.calls.lock().expect("downloader mutex poisoned")
    }

    /// Builds a stub with the given behavior.
    fn with(behavior: Behavior) -> Self {
        Self {
            behavior,
            calls: Mutex::new(0),
        }
    }
}

#[async_trait]
impl AudioDownloaderPort for StubAudioDownloader {
    async fn download(
        &self,
        track: &Track,
        dest_dir: &Path,
    ) -> Result<DownloadedAudio, DownloadError> {
        *self.calls.lock().expect("downloader mutex poisoned") += 1;
        match self.behavior {
            Behavior::Unavailable => return Err(DownloadError::Unavailable),
            Behavior::ToolMissing => return Err(DownloadError::ToolMissing),
            Behavior::Succeed => {}
        }
        // The real adapter creates the directory too, so the fake must not require the caller to.
        std::fs::create_dir_all(dest_dir).map_err(io_error)?;
        let path: PathBuf = dest_dir.join(format!("{}.mp3", track.source_track_id()));
        std::fs::write(&path, STUB_AUDIO_BYTES).map_err(io_error)?;
        Ok(DownloadedAudio { path })
    }
}

/// Wraps a filesystem failure as the port's own typed I/O error.
fn io_error(error: std::io::Error) -> DownloadError {
    DownloadError::Io {
        source: Box::new(error),
    }
}
