//! `IdProvider` — the only sanctioned source of new identifiers (no `Uuid::new_v4` elsewhere).

use uuid::Uuid;

/// Mints fresh UUIDs for new entities. In-memory fakes return a deterministic sequence.
pub trait IdProvider: Send + Sync {
    /// Returns a freshly minted UUID.
    fn new_id(&self) -> Uuid;
}
