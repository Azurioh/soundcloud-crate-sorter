//! `AuditLogPort` — the append-only trail that makes classification answerable (Principle VII).

use async_trait::async_trait;
use thiserror::Error;

use domain::audit::AuditEvent;
use domain::track::TrackId;

use crate::ports::BoxError;

/// Failure writing/reading the audit log. Failing to record is logged loudly — audit is not optional.
#[derive(Debug, Error)]
pub enum AuditError {
    /// An I/O failure persisting or reading an event.
    #[error("audit log I/O error")]
    Io {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
}

/// Append-only audit store. Every classification, triage action, download, dedup skip, and export
/// result is recorded; `events_for_track` answers "why is this track in this crate?".
#[async_trait]
pub trait AuditLogPort: Send + Sync {
    /// Appends one event.
    ///
    /// # Errors
    /// [`AuditError::Io`] on failure to persist.
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError>;

    /// Returns every event concerning a given track, oldest first.
    ///
    /// # Errors
    /// [`AuditError::Io`] on failure to read.
    async fn events_for_track(&self, track_id: &TrackId) -> Result<Vec<AuditEvent>, AuditError>;
}
