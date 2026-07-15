//! `SettingsRepository` contract run against both the in-memory fake and the SQLite adapter.

mod support;

use adapters::sqlite_schema::SqliteDatabase;
use adapters::sqlite_settings_repository::SqliteSettingsRepository;
use application::testkit::in_memory_settings_repository::InMemorySettingsRepository;

#[tokio::test]
async fn in_memory_settings_repository_honors_contract() {
    let repo = InMemorySettingsRepository::new();
    support::settings_repository_suite(&repo).await;
}

#[tokio::test]
async fn sqlite_settings_repository_honors_contract() {
    let db = SqliteDatabase::open_in_memory().expect("open db");
    let repo = SqliteSettingsRepository::new(db.connection());
    support::settings_repository_suite(&repo).await;
}
