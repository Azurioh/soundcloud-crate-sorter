//! `TrackRepository` — persistence for `Track`, keyed for dedup + idempotent incremental scans.

use async_trait::async_trait;

use domain::crate_::CrateId;
use domain::track::{Track, TrackId};

use crate::ports::repo_error::RepoError;

/// Stores and queries tracks. `find_*` returns `Option` (never throws on absence); a `get_*` naming
/// would imply a `NotFound` error. `upsert` MUST preserve a prior `ManuallyDecided` status.
#[async_trait]
pub trait TrackRepository: Send + Sync {
    /// Looks up a track by SoundCloud's stable id (the dedup key).
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn find_by_source_id(&self, source_id: &str) -> Result<Option<Track>, RepoError>;

    /// Looks up a track by its internal id.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn find_by_id(&self, id: &TrackId) -> Result<Option<Track>, RepoError>;

    /// Inserts or updates a track. Preserves an existing `ManuallyDecided` status (Principle IV).
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn upsert(&self, track: &Track) -> Result<(), RepoError>;

    /// Returns every track.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn list_all(&self) -> Result<Vec<Track>, RepoError>;

    /// Returns tracks currently awaiting manual triage.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn list_in_triage(&self) -> Result<Vec<Track>, RepoError>;

    /// Returns tracks assigned to a given crate.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn list_by_crate(&self, crate_id: &CrateId) -> Result<Vec<Track>, RepoError>;
}
