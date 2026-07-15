//! Stub `AudioAnalyzerPort` — returns preloaded features with no decoding and no native libraries.

use std::path::Path;
use std::sync::Mutex;

use domain::audio::{AudioFeatures, TempoAmbiguity};
use domain::camelot_key::{CamelotKey, CamelotLetter};
use domain::confidence::Energy;

use crate::ports::audio_analyzer::{AnalyzeError, AudioAnalyzerPort};

/// What the stub does when asked to analyze.
enum Behavior {
    /// Return fixed features.
    Return(AudioFeatures),
    /// Fail the way an undecodable file does.
    Decode,
    /// Report the file as holding no analyzable audio.
    Silence,
}

/// An `AudioAnalyzerPort` returning fixed features. Deterministic by construction, which is the
/// point: a use-case test asserting on BPM/key/energy must never depend on a real decoder.
pub struct StubAudioAnalyzer {
    behavior: Behavior,
    calls: Mutex<usize>,
}

impl StubAudioAnalyzer {
    /// Builds a stub returning `features` for every file.
    #[must_use]
    pub fn returning(features: AudioFeatures) -> Self {
        Self::with(Behavior::Return(features))
    }

    /// Builds a stub returning plausible confident features (128 BPM, 8B, energy 80).
    #[must_use]
    pub fn typical() -> Self {
        Self::returning(AudioFeatures {
            bpm: 128,
            tempo_ambiguity: TempoAmbiguity::Confident,
            key: Some(CamelotKey::new(8, CamelotLetter::B).expect("8B is on the wheel")),
            energy: Energy::new(80).expect("80 is in range"),
        })
    }

    /// Builds a stub whose tempo is ambiguous with its half/double (FR-031).
    #[must_use]
    pub fn uncertain_tempo() -> Self {
        Self::returning(AudioFeatures {
            bpm: 70,
            tempo_ambiguity: TempoAmbiguity::HalfOrDoubleTime,
            key: Some(CamelotKey::new(8, CamelotLetter::B).expect("8B is on the wheel")),
            energy: Energy::new(80).expect("80 is in range"),
        })
    }

    /// Builds a stub that fails to decode every file.
    #[must_use]
    pub fn failing() -> Self {
        Self::with(Behavior::Decode)
    }

    /// Builds a stub that reports every file as holding no analyzable audio.
    #[must_use]
    pub fn silent() -> Self {
        Self::with(Behavior::Silence)
    }

    /// Number of times `analyze` was invoked.
    #[must_use]
    pub fn call_count(&self) -> usize {
        *self.calls.lock().expect("analyzer mutex poisoned")
    }

    /// Builds a stub with the given behavior.
    fn with(behavior: Behavior) -> Self {
        Self {
            behavior,
            calls: Mutex::new(0),
        }
    }
}

impl AudioAnalyzerPort for StubAudioAnalyzer {
    fn analyze(&self, _audio_path: &Path) -> Result<AudioFeatures, AnalyzeError> {
        *self.calls.lock().expect("analyzer mutex poisoned") += 1;
        match &self.behavior {
            Behavior::Return(features) => Ok(*features),
            Behavior::Decode => Err(AnalyzeError::Decode {
                source: "stubbed decode failure".into(),
            }),
            Behavior::Silence => Err(AnalyzeError::Silence),
        }
    }
}
