//! Deterministic `IdProvider` fake.

use std::sync::Mutex;

use uuid::Uuid;

use crate::ports::id_provider::IdProvider;

/// An id provider that hands out a deterministic ascending UUID sequence (`00…01`, `00…02`, …),
/// so ids are stable and readable across test runs.
#[derive(Debug)]
pub struct SeqIdProvider {
    next: Mutex<u128>,
}

impl SeqIdProvider {
    /// Builds a provider starting the sequence at `1`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: Mutex::new(1),
        }
    }
}

impl Default for SeqIdProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl IdProvider for SeqIdProvider {
    fn new_id(&self) -> Uuid {
        let mut guard = self.next.lock().expect("SeqIdProvider mutex poisoned");
        let value = *guard;
        *guard += 1;
        Uuid::from_u128(value)
    }
}
