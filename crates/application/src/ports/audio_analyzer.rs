//! `AudioAnalyzerPort` — deterministic BPM/key/energy (User Story 4; adapter added in US4).

use std::path::Path;

use thiserror::Error;

use domain::camelot_key::CamelotKey;
use domain::confidence::Energy;

/// Deterministic audio features. `key` is `None` when no key is detectable (SILENCE), never fabricated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFeatures {
    /// Beats per minute (aubio).
    pub bpm: u16,
    /// Camelot key (libKeyFinder), or `None` if undetectable.
    pub key: Option<CamelotKey>,
    /// Normalized energy (RMS/loudness).
    pub energy: Energy,
}

/// Failure analyzing audio.
#[derive(Debug, Error)]
pub enum AnalyzeError {
    /// The file could not be decoded to PCM.
    #[error("audio decode error")]
    Decode,
    /// The analysis stage failed.
    #[error("audio analysis error")]
    Analysis,
    /// No detectable key (key omitted, not fabricated).
    #[error("no detectable key (silence)")]
    Silence,
}

/// Computes deterministic BPM/key/energy for a decoded audio file. Same file → same result.
/// Never AI (constitution Principle I).
pub trait AudioAnalyzerPort: Send + Sync {
    /// Analyzes the audio at `audio_path`.
    ///
    /// # Errors
    /// [`AnalyzeError::Decode`] / [`AnalyzeError::Analysis`] / [`AnalyzeError::Silence`].
    fn analyze(&self, audio_path: &Path) -> Result<AudioFeatures, AnalyzeError>;
}
