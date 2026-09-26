use std::collections::{BTreeMap, VecDeque};

use crate::{MediaError, Result};

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_SAMPLES: usize = 480;
pub const QUEUE_FRAMES: usize = 6;
pub const MAX_AUDIO_TRACKS: usize = 10;

#[derive(Clone)]
pub struct AudioFrame(pub [f32; FRAME_SAMPLES]);

impl AudioFrame {
    pub fn silence() -> Self {
        Self([0.0; FRAME_SAMPLES])
    }

    pub fn pcm(&self) -> [i16; FRAME_SAMPLES] {
        self.0
            .map(|sample| (finite(sample).clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
    }
}

pub fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[derive(Default)]
pub struct Mixer {
    tracks: BTreeMap<String, VecDeque<AudioFrame>>,
    dropped: u64,
}

impl Mixer {
    pub fn add(&mut self, track: &str) -> Result<()> {
        if self.tracks.contains_key(track) {
            return Ok(());
        }
        if self.tracks.len() == MAX_AUDIO_TRACKS {
            return Err(MediaError("Too many incoming audio tracks".into()));
        }
        self.tracks
            .insert(track.to_owned(), VecDeque::with_capacity(QUEUE_FRAMES));
        Ok(())
    }

    pub fn remove(&mut self, track: &str) {
        self.tracks.remove(track);
    }

    pub fn push(&mut self, track: &str, frame: AudioFrame) -> Result<()> {
        let queue = self
            .tracks
            .get_mut(track)
            .ok_or_else(|| MediaError("Audio track is no longer admitted".into()))?;
        if queue.len() == QUEUE_FRAMES {
            queue.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        queue.push_back(frame);
        Ok(())
    }

    pub fn render(&mut self, volume: f32) -> AudioFrame {
        let mut output = AudioFrame::silence();
        let volume = finite(volume).clamp(0.0, 1.0);
        for queue in self.tracks.values_mut() {
            if let Some(frame) = queue.pop_front() {
                for (out, sample) in output.0.iter_mut().zip(frame.0) {
                    *out += finite(sample) * volume;
                }
            }
        }
        for sample in &mut output.0 {
            *sample = sample.clamp(-1.0, 1.0);
        }
        output
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_latency_is_bounded_and_stale_samples_are_dropped() {
        let mut mixer = Mixer::default();
        mixer.add("peer").unwrap();
        for index in 0..1000 {
            mixer
                .push("peer", AudioFrame([index as f32 / 1000.0; FRAME_SAMPLES]))
                .unwrap();
        }
        assert_eq!(mixer.dropped(), 994);
        assert_eq!(mixer.render(1.0).0[0], 0.994);
        for _ in 0..5 {
            mixer.render(1.0);
        }
        assert_eq!(mixer.render(1.0).0[0], 0.0);
    }

    #[test]
    fn revocation_discards_queued_audio_and_rejects_late_frames() {
        let mut mixer = Mixer::default();
        mixer.add("peer").unwrap();
        mixer
            .push("peer", AudioFrame([0.8; FRAME_SAMPLES]))
            .unwrap();
        mixer.remove("peer");
        assert_eq!(mixer.render(1.0).0[0], 0.0);
        assert!(mixer.push("peer", AudioFrame::silence()).is_err());
    }

    #[test]
    fn mixed_audio_matches_the_volume_and_never_exceeds_the_sample_range() {
        let mut mixer = Mixer::default();
        for peer in ["a", "b"] {
            mixer.add(peer).unwrap();
            mixer.push(peer, AudioFrame([0.8; FRAME_SAMPLES])).unwrap();
        }
        assert_eq!(mixer.render(0.5).0[0], 0.8);
        mixer
            .push("a", AudioFrame([f32::NAN; FRAME_SAMPLES]))
            .unwrap();
        assert_eq!(mixer.render(1.0).pcm()[0], 0);
    }

    #[test]
    fn unsolicited_tracks_cannot_grow_the_mixer() {
        let mut mixer = Mixer::default();
        assert!(mixer.push("unknown", AudioFrame::silence()).is_err());
        for index in 0..MAX_AUDIO_TRACKS {
            mixer.add(&index.to_string()).unwrap();
        }
        assert!(mixer.add("extra").is_err());
    }
}
