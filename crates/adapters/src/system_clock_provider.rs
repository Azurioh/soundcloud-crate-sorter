//! `SystemClockProvider` — the one place `SystemTime::now` is called (code-reuse canonical path).

use std::time::{SystemTime, UNIX_EPOCH};

use application::ports::clock::ClockPort;
use domain::timestamp::Timestamp;

/// A `ClockPort` backed by the operating-system clock.
#[derive(Debug, Default, Clone)]
pub struct SystemClockProvider;

impl SystemClockProvider {
    /// Builds the provider.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ClockPort for SystemClockProvider {
    fn now(&self) -> Timestamp {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Timestamp::from_millis(millis)
    }
}
