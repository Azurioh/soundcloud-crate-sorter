//! Confidence, threshold, and energy value objects — all range-guarded newtypes.

use crate::validation::ValueError;

/// Inclusive lower bound shared by `Confidence` and `ConfidenceThreshold`.
const PROBABILITY_MIN: f32 = 0.0;
/// Inclusive upper bound shared by `Confidence` and `ConfidenceThreshold`.
const PROBABILITY_MAX: f32 = 1.0;
/// Lowest energy on the normalized scale.
const ENERGY_MIN: u8 = 0;
/// Highest energy on the normalized scale.
const ENERGY_MAX: u8 = 100;

/// A classification confidence in `[0.0, 1.0]`.
///
/// Produced by the classification use case from source-tag certainty and/or the AI
/// classifier's candidate score. Never fabricated for deterministic audio features.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Confidence(f32);

impl Confidence {
    /// Builds a confidence, validating the `[0.0, 1.0]` range.
    ///
    /// # Errors
    /// Returns [`ValueError::OutOfRange`] when `value` is outside `[0.0, 1.0]`.
    pub fn new(value: f32) -> Result<Self, ValueError> {
        ensure_probability("confidence", value)?;
        Ok(Self(value))
    }

    /// Returns the underlying `f32`.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.0
    }

    /// Whether this confidence meets or exceeds `threshold` (auto-classification gate).
    #[must_use]
    pub fn meets(self, threshold: ConfidenceThreshold) -> bool {
        self.0 >= threshold.value()
    }
}

/// The adjustable cutoff separating auto-classification from manual triage.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ConfidenceThreshold(f32);

impl ConfidenceThreshold {
    /// Builds a threshold, validating the `[0.0, 1.0]` range.
    ///
    /// # Errors
    /// Returns [`ValueError::OutOfRange`] when `value` is outside `[0.0, 1.0]`.
    pub fn new(value: f32) -> Result<Self, ValueError> {
        ensure_probability("confidence_threshold", value)?;
        Ok(Self(value))
    }

    /// Returns the underlying `f32`.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.0
    }
}

/// Normalized track energy in `[0, 100]` (loudness/RMS-derived; audio-analysis only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Energy(u8);

impl Energy {
    /// Builds an energy value, validating the `[0, 100]` range.
    ///
    /// # Errors
    /// Returns [`ValueError::OutOfRange`] when `value` exceeds 100.
    pub fn new(value: u8) -> Result<Self, ValueError> {
        if value > ENERGY_MAX {
            return Err(ValueError::OutOfRange {
                field: "energy",
                min: f64::from(ENERGY_MIN),
                max: f64::from(ENERGY_MAX),
                value: f64::from(value),
            });
        }
        Ok(Self(value))
    }

    /// Returns the underlying `u8`.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// Validates that `value` lies within the shared probability range.
fn ensure_probability(field: &'static str, value: f32) -> Result<(), ValueError> {
    if !(PROBABILITY_MIN..=PROBABILITY_MAX).contains(&value) || value.is_nan() {
        return Err(ValueError::OutOfRange {
            field,
            min: f64::from(PROBABILITY_MIN),
            max: f64::from(PROBABILITY_MAX),
            value: f64::from(value),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_rejects_out_of_range() {
        assert!(Confidence::new(-0.1).is_err());
        assert!(Confidence::new(1.1).is_err());
        assert!(Confidence::new(f32::NAN).is_err());
    }

    #[test]
    fn confidence_meets_threshold_at_or_above() {
        let threshold = ConfidenceThreshold::new(0.6).unwrap();
        assert!(Confidence::new(0.6).unwrap().meets(threshold));
        assert!(Confidence::new(0.9).unwrap().meets(threshold));
        assert!(!Confidence::new(0.59).unwrap().meets(threshold));
    }

    #[test]
    fn energy_rejects_above_hundred() {
        assert!(Energy::new(101).is_err());
        assert_eq!(Energy::new(100).unwrap().value(), 100);
    }
}
