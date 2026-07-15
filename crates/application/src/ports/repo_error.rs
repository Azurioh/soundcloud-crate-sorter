//! Shared repository error, used by both `TrackRepository` and `CrateRepository`.

use thiserror::Error;

use crate::ports::BoxError;

/// A failure from a persistence adapter. The underlying DB error is carried as `source` and never
/// surfaced verbatim to the client.
#[derive(Debug, Error)]
pub enum RepoError {
    /// An I/O or connection-level failure.
    #[error("repository I/O error")]
    Io {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
    /// A constraint was violated (e.g. the `source_track_id` UNIQUE dedup key).
    #[error("repository constraint violation")]
    Constraint {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
    /// A row could not be mapped to/from its domain type.
    #[error("repository serialization error")]
    Serialization {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
}
