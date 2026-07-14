//! `PlaylistPublisherPort` — SoundCloud playlist write. DEFERRED to v2 (gated write API, research
//! R3): the trait is defined so v2 slots in without redesign; no v1 adapter implements it.

use async_trait::async_trait;
use thiserror::Error;

use domain::crate_::Crate;

use crate::ports::BoxError;

/// Where a crate's tracks are published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishTarget {
    /// Create a new playlist.
    NewPlaylist,
    /// Fill an existing playlist by id.
    ExistingPlaylist(String),
}

/// The result of a publish, deduplicated against the playlist's existing contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishOutcome {
    /// Whether a new playlist was created.
    pub created: bool,
    /// Number of tracks added.
    pub added: usize,
    /// Number of tracks skipped because already present.
    pub skipped: usize,
}

/// Failure publishing to SoundCloud (v2).
#[derive(Debug, Error)]
pub enum PublishError {
    /// Authentication/authorization failed.
    #[error("not authorized to publish")]
    Unauthorized,
    /// The target playlist was not found.
    #[error("target playlist not found")]
    PlaylistNotFound,
    /// A transport failure.
    #[error("transport error publishing playlist")]
    Transport {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
}

/// Authenticated playlist write: create-new or fill-existing, skipping tracks already present
/// (FR-019–021). **No v1 adapter** — defined for architectural stability only.
#[async_trait]
pub trait PlaylistPublisherPort: Send + Sync {
    /// Publishes `crate_`'s tracks to `target`.
    ///
    /// # Errors
    /// [`PublishError::Unauthorized`] / [`PublishError::PlaylistNotFound`] / [`PublishError::Transport`].
    async fn publish(
        &self,
        crate_: &Crate,
        target: PublishTarget,
    ) -> Result<PublishOutcome, PublishError>;
}
