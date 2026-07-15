//! `TagWriterPort` — ID3 tag + M3U8 writing for local export (User Story 5; adapter added in US5).

use std::path::Path;

use thiserror::Error;

use domain::camelot_key::CamelotKey;
use domain::confidence::Energy;
use domain::crate_::Crate;
use domain::track::Track;

use crate::ports::BoxError;

/// The tag values written into a file's ID3 frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackTags {
    /// Genre (`TCON`).
    pub genre: String,
    /// BPM (`TBPM`), if analyzed.
    pub bpm: Option<u16>,
    /// Camelot key (`TKEY`), if analyzed.
    pub key: Option<CamelotKey>,
    /// Energy (`TXXX`/`COMM`), if analyzed.
    pub energy: Option<Energy>,
    /// Vibe tags (`COMM`/`TXXX`).
    pub vibe_tags: Vec<String>,
}

/// Failure writing tags or a playlist.
#[derive(Debug, Error)]
pub enum TagWriteError {
    /// The track has no local audio to tag (caller reports the skip, FR-024).
    #[error("track has no local audio")]
    NoAudio,
    /// The audio format is not supported for tagging.
    #[error("unsupported audio format")]
    UnsupportedFormat,
    /// A local I/O failure.
    #[error("tag write I/O error")]
    Io {
        /// The wrapped lower-level error.
        #[source]
        source: BoxError,
    },
}

/// Writes ID3 tags onto downloaded files and emits one M3U8 per crate. Idempotent.
pub trait TagWriterPort: Send + Sync {
    /// Writes `tags` into the file at `audio_path`.
    ///
    /// # Errors
    /// [`TagWriteError::NoAudio`] / [`TagWriteError::UnsupportedFormat`] / [`TagWriteError::Io`].
    fn write_tags(&self, audio_path: &Path, tags: &TrackTags) -> Result<(), TagWriteError>;

    /// Writes an M3U8 playlist for `crate_` listing `tracks` into `dest`.
    ///
    /// # Errors
    /// [`TagWriteError::Io`] on failure to write the playlist.
    fn write_playlist(
        &self,
        crate_: &Crate,
        tracks: &[Track],
        dest: &Path,
    ) -> Result<(), TagWriteError>;
}
