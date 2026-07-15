# Research: SoundCloud Crate Sorter

**Phase 0 output** — resolves the Technical Context unknowns. Verified via Context7 (Rust crates)
and web sources (SoundCloud, libKeyFinder), 2026-07-14. Where a source was secondary, the exact
version is flagged "confirm at build time".

## R1. Reading SoundCloud likes (no login)

> **Corrected 2026-07-15 against the live API.** As first written, this section was wrong in two ways
> and made scanning impossible — the app failed with "transport error reading SoundCloud likes" for
> every profile. Both errors are recorded below rather than quietly overwritten: R1 claimed "confirmed
> HIGH confidence" for an endpoint that returns 404, which is exactly the claim a future reader should
> distrust. Its own follow-up ("validate the fields we consume against a live response") was the step
> that would have caught this, and it was never carried out — the real-adapter contract test needs
> `SC_TEST_PROFILE` plus network, so `cargo test` stayed green while the feature could never work.

- **Decision**: Read public likes through the unofficial `api-v2.soundcloud.com` endpoint
  `GET /users/{user_id}/track_likes?client_id={id}&limit=200&linked_partitioning=1`, following the
  `next_href` cursor until absent **and re-attaching `client_id` to every cursor URL** (see below).
  Resolve a profile URL to a user id via
  `GET /resolve?url=https://soundcloud.com/{username}&client_id={id}`. Obtain the `client_id` by
  extracting it from the web player's bundled JS at runtime (it rotates).
- **Verified against the live API** (a 127-like public library):
  - `GET /users/{id}/track_likes` → **200**. This is the endpoint to use: it returns tracks only.
  - `GET /users/{id}/likes/tracks` → **404**. Originally recorded here; this path does not exist.
  - `GET /users/{id}/likes` → 200, but mixes in playlist likes.
  - `GET /users/{id}/favorites` → 404.
- **Response shape**: the endpoint returns **likes, not tracks**. Each `collection` entry is
  `{ created_at, kind: "like", track: { … } }` — the track object is *nested*, not the entry itself.
  All consumed fields (`id`, `title`, `genre`, `duration`, `permalink_url`, `artwork_url`,
  `user.username`) are present on the nested object.
- **The cursor drops the `client_id`**: `next_href` comes back as
  `…/track_likes?offset=…&limit=200` with no `client_id`. Following it verbatim is an anonymous
  request → **401**, which reads as "profile is private". The id must be re-attached to every page.
  SoundCloud emits a cursor even when the first page already returned the whole library, so this
  broke *every* scan regardless of size.
- **Rationale**: no OAuth is needed for reading — likes are public by default.
- **Alternatives considered**: Official API (`api.soundcloud.com`) — rejected, app registration is
  gated (see R3). Offset pagination — rejected, deprecated by SoundCloud in 2020 (cursor only).
- **Follow-ups**: client_id extraction must be refreshable (IDs rotate; rate ~60–80 req/min). The
  endpoint is unofficial and undocumented, so the shape above is a snapshot, not a contract: the
  `real-adapters` likes contract test is the only thing that will notice when it next changes, and it
  only runs when someone sets `SC_TEST_PROFILE`.

## R2. Audio download

- **Decision**: Download opt-in via a subprocess call to the `yt-dlp` binary:
  `yt-dlp -f bestaudio -o "<template>" --embed-metadata <track_permalink_url>`. Resolve per-track
  from the permalink already captured at scan time (do not depend on `--cookies-from-browser` for
  public tracks).
- **Rationale**: yt-dlp is actively maintained, supports SoundCloud tracks and likes URLs, HIGH
  confidence. Keeping it a subprocess keeps the native download logic out of our binary.
- **Alternatives considered**: A pure-Rust SoundCloud downloader — none mature enough. Embedding the
  download inside the read adapter — rejected, download is a separate opt-in port.
- **Follow-ups**: SoundCloud serves MP3 or HLS; `bestaudio` selects the best available. Handle
  per-track download failure (deleted/geo-blocked) as a reported skip, never a run-stopper.

## R3. SoundCloud playlist write — DEFERRED TO v2

- **Decision**: **Out of v1 scope.** Do not build the `PlaylistPublisherPort` adapter in v1; keep the
  port trait defined (architecture stability) with no v1 implementation.
- **Rationale**: The official write API requires OAuth 2.1 + an app registration that has been
  gated/closed since ~2022. A personal tool cannot realistically obtain approval. HIGH confidence.
- **Alternatives considered**: Grey-area replay of the web client's authenticated `api-v2` calls
  using the user's browser session token — technically possible (the web app itself does this) but
  higher ToS risk and fragility; retained as a v2 candidate, not v1. Dropping the port entirely —
  rejected, we keep the seam so v2 slots in without a redesign.
- **Impact**: v1's usable output is local/Rekordbox export (R5–R6), which requires downloaded audio.
  Classification still degrades gracefully to metadata-only, but the exportable deliverable in v1
  is the local path.

## R4. Audio analysis — BPM, key (Camelot), energy

- **Decision**: `AudioAnalyzerPort` implemented by an adapter that:
  - decodes audio to normalized **f32 PCM [-1.0, 1.0]** with `symphonia` (we own decoding),
  - computes **key** via **libKeyFinder** through the `libkeyfinder-sys` crate (v0.1.0, `cxx`-based
    FFI), mapping its `key_t` enum (0–23) to Camelot with a static table (24 = SILENCE → no key),
  - computes **BPM** via **aubio** (native lib + `aubio-rs` wrapper),
  - computes **energy** directly from RMS/loudness of the decoded PCM.
- **Rationale**: libKeyFinder is the library Mixxx ships — deterministic, DJ-grade key detection,
  satisfying Principle I (never AI-guess key). Determinism across BPM/key/energy is required.
- **Alternatives considered**: Python sidecar (essentia/librosa) — rejected for v1 to keep a single
  runtime (kept as the swap target behind the port if the FFI build fights us). Pure-Rust key
  estimator (bliss-rs + custom) — rejected, accuracy risk + still needs ffmpeg.
- **Native deps (macOS Apple Silicon)**: `brew install libkeyfinder` (2.2.8) auto-pulls `fftw`
  (3.3.11); `brew install aubio`. `libkeyfinder-sys` locates the lib via `pkg-config` (needs the
  `.pc` on `PKG_CONFIG_PATH` — Homebrew provides it).
- **Licensing**: libKeyFinder is **GPL-3.0**; it makes the whole binary GPL-3.0. **Accepted** — this
  is a personal, non-distributed tool (Principle II). Documented as a constraint on future
  distribution; the port isolates it if that ever changes.
- **Risks**: (1) `libkeyfinder-sys` is v0.1.0/single-author — **vendor/fork it**, pilot against a few
  known-key tracks and cross-check with Mixxx before trusting at scale. (2) System-lib build
  reproducibility (pkg-config, Homebrew paths). (3) Bad PCM preprocessing → wrong keys.
- **Camelot mapping** (from libKeyFinder `constants.h`; major = "B" letter, minor = "A"):
  A=11B, Am=8A, B♭=6B, B♭m=3A, B=1B, Bm=10A, C=8B, Cm=5A, D♭=3B, D♭m=12A, D=10B, Dm=7A, E♭=5B,
  E♭m=2A, E=12B, Em=9A, F=7B, Fm=4A, G♭=2B, G♭m=11A, G=9B, Gm=6A, A♭=4B, A♭m=1A.

## R5. Tag writing & local export

- **Decision**: `TagWriterPort` implemented with **lofty** — write ID3v2 frames on the downloaded
  files: genre (`TCON`), BPM (`TBPM`), key (`TKEY`), and energy + vibe in `COMM`/`TXXX`. Generate one
  **M3U8** playlist per crate (plain UTF-8 text). Lay out one folder per crate.
- **Rationale**: lofty is the mature Rust tag library, reads/writes ID3v2 and others via the
  `Accessor` pattern. Rekordbox reads ID3 tags and M3U8 on import.
- **Alternatives considered**: `id3` crate — narrower format support than lofty. Writing Rekordbox's
  own DB — rejected by Principle IV (non-destructive; never touch the Rekordbox database).
- **Follow-ups**: confirm lofty's exact frame accessors for `TBPM`/`TKEY`/`TXXX` against its docs at
  implementation time (Context7 entry didn't detail field names).

## R6. Persistence & audit trail

- **Decision**: **rusqlite** over a local SQLite file. One schema holds tracks, crates, classification
  decisions, and the **audit log** (Principle VII) — queryable and persisted across runs (supports
  incremental/idempotent runs, Principle IV).
- **Rationale**: For a single-user local desktop app, rusqlite is synchronous, zero connection-pool
  overhead, and simplest. sqlx's async-first model buys nothing here.
- **Alternatives considered**: sqlx — rejected (async overhead, server-oriented). Flat JSON files —
  rejected, no queryable audit trail.

## R7. Observability

- **Decision**: `tracing` for structured logging (spans/events, levels), emitting run id / track id /
  stage / outcome at every pipeline stage. Never log tokens/secrets. The durable audit trail lives in
  SQLite (R6); `tracing` is the ephemeral operational log.
- **Rationale**: `tracing` is the Rust standard for structured, leveled logs; pairs with the SQLite
  audit trail for "why is this track in this crate?" answerability.

## R8. AI genre/vibe classification

- **Decision**: `GenreVibeClassifierPort` calls the Claude API over HTTP (reqwest + serde), used ONLY
  for ambiguous/missing genre and vibe/mood, returning a label + confidence. Default model
  **`claude-haiku-4-5-20251001`** (cheap, fast, high-volume classification); escalate to Sonnet/Opus
  only if quality proves insufficient. Never used for BPM/key/energy (Principle I).
- **Rationale**: Genre/vibe from title/description/tags is a light classification task — Haiku is the
  cost-appropriate tier. Confirm model id/pricing via the `claude-api` reference at implementation.
- **Alternatives considered**: A local embedding/heuristic classifier — kept as a possible cheaper
  first pass before calling the API, but the AI port is the reliable fallback for ambiguity.

## R9. UI stack (Tauri webview)

- **Decision**: **Tauri v2** (Rust core exposing `#[tauri::command]` handlers) + **React + TypeScript +
  Vite** frontend. Swipe-card triage: `motion` (12.x) + `@use-gesture/react` (10.x); keyboard
  shortcuts via `react-hotkeys-hook` or native handlers; dense assignment table via
  `@tanstack/react-table` (8.x) + TanStack Virtual. Audio preview via `<audio>` + Tauri asset
  protocol (`convertFileSrc`).
- **Rationale**: On Tauri the webview is the OS's, so React's runtime cost is negligible; the dense
  keyboard-navigable table (TanStack) and the developer's existing React/TanStack standards are the
  tiebreakers. Swipe + keyboard share one "commit assignment" action.
- **Alternatives considered**: Svelte — genuinely lighter but its TanStack-Table adapter and
  swipe-card libs lag on Svelte 5, and it diverges from the developer's standards. `react-tinder-card`
  — rejected, unmaintained (1.6.4, ~3 yr stale).
- **Confirm at build time**: exact Tauri patch version (secondary source cited 2.9.x).

## Consolidated crate list (confirm exact versions on crates.io at build time)

| Concern | Crate / tool | Note |
|---|---|---|
| Desktop shell | `tauri` v2.9.x | Rust core + webview; strict ACL, register commands |
| HTTP | `reqwest` | async client for api-v2 + Anthropic |
| JSON | `serde`, `serde_json` | derive on boundary DTOs, map to domain at edge |
| Key detection | `libkeyfinder-sys` 0.1.0 (GPL-3.0) | `cxx` FFI; vendor/fork; needs `brew libkeyfinder`+`fftw` |
| BPM | `aubio-rs` | thin wrapper; needs `brew aubio` |
| Audio decode | `symphonia` | decode to f32 PCM for the analyzer |
| Tags | `lofty` | ID3v2 write + M3U8 alongside |
| Storage/audit | `rusqlite` | local SQLite, sync |
| Logging | `tracing` | structured, leveled |
| Ids/time | `uuid`, `std::time`/`time` | behind IdProvider / ClockPort |
| Frontend | React + TS + Vite | `motion`, `@use-gesture/react`, `@tanstack/react-table`, `react-hotkeys-hook` |
| AI | Claude API (`claude-haiku-4-5-20251001`) | HTTP; genre/vibe only |
