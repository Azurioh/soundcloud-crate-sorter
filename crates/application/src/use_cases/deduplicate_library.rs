//! `DeduplicateLibrary` — collapses liked tracks by `source_track_id` (FR-003), auditing each
//! collapsed duplicate.

use std::collections::HashSet;
use std::sync::Arc;

use domain::audit::{AuditDetail, AuditKind, AuditOutcome, PipelineStage, RunId};
use domain::track::LikedTrack;

use crate::audit_recorder::{AuditRecorder, RecordParams};
use crate::ports::audit_log::AuditError;

/// The outcome of deduplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupResult {
    /// The unique liked tracks (first occurrence of each `source_track_id` wins).
    pub unique: Vec<LikedTrack>,
    /// How many duplicates were collapsed.
    pub duplicates_skipped: usize,
}

/// Collapses duplicate likes within a scanned batch and records a `DedupSkip` event per duplicate.
pub struct DeduplicateLibrary {
    audit: Arc<AuditRecorder>,
}

impl DeduplicateLibrary {
    /// Builds the use case over the audit recorder.
    #[must_use]
    pub fn new(audit: Arc<AuditRecorder>) -> Self {
        Self { audit }
    }

    /// Returns the unique subset of `liked`, auditing each collapsed duplicate under `run_id`.
    ///
    /// # Errors
    /// [`AuditError`] if recording a dedup event fails.
    pub async fn execute(
        &self,
        run_id: RunId,
        liked: Vec<LikedTrack>,
    ) -> Result<DedupResult, AuditError> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut unique: Vec<LikedTrack> = Vec::new();
        let mut duplicates_skipped = 0;

        for track in liked {
            if seen.insert(track.source_track_id.clone()) {
                unique.push(track);
            } else {
                duplicates_skipped += 1;
                self.record_skip(run_id, &track.source_track_id).await?;
            }
        }

        Ok(DedupResult {
            unique,
            duplicates_skipped,
        })
    }

    /// Records one duplicate-skip audit event (source id is public metadata, safe to include).
    async fn record_skip(&self, run_id: RunId, source_track_id: &str) -> Result<(), AuditError> {
        self.audit
            .record(RecordParams {
                run_id,
                track_id: None,
                stage: PipelineStage::Dedup,
                kind: AuditKind::DedupSkip,
                outcome: AuditOutcome::SkippedDuplicate,
                detail: AuditDetail::new().with("source_track_id", source_track_id),
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::audit::RunId;
    use uuid::Uuid;

    use crate::audit_recorder::AuditRecorder;
    use crate::testkit::fixed_clock::FixedClock;
    use crate::testkit::in_memory_audit_log::InMemoryAuditLog;
    use crate::testkit::seq_id_provider::SeqIdProvider;

    fn liked(source_id: &str) -> LikedTrack {
        LikedTrack {
            source_track_id: source_id.to_owned(),
            title: "t".into(),
            artist: "a".into(),
            source_genre: None,
            duration_ms: 200_000,
            permalink_url: "https://sc/x".into(),
            artwork_url: None,
        }
    }

    fn recorder() -> (Arc<AuditRecorder>, Arc<InMemoryAuditLog>) {
        let audit = Arc::new(InMemoryAuditLog::new());
        let recorder = Arc::new(AuditRecorder::new(
            audit.clone(),
            Arc::new(FixedClock::at_millis(1_000)),
            Arc::new(SeqIdProvider::new()),
        ));
        (recorder, audit)
    }

    #[tokio::test]
    async fn collapses_duplicates_and_audits_each() {
        let (recorder, audit) = recorder();
        let dedup = DeduplicateLibrary::new(recorder);
        let run = RunId::from_uuid(Uuid::nil());

        let input = vec![liked("sc:1"), liked("sc:2"), liked("sc:1")];
        let result = dedup.execute(run, input).await.unwrap();

        assert_eq!(result.unique.len(), 2);
        assert_eq!(result.duplicates_skipped, 1);
        assert_eq!(audit.all().len(), 1);
    }

    #[tokio::test]
    async fn no_duplicates_records_nothing() {
        let (recorder, audit) = recorder();
        let dedup = DeduplicateLibrary::new(recorder);
        let run = RunId::from_uuid(Uuid::nil());

        let result = dedup
            .execute(run, vec![liked("sc:1"), liked("sc:2")])
            .await
            .unwrap();

        assert_eq!(result.unique.len(), 2);
        assert_eq!(result.duplicates_skipped, 0);
        assert!(audit.all().is_empty());
    }
}
