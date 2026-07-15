//! In-memory `DecisionRepository` fake — append-only decision history keyed by track.

use std::sync::Mutex;

use async_trait::async_trait;

use domain::classification::ClassificationDecision;
use domain::track::TrackId;

use crate::ports::decision_repository::DecisionRepository;
use crate::ports::repo_error::RepoError;

/// A `DecisionRepository` backed by an in-memory vector, appended in call order.
pub struct InMemoryDecisionRepository {
    decisions: Mutex<Vec<ClassificationDecision>>,
}

impl InMemoryDecisionRepository {
    /// Builds an empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self {
            decisions: Mutex::new(Vec::new()),
        }
    }

    /// Returns how many decisions have been recorded (test assertions).
    #[must_use]
    pub fn len(&self) -> usize {
        self.decisions
            .lock()
            .expect("decision mutex poisoned")
            .len()
    }

    /// Whether no decision has been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for InMemoryDecisionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecisionRepository for InMemoryDecisionRepository {
    async fn record(&self, decision: &ClassificationDecision) -> Result<(), RepoError> {
        let mut decisions = self.decisions.lock().expect("decision mutex poisoned");
        decisions.push(decision.clone());
        Ok(())
    }

    async fn find_latest_for_track(
        &self,
        track_id: &TrackId,
    ) -> Result<Option<ClassificationDecision>, RepoError> {
        let decisions = self.decisions.lock().expect("decision mutex poisoned");
        Ok(latest_for(&decisions, track_id))
    }
}

/// Picks the newest decision for `track_id`, breaking `decided_at` ties by insertion order.
///
/// Ties are the normal case here, not an edge case: a re-classification records its decision within
/// the same millisecond as the previous one under a fixed test clock. `max_by_key` returns the
/// **last** of several equal maxima, which mirrors the real adapter's `rowid DESC` tie-break — so
/// both sides of the contract agree on which decision is authoritative.
fn latest_for(
    decisions: &[ClassificationDecision],
    track_id: &TrackId,
) -> Option<ClassificationDecision> {
    decisions
        .iter()
        .filter(|decision| decision.track_id() == track_id)
        .max_by_key(|decision| decision.decided_at().as_millis())
        .cloned()
}
