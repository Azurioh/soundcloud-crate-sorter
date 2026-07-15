//! Synthetic WAV fixtures for the `AudioAnalyzerPort` contract (T049).
//!
//! The analyzer's contract is about *measured* facts, so testing it needs audio whose ground truth
//! is known. These fixtures are generated from first principles rather than committed as binary
//! files: a C-major triad is C major because of the frequencies summed below, and a click every 0.5s
//! is 120 BPM by arithmetic. That is a ground truth the test states rather than asserts on faith —
//! and it keeps the repository free of opaque audio blobs no reviewer can check.
//!
//! Real-world accuracy is a different question, and not one a unit test can answer: research.md R4
//! calls for piloting against known-key tracks cross-checked with Mixxx before trusting the analyzer
//! at scale. That is a human step, recorded in the vendored crate's README.
#![allow(dead_code)]

use std::f32::consts::PI;
use std::io::Write;
use std::path::{Path, PathBuf};

/// CD-quality sample rate — what libKeyFinder's defaults are tuned for.
const SAMPLE_RATE: u32 = 44_100;
/// Bits per sample in the generated files.
const BITS_PER_SAMPLE: u16 = 16;
/// Mono: the analyzer folds to mono anyway, and it halves the fixture size.
const CHANNELS: u16 = 1;
/// Full-scale value of a 16-bit sample, used to convert from normalized f32.
const I16_FULL_SCALE: f32 = 32_767.0;
/// Byte length of the canonical 44-byte PCM WAV header, minus the leading `RIFF` + size fields.
const RIFF_HEADER_TAIL: u32 = 36;
/// Size of the `fmt ` chunk body for uncompressed PCM.
const FMT_CHUNK_SIZE: u32 = 16;
/// WAV format tag for uncompressed integer PCM.
const FORMAT_PCM: u16 = 1;

/// Frequencies of a C-major triad in the 4th octave (C4, E4, G4). Chosen because the resulting key
/// is unambiguous: these are the three notes that define C major.
const C_MAJOR_TRIAD_HZ: [f32; 3] = [261.63, 329.63, 392.00];

/// Frequency of the click used in the tempo fixture.
const CLICK_HZ: f32 = 1_000.0;
/// Length of each click burst, in samples — short enough to read as a transient.
const CLICK_SAMPLES: usize = 400;

/// Writes `samples` to `path` as a mono 16-bit PCM WAV file.
///
/// # Panics
/// If the file cannot be written — a fixture that will not write is a broken test, not a test failure.
pub fn write_wav(path: &Path, samples: &[f32]) {
    let data_size = (samples.len() * 2) as u32;
    let mut file = std::fs::File::create(path).expect("create wav fixture");

    file.write_all(b"RIFF").expect("write");
    file.write_all(&(RIFF_HEADER_TAIL + data_size).to_le_bytes())
        .expect("write");
    file.write_all(b"WAVE").expect("write");

    file.write_all(b"fmt ").expect("write");
    file.write_all(&FMT_CHUNK_SIZE.to_le_bytes())
        .expect("write");
    file.write_all(&FORMAT_PCM.to_le_bytes()).expect("write");
    file.write_all(&CHANNELS.to_le_bytes()).expect("write");
    file.write_all(&SAMPLE_RATE.to_le_bytes()).expect("write");
    let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE) / 8;
    file.write_all(&byte_rate.to_le_bytes()).expect("write");
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;
    file.write_all(&block_align.to_le_bytes()).expect("write");
    file.write_all(&BITS_PER_SAMPLE.to_le_bytes())
        .expect("write");

    file.write_all(b"data").expect("write");
    file.write_all(&data_size.to_le_bytes()).expect("write");
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let encoded = (clamped * I16_FULL_SCALE) as i16;
        file.write_all(&encoded.to_le_bytes()).expect("write");
    }
    file.flush().expect("flush wav fixture");
}

/// Generates a sustained C-major triad — audio whose key is known by construction.
#[must_use]
pub fn c_major_triad(seconds: f32, amplitude: f32) -> Vec<f32> {
    let count = (SAMPLE_RATE as f32 * seconds) as usize;
    (0..count)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE as f32;
            let sum: f32 = C_MAJOR_TRIAD_HZ
                .iter()
                .map(|frequency| (2.0 * PI * frequency * t).sin())
                .sum();
            sum / C_MAJOR_TRIAD_HZ.len() as f32 * amplitude
        })
        .collect()
}

/// Generates a click track at `bpm` — audio whose tempo is known by construction.
#[must_use]
pub fn click_track(seconds: f32, bpm: f32) -> Vec<f32> {
    let count = (SAMPLE_RATE as f32 * seconds) as usize;
    let period = (SAMPLE_RATE as f32 * 60.0 / bpm) as usize;
    (0..count)
        .map(|index| {
            let phase = index % period;
            if phase >= CLICK_SAMPLES {
                return 0.0;
            }
            // Decay the burst so it reads as a percussive transient rather than a tone.
            let envelope = 1.0 - (phase as f32 / CLICK_SAMPLES as f32);
            let t = index as f32 / SAMPLE_RATE as f32;
            (2.0 * PI * CLICK_HZ * t).sin() * envelope
        })
        .collect()
}

/// Generates digital silence.
#[must_use]
pub fn silence(seconds: f32) -> Vec<f32> {
    vec![0.0; (SAMPLE_RATE as f32 * seconds) as usize]
}

/// Writes `samples` into `dir` under `name` and returns the path.
pub fn fixture_file(dir: &Path, name: &str, samples: &[f32]) -> PathBuf {
    let path = dir.join(name);
    write_wav(&path, samples);
    path
}
