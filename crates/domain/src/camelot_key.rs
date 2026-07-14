//! Camelot-wheel key value object. The libKeyFinder `key_t` → Camelot mapping lives in the
//! analyzer adapter (research.md R4); the domain only holds the validated result.

use std::fmt;

use crate::validation::ValueError;

/// Lowest valid Camelot wheel position.
const POSITION_MIN: u8 = 1;
/// Highest valid Camelot wheel position.
const POSITION_MAX: u8 = 12;

/// The major/minor axis of the Camelot wheel: `B` = major, `A` = minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CamelotLetter {
    /// Minor keys (inner wheel).
    A,
    /// Major keys (outer wheel).
    B,
}

impl CamelotLetter {
    /// The single-character wheel label (`"A"` or `"B"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
        }
    }

    /// Parses a single-character label (`"A"`/`"B"`, case-insensitive).
    ///
    /// # Errors
    /// Returns `None` for anything other than `A`/`B`.
    #[must_use]
    pub fn from_char(letter: char) -> Option<Self> {
        match letter.to_ascii_uppercase() {
            'A' => Some(Self::A),
            'B' => Some(Self::B),
            _ => None,
        }
    }
}

/// A harmonic key as a Camelot-wheel position (`1..=12`) plus letter, e.g. `8A`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CamelotKey {
    position: u8,
    letter: CamelotLetter,
}

impl CamelotKey {
    /// Builds a Camelot key, validating the `1..=12` wheel position.
    ///
    /// # Errors
    /// Returns [`ValueError::OutOfRange`] when `position` is outside `1..=12`.
    pub fn new(position: u8, letter: CamelotLetter) -> Result<Self, ValueError> {
        if !(POSITION_MIN..=POSITION_MAX).contains(&position) {
            return Err(ValueError::OutOfRange {
                field: "camelot_position",
                min: f64::from(POSITION_MIN),
                max: f64::from(POSITION_MAX),
                value: f64::from(position),
            });
        }
        Ok(Self { position, letter })
    }

    /// Parses the canonical `"<position><letter>"` form (e.g. `"8A"`).
    ///
    /// # Errors
    /// Returns [`ValueError::OutOfRange`] for a bad position or malformed input.
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let malformed = || ValueError::OutOfRange {
            field: "camelot_key",
            min: f64::from(POSITION_MIN),
            max: f64::from(POSITION_MAX),
            value: f64::NAN,
        };
        let letter_char = text.chars().last().ok_or_else(malformed)?;
        let letter = CamelotLetter::from_char(letter_char).ok_or_else(malformed)?;
        let position: u8 = text[..text.len() - 1].parse().map_err(|_| malformed())?;
        Self::new(position, letter)
    }

    /// The wheel position (`1..=12`).
    #[must_use]
    pub const fn position(self) -> u8 {
        self.position
    }

    /// The major/minor letter.
    #[must_use]
    pub const fn letter(self) -> CamelotLetter {
        self.letter
    }
}

impl fmt::Display for CamelotKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.position, self.letter.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_as_position_then_letter() {
        let key = CamelotKey::new(8, CamelotLetter::A).unwrap();
        assert_eq!(key.to_string(), "8A");
    }

    #[test]
    fn rejects_position_outside_wheel() {
        assert!(CamelotKey::new(0, CamelotLetter::B).is_err());
        assert!(CamelotKey::new(13, CamelotLetter::B).is_err());
    }
}
