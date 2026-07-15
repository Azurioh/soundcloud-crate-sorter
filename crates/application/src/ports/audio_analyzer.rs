//! `AudioAnalyzerPort` — deterministic BPM/key/energy (User Story 4).

use std::path::Path;

use thiserror::Error;

use domain::audio::AudioFeatures;

use crate::ports::BoxError;

/// Failure analyzing audio. Per-track failure is a reported skip — never a run-stopper
/// (constitution Principle III).
#[derive(Debug, Error)]
pub enum AnalyzeError {
    /// The file could not be decoded to PCM (unsupported/corrupt container or codec).
    #[error("audio decode error")]
    Decode {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
    /// Decoding succeeded but the analysis stage failed.
    #[error("audio analysis error")]
    Analysis {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
    /// The file holds no analyzable audio at all (silence): there is no tempo and no energy to
    /// report, so nothing is returned rather than a fabricated zero.
    ///
    /// Note this is *not* the "no detectable key" case — a track with a beat but no clear tonal
    /// centre analyzes fine and simply carries `key: None` in its [`AudioFeatures`].
    #[error("no analyzable audio (silence)")]
    Silence,
}

/// Computes deterministic BPM/key/energy for a decoded audio file. Same file → same result.
/// Never AI (constitution Principle I).
pub trait AudioAnalyzerPort: Send + Sync {
    /// Analyzes the audio at `audio_path`.
    ///
    /// # Errors
    /// [`AnalyzeError::Decode`] if the file cannot be decoded, [`AnalyzeError::Analysis`] if
    /// analysis fails, [`AnalyzeError::Silence`] if the file holds no analyzable audio.
    fn analyze(&self, audio_path: &Path) -> Result<AudioFeatures, AnalyzeError>;
}
