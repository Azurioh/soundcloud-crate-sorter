//! T048 — `AudioDownloaderPort` contract. The in-memory fake always runs; the real `yt-dlp` adapter
//! runs only under `--features real-adapters` (needs network, the `yt-dlp` binary, and a public
//! SoundCloud track permalink in `SC_TEST_TRACK`).

mod support;

use application::ports::audio_downloader::{AudioDownloaderPort, DownloadError};
use application::testkit::stub_audio_downloader::StubAudioDownloader;
use domain::track::{LikedTrack, Track, TrackId};
use uuid::Uuid;

/// Builds a track pointing at `permalink_url`.
fn track(permalink_url: &str) -> Track {
    Track::from_scan(
        TrackId::from_uuid(Uuid::from_u128(1)),
        LikedTrack {
            source_track_id: "sc:1".into(),
            title: "Title".into(),
            artist: "Artist".into(),
            source_genre: Some("House".into()),
            duration_ms: 240_000,
            permalink_url: permalink_url.to_owned(),
            artwork_url: None,
        },
    )
}

#[tokio::test]
async fn in_memory_downloader_honors_contract() {
    let dest = tempfile::tempdir().expect("temp dir");
    let downloader = StubAudioDownloader::available();
    support::audio_downloader_suite(
        &downloader,
        &track("https://soundcloud.com/a/track"),
        dest.path(),
    )
    .await;
}

/// Both sides must report an unavailable track as `Unavailable` rather than inventing an empty file.
#[tokio::test]
async fn in_memory_downloader_reports_unavailable() {
    let dest = tempfile::tempdir().expect("temp dir");
    let downloader = StubAudioDownloader::unavailable();
    let error = downloader
        .download(&track("https://soundcloud.com/a/track"), dest.path())
        .await
        .expect_err("an unavailable track must not succeed");
    assert!(matches!(error, DownloadError::Unavailable));
}

#[cfg(feature = "real-adapters")]
#[tokio::test]
async fn real_downloader_honors_contract() {
    use adapters::ytdlp_audio_downloader::YtdlpAudioDownloader;
    let permalink = std::env::var("SC_TEST_TRACK")
        .expect("set SC_TEST_TRACK to a public SoundCloud track permalink");
    let dest = tempfile::tempdir().expect("temp dir");
    let downloader = YtdlpAudioDownloader::new();
    support::audio_downloader_suite(&downloader, &track(&permalink), dest.path()).await;
}

/// The real adapter must map a track that does not exist onto the port's own `Unavailable`, rather
/// than leaking a subprocess exit code or hanging the run (Principle III).
#[cfg(feature = "real-adapters")]
#[tokio::test]
async fn real_downloader_reports_a_missing_track_as_unavailable() {
    use adapters::ytdlp_audio_downloader::YtdlpAudioDownloader;
    let dest = tempfile::tempdir().expect("temp dir");
    let downloader = YtdlpAudioDownloader::new();
    let error = downloader
        .download(
            &track("https://soundcloud.com/this-user-does-not-exist-xyz/nor-does-this-track"),
            dest.path(),
        )
        .await
        .expect_err("a missing track must not succeed");
    assert!(matches!(error, DownloadError::Unavailable));
}
