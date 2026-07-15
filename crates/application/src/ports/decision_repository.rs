//! `DecisionRepository` — persistence for `ClassificationDecision`, the answerable record of how a
//! track reached its crate.
//!
//! The audit log records the same facts for observability, but it is an append-only trail with
//! stringly-typed detail; this port is the typed, queryable read path the triage queue needs to
//! rebuild a card's top suggestion and alternatives (FR-015) after the track itself has dropped its
//! crate on the way into triage.

use async_trait::async_trait;

use domain::classification::ClassificationDecision;
use domain::track::TrackId;

use crate::ports::repo_error::RepoError;

/// Appends and queries classification decisions. History is retained per track; the most recent
/// decision is authoritative (data-model: `Track 1→* ClassificationDecision`).
#[async_trait]
pub trait DecisionRepository: Send + Sync {
    /// Appends `decision` to the track's decision history.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn record(&self, decision: &ClassificationDecision) -> Result<(), RepoError>;

    /// Returns the most recent decision for `track_id`, or `None` when the track was never
    /// classified.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn find_latest_for_track(
        &self,
        track_id: &TrackId,
    ) -> Result<Option<ClassificationDecision>, RepoError>;
}
