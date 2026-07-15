//! T025 — `LikesSourcePort` contract. The in-memory fake always runs; the real api-v2 adapter runs
//! only under `--features real-adapters` (needs network + a public profile URL in `SC_TEST_PROFILE`).

mod support;

use application::testkit::stub_likes_source::StubLikesSource;
use domain::track::LikedTrack;

fn liked(source_id: &str) -> LikedTrack {
    LikedTrack {
        source_track_id: source_id.to_owned(),
        title: "Title".into(),
        artist: "Artist".into(),
        source_genre: Some("House".into()),
        duration_ms: 240_000,
        permalink_url: "https://soundcloud.com/a/track".into(),
        artwork_url: None,
    }
}

#[tokio::test]
async fn in_memory_likes_source_honors_contract() {
    let source = StubLikesSource::with_likes("u1", vec![liked("sc:1"), liked("sc:2")]);
    support::likes_source_suite(&source, "https://soundcloud.com/u1").await;
}

#[cfg(feature = "real-adapters")]
#[tokio::test]
async fn real_likes_source_honors_contract() {
    use adapters::internal_api_likes_source::InternalApiLikesSource;
    let profile = std::env::var("SC_TEST_PROFILE")
        .expect("set SC_TEST_PROFILE to a public SoundCloud profile URL with likes");
    let source = InternalApiLikesSource::new();
    support::likes_source_suite(&source, &profile).await;
}
