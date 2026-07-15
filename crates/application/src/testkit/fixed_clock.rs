//! Deterministic `ClockPort` fake.

use domain::timestamp::Timestamp;

use crate::ports::clock::ClockPort;

/// A clock that always returns the same seeded instant — keeps timestamps deterministic in tests.
#[derive(Debug, Clone)]
pub struct FixedClock {
    millis: i64,
}

impl FixedClock {
    /// Builds a clock pinned to `millis` since the Unix epoch.
    #[must_use]
    pub const fn at_millis(millis: i64) -> Self {
        Self { millis }
    }
}

impl ClockPort for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(self.millis)
    }
}
