//! In-memory `AuditLogPort` fake — append-only, with test-only inspection helpers.

use std::sync::Mutex;

use async_trait::async_trait;

use domain::audit::AuditEvent;
use domain::track::TrackId;

use crate::ports::audit_log::{AuditError, AuditLogPort};

/// An append-only `AuditLogPort` backed by an in-memory vector.
#[derive(Debug, Default)]
pub struct InMemoryAuditLog {
    events: Mutex<Vec<AuditEvent>>,
}

impl InMemoryAuditLog {
    /// Builds an empty audit log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of every recorded event (test inspection helper).
    #[must_use]
    pub fn all(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .expect("audit log mutex poisoned")
            .clone()
    }
}

#[async_trait]
impl AuditLogPort for InMemoryAuditLog {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.events
            .lock()
            .expect("audit log mutex poisoned")
            .push(event);
        Ok(())
    }

    async fn events_for_track(&self, track_id: &TrackId) -> Result<Vec<AuditEvent>, AuditError> {
        let events = self.events.lock().expect("audit log mutex poisoned");
        let mut matching: Vec<AuditEvent> = events
            .iter()
            .filter(|e| e.track_id() == Some(track_id))
            .cloned()
            .collect();
        matching.sort_by_key(|e| e.occurred_at().as_millis());
        Ok(matching)
    }
}
