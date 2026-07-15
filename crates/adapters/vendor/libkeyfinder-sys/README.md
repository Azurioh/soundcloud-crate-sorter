# libkeyfinder-sys (vendored)

Third-party crate, vendored into this repository. **Not project code** — do not refactor it to match
the project's conventions; keeping it close to upstream is what makes a future update a cheap diff.

| | |
|---|---|
| Upstream | <https://github.com/evanpurkhiser/libkeyfinder-sys> (crates.io `libkeyfinder-sys` 0.1.0) |
| Author | Evan Purkhiser |
| License | GPL-3.0-or-later (see [LICENSE](./LICENSE)) |
| Vendored at | 0.1.0 |

## Why it is here

`libkeyfinder-sys` is the only Rust binding to [libKeyFinder](https://github.com/mixxxdj/libkeyfinder),
the key detector Mixxx ships. The constitution forbids AI-guessed keys (Principle I), so the whole
audio path depends on this one binding — and upstream is a single-author 0.1.0 crate with ~200
downloads and no second release (research.md R4 flagged exactly this). A yank or a deleted repo would
take the audio path down with it, so the source lives here.

## Fork changes vs upstream

`src/lib.rs`, `src/bridge.cpp` and `src/bridge.h` are **byte-for-byte upstream**. Only packaging and
the build's failure message changed — the exhaustive list lives in [Cargo.toml](./Cargo.toml)'s header:

1. `edition` 2024 → 2021 (keeps the workspace MSRV of 1.80 honest).
2. `crate-type` `["cdylib", "rlib"]` → `["rlib"]` (the cdylib is a dead artifact here).
3. `publish = false`.
4. `build.rs`: a missing libkeyfinder fails with an install hint instead of a bare pkg-config dump.

## Building on macOS (Apple Silicon)

The crate links the **system** libKeyFinder; it does not build it. Homebrew provides the library, its
`fftw` dependency, and the `.pc` file that `build.rs` probes:

```sh
brew install libkeyfinder   # pulls fftw automatically
pkg-config --exists libkeyfinder && echo ok
```

`scripts/check-system-libs.sh` verifies this and the rest of the audio prerequisites at once.

Homebrew installs the `.pc` under its own prefix, which is on `pkg-config`'s default search path on a
standard install. If the probe fails anyway, put it there explicitly:

```sh
export PKG_CONFIG_PATH="$(brew --prefix libkeyfinder)/lib/pkgconfig:$PKG_CONFIG_PATH"
```

## Licensing consequence (accepted, documented)

libKeyFinder is **GPL-3.0**, so linking it makes the whole binary GPL-3.0. This is accepted for a
personal, non-distributed tool (constitution Principle II; plan.md records it as a constraint, not a
violation). `AudioAnalyzerPort` is the seam that isolates it if distribution ever becomes a goal.

## Updating

Upstream releases rarely. To take a new version: replace `src/` from the new release, re-apply the
four packaging changes above, and re-run `cargo test -p adapters --features real-adapters`, which
exercises the binding against the known-key fixtures.
