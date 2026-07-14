//! Shared validation error for newtype value objects.

use thiserror::Error;

/// Error raised when a value object is constructed from an out-of-range primitive.
///
/// Bounds are carried so callers can build a rich, client-actionable message without
/// re-typing the limits (limits come from the value object's own named constants).
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ValueError {
    /// The provided value fell outside the inclusive `[min, max]` range for `field`.
    #[error("{field} must be within [{min}, {max}], got {value}")]
    OutOfRange {
        /// Name of the value object / field being validated (e.g. "confidence").
        field: &'static str,
        /// Inclusive lower bound.
        min: f64,
        /// Inclusive upper bound.
        max: f64,
        /// The offending value.
        value: f64,
    },
}
