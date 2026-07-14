//! `AuditLogPort` contract run against both the in-memory fake and the SQLite adapter.

mod support;

use adapters::sqlite_audit_log::SqliteAuditLog;
use adapters::sqlite_schema::SqliteDatabase;
use application::testkit::in_memory_audit_log::InMemoryAuditLog;

#[tokio::test]
async fn in_memory_audit_log_honors_contract() {
    let log = InMemoryAuditLog::new();
    support::audit_log_suite(&log).await;
}

#[tokio::test]
async fn sqlite_audit_log_honors_contract() {
    let db = SqliteDatabase::open_in_memory().expect("open db");
    let log = SqliteAuditLog::new(db.connection());
    support::audit_log_suite(&log).await;
}
