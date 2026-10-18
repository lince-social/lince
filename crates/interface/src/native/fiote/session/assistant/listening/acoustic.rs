use super::*;
use cell::fiote_communication::speech::{segment, wake};
use std::collections::VecDeque;

pub(super) struct Gate {
    detector: wake::Detector,
    segmenter: segment::Segmenter,
    history: VecDeque<f32>,
    generation: u64,
    total: u64,
    next: u64,
    window: Option<Instant>,
    conversation: bool,
}

impl Gate {
    pub(super) fn new(profile: wake::Profile, generation: u64) -> Result<Self, String> {
        Ok(Self {
            detector: wake::Detector::new(profile)?,
            segmenter: segment::Segmenter::new(generation),
            history: VecDeque::new(),
            generation,
            total: 0,
            next: 0,
            window: None,
            conversation: false,
        })
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.window.filter(|_| !self.segmenter.capturing())
    }

    pub(super) fn set_conversation(&mut self, active: bool) {
        if self.conversation != active {
            self.close();
            self.conversation = active;
        }
    }

    fn clear_window(&mut self) {
        self.window = None;
        self.history.clear();
        self.detector.reset_at(self.total);
    }

    fn close(&mut self) {
        self.clear_window();
        self.segmenter = segment::Segmenter::new(self.generation);
    }

    pub(super) fn push(&mut self, samples: &[f32]) -> Result<Vec<Event>, String> {
        if samples.len() > 16_000
            || samples
                .iter()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err("Wake listening requires bounded normalized mono 16 kHz audio.".into());
        }
        let mut events = Vec::new();
        if self
            .window
            .is_some_and(|deadline| Instant::now() >= deadline)
            && !self.segmenter.capturing()
        {
            self.close();
            events.push(Event::Notice(
                "Wake request window expired; say Hey Fiote again. Nothing was sent.".into(),
            ));
        }
        for part in samples.chunks(160) {
            self.total += part.len() as u64;
            if self.window.is_some() || self.conversation {
                self.collect(part, &mut events)?;
                continue;
            }
            self.history.extend(part.iter().copied());
            while self.history.len() > wake::HISTORY_SAMPLES {
                self.history.pop_front();
            }
            if let Some(detection) = self.detector.push(part)? {
                self.window = Some(Instant::now() + Duration::from_secs(10));
                self.segmenter = segment::Segmenter::new(self.generation);
                events.push(Event::Notice(
                    "Heard Hey Fiote. Start one request within ten seconds, then pause when finished. Requests longer than twenty seconds are refused.".into(),
                ));
                let keep = (self.total.saturating_sub(detection.end_sample) as usize)
                    .min(self.history.len());
                let tail: Vec<_> = self
                    .history
                    .iter()
                    .skip(self.history.len() - keep)
                    .copied()
                    .collect();
                self.history.clear();
                self.collect(&tail, &mut events)?;
            }
        }
        Ok(events)
    }

    fn collect(&mut self, samples: &[f32], events: &mut Vec<Event>) -> Result<(), String> {
        let capturing = self.segmenter.capturing();
        for event in self.segmenter.push(samples)? {
            match event {
                segment::Event::Utterance { samples, .. } => {
                    self.next = self
                        .next
                        .checked_add(1)
                        .ok_or("Wake request sequence exhausted.")?;
                    let audio =
                        crate::native::speech::encode(crate::native::sound::library::Clip {
                            samples,
                            rate: 16_000,
                        })?;
                    events.push(if self.conversation {
                        Event::Segment {
                            generation: self.generation,
                            id: self.next,
                            audio,
                        }
                    } else {
                        Event::WokenSegment {
                            generation: self.generation,
                            id: self.next,
                            audio,
                        }
                    });
                    self.close();
                }
                segment::Event::Rejected { reason } => {
                    events.push(Event::Notice(reason.into()));
                    self.clear_window();
                }
            }
        }
        if !capturing && self.segmenter.capturing() {
            events.push(Event::Notice(
                "Listening to your request. Pause when finished; requests longer than twenty seconds are refused. Nothing is sent before finalization.".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Gate {
        Gate::new(
            wake::Profile {
                version: 2,
                threshold: 0.9999,
                references: vec![vec![vec![0.0; 96]; 40]; 3],
                end_padding: vec![4000; 3],
            },
            7,
        )
        .unwrap()
    }

    fn request(gate: &mut Gate) -> Vec<Event> {
        let reader = hound::WavReader::new(std::io::Cursor::new(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fiote/tests/fixtures/speech-goforward.wav"
        ))))
        .unwrap();
        let samples = reader
            .into_samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / 32768.0)
            .collect::<Vec<_>>();
        let mut events = Vec::new();
        for part in samples.chunks(1600) {
            events.extend(gate.push(part).unwrap());
        }
        for _ in 0..15 {
            events.extend(gate.push(&vec![0.0; 1600]).unwrap());
        }
        events
    }

    #[test]
    fn one_acoustic_window_delivers_one_complete_request_and_ids_survive_rearming() {
        let mut gate = gate();
        assert!(
            request(&mut gate)
                .iter()
                .all(|event| !matches!(event, Event::WokenSegment { .. }))
        );
        for expected in [1, 2] {
            gate.window = Some(Instant::now() + Duration::from_secs(10));
            let events = request(&mut gate);
            let segments: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    Event::WokenSegment {
                        generation,
                        id,
                        audio,
                    } => Some((*generation, *id, audio)),
                    _ => None,
                })
                .collect();
            assert_eq!(segments.len(), 1);
            assert_eq!((segments[0].0, segments[0].1), (7, expected));
            assert!(
                cell::fiote_communication::speech::decode_audio(segments[0].2)
                    .unwrap()
                    .len()
                    > 16_000
            );
            assert!(gate.deadline().is_none());
        }
    }

    #[test]
    fn active_conversation_emits_unmarked_followups_and_ending_rearms_the_wake_gate() {
        let mut gate = gate();
        gate.set_conversation(true);
        for expected in [1, 2] {
            let events = request(&mut gate);
            let segments = events
                .iter()
                .filter_map(|event| match event {
                    Event::Segment { generation, id, .. } => Some((*generation, *id)),
                    Event::WokenSegment { .. } => {
                        panic!("Follow-ups must not carry a fresh acoustic activation")
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(segments, vec![(7, expected)]);
        }
        gate.set_conversation(false);
        assert!(
            request(&mut gate)
                .iter()
                .all(|event| !matches!(event, Event::Segment { .. } | Event::WokenSegment { .. }))
        );
        gate.window = Some(Instant::now() + Duration::from_secs(10));
        assert!(request(&mut gate).iter().any(|event| matches!(
            event,
            Event::WokenSegment {
                generation: 7,
                id: 3,
                ..
            }
        )));
    }

    #[test]
    fn a_started_request_survives_the_empty_window_deadline_and_delivers_once() {
        let reader = hound::WavReader::new(std::io::Cursor::new(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fiote/tests/fixtures/speech-goforward.wav"
        ))))
        .unwrap();
        let samples = reader
            .into_samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / 32768.0)
            .collect::<Vec<_>>();
        let mut gate = gate();
        gate.window = Some(Instant::now() + Duration::from_secs(10));
        let mut advanced = false;
        let mut events = Vec::new();
        for part in samples.chunks(160) {
            events.extend(gate.push(part).unwrap());
            if !advanced && gate.segmenter.capturing() {
                gate.window = Some(Instant::now() - Duration::from_secs(1));
                assert!(gate.deadline().is_none());
                advanced = true;
            }
        }
        assert!(
            advanced,
            "The independent human recording must start real VAD capture"
        );
        for _ in 0..15 {
            events.extend(gate.push(&vec![0.0; 1600]).unwrap());
        }
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::WokenSegment { .. }))
                .count(),
            1
        );
        assert!(!events.iter().any(
            |event| matches!(event, Event::Notice(detail) if detail.contains("window expired"))
        ));
        assert!(gate.window.is_none());
        assert!(
            request(&mut gate)
                .iter()
                .all(|event| !matches!(event, Event::WokenSegment { .. } | Event::Segment { .. }))
        );
    }

    #[test]
    fn oversized_followups_discard_the_continuing_tail_until_silence() {
        let reader = hound::WavReader::new(std::io::Cursor::new(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fiote/tests/fixtures/speech-goforward.wav"
        ))))
        .unwrap();
        let samples = reader
            .into_samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / 32768.0)
            .collect::<Vec<_>>();
        let audible = |frame: &[f32]| {
            (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32).sqrt()
                > 0.01
        };
        let first = samples.chunks(160).position(audible).unwrap() * 160;
        let last = (samples.chunks(160).rposition(audible).unwrap() + 1) * 160;
        let continuous = samples[first..last.min(samples.len())]
            .iter()
            .copied()
            .cycle()
            .take(16_000 * 25)
            .collect::<Vec<_>>();
        let mut gate = gate();
        gate.set_conversation(true);
        let mut events = Vec::new();
        for part in continuous.chunks(1600) {
            events.extend(gate.push(part).unwrap());
        }
        for _ in 0..15 {
            events.extend(gate.push(&vec![0.0; 1600]).unwrap());
        }
        assert_eq!(events.iter().filter(|event| matches!(event, Event::Notice(detail) if detail.contains("exceeded twenty seconds"))).count(), 1);
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, Event::Segment { .. } | Event::WokenSegment { .. }))
        );
        assert_eq!(
            request(&mut gate)
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Segment {
                        id: 1,
                        generation: 7,
                        ..
                    }
                ))
                .count(),
            1
        );
    }

    #[test]
    fn empty_expired_or_invalid_windows_do_not_dispatch() {
        let mut gate = gate();
        gate.window = Some(Instant::now() - Duration::from_millis(1));
        let events = gate.push(&[]).unwrap();
        assert!(gate.deadline().is_none());
        assert!(events.iter().all(|event| matches!(event, Event::Notice(_))));
        assert!(gate.push(&[f32::INFINITY]).is_err());
        assert!(gate.push(&vec![0.0; 16_001]).is_err());
        assert_eq!(gate.next, 0);
    }
}
