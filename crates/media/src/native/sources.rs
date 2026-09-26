use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use opus_rs::{Application, OpusEncoder};

use super::{codec::VideoEncoder, error, peer::Source};
use crate::{MediaError, Result, audio::FRAME_SAMPLES, video::VideoFrame};

#[derive(Clone)]
pub(crate) struct Packet {
    pub sequence: u64,
    pub timestamp: u64,
    pub at: Instant,
    pub data: Arc<[u8]>,
}

struct Track {
    enabled: bool,
    generation: u64,
    sequence: u64,
    audio_time: u64,
    packets: VecDeque<Packet>,
    frame: Option<VideoFrame>,
    keyframe: bool,
    encoder: Option<OpusEncoder>,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            enabled: false,
            generation: 0,
            sequence: 0,
            audio_time: 0,
            packets: VecDeque::new(),
            frame: None,
            keyframe: true,
            encoder: None,
        }
    }
}

struct State {
    tracks: [Track; 4],
    devices: usize,
    failure: Option<MediaError>,
    started: Instant,
}

pub struct Sources {
    state: Arc<Mutex<State>>,
    running: Arc<AtomicBool>,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                tracks: std::array::from_fn(|_| Track::default()),
                devices: 2,
                failure: None,
                started: Instant::now(),
            })),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Sources {
    pub fn enabled(&self, source: Source) -> bool {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).tracks[source as usize].enabled
    }

    pub fn set_enabled(&self, source: Source, enabled: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let track = &mut state.tracks[source as usize];
        if track.enabled != enabled {
            track.enabled = enabled;
            track.generation += 1;
            track.packets.clear();
            track.frame = None;
            track.encoder = None;
            track.keyframe = true;
        }
    }

    pub async fn audio(&self, source: Source, samples: [i16; FRAME_SAMPLES]) -> Result<()> {
        if !source.audio() {
            return Err(MediaError("Choose an audio source".into()));
        }
        let mut state = self.state.lock().map_err(error)?;
        let clock = state.started.elapsed().as_millis() as u64 / 10 * FRAME_SAMPLES as u64;
        let track = &mut state.tracks[source as usize];
        if !track.enabled {
            return Ok(());
        }
        if track.encoder.is_none() {
            let mut encoder = OpusEncoder::new(48_000, 1, Application::Audio).map_err(error)?;
            encoder.bitrate_bps = if source == Source::Microphone {
                32_000
            } else {
                48_000
            };
            track.encoder = Some(encoder);
        }
        let mut data = vec![0; 1275];
        let len = track
            .encoder
            .as_mut()
            .unwrap()
            .encode_i16(&samples, FRAME_SAMPLES, &mut data)
            .map_err(error)?;
        data.truncate(len);
        track.sequence += 1;
        track.audio_time = track.audio_time.max(clock);
        let packet = Packet {
            sequence: track.sequence,
            timestamp: track.audio_time,
            at: Instant::now(),
            data: data.into(),
        };
        track.audio_time += FRAME_SAMPLES as u64;
        if track.packets.len() >= 6 {
            track.packets.pop_front();
        }
        track.packets.push_back(packet);
        Ok(())
    }

    pub fn video(&self, source: Source, frame: &VideoFrame) -> Result<()> {
        if source.audio() {
            return Err(MediaError("Choose a video source".into()));
        }
        let mut state = self.state.lock().map_err(error)?;
        if let Some(err) = state.failure.take() {
            return Err(err);
        }
        if !state.tracks[source as usize].enabled {
            return Ok(());
        }
        state.tracks[source as usize].frame = Some(frame.clone());
        if !self.running.swap(true, Ordering::AcqRel) {
            let state = Arc::downgrade(&self.state);
            let running = self.running.clone();
            let result = std::thread::Builder::new()
                .name("lince-video-encode".into())
                .spawn(move || {
                    let mut encoders: [Option<VideoEncoder>; 2] = [None, None];
                    let mut generations = [0; 2];
                    let mut attendance = [0; 2];
                    let mut last = [Instant::now() - Duration::from_secs(1); 2];
                    while running.load(Ordering::Acquire) {
                        let Some(shared) = state.upgrade() else { break };
                        for index in 2..4 {
                            if last[index - 2].elapsed() < Duration::from_millis(100) {
                                continue;
                            }
                            let (input, generation, keyframe, devices, started) = {
                                let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                                let devices = state.devices;
                                let started = state.started;
                                let track = &mut state.tracks[index];
                                let Some(input) = track.frame.take() else {
                                    continue;
                                };
                                (
                                    input,
                                    track.generation,
                                    std::mem::take(&mut track.keyframe),
                                    devices,
                                    started,
                                )
                            };
                            last[index - 2] = Instant::now();
                            let maximum = if index == 2 {
                                if devices > 3 { 320 } else { 640 }
                            } else {
                                1280
                            };
                            let ratio = (maximum as f64 / input.width() as f64).min(1.0);
                            let width = ((input.width() as f64 * ratio) as usize / 2 * 2).max(16);
                            let height = ((input.height() as f64 * ratio) as usize / 2 * 2).max(16);
                            let result = (|| -> Result<Option<Vec<u8>>> {
                                if encoders[index - 2]
                                    .as_ref()
                                    .is_none_or(|e| e.dimensions() != (width, height))
                                    || generations[index - 2] != generation
                                    || attendance[index - 2] != devices
                                {
                                    let bitrate = if index == 2 { 600_000 } else { 1_000_000 }
                                        / (devices as i32 - 1).max(1);
                                    encoders[index - 2] =
                                        Some(VideoEncoder::new(width, height, bitrate)?);
                                    generations[index - 2] = generation;
                                    attendance[index - 2] = devices;
                                }
                                encoders[index - 2]
                                    .as_mut()
                                    .unwrap()
                                    .encode(&input, keyframe)
                            })();
                            let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                            let track = &mut state.tracks[index];
                            if !track.enabled || track.generation != generation {
                                continue;
                            }
                            match result {
                                Ok(Some(data)) => {
                                    track.sequence += 1;
                                    let packet = Packet {
                                        sequence: track.sequence,
                                        timestamp: started.elapsed().as_micros() as u64 * 90 / 1000,
                                        at: Instant::now(),
                                        data: data.into(),
                                    };
                                    track.packets.clear();
                                    track.packets.push_back(packet);
                                }
                                Ok(None) => {}
                                Err(err) => state.failure = Some(err),
                            }
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                });
            if let Err(err) = result {
                self.running.store(false, Ordering::Release);
                return Err(error(err));
            }
        }
        Ok(())
    }

    pub(crate) fn reader(&self) -> SourceReader {
        SourceReader(self.state.clone())
    }
}

impl Drop for Sources {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

pub(crate) struct SourceReader(Arc<Mutex<State>>);

impl SourceReader {
    pub fn packets(&self, source: Source, after: u64) -> Vec<Packet> {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let track = &state.tracks[source as usize];
        if !track.enabled {
            return Vec::new();
        }
        track
            .packets
            .iter()
            .filter(|packet| packet.sequence > after)
            .cloned()
            .collect()
    }

    pub fn keyframe(&self, source: Source) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).tracks[source as usize].keyframe = true;
    }

    pub fn attendance(&self, devices: usize) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).devices = devices;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn muted_sources_discard_audio_and_do_not_affect_shared_audio() {
        let sources = Sources::default();
        let reader = sources.reader();
        sources
            .audio(Source::Microphone, [1000; FRAME_SAMPLES])
            .await
            .unwrap();
        assert!(reader.packets(Source::Microphone, 0).is_empty());
        sources.set_enabled(Source::Microphone, true);
        sources.set_enabled(Source::SharedAudio, true);
        for _ in 0..12 {
            sources
                .audio(Source::Microphone, [1000; FRAME_SAMPLES])
                .await
                .unwrap();
            sources
                .audio(Source::SharedAudio, [1000; FRAME_SAMPLES])
                .await
                .unwrap();
        }
        assert_eq!(reader.packets(Source::Microphone, 0).len(), 6);
        sources.set_enabled(Source::Microphone, false);
        sources
            .audio(Source::Microphone, [1000; FRAME_SAMPLES])
            .await
            .unwrap();
        assert!(reader.packets(Source::Microphone, 0).is_empty());
        assert_eq!(reader.packets(Source::SharedAudio, 0).len(), 6);
        sources.set_enabled(Source::Microphone, true);
        assert!(reader.packets(Source::Microphone, 0).is_empty());
    }
}
