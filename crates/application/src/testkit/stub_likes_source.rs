//! Stub `LikesSourcePort` — returns preloaded likes (or a configured error) with no network.

use async_trait::async_trait;

use domain::track::LikedTrack;

use crate::ports::likes_source::{LikesSourceError, LikesSourcePort, SourceUserId};

/// A `LikesSourcePort` that resolves any profile URL to a fixed user id and returns a preloaded
/// set of likes. The `profile_not_found` constructor exercises the resolve failure path.
#[derive(Debug, Clone)]
pub struct StubLikesSource {
    user_id: String,
    likes: Vec<LikedTrack>,
    profile_not_found: bool,
}

impl StubLikesSource {
    /// Builds a stub that resolves to `user_id` and returns `likes`.
    #[must_use]
    pub fn with_likes(user_id: impl Into<String>, likes: Vec<LikedTrack>) -> Self {
        Self {
            user_id: user_id.into(),
            likes,
            profile_not_found: false,
        }
    }

    /// Builds a stub whose `resolve_user` fails with `ProfileNotFound`.
    #[must_use]
    pub fn profile_not_found() -> Self {
        Self {
            user_id: String::new(),
            likes: Vec::new(),
            profile_not_found: true,
        }
    }
}

#[async_trait]
impl LikesSourcePort for StubLikesSource {
    async fn resolve_user(&self, _profile_url: &str) -> Result<SourceUserId, LikesSourceError> {
        if self.profile_not_found {
            return Err(LikesSourceError::ProfileNotFound);
        }
        Ok(SourceUserId::new(self.user_id.clone()))
    }

    async fn list_likes(&self, _user: &SourceUserId) -> Result<Vec<LikedTrack>, LikesSourceError> {
        Ok(self.likes.clone())
    }
}
