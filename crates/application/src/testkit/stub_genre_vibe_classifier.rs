//! Stub `GenreVibeClassifierPort` — returns a preloaded suggestion (or an error) with no network.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::ports::genre_vibe_classifier::{
    ClassificationInput, ClassifyError, GenreVibeClassifierPort, GenreVibeSuggestion,
};

/// A `GenreVibeClassifierPort` that returns a fixed suggestion and counts calls, or fails with
/// `BadResponse` so tests can assert the "treat as low confidence" path.
pub struct StubGenreVibeClassifier {
    suggestion: Option<GenreVibeSuggestion>,
    calls: Mutex<usize>,
}

impl StubGenreVibeClassifier {
    /// Builds a stub that always returns `suggestion`.
    #[must_use]
    pub fn always(suggestion: GenreVibeSuggestion) -> Self {
        Self {
            suggestion: Some(suggestion),
            calls: Mutex::new(0),
        }
    }

    /// Builds a stub that always fails with `BadResponse`.
    #[must_use]
    pub fn failing() -> Self {
        Self {
            suggestion: None,
            calls: Mutex::new(0),
        }
    }

    /// Number of times `classify` was invoked (asserts the classifier is only called when needed).
    #[must_use]
    pub fn call_count(&self) -> usize {
        *self.calls.lock().expect("classifier mutex poisoned")
    }
}

#[async_trait]
impl GenreVibeClassifierPort for StubGenreVibeClassifier {
    async fn classify(
        &self,
        _input: &ClassificationInput,
    ) -> Result<GenreVibeSuggestion, ClassifyError> {
        *self.calls.lock().expect("classifier mutex poisoned") += 1;
        match &self.suggestion {
            Some(s) => Ok(s.clone()),
            None => Err(ClassifyError::BadResponse {
                source: "stubbed failure".into(),
            }),
        }
    }
}
