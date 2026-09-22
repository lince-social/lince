use super::library::{Clip, MAX_SECONDS};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct Capture {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    pub error: Arc<Mutex<Option<String>>>,
    rate: u32,
}

impl Capture {
    pub fn start() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("No microphone available")?;
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate().0;
        if !(8000..=192000).contains(&rate) || config.channels() == 0 || config.channels() > 8 {
            return Err("Unsupported microphone format".into());
        }
        let samples = Arc::new(Mutex::new(Vec::with_capacity(
            rate as usize * MAX_SECONDS as usize,
        )));
        let error = Arc::new(Mutex::new(None));
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                input::<f32>(&device, &config.into(), samples.clone(), error.clone())
            }
            cpal::SampleFormat::I16 => {
                input::<i16>(&device, &config.into(), samples.clone(), error.clone())
            }
            cpal::SampleFormat::U16 => {
                input::<u16>(&device, &config.into(), samples.clone(), error.clone())
            }
            cpal::SampleFormat::I32 => {
                input::<i32>(&device, &config.into(), samples.clone(), error.clone())
            }
            _ => return Err("Unsupported microphone sample format".into()),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            stream,
            samples,
            error,
            rate,
        })
    }

    pub fn full(&self) -> bool {
        self.samples.lock().unwrap().len() >= self.rate as usize * MAX_SECONDS as usize
    }

    pub fn finish(self) -> Clip {
        drop(self.stream);
        let samples = std::mem::take(&mut *self.samples.lock().unwrap());
        Clip {
            samples,
            rate: self.rate,
        }
    }
}

fn input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = config.channels as usize;
    let limit = config.sample_rate.0 as usize * MAX_SECONDS as usize;
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let Ok(mut samples) = samples.try_lock() else {
                    return;
                };
                for frame in data.chunks_exact(channels) {
                    if samples.len() >= limit {
                        break;
                    }
                    let sample = frame
                        .iter()
                        .map(|s| <f32 as cpal::Sample>::from_sample(*s))
                        .sum::<f32>()
                        / channels as f32;
                    samples.push(if sample.is_finite() {
                        sample.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    });
                }
            },
            move |e| {
                *error.lock().unwrap() = Some(e.to_string());
            },
            None,
        )
        .map_err(|e| e.to_string())
}

struct Voice {
    clip: Arc<Clip>,
    cursor: f64,
    gain: f32,
    owner: bevy::prelude::Entity,
}

pub struct Output {
    _stream: cpal::Stream,
    voices: Arc<Mutex<Vec<Voice>>>,
    pub error: Arc<Mutex<Option<String>>>,
}

impl Output {
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("No sound output available")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let voices = Arc::new(Mutex::new(Vec::with_capacity(16)));
        let error = Arc::new(Mutex::new(None));
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                output::<f32>(&device, &config.into(), voices.clone(), error.clone())
            }
            cpal::SampleFormat::I16 => {
                output::<i16>(&device, &config.into(), voices.clone(), error.clone())
            }
            cpal::SampleFormat::U16 => {
                output::<u16>(&device, &config.into(), voices.clone(), error.clone())
            }
            cpal::SampleFormat::I32 => {
                output::<i32>(&device, &config.into(), voices.clone(), error.clone())
            }
            _ => return Err("Unsupported output sample format".into()),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            voices,
            error,
        })
    }

    pub fn play(&self, owner: bevy::prelude::Entity, clip: Arc<Clip>, gain: f32, replace: bool) {
        let mut voices = self.voices.lock().unwrap();
        if replace {
            voices.retain(|voice| voice.owner != owner);
        }
        if voices.len() < 16 {
            voices.push(Voice {
                clip,
                cursor: 0.0,
                gain: gain.clamp(0.0, 1.0),
                owner,
            });
        }
    }

    pub fn stop(&self, owner: bevy::prelude::Entity) {
        self.voices
            .lock()
            .unwrap()
            .retain(|voice| voice.owner != owner);
    }
}

fn output<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    voices: Arc<Mutex<Vec<Voice>>>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let rate = f64::from(config.sample_rate.0);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                data.fill(T::from_sample(0.0));
                let Ok(mut voices) = voices.try_lock() else {
                    return;
                };
                for frame in data.chunks_exact_mut(channels) {
                    let mut mixed = 0.0;
                    for voice in voices.iter_mut() {
                        let index = voice.cursor as usize;
                        if let Some(&a) = voice.clip.samples.get(index) {
                            let b = voice.clip.samples.get(index + 1).copied().unwrap_or(a);
                            mixed += (a + (b - a) * voice.cursor.fract() as f32) * voice.gain;
                            voice.cursor += f64::from(voice.clip.rate) / rate;
                        }
                    }
                    frame.fill(T::from_sample(mixed.clamp(-1.0, 1.0)));
                }
                voices.retain(|voice| (voice.cursor as usize) < voice.clip.samples.len());
            },
            move |e| {
                *error.lock().unwrap() = Some(e.to_string());
            },
            None,
        )
        .map_err(|e| e.to_string())
}
