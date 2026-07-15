//! `GenreVibeClassifierPort` — AI genre/vibe classification, used ONLY for ambiguous/missing genre.
//! MUST NOT return BPM/key/energy (those are deterministic; constitution Principle I).

use async_trait::async_trait;
use thiserror::Error;

use domain::confidence::Confidence;

use crate::ports::BoxError;

/// The text signals handed to the classifier. Deterministic audio features are never sent here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassificationInput {
    /// Track title.
    pub title: String,
    /// Artist / uploader.
    pub artist: String,
    /// SoundCloud genre tag, if any (may be ambiguous — that's why we ask).
    pub source_genre: Option<String>,
    /// Free-text description, if available.
    pub description: Option<String>,
}

/// One candidate genre with the model's confidence in it.
#[derive(Debug, Clone, PartialEq)]
pub struct GenreCandidate {
    /// The candidate genre label.
    pub genre: String,
    /// The model's confidence for this candidate.
    pub confidence: Confidence,
}

/// The classifier's answer: ranked genre candidates plus vibe tags.
#[derive(Debug, Clone, PartialEq)]
pub struct GenreVibeSuggestion {
    /// Candidate genres, most-confident first.
    pub candidates: Vec<GenreCandidate>,
    /// Optional vibe/mood tags (never required).
    pub vibe_tags: Vec<String>,
}

/// Failure classifying. A schema mismatch is surfaced as `BadResponse` so the use case can treat it
/// as low confidence rather than a hard failure.
#[derive(Debug, Error)]
pub enum ClassifyError {
    /// The AI provider rate-limited us.
    #[error("rate limited by the classifier")]
    RateLimited,
    /// A transport failure talking to the provider.
    #[error("transport error calling the classifier")]
    Transport {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
    /// The response did not match the expected schema.
    #[error("classifier returned an unparseable response")]
    BadResponse {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
}

/// Classifies genre/vibe from text signals. Returns candidates + confidence that feed the
/// confidence score; the **use case**, not the model, owns the threshold decision.
#[async_trait]
pub trait GenreVibeClassifierPort: Send + Sync {
    /// Classifies one track's genre/vibe.
    ///
    /// # Errors
    /// [`ClassifyError::RateLimited`] / [`ClassifyError::Transport`] / [`ClassifyError::BadResponse`].
    async fn classify(
        &self,
        input: &ClassificationInput,
    ) -> Result<GenreVibeSuggestion, ClassifyError>;
}
