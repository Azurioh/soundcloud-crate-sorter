//! `YtdlpAudioDownloader` — `AudioDownloaderPort` over the `yt-dlp` binary (T051, research.md R2).
//!
//! `yt-dlp` stays a subprocess rather than a linked library: it is the volatile part (SoundCloud's
//! delivery changes, the tool ships fixes weekly) and keeping it behind a process boundary means it
//! can be upgraded independently of this binary.
//!
//! The tool's own vocabulary never crosses the port: exit codes and stderr become `DownloadError`.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use application::ports::audio_downloader::{AudioDownloaderPort, DownloadError, DownloadedAudio};
use async_trait::async_trait;
use domain::track::Track;
use tokio::process::Command;

/// The binary invoked. On PATH — `scripts/check-system-libs.sh` verifies it.
const YTDLP_BINARY: &str = "yt-dlp";

/// Selects the best audio-only stream SoundCloud offers (research.md R2).
const FORMAT_SELECTOR: &str = "bestaudio";

/// Asks yt-dlp to print the final file's path — after any post-processing move, so the path is where
/// the file actually ended up rather than where the download started. Parsing the human-readable
/// progress output for a filename would break the first time the tool reworded it.
const PRINT_FINAL_PATH: &str = "after_move:filepath";

/// Downloads track audio by shelling out to `yt-dlp`.
#[derive(Debug, Default)]
pub struct YtdlpAudioDownloader;

impl YtdlpAudioDownloader {
    /// Builds the downloader.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AudioDownloaderPort for YtdlpAudioDownloader {
    async fn download(
        &self,
        track: &Track,
        dest_dir: &Path,
    ) -> Result<DownloadedAudio, DownloadError> {
        tokio::fs::create_dir_all(dest_dir).await.map_err(io)?;

        let output = Command::new(YTDLP_BINARY)
            .arg("--format")
            .arg(FORMAT_SELECTOR)
            // A permalink can resolve to a set; we want the one track we asked for.
            .arg("--no-playlist")
            // Writes title/artist into the file, so the export path (US5) starts from real tags.
            .arg("--embed-metadata")
            // `--print` implies simulate; this asks for the file as well as the path.
            .arg("--no-simulate")
            .arg("--print")
            .arg(PRINT_FINAL_PATH)
            .arg("--output")
            .arg(output_template(track, dest_dir))
            .arg(track.permalink_url())
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(spawn_error)?;

        if !output.status.success() {
            // Every per-track failure yt-dlp reports — 404, private, geo-block, a transient network
            // fault — is `Unavailable`, which the port defines as "deleted, geo-blocked, or otherwise
            // unavailable". Sorting them apart would mean pattern-matching yt-dlp's English error
            // prose, which changes release to release, to draw a distinction the caller does not act
            // on: every one of them is the same reported skip (Principle III). The stderr detail is
            // not lost — it is traced below for the human reading the log.
            trace_failure(track, &output.stderr);
            return Err(DownloadError::Unavailable);
        }

        let path = parse_printed_path(&output.stdout).ok_or_else(|| DownloadError::Io {
            source: "yt-dlp reported success but printed no output path".into(),
        })?;
        if !path.is_file() {
            return Err(DownloadError::Io {
                source: "yt-dlp reported a path that does not exist".into(),
            });
        }
        Ok(DownloadedAudio { path })
    }
}

/// The `-o` template: our own track id plus yt-dlp's extension placeholder.
///
/// Keyed on the internal id, not the title: titles carry slashes, emoji and unicode that a filename
/// cannot, and the id is stable, unique and filesystem-safe by construction — so a re-run overwrites
/// the same file instead of littering `Track (1).mp3` beside it (Principle IV).
fn output_template(track: &Track, dest_dir: &Path) -> PathBuf {
    dest_dir.join(format!("{}.%(ext)s", track.id()))
}

/// Reads the path yt-dlp printed on stdout (the last non-empty line).
fn parse_printed_path(stdout: &[u8]) -> Option<PathBuf> {
    let text = String::from_utf8_lossy(stdout);
    let line = text.lines().rev().find(|line| !line.trim().is_empty())?;
    Some(PathBuf::from(line.trim()))
}

/// Logs why a track could not be downloaded, at the level of the operational log rather than the
/// user-facing error (constitution Principle VII: structured logs, never print-debugging).
fn trace_failure(track: &Track, stderr: &[u8]) {
    tracing::warn!(
        track_id = %track.id(),
        stage = "download",
        outcome = "unavailable",
        detail = %String::from_utf8_lossy(stderr).trim(),
        "yt-dlp could not download the track"
    );
}

/// Distinguishes "the tool is not installed" from any other spawn failure — the one distinction the
/// caller genuinely acts on, since a missing binary dooms every remaining track in the run.
fn spawn_error(error: std::io::Error) -> DownloadError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return DownloadError::ToolMissing;
    }
    io(error)
}

/// Wraps a filesystem/process failure as the port's typed I/O error, keeping the original as source.
fn io(error: std::io::Error) -> DownloadError {
    DownloadError::Io {
        source: Box::new(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::track::{LikedTrack, TrackId};
    use uuid::Uuid;

    fn track() -> Track {
        Track::from_scan(
            TrackId::from_uuid(Uuid::from_u128(1)),
            LikedTrack {
                source_track_id: "sc:1".into(),
                title: "A / B: a title with \"unsafe\" characters".into(),
                artist: "Artist".into(),
                source_genre: None,
                duration_ms: 240_000,
                permalink_url: "https://soundcloud.com/a/track".into(),
                artwork_url: None,
            },
        )
    }

    /// The template must be built from the id, so a hostile title cannot escape `dest_dir`.
    #[test]
    fn output_template_is_keyed_on_the_id_not_the_title() {
        let template = output_template(&track(), Path::new("/downloads"));
        assert_eq!(
            template,
            PathBuf::from("/downloads/00000000-0000-0000-0000-000000000001.%(ext)s")
        );
    }

    #[test]
    fn printed_path_is_the_last_non_empty_line() {
        let stdout = b"/downloads/a.mp3\n\n";
        assert_eq!(
            parse_printed_path(stdout),
            Some(PathBuf::from("/downloads/a.mp3"))
        );
    }

    #[test]
    fn no_printed_path_is_reported_rather_than_guessed() {
        assert_eq!(parse_printed_path(b"\n  \n"), None);
    }

    /// A missing binary must be its own error: it is environmental, and the use case stops the whole
    /// download stage on it instead of retrying once per track.
    #[test]
    fn a_missing_binary_maps_to_tool_missing() {
        let error = spawn_error(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such file",
        ));
        assert!(matches!(error, DownloadError::ToolMissing));
    }

    #[test]
    fn other_spawn_failures_stay_io_errors() {
        let error = spawn_error(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ));
        assert!(matches!(error, DownloadError::Io { .. }));
    }
}
