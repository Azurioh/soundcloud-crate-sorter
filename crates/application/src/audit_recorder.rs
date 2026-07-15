//! `AuditRecorder` — an application service that composes `AuditLogPort` + `ClockPort` +
//! `IdProvider` into a single call for recording events, so every use case records the trail the
//! same way (DRY) without re-wiring three ports each time.

use std::sync::Arc;

use domain::audit::{
    AuditDetail, AuditEvent, AuditEventId, AuditKind, AuditOutcome, NewAuditEvent, PipelineStage,
    RunId,
};
use domain::track::TrackId;

use crate::ports::audit_log::{AuditError, AuditLogPort};
use crate::ports::clock::ClockPort;
use crate::ports::id_provider::IdProvider;

/// The fields describing one audit event to record (grouped per the 2+-params convention).
#[derive(Debug, Clone)]
pub struct RecordParams {
    /// The run this event belongs to.
    pub run_id: RunId,
    /// The track this event concerns, if any (`None` for run-level events).
    pub track_id: Option<TrackId>,
    /// The pipeline stage.
    pub stage: PipelineStage,
    /// The event category.
    pub kind: AuditKind,
    /// The outcome.
    pub outcome: AuditOutcome,
    /// Structured, secret-free detail.
    pub detail: AuditDetail,
}

/// Records audit events, minting the event id and stamping the time from the canonical providers.
pub struct AuditRecorder {
    audit: Arc<dyn AuditLogPort>,
    clock: Arc<dyn ClockPort>,
    ids: Arc<dyn IdProvider>,
}

impl AuditRecorder {
    /// Builds a recorder over the audit log and the time/id providers.
    #[must_use]
    pub fn new(
        audit: Arc<dyn AuditLogPort>,
        clock: Arc<dyn ClockPort>,
        ids: Arc<dyn IdProvider>,
    ) -> Self {
        Self { audit, clock, ids }
    }

    /// Records one event, assigning its id and timestamp from the canonical providers.
    ///
    /// # Errors
    /// [`AuditError::Io`] if the underlying store fails to persist.
    pub async fn record(&self, params: RecordParams) -> Result<(), AuditError> {
        let event = AuditEvent::new(NewAuditEvent {
            id: AuditEventId::from_uuid(self.ids.new_id()),
            run_id: params.run_id,
            track_id: params.track_id,
            stage: params.stage,
            kind: params.kind,
            outcome: params.outcome,
            detail: params.detail,
            occurred_at: self.clock.now(),
        });
        self.audit.record(event).await
    }
}
