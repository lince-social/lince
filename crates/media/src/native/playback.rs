use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::error;
use crate::audio::{AudioFrame, FRAME_SAMPLES, Mixer, QUEUE_FRAMES, SAMPLE_RATE, finite};
use crate::{MediaError, Result};

#[derive(Clone, Default)]
pub struct Speaker {
    mixer: Arc<Mutex<Mixer>>,
    played: Arc<Mutex<VecDeque<AudioFrame>>>,
    volume: Arc<AtomicU32>,
    failure: Arc<Mutex<Option<String>>>,
}

impl Speaker {
    pub fn add(&self, track: &str) -> Result<()> {
        self.mixer.lock().map_err(error)?.add(track)
    }

    pub fn remove(&self, track: &str) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.remove(track);
        }
    }

    pub fn push(&self, track: &str, frame: AudioFrame) -> Result<()> {
        self.mixer.lock().map_err(error)?.push(track, frame)
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(finite(volume).clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn take_played(&self) -> Option<AudioFrame> {
        self.played.lock().ok()?.pop_front()
    }

    pub fn take_error(&self) -> Option<String> {
        self.failure.lock().ok()?.take()
    }
}

pub struct Playback {
    _stream: cpal::Stream,
    pub speaker: Speaker,
}

pub fn devices() -> Result<Vec<String>> {
    cpal::default_host()
        .output_devices()
        .map_err(error)?
        .map(|device| device.name().map_err(error))
        .collect()
}

impl Playback {
    pub fn start(device_name: Option<&str>) -> Result<Self> {
        let speaker = Speaker::default();
        speaker.set_volume(1.0);
        Self::with_speaker(device_name, speaker)
    }

    pub fn with_speaker(device_name: Option<&str>, speaker: Speaker) -> Result<Self> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            host.output_devices()
                .map_err(error)?
                .find(|device| device.name().as_deref() == Ok(name))
        } else {
            host.default_output_device()
        }
        .ok_or_else(|| MediaError("The selected speaker is unavailable".into()))?;
        let config = device
            .supported_output_configs()
            .map_err(error)?
            .filter(|config| config.channels() > 0 && config.channels() <= 8)
            .find(|config| {
                config.min_sample_rate().0 <= SAMPLE_RATE
                    && config.max_sample_rate().0 >= SAMPLE_RATE
                    && matches!(
                        config.sample_format(),
                        cpal::SampleFormat::F32 | cpal::SampleFormat::I16 | cpal::SampleFormat::U16
                    )
            })
            .ok_or_else(|| {
                MediaError("The selected speaker does not support 48 kHz playback".into())
            })?
            .with_sample_rate(cpal::SampleRate(SAMPLE_RATE));
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => output::<f32>(&device, &config.config(), speaker.clone()),
            cpal::SampleFormat::I16 => output::<i16>(&device, &config.config(), speaker.clone()),
            cpal::SampleFormat::U16 => output::<u16>(&device, &config.config(), speaker.clone()),
            _ => return Err(MediaError("Unsupported speaker format".into())),
        }?;
        stream.play().map_err(error)?;
        Ok(Self {
            _stream: stream,
            speaker,
        })
    }
}

fn output<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    speaker: Speaker,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let failure = speaker.failure.clone();
    let channels = config.channels as usize;
    let mut frame = AudioFrame::silence();
    let mut cursor = FRAME_SAMPLES;
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                for sample in output.chunks_mut(channels) {
                    if cursor == FRAME_SAMPLES {
                        if let Ok(mut played) = speaker.played.try_lock() {
                            if played.len() == QUEUE_FRAMES {
                                played.pop_front();
                            }
                            played.push_back(frame.clone());
                        }
                        frame = speaker
                            .mixer
                            .try_lock()
                            .map(|mut mixer| {
                                mixer.render(f32::from_bits(speaker.volume.load(Ordering::Relaxed)))
                            })
                            .unwrap_or_else(|_| AudioFrame::silence());
                        cursor = 0;
                    }
                    sample.fill(T::from_sample(frame.0[cursor]));
                    cursor += 1;
                }
            },
            move |err| {
                if let Ok(mut failure) = failure.try_lock() {
                    *failure = Some(err.to_string());
                }
            },
            None,
        )
        .map_err(error)
}
