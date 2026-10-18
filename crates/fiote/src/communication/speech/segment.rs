use std::collections::VecDeque;

const FRAME: usize = 256;
const PRE_ROLL: usize = 16000;
const END_SILENCE: usize = 44;
const START_FRAMES: usize = 4;
const MIN_SPEECH: usize = 13;
const TAIL_FRAMES: usize = 13;
const MAX_UTTERANCE: usize = 320_000;

#[derive(Debug)]
pub enum Event {
    Utterance {
        id: u64,
        generation: u64,
        samples: Vec<f32>,
    },
    Rejected {
        reason: &'static str,
    },
}

pub struct Segmenter {
    generation: u64,
    next: u64,
    pending: Vec<f32>,
    pre_roll: VecDeque<f32>,
    utterance: Option<Vec<f32>>,
    detector: earshot::Detector,
    voiced: usize,
    speech: usize,
    silence: usize,
    dropping: bool,
}

impl Segmenter {
    pub fn capturing(&self) -> bool {
        self.utterance.is_some()
    }

    pub fn new(generation: u64) -> Self {
        Self {
            generation,
            next: 0,
            pending: Vec::new(),
            pre_roll: VecDeque::with_capacity(PRE_ROLL),
            utterance: None,
            detector: earshot::Detector::default(),
            voiced: 0,
            speech: 0,
            silence: 0,
            dropping: false,
        }
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<Event>, String> {
        if samples.len() > 16_000
            || samples
                .iter()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err("Listening accepts at most one second of finite normalized mono 16 kHz audio per frame.".into());
        }
        self.pending.extend_from_slice(samples);
        let frames = self.pending.len() / FRAME;
        let mut events = Vec::new();
        for index in 0..frames {
            let frame = self.pending[index * FRAME..(index + 1) * FRAME].to_vec();
            let voiced = self.detector.predict_f32(&frame) >= 0.5;
            if let Some(event) = self.frame(&frame, voiced) {
                events.push(event);
            }
        }
        self.pending.drain(..frames * FRAME);
        Ok(events)
    }

    fn frame(&mut self, frame: &[f32], voiced: bool) -> Option<Event> {
        if self.dropping {
            self.silence = if voiced { 0 } else { self.silence + 1 };
            if self.silence >= END_SILENCE {
                self.dropping = false;
                self.silence = 0;
            }
            return None;
        }
        if let Some(utterance) = &mut self.utterance {
            utterance.extend_from_slice(frame);
            if voiced {
                self.speech += 1;
                self.silence = 0;
            } else {
                self.silence += 1;
            }
            if utterance.len() >= MAX_UTTERANCE {
                self.utterance = None;
                self.dropping = true;
                self.silence = 0;
                self.pre_roll.clear();
                self.voiced = 0;
                self.speech = 0;
                return Some(Event::Rejected {
                    reason: "Speech exceeded twenty seconds. Submit a shorter explicit request.",
                });
            }
            if self.silence >= END_SILENCE {
                let mut samples = self.utterance.take().unwrap();
                let valid = self.speech >= MIN_SPEECH;
                samples.truncate(
                    samples
                        .len()
                        .saturating_sub((END_SILENCE - TAIL_FRAMES) * FRAME),
                );
                self.pre_roll.clear();
                self.voiced = 0;
                self.speech = 0;
                self.silence = 0;
                if valid {
                    self.next += 1;
                    return Some(Event::Utterance {
                        id: self.next,
                        generation: self.generation,
                        samples,
                    });
                }
                return None;
            }
            return None;
        }
        self.pre_roll.extend(frame.iter().copied());
        while self.pre_roll.len() > PRE_ROLL {
            self.pre_roll.pop_front();
        }
        if voiced {
            self.voiced += 1;
        } else {
            self.voiced = 0;
        }
        if self.voiced >= START_FRAMES {
            self.utterance = Some(self.pre_roll.drain(..).collect());
            self.speech = self.voiced;
            self.silence = 0;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(segmenter: &mut Segmenter, voiced: bool, milliseconds: usize) -> Vec<Event> {
        let mut events = Vec::new();
        let frame = vec![if voiced { 0.1 } else { 0.0 }; FRAME];
        for _ in 0..milliseconds * 16 / FRAME {
            events.extend(segmenter.frame(&frame, voiced));
        }
        events
    }

    #[test]
    fn capture_remains_active_through_short_pauses_and_ends_on_finalization_or_rejection() {
        let mut segmenter = Segmenter::new(3);
        assert!(!segmenter.capturing());
        assert!(feed(&mut segmenter, true, 700).is_empty());
        assert!(segmenter.capturing());
        assert!(feed(&mut segmenter, false, 400).is_empty());
        assert!(segmenter.capturing());
        assert_eq!(feed(&mut segmenter, false, 500).len(), 1);
        assert!(!segmenter.capturing());
        assert!(matches!(
            feed(&mut segmenter, true, 30_000).as_slice(),
            [Event::Rejected { .. }]
        ));
        assert!(!segmenter.capturing());
    }

    #[test]
    fn classified_frames_keep_short_pauses_and_refuse_isolated_peaks() {
        let mut segmenter = Segmenter::new(7);
        assert!(feed(&mut segmenter, false, 2000).is_empty());
        assert!(feed(&mut segmenter, true, 20).is_empty());
        assert!(feed(&mut segmenter, false, 1000).is_empty());
        assert!(feed(&mut segmenter, true, 700).is_empty());
        assert!(feed(&mut segmenter, false, 400).is_empty());
        assert!(feed(&mut segmenter, true, 700).is_empty());
        let events = feed(&mut segmenter, false, 1000);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Utterance {
                id,
                generation,
                samples,
            } => {
                assert_eq!((*id, *generation), (1, 7));
                assert!(samples.len() > 16_000);
            }
            Event::Rejected { .. } => panic!("Normal speech was rejected"),
        }
        assert!(feed(&mut segmenter, false, 2000).is_empty());
    }

    #[test]
    fn long_or_invalid_audio_cannot_dispatch_partial_commands_and_recovers_after_silence() {
        let mut segmenter = Segmenter::new(2);
        assert!(segmenter.push(&[f32::NAN]).is_err());
        assert!(segmenter.push(&[2.0]).is_err());
        let events = feed(&mut segmenter, true, 30_000);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Event::Rejected { .. }));
        assert!(feed(&mut segmenter, false, 1000).is_empty());
        assert!(feed(&mut segmenter, true, 500).is_empty());
        let events = feed(&mut segmenter, false, 800);
        assert!(matches!(
            events.as_slice(),
            [Event::Utterance {
                generation: 2,
                id: 1,
                ..
            }]
        ));
    }

    #[test]
    fn continuous_background_noise_does_not_hold_the_segmenter_open() {
        let mut segmenter = Segmenter::new(11);
        let mut seed = 73_u32;
        for _ in 0..300 {
            let samples = (0..1600)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed as f64 / u32::MAX as f64 * 0.3 - 0.15) as f32
                })
                .collect::<Vec<_>>();
            assert!(segmenter.push(&samples).unwrap().is_empty());
        }
        assert!(segmenter.utterance.is_none());
        assert!(!segmenter.dropping);
    }

    #[test]
    fn delayed_voice_classification_preserves_the_leading_audio() {
        let mut segmenter = Segmenter::new(23);
        for index in 0..50 {
            let sample = if index == 4 { 0.25 } else { 0.01 };
            assert!(segmenter.frame(&vec![sample; FRAME], index >= 30).is_none());
        }
        let events = feed(&mut segmenter, false, 800);
        let [Event::Utterance { samples, .. }] = events.as_slice() else {
            panic!("The classified speech did not finalize once");
        };
        assert!(samples.contains(&0.25));
    }

    #[test]
    fn real_human_speech_finalizes_once_independent_of_input_chunk_boundaries() {
        let mut wav = hound::WavReader::new(std::io::Cursor::new(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/speech-goforward.wav"
        ))))
        .unwrap();
        assert_eq!(wav.spec().sample_rate, 16_000);
        assert_eq!(wav.spec().channels, 1);
        let mut samples = wav
            .samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / 32_768.0)
            .collect::<Vec<_>>();
        samples.extend(vec![0.0; 16_000]);
        let mut reference = None;
        for chunk_size in [1600, 97] {
            let mut segmenter = Segmenter::new(19);
            let events = samples
                .chunks(chunk_size)
                .flat_map(|chunk| segmenter.push(chunk).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(events.len(), 1);
            let Event::Utterance {
                id,
                generation,
                samples,
            } = events.into_iter().next().unwrap()
            else {
                panic!("Normal human speech was refused");
            };
            assert_eq!((id, generation), (1, 19));
            assert!(samples.len() > 16_000);
            if let Some(reference) = &reference {
                assert_eq!(&samples, reference);
            } else {
                reference = Some(samples);
            }
            assert!(segmenter.utterance.is_none());
            assert!(!segmenter.dropping);
        }
        println!(
            "Embedded voice detector state: {} bytes",
            std::mem::size_of::<earshot::Detector>()
        );
    }
}
