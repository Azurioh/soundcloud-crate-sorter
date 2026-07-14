//! `ClockPort` — the only sanctioned source of wall-clock time (no `SystemTime::now` elsewhere).

use domain::timestamp::Timestamp;

/// Supplies the current time as a domain `Timestamp`. In-memory fakes return fixed/seeded values
/// for deterministic tests.
pub trait ClockPort: Send + Sync {
    /// Returns the current instant.
    fn now(&self) -> Timestamp;
}
