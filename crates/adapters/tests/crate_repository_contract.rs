//! `CrateRepository` contract run against both the in-memory fake and the SQLite adapter.

mod support;

use std::sync::Arc;

use adapters::sqlite_crate_repository::SqliteCrateRepository;
use adapters::sqlite_schema::SqliteDatabase;
use adapters::uuid_id_provider::UuidIdProvider;
use application::ports::id_provider::IdProvider;
use application::testkit::in_memory_crate_repository::InMemoryCrateRepository;
use application::testkit::seq_id_provider::SeqIdProvider;

#[tokio::test]
async fn in_memory_crate_repository_honors_contract() {
    let ids: Arc<dyn IdProvider> = Arc::new(SeqIdProvider::new());
    let repo = InMemoryCrateRepository::new(ids);
    support::crate_repository_suite(&repo).await;
}

#[tokio::test]
async fn sqlite_crate_repository_honors_contract() {
    let db = SqliteDatabase::open_in_memory().expect("open db");
    let ids: Arc<dyn IdProvider> = Arc::new(UuidIdProvider::new());
    let repo = SqliteCrateRepository::new(db.connection(), ids);
    support::crate_repository_suite(&repo).await;
}
