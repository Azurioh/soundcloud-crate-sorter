//! Shared plumbing for the SQLite adapters: the shared connection handle and the vendor→domain
//! error mapping (keeps `rusqlite::Error` out of the application layer).

use std::sync::{Arc, Mutex};

use application::ports::repo_error::RepoError;
use rusqlite::{Connection, ErrorCode};

/// A single SQLite connection shared across all repositories, serialized by a mutex. Sufficient for
/// a single-user local desktop app (research.md R6); no connection pool needed.
pub type SharedConnection = Arc<Mutex<Connection>>;

/// Maps a `rusqlite::Error` onto the vendor-free [`RepoError`], carrying the original as `source`
/// (never surfaced verbatim to the client).
#[must_use]
pub fn to_repo_error(error: rusqlite::Error) -> RepoError {
    if let rusqlite::Error::SqliteFailure(failure, _) = &error {
        if failure.code == ErrorCode::ConstraintViolation {
            return RepoError::Constraint {
                source: Box::new(error),
            };
        }
    }
    RepoError::Io {
        source: Box::new(error),
    }
}
