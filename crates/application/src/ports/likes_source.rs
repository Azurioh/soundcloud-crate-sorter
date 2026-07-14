//! `LikesSourcePort` — reads a user's public likes (no login).

use async_trait::async_trait;
use thiserror::Error;

use domain::track::LikedTrack;

use crate::ports::BoxError;

/// A resolved SoundCloud user identity (opaque string id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUserId(String);

impl SourceUserId {
    /// Wraps a raw source user id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The underlying id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Failure reading likes. Messages are neutral — a raw profile URL is never interpolated (it is
/// user input; details go in `source`, error-design).
#[derive(Debug, Error)]
pub enum LikesSourceError {
    /// The profile could not be resolved.
    #[error("SoundCloud profile not found")]
    ProfileNotFound,
    /// The profile (or its likes) is private / not publicly readable.
    #[error("SoundCloud profile is private")]
    ProfilePrivate,
    /// The source rate-limited us (retry after backoff).
    #[error("rate limited by SoundCloud")]
    RateLimited,
    /// A transport/parse failure talking to the source.
    #[error("transport error reading SoundCloud likes")]
    Transport {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
}

/// Reads public likes and resolves profile URLs. Pagination (`next_href`) and rotating `client_id`
/// are handled inside the adapter.
#[async_trait]
pub trait LikesSourcePort: Send + Sync {
    /// Resolves a public profile URL to a stable source user id.
    ///
    /// # Errors
    /// [`LikesSourceError::ProfileNotFound`] / [`LikesSourceError::ProfilePrivate`] /
    /// [`LikesSourceError::RateLimited`] / [`LikesSourceError::Transport`].
    async fn resolve_user(&self, profile_url: &str) -> Result<SourceUserId, LikesSourceError>;

    /// Returns every publicly readable liked track, following pagination internally.
    ///
    /// # Errors
    /// [`LikesSourceError::ProfilePrivate`] / [`LikesSourceError::RateLimited`] /
    /// [`LikesSourceError::Transport`].
    async fn list_likes(&self, user: &SourceUserId) -> Result<Vec<LikedTrack>, LikesSourceError>;
}
