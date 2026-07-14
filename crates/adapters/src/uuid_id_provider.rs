//! `UuidIdProvider` — the one place `Uuid::new_v4` is called (code-reuse canonical path).

use application::ports::id_provider::IdProvider;
use uuid::Uuid;

/// An `IdProvider` backed by random (v4) UUIDs.
#[derive(Debug, Default, Clone)]
pub struct UuidIdProvider;

impl UuidIdProvider {
    /// Builds the provider.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IdProvider for UuidIdProvider {
    fn new_id(&self) -> Uuid {
        Uuid::new_v4()
    }
}
