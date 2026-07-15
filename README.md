# SoundCloud Crate Sorter

Turn a SoundCloud **likes** library into organized DJ crates. The tool scans a user's public likes
(no login), deduplicates them, and auto-classifies each track into one crate (genre × energy role)
with a confidence score. Tracks below an adjustable confidence threshold go to a manual triage UI.
Opting into audio download unlocks deterministic BPM / Camelot-key / energy analysis and a
local/Rekordbox export (per-crate folders + ID3-tagged files + M3U8 playlists).

Desktop app: a **Rust** core in strict Clean Architecture (traits = ports), packaged with **Tauri v2**
and a **React + TypeScript** webview. See [`specs/001-soundcloud-crate-sorter/`](specs/001-soundcloud-crate-sorter/)
for the full spec, plan, data model, and port contracts.

## Status

| Increment | User stories | State |
|---|---|---|
| **MVP** | US1 — scan → dedup → classify → crates (metadata only) | done |
| Increment 2 | US2 — manual triage | done |
| Increment 3a | US4 — opt-in audio analysis (BPM / Camelot key / energy) | done |
| Increment 3b | US5 — local/Rekordbox export | **next** |
| v2 | US3 — SoundCloud playlist write (gated API) | deferred |

## Architecture

A Cargo workspace, one crate per Clean Architecture layer — the Dependency Rule is a compile-time
guarantee:

```
crates/domain       — Layer 1: entities (vendor-free, depends on nothing)
crates/application  — Layer 2: use cases + port traits (depends on domain only)
crates/adapters     — Layer 3: interface adapters (map vendor/DB types at the edge)
crates/app          — Layer 4: composition root + Tauri (the only crate that wires concretes)
ui/                 — React + TypeScript (Tauri webview)
```

## Prerequisites

- **Rust** (stable, 1.80+) + Cargo.
- **Node.js** + **pnpm** for the `ui/` React app.
- **Metadata-only MVP (US1/US2) needs nothing else** — no system libraries.
- **Audio analysis / export (US4/US5)** needs these native libs (macOS Apple Silicon):
  ```sh
  brew install libkeyfinder   # pulls fftw
  brew install aubio
  brew install yt-dlp
  ```
  Verify with:
  ```sh
  ./scripts/check-system-libs.sh
  ```
  These are linked, not built: `libkeyfinder-sys` finds libKeyFinder through `pkg-config`, and
  `aubio-sys` generates its bindings against Homebrew's headers. Homebrew nests `aubio.h` one level
  deeper than aubio-sys expects, so [`.cargo/config.toml`](.cargo/config.toml) points bindgen at the
  right directory — a non-default Homebrew prefix only needs `BINDGEN_EXTRA_CLANG_ARGS` set in the
  shell, no edit.

  `libkeyfinder-sys` is **vendored** (a single-author 0.1.0 crate the whole key path depends on) —
  see [`crates/adapters/vendor/libkeyfinder-sys/README.md`](crates/adapters/vendor/libkeyfinder-sys/README.md).

  > libKeyFinder is **GPL-3.0**, which makes the linked binary GPL-3.0. This is a personal,
  > non-distributed build (constitution Principle II).

## Configuration

The AI genre/vibe classifier calls the Claude API. Provide a key via the environment — it is never
logged and never committed:

```sh
cp .env.example .env
# edit .env and set ANTHROPIC_API_KEY=...
```

Two optional overrides, both defaulting inside the OS application-data directory: `SCS_DB_PATH`
(the SQLite library) and `SCS_DOWNLOAD_DIR` (opt-in downloaded audio).

**Audio download is off by default and stays off until you turn it on in the app**, where the
personal-use / terms-of-service note is shown (constitution Principle V). Nothing is ever downloaded
without that opt-in, and everything except BPM/key/energy works without it.

## Build & run

```sh
cargo build                 # domain, application, adapters, app
cd ui && pnpm install       # frontend deps
cd .. && cargo tauri dev    # run the desktop app
```

## Test

```sh
cargo test                              # use-case unit tests (in-memory fakes) + port contract tests
cargo test --features real-adapters     # contract tests against real adapters (network / system libs)

# The audio analyzer against the real native libraries — no network or API key, just brew's
# libkeyfinder + aubio. Fixtures are generated, so the ground truth is arithmetic:
cargo test -p adapters --features real-adapters --test audio_analyzer_contract
```

Every use case has isolation tests against in-memory port fakes; every port has a contract test that
runs against **both** the in-memory fake and the real adapter (constitution Principle VI).

The `real-adapters` suites that need a network or a key read them from the environment:
`SC_TEST_PROFILE` (a public profile URL), `SC_TEST_TRACK` (a public track permalink), and
`ANTHROPIC_API_KEY`.
