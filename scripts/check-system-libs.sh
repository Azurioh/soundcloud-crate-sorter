#!/usr/bin/env bash
# Verify the native prerequisites for the audio-analysis path (User Story 4, v1).
# The metadata-only MVP (US1/US2) does NOT need any of these — it runs on the Rust core alone.
# This script only reports; it installs nothing.
set -euo pipefail

missing=0

note_ok() { printf '  \033[32mok\033[0m   %s\n' "$1"; }
note_bad() { printf '  \033[31mMISS\033[0m %s\n' "$1"; missing=1; }

echo "Checking audio-analysis system prerequisites (macOS Apple Silicon)…"

# Homebrew is how the native libs are provisioned (research.md R4).
if command -v brew >/dev/null 2>&1; then
  note_ok "brew present"
  for formula in libkeyfinder fftw aubio yt-dlp; do
    if brew list --formula "$formula" >/dev/null 2>&1; then
      note_ok "brew: $formula"
    else
      note_bad "brew: $formula  (run: brew install $formula)"
    fi
  done
else
  note_bad "brew  (install Homebrew from https://brew.sh, then: brew install libkeyfinder aubio yt-dlp)"
fi

# yt-dlp is invoked as a subprocess by the download adapter (research.md R2).
if command -v yt-dlp >/dev/null 2>&1; then
  note_ok "yt-dlp on PATH"
else
  note_bad "yt-dlp not on PATH  (brew install yt-dlp)"
fi

# libkeyfinder-sys locates the lib through pkg-config (research.md R4).
if command -v pkg-config >/dev/null 2>&1; then
  if pkg-config --exists libkeyfinder; then
    note_ok "pkg-config finds libkeyfinder"
  else
    note_bad "pkg-config cannot find libkeyfinder  (ensure Homebrew's .pc is on PKG_CONFIG_PATH)"
  fi
else
  note_bad "pkg-config not on PATH  (brew install pkg-config)"
fi

if [ "$missing" -eq 0 ]; then
  echo "All audio-analysis prerequisites present."
else
  echo "Some prerequisites are missing (see MISS above). The metadata-only MVP still runs without them."
  exit 1
fi
