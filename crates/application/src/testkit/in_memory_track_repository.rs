//! In-memory `TrackRepository` fake — mirrors the real adapter's dedup + idempotency semantics.

use std::sync::Mutex;

use async_trait::async_trait;

use domain::crate_::CrateId;
use domain::track::{Track, TrackId, TrackStatus};

use crate::ports::repo_error::RepoError;
use crate::ports::track_repository::TrackRepository;

/// A `TrackRepository` backed by an in-memory vector. Enforces `source_track_id` uniqueness and
/// preserves a prior `ManuallyDecided` status on upsert, matching the SQLite adapter (Principle IV).
#[derive(Debug, Default)]
pub struct InMemoryTrackRepository {
    tracks: Mutex<Vec<Track>>,
}

impl InMemoryTrackRepository {
    /// Builds an empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns every track currently in `status`.
    fn list_with_status(&self, status: TrackStatus) -> Vec<Track> {
        let tracks = self.tracks.lock().expect("track repo mutex poisoned");
        tracks
            .iter()
            .filter(|track| track.status() == status)
            .cloned()
            .collect()
    }
}

#[async_trait]
impl TrackRepository for InMemoryTrackRepository {
    async fn find_by_source_id(&self, source_id: &str) -> Result<Option<Track>, RepoError> {
        let tracks = self.tracks.lock().expect("track repo mutex poisoned");
        let found = tracks
            .iter()
            .find(|t| t.source_track_id() == source_id)
            .cloned();
        Ok(found)
    }

    async fn find_by_id(&self, id: &TrackId) -> Result<Option<Track>, RepoError> {
        let tracks = self.tracks.lock().expect("track repo mutex poisoned");
        Ok(tracks.iter().find(|t| t.id() == id).cloned())
    }

    async fn upsert(&self, track: &Track) -> Result<(), RepoError> {
        let mut tracks = self.tracks.lock().expect("track repo mutex poisoned");

        // Reject a second row claiming an existing source id under a different internal id
        // (the real schema enforces this with a UNIQUE constraint).
        let source_conflict = tracks
            .iter()
            .any(|t| t.source_track_id() == track.source_track_id() && t.id() != track.id());
        if source_conflict {
            return Err(RepoError::Constraint {
                source: "duplicate source_track_id".into(),
            });
        }

        match tracks.iter().position(|t| t.id() == track.id()) {
            Some(index) => {
                let keep_manual =
                    tracks[index].is_manually_decided() && !track.is_manually_decided();
                if !keep_manual {
                    tracks[index] = track.clone();
                }
            }
            None => tracks.push(track.clone()),
        }
        Ok(())
    }

    async fn list_all(&self) -> Result<Vec<Track>, RepoError> {
        let tracks = self.tracks.lock().expect("track repo mutex poisoned");
        Ok(tracks.clone())
    }

    async fn list_in_triage(&self) -> Result<Vec<Track>, RepoError> {
        Ok(self.list_with_status(TrackStatus::InTriage))
    }

    async fn list_deferred(&self) -> Result<Vec<Track>, RepoError> {
        Ok(self.list_with_status(TrackStatus::Deferred))
    }

    async fn list_by_crate(&self, crate_id: &CrateId) -> Result<Vec<Track>, RepoError> {
        let tracks = self.tracks.lock().expect("track repo mutex poisoned");
        let members = tracks
            .iter()
            .filter(|t| t.crate_id() == Some(crate_id))
            .cloned()
            .collect();
        Ok(members)
    }
}
