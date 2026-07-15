//! T020 — `DecisionRepository` contract run against both the in-memory fake and the SQLite adapter.

mod support;

use std::sync::Arc;

use adapters::sqlite_crate_repository::SqliteCrateRepository;
use adapters::sqlite_decision_repository::SqliteDecisionRepository;
use adapters::sqlite_schema::SqliteDatabase;
use adapters::sqlite_track_repository::SqliteTrackRepository;
use adapters::uuid_id_provider::UuidIdProvider;
use application::ports::crate_repository::{CrateRepository, CrateSpec};
use application::ports::id_provider::IdProvider;
use application::ports::track_repository::TrackRepository;
use application::testkit::in_memory_decision_repository::InMemoryDecisionRepository;
use domain::crate_::{CrateId, CrateOrigin};
use domain::track::{LikedTrack, Track, TrackId};
use uuid::Uuid;

/// The track a decision points at.
fn track_id() -> TrackId {
    TrackId::from_uuid(Uuid::from_u128(880))
}

#[tokio::test]
async fn in_memory_decision_repository_honors_contract() {
    let repo = InMemoryDecisionRepository::new();
    // The fake does not enforce foreign keys, so any track/crate id is fine.
    support::decision_repository_suite(&repo, track_id(), CrateId::from_uuid(Uuid::from_u128(881)))
        .await;
}

#[tokio::test]
async fn sqlite_decision_repository_honors_contract() {
    let db = SqliteDatabase::open_in_memory().expect("open db");
    let ids: Arc<dyn IdProvider> = Arc::new(UuidIdProvider::new());

    // The classification_decisions FKs require real track and crate rows.
    let crates = SqliteCrateRepository::new(db.connection(), ids);
    let crate_ = crates
        .find_or_create(&CrateSpec {
            genre: "House".to_owned(),
            role: None,
            origin: CrateOrigin::Auto,
        })
        .await
        .expect("seed crate");
    let tracks = SqliteTrackRepository::new(db.connection());
    tracks
        .upsert(&Track::from_scan(
            track_id(),
            LikedTrack {
                source_track_id: "sc:880".to_owned(),
                title: "Title".to_owned(),
                artist: "Artist".to_owned(),
                source_genre: None,
                duration_ms: 240_000,
                permalink_url: "https://soundcloud.com/a/track".to_owned(),
                artwork_url: None,
            },
        ))
        .await
        .expect("seed track");

    let repo = SqliteDecisionRepository::new(db.connection());
    support::decision_repository_suite(&repo, track_id(), *crate_.id()).await;
}
