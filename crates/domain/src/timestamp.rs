//! Wall-clock time as a vendor-free value object.

/// An instant expressed as Unix-epoch **milliseconds**.
///
/// The domain never reads the system clock; every `Timestamp` originates from the
/// application's `ClockPort` (constitution: one canonical time source). Stored as `i64`
/// so it maps directly onto a SQLite integer column at the adapter edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Builds a timestamp from Unix-epoch milliseconds.
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Returns the underlying Unix-epoch milliseconds.
    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }
}
