//! T020 — `TrackRepository` contract run against both the in-memory fake and the SQLite adapter.

mod support;

use std::sync::Arc;

use adapters::sqlite_crate_repository::SqliteCrateRepository;
use adapters::sqlite_schema::SqliteDatabase;
use adapters::sqlite_track_repository::SqliteTrackRepository;
use adapters::uuid_id_provider::UuidIdProvider;
use application::ports::crate_repository::{CrateRepository, CrateSpec};
use application::ports::id_provider::IdProvider;
use application::testkit::in_memory_track_repository::InMemoryTrackRepository;
use domain::crate_::{CrateId, CrateOrigin};
use uuid::Uuid;

#[tokio::test]
async fn in_memory_track_repository_honors_contract() {
    let repo = InMemoryTrackRepository::new();
    // The fake does not enforce foreign keys, so any crate id is fine.
    support::track_repository_suite(&repo, CrateId::from_uuid(Uuid::from_u128(999))).await;
}

#[tokio::test]
async fn sqlite_track_repository_honors_contract() {
    let db = SqliteDatabase::open_in_memory().expect("open db");
    let ids: Arc<dyn IdProvider> = Arc::new(UuidIdProvider::new());
    let crates = SqliteCrateRepository::new(db.connection(), ids);
    // The tracks.crate_id FK requires a real crate row.
    let crate_ = crates
        .find_or_create(&CrateSpec {
            genre: "House".to_owned(),
            role: None,
            origin: CrateOrigin::Auto,
        })
        .await
        .expect("seed crate");
    let repo = SqliteTrackRepository::new(db.connection());
    support::track_repository_suite(&repo, *crate_.id()).await;
}
