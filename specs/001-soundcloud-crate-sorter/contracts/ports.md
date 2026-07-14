# Port Contracts: SoundCloud Crate Sorter

**Phase 1 output.** The application's interface contracts are its **ports** — Rust traits declared in
`crates/application/src/ports` (Principle VI). Each trait is the seam an adapter implements and a
use case depends on. Signatures below are indicative Rust (async where I/O-bound); the contract is
the behavior, error semantics, and boundary purity — not the exact syntax.

**Universal rules (all ports)**
- Inputs/outputs are **domain types only**. No vendor/SDK/DB type crosses the trait (map at the edge).
- Errors are the port's own typed error enum; adapters wrap vendor errors, carrying the original as
  `source` (never leaking secrets).
- Every adapter has an **in-memory fake** for use-case unit tests, and a **contract test** run against
  both the fake and the real adapter.
- No adapter reads the clock or generates ids directly — it receives `ClockPort` / `IdProvider`.

---

## LikesSourcePort  → `internal-api-likes.source`

```rust
async fn resolve_user(&self, profile_url: &str) -> Result<SourceUserId, LikesSourceError>;
async fn list_likes(&self, user: &SourceUserId) -> Result<Vec<LikedTrack>, LikesSourceError>;
```
- **Contract**: returns every publicly readable liked track, following pagination internally
  (`next_href`). `LikedTrack` carries only mapped metadata (source id, title, artist, genre?, duration,
  permalink, artwork?). No login. Rotating `client_id` handled inside the adapter.
- **Errors**: `ProfileNotFound`, `ProfilePrivate`, `RateLimited`, `Transport`.

## AudioDownloaderPort  → `ytdlp-audio.downloader`

```rust
async fn download(&self, track: &Track, dest_dir: &Path) -> Result<DownloadedAudio, DownloadError>;
```
- **Contract**: opt-in; downloads best audio for one track to a local file, returns the path.
  Per-track failure is a typed error the use case turns into a reported skip — never aborts the run
  (Principle III).
- **Errors**: `Unavailable` (deleted/geo-blocked), `ToolMissing` (yt-dlp absent), `Io`.

## AudioAnalyzerPort  → `libkeyfinder-aubio-audio.analyzer`

```rust
fn analyze(&self, audio_path: &Path) -> Result<AudioFeatures, AnalyzeError>;
// AudioFeatures { bpm: u16, key: CamelotKey /*None if SILENCE*/, energy: Energy }
```
- **Contract**: **deterministic** BPM (aubio), Camelot key (libKeyFinder), energy (RMS/loudness). Same
  file → same result. Never AI (Principle I). Decodes to f32 PCM internally (symphonia).
- **Errors**: `Decode`, `Analysis`, `Silence` (no detectable key → key omitted, not fabricated).

## GenreVibeClassifierPort  → `anthropic-genre-vibe.classifier`

```rust
async fn classify(&self, input: &ClassificationInput) -> Result<GenreVibeSuggestion, ClassifyError>;
// input: title, artist, source_genre?, description?, (optional) audio features
// output: genre + candidate crates with per-candidate confidence, vibe_tags
```
- **Contract**: used ONLY for ambiguous/missing genre and vibe. Returns candidates + confidence that
  feed the confidence score; MUST NOT return BPM/key/energy. Deterministic inputs → the use case, not
  the model, owns the threshold decision.
- **Errors**: `RateLimited`, `Transport`, `BadResponse` (schema mismatch → treat as low confidence).

## TagWriterPort  → `lofty-tag.writer`

```rust
fn write_tags(&self, audio_path: &Path, tags: &TrackTags) -> Result<(), TagWriteError>;
fn write_playlist(&self, crate_: &Crate, tracks: &[Track], dest: &Path) -> Result<(), TagWriteError>;
```
- **Contract**: writes genre/BPM/key/energy/vibe into the file's ID3 tags and emits one M3U8 per crate.
  Idempotent (re-writing the same tags is a no-op-equivalent). Tracks without local audio are rejected
  with `NoAudio` (caller reports skip — FR-024).
- **Errors**: `NoAudio`, `UnsupportedFormat`, `Io`.

## TrackRepository / CrateRepository  → `sqlite-*.repository`

```rust
// TrackRepository
async fn find_by_source_id(&self, source_id: &str) -> Result<Option<Track>, RepoError>;
async fn upsert(&self, track: &Track) -> Result<(), RepoError>;
async fn list_in_triage(&self) -> Result<Vec<Track>, RepoError>;
// CrateRepository
async fn find_or_create(&self, genre: &str, role: Option<EnergyRole>) -> Result<Crate, RepoError>;
async fn list(&self) -> Result<Vec<Crate>, RepoError>;
```
- **Contract**: `find_by_source_id` enables dedup + incremental idempotent scans (Principle IV).
  `upsert` preserves prior `ManuallyDecided` status (never silently overwritten). `find*` returns
  `Option`; a `get*`-style call would throw `NotFound` — naming follows that convention.
- **Errors**: `RepoError { Io, Constraint, Serialization }`.

## AuditLogPort  → `sqlite-audit.log`

```rust
async fn record(&self, event: AuditEvent) -> Result<(), AuditError>;
async fn events_for_track(&self, track_id: &TrackId) -> Result<Vec<AuditEvent>, AuditError>;
```
- **Contract**: append-only. Every classification, triage action, download, dedup skip, and export
  result is recorded with run id, stage, outcome, and secret-free structured detail.
  `events_for_track` MUST make "why is this track in this crate?" answerable (Principle VII).
- **Errors**: `AuditError { Io }`. Failing to record is logged loudly — audit is not optional.

## ClockPort / IdProvider  → `system-clock.provider` / `uuid-id.provider`

```rust
fn now(&self) -> Timestamp;      // ClockPort
fn new_id(&self) -> Uuid;        // IdProvider
```
- **Contract**: the ONLY sanctioned sources of time/randomness (no `SystemTime::now`/`Uuid::new_v4`
  elsewhere). In-memory fakes return fixed/seeded values for deterministic tests.

## PlaylistPublisherPort  → *(defined, no v1 adapter — DEFERRED to v2)*

```rust
async fn publish(&self, crate_: &Crate, target: PublishTarget) -> Result<PublishOutcome, PublishError>;
// target: NewPlaylist | ExistingPlaylist(id); outcome dedups against existing contents
```
- **Contract (v2)**: authenticated write; create-new or fill-existing; skip tracks already present
  (FR-019–021). The trait exists in v1 so v2 slots in without redesign; no adapter is wired in v1.

---

## Use-case → port dependency matrix (v1)

| Use case | Ports used |
|---|---|
| ScanLikes | LikesSourcePort, TrackRepository, AuditLogPort, Clock, Id |
| DeduplicateLibrary | TrackRepository, AuditLogPort |
| ClassifyTrack | GenreVibeClassifierPort (as needed), CrateRepository, TrackRepository, AuditLogPort, Clock |
| RouteToTriage | TrackRepository (reads Confidence vs threshold), AuditLogPort |
| ApplyManualDecision | TrackRepository, CrateRepository, AuditLogPort, Clock |
| DownloadAudio | AudioDownloaderPort, TrackRepository, AuditLogPort |
| AnalyzeAudio | AudioAnalyzerPort, TrackRepository, AuditLogPort |
| ExportToLocal | TagWriterPort, TrackRepository, CrateRepository, AuditLogPort |
| *ExportToSoundCloud (v2)* | *PlaylistPublisherPort, TrackRepository, AuditLogPort* |
