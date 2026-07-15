//! Deterministic audio-analysis results (User Story 4).
//!
//! These are the measured facts about a track's audio: tempo, harmonic key, loudness. The
//! constitution forbids any of them being AI-guessed (Principle I) — they come from the analyzer
//! adapter or they are absent. Nothing here is inferred, and nothing here is fabricated to fill a
//! gap: an undetectable key is `None`, not a plausible-looking default.

use crate::camelot_key::CamelotKey;
use crate::confidence::Energy;

/// How much to trust a detected tempo.
///
/// Beat trackers routinely lock onto half or double the true tempo — a 140 BPM track reported as 70
/// is the classic failure. The tempo is still the analyzer's best estimate; what changes is whether
/// we may act on it silently (FR-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempoAmbiguity {
    /// The estimate showed no half-/double-time conflict.
    Confident,
    /// The estimate is ambiguous with its half or double. The BPM MUST be treated as uncertain and
    /// the track routed to manual triage rather than silently tagged (FR-031).
    HalfOrDoubleTime,
}

impl TempoAmbiguity {
    /// Whether the tempo is too ambiguous to act on without a human (FR-031).
    #[must_use]
    pub fn is_uncertain(self) -> bool {
        matches!(self, Self::HalfOrDoubleTime)
    }

    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Confident => "confident",
            Self::HalfOrDoubleTime => "half_or_double_time",
        }
    }

    /// Parses a persistence token back into a tempo ambiguity.
    ///
    /// # Errors
    /// Returns `None` for an unrecognized token — an unreadable value must surface as an error, not
    /// decay into `Confident`, which would silently promote a doubtful BPM to a trusted one.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "confident" => Some(Self::Confident),
            "half_or_double_time" => Some(Self::HalfOrDoubleTime),
            _ => None,
        }
    }
}

/// The deterministic features of one track's audio. Same file → same values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFeatures {
    /// Detected tempo in beats per minute.
    pub bpm: u16,
    /// Whether that tempo is trustworthy enough to act on unattended (FR-031).
    pub tempo_ambiguity: TempoAmbiguity,
    /// Detected Camelot key, or `None` when no key is detectable (silence). Never fabricated.
    pub key: Option<CamelotKey>,
    /// Normalized loudness-derived energy.
    pub energy: Energy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_half_or_double_time_is_uncertain() {
        assert!(!TempoAmbiguity::Confident.is_uncertain());
        assert!(TempoAmbiguity::HalfOrDoubleTime.is_uncertain());
    }

    #[test]
    fn tokens_round_trip() {
        for ambiguity in [TempoAmbiguity::Confident, TempoAmbiguity::HalfOrDoubleTime] {
            assert_eq!(
                TempoAmbiguity::from_token(ambiguity.as_str()),
                Some(ambiguity)
            );
        }
    }

    /// An unknown token must not decay into `Confident` — that would launder a doubtful BPM into a
    /// trusted one behind the user's back (Principle I).
    #[test]
    fn unknown_token_is_rejected_not_defaulted() {
        assert_eq!(TempoAmbiguity::from_token("probably_fine"), None);
    }
}
