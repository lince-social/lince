use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use wake_reference::MfccComparator;

#[path = "wake/features.rs"]
mod features;
use features::{Features, WIDTH};

pub const HISTORY_SAMPLES: usize = 40_000;
pub const NOTICE: &str = include_str!("../../../../vendor/wake-reference/CREDITS.txt");
pub const LICENSE: &str = include_str!("../../../../vendor/wake-reference/LICENSE");
pub const FFT_LICENSE: &str = include_str!("../../../assets/wake/LICENSE-RustFFT-MIT.txt");
pub const MODEL_LICENSE: &str =
    include_str!("../../../assets/wake/LICENSE-Feature-Models-Apache-2.0.txt");
pub const MODEL_NOTICE: &str = include_str!("../../../assets/wake/FEATURE-MODELS-NOTICE.txt");
pub const TRACT_LICENSE: &str = include_str!("../../../assets/wake/LICENSE-tract-MIT.txt");
const PEAK_FRAMES: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub threshold: f32,
    pub references: Vec<Vec<Vec<f32>>>,
    pub end_padding: Vec<u32>,
}

impl Profile {
    pub fn enroll(recordings: &[Vec<f32>]) -> Result<Self, String> {
        if !(3..=8).contains(&recordings.len()) {
            return Err("Record between three and eight local examples of Hey Fiote.".into());
        }
        let mut references = Vec::new();
        let mut end_padding = Vec::new();
        for samples in recordings {
            if !(8000..=64_000).contains(&samples.len())
                || samples
                    .iter()
                    .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
            {
                return Err("Wake references require half a second to four seconds of normalized mono 16 kHz audio.".into());
            }
            let energies: Vec<_> = samples
                .chunks(160)
                .map(|frame| {
                    (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32)
                        .sqrt()
                })
                .collect();
            let floor = (energies.iter().copied().fold(0.0, f32::max) * 0.12).max(0.003);
            let first =
                energies.iter().position(|energy| *energy >= floor).ok_or(
                    "No clear wake phrase was recorded. Check the microphone and try again.",
                )? * 160;
            let last = ((energies
                .iter()
                .rposition(|energy| *energy >= floor)
                .unwrap()
                + 1)
                * 160)
                .min(samples.len());
            if !(3200..=48_000).contains(&(last - first)) {
                return Err("Record one clear wake phrase, lasting between a fifth of a second and three seconds.".into());
            }
            let before = first.min(5120);
            let after = (samples.len() - last).min(1280);
            let mut canonical = vec![0.0; 1920 + 5120 - before];
            canonical.extend_from_slice(&samples[first - before..last + after]);
            let speech_end = 1920 + 5120 + last - first;
            canonical.extend(vec![0.0; 6400 + 1280 - after]);
            let mut extractor = Features::new()?;
            let mut reference = Vec::new();
            let mut end = 0u64;
            let audio_end = canonical.len();
            canonical.resize(audio_end.div_ceil(features::STEP) * features::STEP, 0.0);
            for part in canonical.chunks(16_000) {
                for frame in extractor.push(part)? {
                    if frame.end >= 12_512 && frame.end <= audio_end as u64 {
                        end = frame.end;
                        reference.push(frame.values);
                    }
                }
            }
            end_padding.push(end.saturating_sub(speech_end as u64) as u32);
            references.push(reference);
        }
        let profile = Self {
            version: 2,
            threshold: 0.81,
            references,
            end_padding,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 524_288 {
            return Err("Wake reference file exceeds 512 KiB.".into());
        }
        let profile: Self = serde_json::from_slice(bytes).map_err(|_| "Choose a compatible Lince wake reference JSON file, or record three examples again.")?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 2
            || !self.threshold.is_finite()
            || !(0.80..=0.9999).contains(&self.threshold)
            || !(3..=8).contains(&self.references.len())
            || self.end_padding.len() != self.references.len()
            || self.end_padding.iter().any(|padding| *padding > 8000)
        {
            return Err("Unsupported wake reference version, sensitivity or examples. Record three local examples again.".into());
        }
        for reference in &self.references {
            if !(3..=40).contains(&reference.len())
                || reference.iter().any(|frame| {
                    frame.len() != WIDTH
                        || frame
                            .iter()
                            .any(|value| !value.is_finite() || value.abs() > 10000.0)
                })
            {
                return Err(
                    "The wake reference contains invalid or excessive acoustic features.".into(),
                );
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| "Cannot serialize wake references.")?
            .len()
            > 524_288
        {
            return Err("Wake references exceed 512 KiB. Use fewer or shorter examples.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Peak {
    score: f32,
    end: u64,
    frame: u64,
}

pub struct Detector {
    profile: Profile,
    features: Features,
    frames: VecDeque<Vec<f32>>,
    maximum: usize,
    processed: u64,
    base: u64,
    index: u64,
    cooldown: u64,
    audible: Option<u64>,
    peak: Option<Peak>,
}

#[derive(Debug, Clone, Copy)]
pub struct Detection {
    pub end_sample: u64,
    pub score: f32,
}

impl Detector {
    pub fn new(profile: Profile) -> Result<Self, String> {
        profile.validate()?;
        let maximum = profile.references.iter().map(Vec::len).max().unwrap() + 1;
        Ok(Self {
            profile,
            features: Features::new()?,
            frames: VecDeque::new(),
            maximum,
            processed: 0,
            base: 0,
            index: 0,
            cooldown: 0,
            audible: None,
            peak: None,
        })
    }

    pub fn reset(&mut self) {
        self.reset_at(self.processed);
    }

    pub fn reset_at(&mut self, processed: u64) {
        self.features.reset();
        self.frames.clear();
        self.processed = processed;
        self.base = processed;
        self.index = 0;
        self.cooldown = processed.saturating_add(8000);
        self.audible = None;
        self.peak = None;
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<Option<Detection>, String> {
        if samples.len() > 16_000
            || samples
                .iter()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err("Wake detection accepts at most one second of normalized mono 16 kHz audio at a time.".into());
        }
        let mut detection = None;
        for part in samples.chunks(160) {
            self.processed = self
                .processed
                .checked_add(part.len() as u64)
                .ok_or("Wake audio position overflowed.")?;
            let energy =
                (part.iter().map(|sample| sample * sample).sum::<f32>() / part.len() as f32).sqrt();
            if energy >= 0.002 {
                self.audible = Some(self.processed);
            }
            for frame in self.features.push(part)? {
                let end = self
                    .base
                    .checked_add(frame.end)
                    .ok_or("Wake audio position overflowed.")?;
                self.index = self
                    .index
                    .checked_add(1)
                    .ok_or("Wake feature position overflowed.")?;
                self.frames.push_back(frame.values);
                while self.frames.len() > self.maximum {
                    self.frames.pop_front();
                }
                if detection.is_some() || end < self.cooldown {
                    continue;
                }
                if self
                    .audible
                    .is_some_and(|audible| end <= audible.saturating_add(12_800))
                {
                    let (score, padding) = self.score();
                    if score >= self.profile.threshold
                        && self.peak.is_none_or(|peak| score > peak.score)
                    {
                        self.peak = Some(Peak {
                            score,
                            end: end.saturating_sub(u64::from(padding)),
                            frame: self.index,
                        });
                    }
                }
                if let Some(peak) = self
                    .peak
                    .filter(|peak| self.index.saturating_sub(peak.frame) >= PEAK_FRAMES)
                {
                    detection = Some(Detection {
                        end_sample: peak.end,
                        score: peak.score,
                    });
                    self.cooldown = end.saturating_add(24_000);
                    self.peak = None;
                }
            }
        }
        Ok(detection)
    }

    fn score(&self) -> (f32, u32) {
        let mut best = (0.0f32, 0);
        for (reference, padding) in self
            .profile
            .references
            .iter()
            .zip(&self.profile.end_padding)
        {
            for length in [reference.len() - 1, reference.len(), reference.len() + 1] {
                if length > self.frames.len() {
                    continue;
                }
                let window: Vec<_> = self
                    .frames
                    .iter()
                    .skip(self.frames.len() - length)
                    .map(Vec::as_slice)
                    .collect();
                let score = similarity(&window, reference);
                if score > best.0 {
                    best = (score, *padding);
                }
            }
        }
        best
    }
}

fn similarity(window: &[&[f32]], reference: &[Vec<f32>]) -> f32 {
    let mut previous = vec![f32::INFINITY; reference.len() + 1];
    let mut current = previous.clone();
    previous[0] = 0.0;
    for frame in window {
        current.fill(f32::INFINITY);
        for column in 1..=reference.len() {
            current[column] = MfccComparator::calculate_distance(frame, &reference[column - 1])
                + previous[column]
                    .min(previous[column - 1])
                    .min(current[column - 1]);
        }
        std::mem::swap(&mut current, &mut previous);
    }
    (1.0 - previous[reference.len()] / (window.len() + reference.len()) as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrollment_ignores_variable_silence_and_import_rejects_invalid_timing_or_dimensions() {
        let samples = super::super::decode_audio(&{
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            STANDARD.encode(include_bytes!(
                "../../../tests/fixtures/conversation/wake-statement.wav"
            ))
        })
        .unwrap();
        let profile =
            Profile::enroll(&[samples.clone(), samples.clone(), samples.clone()]).unwrap();
        let mut padded = vec![0.0; 16_000];
        padded.extend(samples);
        padded.extend(vec![0.0; 16_000]);
        let padded = Profile::enroll(&[padded.clone(), padded.clone(), padded]).unwrap();
        assert_eq!(profile.references, padded.references);
        assert_eq!(profile.end_padding, padded.end_padding);
        let mut invalid = profile.clone();
        invalid.end_padding[0] = 8001;
        assert!(invalid.validate().is_err());
        invalid = profile.clone();
        invalid.end_padding.pop();
        assert!(invalid.validate().is_err());
        invalid = profile;
        invalid.references[0][0].pop();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn streaming_detection_and_endpoint_are_independent_of_audio_chunk_sizes() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let samples = super::super::decode_audio(&STANDARD.encode(include_bytes!(
            "../../../tests/fixtures/conversation/wake-statement.wav"
        )))
        .unwrap();
        let profile =
            Profile::enroll(&[samples.clone(), samples.clone(), samples.clone()]).unwrap();
        let mut input = vec![0.0; 32592];
        input.extend(samples);
        input.extend(vec![0.0; 24_000]);
        let mut reference: Option<(u64, f32)> = None;
        for size in [97, 160, 1600, 16_000] {
            let mut detector = Detector::new(profile.clone()).unwrap();
            let detections = input
                .chunks(size)
                .filter_map(|part| detector.push(part).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(detections.len(), 1, "Chunk size {size}");
            if let Some((end, score)) = reference {
                assert_eq!(detections[0].end_sample, end);
                assert!((detections[0].score - score).abs() < 0.000001);
            } else {
                reference = Some((detections[0].end_sample, detections[0].score));
            }
        }
    }

    #[test]
    #[ignore = "Requires LINCE_VOICE_TEST_OUTPUT wake.wav and command.wav; tests acoustic combined/separate activation without ASR"]
    fn actual_acoustic_combined_phrase_detects_without_activating_on_unrelated_human_speech() {
        let directory =
            std::path::PathBuf::from(std::env::var_os("LINCE_VOICE_TEST_OUTPUT").unwrap());
        let read = |name: &str| {
            let reader = hound::WavReader::open(directory.join(name)).unwrap();
            let rate = reader.spec().sample_rate;
            assert_eq!(reader.spec().channels, 1);
            let input = reader
                .into_samples::<i16>()
                .map(|sample| f32::from(sample.unwrap()) / 32768.0)
                .collect::<Vec<_>>();
            (0..input.len() * 16_000 / rate as usize)
                .map(|index| {
                    let position = index as f64 * rate as f64 / 16_000.0;
                    let left = position as usize;
                    let right = (left + 1).min(input.len() - 1);
                    input[left] + (input[right] - input[left]) * position.fract() as f32
                })
                .collect::<Vec<_>>()
        };
        let wake = read("wake.wav");
        let profile = Profile::enroll(&[wake.clone(), wake.clone(), wake.clone()]).unwrap();
        let mut failures = Vec::new();
        for (name, samples, positive) in [
            ("wake", wake, true),
            ("combined", read("command.wav"), true),
            ("ordinary", recording(), false),
        ] {
            let mut detector = Detector::new(profile.clone()).unwrap();
            let mut samples = samples;
            samples.extend(vec![0.0; 24_000]);
            let mut maximum = 0.0f32;
            let mut detections = Vec::new();
            for part in samples.chunks(160) {
                if let Some(detection) = detector.push(part).unwrap() {
                    detections.push(detection);
                }
                maximum = maximum.max(detector.score().0);
            }
            println!(
                "{name}: samples={} reference_frames={} maximum={maximum} detections={detections:?}",
                samples.len(),
                profile.references[0].len()
            );
            if positive == detections.is_empty() {
                failures.push(name);
            }
        }
        assert!(
            failures.is_empty(),
            "Acoustic acceptance failed: {failures:?}"
        );
    }

    fn recording() -> Vec<f32> {
        let reader = hound::WavReader::new(std::io::Cursor::new(include_bytes!(
            "../../../tests/fixtures/speech-goforward.wav"
        )))
        .unwrap();
        reader
            .into_samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / 32768.0)
            .collect()
    }

    #[test]
    fn enrollment_rejects_silence_wrong_counts_and_invalid_reference_files() {
        assert!(Profile::enroll(&vec![vec![0.0; 16_000]; 3]).is_err());
        assert!(Profile::enroll(&[vec![0.1; 16_000]]).is_err());
        assert!(Profile::from_bytes(&vec![b' '; 524_289]).is_err());
        assert!(Profile::from_bytes(br#"{"version":1,"threshold":0.68,"references":[]}"#).is_err());
        let samples = recording();
        let mut profile = Profile::enroll(&[samples.clone(), samples.clone(), samples]).unwrap();
        profile.references[0][0][0] = f32::NAN;
        assert!(profile.validate().is_err());
    }

    #[test]
    fn registered_acoustic_reference_round_trips_and_detects_without_transcription() {
        let samples = recording();
        let profile =
            Profile::enroll(&[samples.clone(), samples.clone(), samples.clone()]).unwrap();
        let encoded = serde_json::to_vec(&profile).unwrap();
        let profile = Profile::from_bytes(&encoded).unwrap();
        let mut detector = Detector::new(profile).unwrap();
        let mut detections = Vec::new();
        let mut streamed = samples.clone();
        streamed.extend(vec![0.0; 24_000]);
        for part in streamed.chunks(160) {
            if let Some(detection) = detector.push(part).unwrap() {
                detections.push(detection);
            }
        }
        assert!(
            !detections.is_empty(),
            "Self-reference smoke check; independent voice acceptance is separate"
        );
        assert!(
            detections
                .iter()
                .all(|detection| detection.score.is_finite()
                    && detection.end_sample <= streamed.len() as u64)
        );
        assert!(detector.push(&[f32::NAN]).is_err());
        assert!(detector.push(&vec![0.0; 16_001]).is_err());
        detector.reset_at(1_000_003);
        assert_eq!(detector.processed, 1_000_003);
        let mut repeated = false;
        for part in streamed.chunks(97) {
            repeated |= detector.push(part).unwrap().is_some();
        }
        assert!(
            repeated,
            "A nonaligned capture boundary must not disable later wake matching"
        );
    }
}
