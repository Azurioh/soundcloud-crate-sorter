//! Application `Settings` — the single-row app state (T024).

use crate::confidence::ConfidenceThreshold;

/// Default confidence cutoff. Tuned to land most tracks in auto-classification while sending the
/// genuinely ambiguous tail to triage (data-model targets a ~85/15 auto/manual split, SC-001).
/// Adjustable at runtime via the settings UI.
const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.6;

/// Where classified crates are exported. v1 supports `Local` only; the SoundCloud variants are
/// reserved for v2 (research.md R3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMode {
    /// Export to per-crate local folders (Rekordbox/USB).
    Local,
    /// Publish to SoundCloud playlists (v2).
    SoundCloud,
    /// Both local and SoundCloud (v2).
    Both,
}

impl ExportMode {
    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::SoundCloud => "soundcloud",
            Self::Both => "both",
        }
    }

    /// Parses a persistence token back into an export mode, defaulting to `Local` for unknown input.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token {
            "soundcloud" => Self::SoundCloud,
            "both" => Self::Both,
            _ => Self::Local,
        }
    }
}

/// The app's mutable settings (persisted as a single row).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    confidence_threshold: ConfidenceThreshold,
    download_enabled: bool,
    export_mode: ExportMode,
}

impl Settings {
    /// Builds settings from explicit values.
    #[must_use]
    pub const fn new(
        confidence_threshold: ConfidenceThreshold,
        download_enabled: bool,
        export_mode: ExportMode,
    ) -> Self {
        Self {
            confidence_threshold,
            download_enabled,
            export_mode,
        }
    }

    /// The confidence cutoff for auto-classification vs triage.
    #[must_use]
    pub const fn confidence_threshold(&self) -> ConfidenceThreshold {
        self.confidence_threshold
    }

    /// Whether opt-in audio download is enabled (default **false**, Principle III/V).
    #[must_use]
    pub const fn download_enabled(&self) -> bool {
        self.download_enabled
    }

    /// The export target mode.
    #[must_use]
    pub const fn export_mode(&self) -> ExportMode {
        self.export_mode
    }

    /// Returns a copy with a new confidence threshold.
    #[must_use]
    pub const fn with_confidence_threshold(&self, threshold: ConfidenceThreshold) -> Self {
        Self {
            confidence_threshold: threshold,
            ..*self
        }
    }

    /// Returns a copy with download enablement toggled to `enabled`.
    #[must_use]
    pub const fn with_download_enabled(&self, enabled: bool) -> Self {
        Self {
            download_enabled: enabled,
            ..*self
        }
    }
}

impl Default for Settings {
    /// Safe defaults: the tuned threshold, download **off**, local export.
    fn default() -> Self {
        let threshold = ConfidenceThreshold::new(DEFAULT_CONFIDENCE_THRESHOLD)
            .expect("DEFAULT_CONFIDENCE_THRESHOLD is a valid in-range constant");
        Self::new(threshold, false, ExportMode::Local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_download_off_and_local_export() {
        let s = Settings::default();
        assert!(!s.download_enabled());
        assert_eq!(s.export_mode(), ExportMode::Local);
        assert_eq!(
            s.confidence_threshold().value(),
            DEFAULT_CONFIDENCE_THRESHOLD
        );
    }
}
