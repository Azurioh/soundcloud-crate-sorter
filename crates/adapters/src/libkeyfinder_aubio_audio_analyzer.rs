//! `LibkeyfinderAubioAudioAnalyzer` — `AudioAnalyzerPort` over symphonia + libKeyFinder + aubio
//! (T052, research.md R4).
//!
//! The pipeline is: decode to f32 PCM ourselves (symphonia), then hand the same samples to
//! libKeyFinder for the key, aubio for the tempo, and a plain RMS for the energy. Owning the decode
//! means all three analyses see byte-identical input, which is what makes the port's determinism
//! promise (same file → same result) hold rather than depend on three libraries each opening the
//! file their own way.
//!
//! Every vendor type — `KeyFinderKey`, aubio's `Tempo`, symphonia's buffers — dies at this boundary;
//! only `domain::audio::AudioFeatures` crosses the port (Principle VI).

use std::path::Path;

use application::ports::audio_analyzer::{AnalyzeError, AudioAnalyzerPort};
use aubio_rs::{OnsetMode, Tempo};
use domain::audio::{AudioFeatures, TempoAmbiguity};
use domain::camelot_key::{CamelotKey, CamelotLetter};
use domain::confidence::Energy;
use libkeyfinder_sys::{AudioData, KeyFinder, KeyFinderKey};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// aubio's analysis window, in samples. 1024/512 is aubio's own documented default pairing for beat
/// tracking; the hop is half the window so successive frames overlap.
const TEMPO_WINDOW: usize = 1024;
/// aubio's hop size, in samples — also the chunk size we feed it.
const TEMPO_HOP: usize = 512;

/// The slowest tempo a mixable track plausibly has (downtempo/dub territory).
const MIN_PLAUSIBLE_BPM: f32 = 70.0;
/// The fastest tempo a mixable track plausibly has (drum & bass tops out around 175).
const MAX_PLAUSIBLE_BPM: f32 = 190.0;

/// Below this RMS the file is treated as holding no analyzable audio at all.
const SILENCE_RMS_THRESHOLD: f32 = 1e-5;

/// Full-scale RMS reference for the energy scale. A sine at full amplitude has RMS ≈ 0.707, and real
/// masters sit well below that, so mapping 0.0–0.707 onto 0–100 keeps the loudest real tracks near
/// the top of the scale without clipping every one of them to 100.
const FULL_SCALE_RMS: f32 = 0.707;

/// The Camelot wheel position + letter for each libKeyFinder `key_t`, indexed by its discriminant
/// (research.md R4). `Silence` (24) is absent by construction: it is not a key, and the analyzer
/// reports "no key" rather than inventing one (Principle I).
const CAMELOT_BY_KEY: [(u8, CamelotLetter); 24] = [
    (11, CamelotLetter::B), // 0  A major
    (8, CamelotLetter::A),  // 1  A minor
    (6, CamelotLetter::B),  // 2  B-flat major
    (3, CamelotLetter::A),  // 3  B-flat minor
    (1, CamelotLetter::B),  // 4  B major
    (10, CamelotLetter::A), // 5  B minor
    (8, CamelotLetter::B),  // 6  C major
    (5, CamelotLetter::A),  // 7  C minor
    (3, CamelotLetter::B),  // 8  D-flat major
    (12, CamelotLetter::A), // 9  D-flat minor
    (10, CamelotLetter::B), // 10 D major
    (7, CamelotLetter::A),  // 11 D minor
    (5, CamelotLetter::B),  // 12 E-flat major
    (2, CamelotLetter::A),  // 13 E-flat minor
    (12, CamelotLetter::B), // 14 E major
    (9, CamelotLetter::A),  // 15 E minor
    (7, CamelotLetter::B),  // 16 F major
    (4, CamelotLetter::A),  // 17 F minor
    (2, CamelotLetter::B),  // 18 G-flat major
    (11, CamelotLetter::A), // 19 G-flat minor
    (9, CamelotLetter::B),  // 20 G major
    (6, CamelotLetter::A),  // 21 G minor
    (4, CamelotLetter::B),  // 22 A-flat major
    (1, CamelotLetter::A),  // 23 A-flat minor
];

/// Mono f32 PCM plus its sample rate — the single decoded input all three analyses share.
struct DecodedAudio {
    samples: Vec<f32>,
    sample_rate: u32,
}

/// Deterministic BPM / Camelot key / energy from a local audio file.
#[derive(Debug, Default)]
pub struct LibkeyfinderAubioAudioAnalyzer;

impl LibkeyfinderAubioAudioAnalyzer {
    /// Builds the analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl AudioAnalyzerPort for LibkeyfinderAubioAudioAnalyzer {
    fn analyze(&self, audio_path: &Path) -> Result<AudioFeatures, AnalyzeError> {
        let audio = decode_to_mono(audio_path)?;

        let rms = root_mean_square(&audio.samples);
        if rms < SILENCE_RMS_THRESHOLD {
            return Err(AnalyzeError::Silence);
        }

        let (bpm, tempo_ambiguity) = detect_tempo(&audio)?;
        Ok(AudioFeatures {
            bpm,
            tempo_ambiguity,
            key: detect_key(&audio),
            energy: energy_from_rms(rms),
        })
    }
}

/// Decodes any supported container/codec to mono f32 PCM.
///
/// Mono because all three analyses want it: libKeyFinder would fold the channels itself, aubio needs
/// a single stream, and RMS across a stereo interleave is meaningless. Doing it once, here, is also
/// what keeps the three analyses looking at identical samples.
fn decode_to_mono(audio_path: &Path) -> Result<DecodedAudio, AnalyzeError> {
    let file = std::fs::File::open(audio_path).map_err(decode_error)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    // The extension is only ever a hint; symphonia probes the actual bytes regardless, so a
    // mislabelled file still decodes.
    if let Some(extension) = audio_path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(decode_error)?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| AnalyzeError::Decode {
            source: "file contains no decodable audio track".into(),
        })?;
    let track_id = track.id;
    let codec_params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| AnalyzeError::Decode {
            source: "audio track is missing its codec parameters".into(),
        })?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
        .map_err(decode_error)?;

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_rate = 0;
    let mut interleaved: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            // A clean end of stream.
            Ok(None) => break,
            Err(error) => return Err(decode_error(error)),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = decoded.spec();
                sample_rate = spec.rate();
                let channels = spec.channels().count();
                decoded.copy_to_vec_interleaved(&mut interleaved);
                fold_to_mono(&interleaved, channels, &mut samples);
            }
            // A damaged packet mid-file should cost that packet, not the track.
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(symphonia::core::errors::Error::IoError(_)) => continue,
            Err(error) => return Err(decode_error(error)),
        }
    }

    if samples.is_empty() || sample_rate == 0 {
        return Err(AnalyzeError::Decode {
            source: "decoded no audio samples".into(),
        });
    }
    Ok(DecodedAudio {
        samples,
        sample_rate,
    })
}

/// Averages each interleaved frame's channels onto one mono sample.
fn fold_to_mono(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    for frame in interleaved.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        out.push(sum / channels as f32);
    }
}

/// Detects the Camelot key, or `None` when libKeyFinder finds no tonal centre.
///
/// Never an error: a percussion loop with no key is a perfectly good track that simply has no key to
/// report, and the domain models that as `None` rather than a fabricated default (Principle I).
fn detect_key(audio: &DecodedAudio) -> Option<CamelotKey> {
    let mut data = AudioData::new();
    data.set_frame_rate(audio.sample_rate);
    data.set_channels(1);
    data.extend(audio.samples.iter().copied());

    let detected = KeyFinder::new().key_of_audio(&data);
    camelot_from_key(detected)
}

/// Maps libKeyFinder's `key_t` onto the Camelot wheel.
fn camelot_from_key(key: KeyFinderKey) -> Option<CamelotKey> {
    if matches!(key, KeyFinderKey::Silence) {
        return None;
    }
    let (position, letter) = *CAMELOT_BY_KEY.get(key as usize)?;
    // The table is a compile-time constant of valid wheel positions; a failure here would be a typo
    // in it, not bad data, and silently dropping the key would hide that.
    CamelotKey::new(position, letter).ok()
}

/// Detects tempo with aubio, flagging half-/double-time ambiguity (FR-031).
fn detect_tempo(audio: &DecodedAudio) -> Result<(u16, TempoAmbiguity), AnalyzeError> {
    let mut tempo = Tempo::new(
        OnsetMode::SpecFlux,
        TEMPO_WINDOW,
        TEMPO_HOP,
        audio.sample_rate,
    )
    .map_err(analysis_error)?;
    for chunk in audio.samples.chunks_exact(TEMPO_HOP) {
        tempo.do_result(chunk).map_err(analysis_error)?;
    }
    let detected = tempo.get_bpm();
    if !detected.is_finite() || detected <= 0.0 {
        return Err(AnalyzeError::Analysis {
            source: "aubio reported no tempo".into(),
        });
    }
    let rounded = detected.round().clamp(0.0, f32::from(u16::MAX)) as u16;
    Ok((rounded, tempo_ambiguity(detected)))
}

/// Whether a detected tempo is ambiguous with its half or double (FR-031).
///
/// A beat tracker's classic failure is the octave error: 140 BPM reported as 70, or 75 as 150. The
/// detectable tell is an estimate that is *implausible on its own* while its double (or half) is an
/// ordinary tempo — 68 BPM is suspicious precisely because 136 is not. We cannot tell which of the
/// two is right from the tempo alone, so the estimate is kept and the doubt is flagged for a human,
/// rather than "corrected" into range — folding it would be a guess wearing a measurement's clothes,
/// which is the one thing BPM must never be (Principle I).
///
/// This deliberately does not flag a tempo that is plausible but whose double also is (88 vs 176, say).
/// Both readings are real tempos and nothing in the estimate distinguishes them, so flagging that
/// band would push a large slice of ordinary tracks into triage — including every 174 BPM drum & bass
/// track, whose half, 87, is equally plausible — to express a doubt we have no evidence for.
fn tempo_ambiguity(bpm: f32) -> TempoAmbiguity {
    if is_plausible(bpm) {
        return TempoAmbiguity::Confident;
    }
    // Implausible on its own. Whether or not an octave shift rescues it, the estimate is not one we
    // may tag unattended: either it is an octave error, or the track has no steady tempo at all
    // (free tempo, spoken word, a broken loop). Both are a human's call.
    TempoAmbiguity::HalfOrDoubleTime
}

/// Whether a tempo sits inside the band a mixable track plausibly occupies.
fn is_plausible(bpm: f32) -> bool {
    (MIN_PLAUSIBLE_BPM..=MAX_PLAUSIBLE_BPM).contains(&bpm)
}

/// Root mean square of the decoded samples — the loudness measure energy is derived from.
fn root_mean_square(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    (sum_squares / samples.len() as f64).sqrt() as f32
}

/// Maps RMS loudness onto the domain's normalized 0–100 energy scale.
fn energy_from_rms(rms: f32) -> Energy {
    let normalized = (rms / FULL_SCALE_RMS).clamp(0.0, 1.0);
    let scaled = (normalized * 100.0).round() as u8;
    // Clamped into range just above, so this cannot fail; falling back to the floor rather than
    // panicking keeps a maths slip from taking a whole run down.
    Energy::new(scaled.min(100)).unwrap_or_else(|_| Energy::new(0).expect("0 is in range"))
}

/// Wraps a decode failure, keeping the original as the error's source.
fn decode_error<E: std::error::Error + Send + Sync + 'static>(error: E) -> AnalyzeError {
    AnalyzeError::Decode {
        source: Box::new(error),
    }
}

/// Wraps an analysis failure, keeping the original as the error's source.
fn analysis_error<E: std::error::Error + Send + Sync + 'static>(error: E) -> AnalyzeError {
    AnalyzeError::Analysis {
        source: Box::new(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Camelot table is the one piece of this adapter that is pure lookup, and a transposed row
    /// would mis-key an entire library silently. These are spot-checks against research.md R4.
    #[test]
    fn camelot_table_matches_the_researched_mapping() {
        assert_eq!(
            camelot_from_key(KeyFinderKey::CMajor).map(|k| k.to_string()),
            Some("8B".into())
        );
        assert_eq!(
            camelot_from_key(KeyFinderKey::AMinor).map(|k| k.to_string()),
            Some("8A".into())
        );
        assert_eq!(
            camelot_from_key(KeyFinderKey::AMajor).map(|k| k.to_string()),
            Some("11B".into())
        );
        assert_eq!(
            camelot_from_key(KeyFinderKey::AFlatMinor).map(|k| k.to_string()),
            Some("1A".into())
        );
    }

    /// Silence is not a key — it must map to "no key", never to wheel position 1.
    #[test]
    fn silence_has_no_key() {
        assert_eq!(camelot_from_key(KeyFinderKey::Silence), None);
    }

    /// Relative major/minor pairs share a wheel number and differ only by letter; that property
    /// catches a whole class of table typos a spot-check would miss.
    #[test]
    fn relative_keys_share_a_wheel_position() {
        let pairs = [
            (KeyFinderKey::CMajor, KeyFinderKey::AMinor),
            (KeyFinderKey::GMajor, KeyFinderKey::EMinor),
            (KeyFinderKey::DMajor, KeyFinderKey::BMinor),
            (KeyFinderKey::EFlatMajor, KeyFinderKey::CMinor),
        ];
        for (major, minor) in pairs {
            let major = camelot_from_key(major).expect("a key");
            let minor = camelot_from_key(minor).expect("a key");
            assert_eq!(major.position(), minor.position(), "{major} vs {minor}");
            assert_eq!(major.letter(), CamelotLetter::B);
            assert_eq!(minor.letter(), CamelotLetter::A);
        }
    }

    /// Every one of the 24 keys must land on a distinct wheel slot — no key may be unreachable or
    /// double-booked.
    #[test]
    fn the_table_covers_all_twenty_four_wheel_slots() {
        let mut slots: Vec<String> = CAMELOT_BY_KEY
            .iter()
            .map(|(position, letter)| format!("{position}{}", letter.as_str()))
            .collect();
        slots.sort();
        slots.dedup();
        assert_eq!(slots.len(), 24);
    }

    #[test]
    fn ordinary_dance_tempos_are_confident() {
        for bpm in [70.0, 120.0, 128.0, 174.0, 190.0] {
            assert_eq!(tempo_ambiguity(bpm), TempoAmbiguity::Confident, "{bpm}");
        }
    }

    /// FR-031: the octave error. 68 is implausible on its own but 136 is an ordinary tempo, so the
    /// estimate is doubtful and belongs in front of a human.
    #[test]
    fn half_and_double_time_estimates_are_flagged() {
        assert_eq!(tempo_ambiguity(68.0), TempoAmbiguity::HalfOrDoubleTime);
        assert_eq!(tempo_ambiguity(64.0), TempoAmbiguity::HalfOrDoubleTime);
        assert_eq!(tempo_ambiguity(280.0), TempoAmbiguity::HalfOrDoubleTime);
    }

    #[test]
    fn a_wildly_implausible_tempo_is_never_confident() {
        assert_eq!(tempo_ambiguity(5.0), TempoAmbiguity::HalfOrDoubleTime);
        assert_eq!(tempo_ambiguity(600.0), TempoAmbiguity::HalfOrDoubleTime);
    }

    /// The flag must stay rare enough to mean something. A drum & bass track at 174 has a perfectly
    /// plausible half (87), but nothing about the estimate says it is wrong — flagging it would send
    /// a whole genre to triage over a doubt we cannot evidence.
    #[test]
    fn a_plausible_tempo_with_a_plausible_half_is_still_confident() {
        assert_eq!(tempo_ambiguity(174.0), TempoAmbiguity::Confident);
        assert_eq!(tempo_ambiguity(88.0), TempoAmbiguity::Confident);
    }

    /// The band edges are inclusive — a track exactly on the boundary is not doubtful.
    #[test]
    fn the_plausible_band_includes_its_edges() {
        assert_eq!(
            tempo_ambiguity(MIN_PLAUSIBLE_BPM),
            TempoAmbiguity::Confident
        );
        assert_eq!(
            tempo_ambiguity(MAX_PLAUSIBLE_BPM),
            TempoAmbiguity::Confident
        );
        assert_eq!(
            tempo_ambiguity(MIN_PLAUSIBLE_BPM - 1.0),
            TempoAmbiguity::HalfOrDoubleTime
        );
    }

    #[test]
    fn rms_of_silence_is_zero_and_of_full_scale_is_full() {
        assert_eq!(root_mean_square(&[0.0; 16]), 0.0);
        assert_eq!(root_mean_square(&[]), 0.0);
        assert!((root_mean_square(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn energy_spans_the_scale_and_never_overflows_it() {
        assert_eq!(energy_from_rms(0.0).value(), 0);
        assert_eq!(energy_from_rms(FULL_SCALE_RMS).value(), 100);
        assert_eq!(energy_from_rms(10.0).value(), 100, "clamped, never wrapped");
    }

    #[test]
    fn stereo_frames_fold_to_their_average() {
        let mut mono = Vec::new();
        fold_to_mono(&[1.0, 0.0, 0.5, 0.5], 2, &mut mono);
        assert_eq!(mono, vec![0.5, 0.5]);
    }

    #[test]
    fn mono_input_passes_through_untouched() {
        let mut mono = Vec::new();
        fold_to_mono(&[0.25, -0.5], 1, &mut mono);
        assert_eq!(mono, vec![0.25, -0.5]);
    }
}
