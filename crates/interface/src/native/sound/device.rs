use super::library::{Clip, MAX_SECONDS};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

static CAPTURING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct CaptureGuard;
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURING.store(false, std::sync::atomic::Ordering::Release);
    }
}

fn acquire(cancelled: Option<&std::sync::atomic::AtomicBool>) -> Result<CaptureGuard, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if cancelled.is_some_and(|cancelled| cancelled.load(std::sync::atomic::Ordering::Acquire)) {
            return Err("Microphone start was cancelled.".into());
        }
        if CAPTURING
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
        {
            return Ok(CaptureGuard);
        }
        if cancelled.is_none() || std::time::Instant::now() >= deadline {
            return Err("Another Lince recorder is using the microphone.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

pub struct Capture {
    stream: Option<cpal::Stream>,
    samples: Arc<Mutex<Vec<f32>>>,
    pub error: Arc<Mutex<Option<String>>>,
    rate: u32,
    _guard: CaptureGuard,
}

impl Capture {
    pub fn start() -> Result<Self, String> {
        Self::start_on(None)
    }

    pub fn devices() -> Result<Vec<String>, String> {
        Ok(cpal::default_host()
            .input_devices()
            .map_err(|error| error.to_string())?
            .filter_map(|device| {
                device
                    .description()
                    .ok()
                    .map(|description| description.name().to_owned())
            })
            .take(32)
            .collect())
    }

    pub fn start_on(name: Option<&str>) -> Result<Self, String> {
        Self::open(name, acquire(None)?)
    }

    pub fn start_when_available(
        name: Option<&str>,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Self, String> {
        Self::open(name, acquire(Some(cancelled))?)
    }

    fn open(name: Option<&str>, guard: CaptureGuard) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = if let Some(name) = name {
            host.input_devices()
                .map_err(|error| error.to_string())?
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| description.name() == name)
                })
                .ok_or("The selected microphone is unavailable.")?
        } else {
            host.default_input_device()
                .ok_or("No microphone available")?
        };
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate();
        if !(8000..=192000).contains(&rate) || config.channels() == 0 || config.channels() > 8 {
            return Err("Unsupported microphone format".into());
        }
        let samples = Arc::new(Mutex::new(Vec::with_capacity(
            rate as usize * MAX_SECONDS as usize,
        )));
        let error = Arc::new(Mutex::new(None));
        let requested = stream_config(&config);
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                input::<f32>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::F64 => {
                input::<f64>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::I8 => {
                input::<i8>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::I16 => {
                input::<i16>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::U16 => {
                input::<u16>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::I32 => {
                input::<i32>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::I64 => {
                input::<i64>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::U8 => {
                input::<u8>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::U32 => {
                input::<u32>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::U64 => {
                input::<u64>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::I24 => {
                input::<cpal::I24>(&device, &requested, samples.clone(), error.clone())
            }
            cpal::SampleFormat::U24 => {
                input::<cpal::U24>(&device, &requested, samples.clone(), error.clone())
            }
            _ => return Err("Unsupported microphone sample format".into()),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _guard: guard,
            stream: Some(stream),
            samples,
            error,
            rate,
        })
    }

    pub fn full(&self) -> bool {
        self.samples.lock().unwrap().len() >= self.rate as usize * MAX_SECONDS as usize
    }

    pub fn drain(&self) -> Clip {
        let mut samples = self.samples.lock().unwrap();
        let mut collected = Vec::with_capacity(self.rate as usize / 10);
        std::mem::swap(&mut *samples, &mut collected);
        Clip {
            samples: collected,
            rate: self.rate,
        }
    }

    pub fn finish(mut self) -> Clip {
        drop(self.stream.take());
        self.drain()
    }
}

fn stream_config(supported: &cpal::SupportedStreamConfig) -> cpal::StreamConfig {
    let mut config = supported.config();
    if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size()
        && *min > 0
        && min <= max
    {
        config.buffer_size = cpal::BufferSize::Fixed(512_u32.clamp(*min, *max));
    }
    config
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
    let limit = config.sample_rate as usize * MAX_SECONDS as usize;
    device
        .build_input_stream(
            *config,
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
    cancelled: Option<Arc<std::sync::atomic::AtomicBool>>,
}

pub struct Output {
    _stream: cpal::Stream,
    voices: Arc<Mutex<Vec<Voice>>>,
    pub error: Arc<Mutex<Option<String>>>,
}

impl Output {
    pub fn open() -> Result<Self, String> {
        Self::open_on(None)
    }

    pub fn devices() -> Result<Vec<String>, String> {
        Ok(cpal::default_host()
            .output_devices()
            .map_err(|error| error.to_string())?
            .filter_map(|device| {
                device
                    .description()
                    .ok()
                    .map(|description| description.name().to_owned())
            })
            .take(32)
            .collect())
    }

    pub fn open_on(name: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = if let Some(name) = name {
            host.output_devices()
                .map_err(|error| error.to_string())?
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| description.name() == name)
                })
                .ok_or("The selected speaker is unavailable.")?
        } else {
            host.default_output_device()
                .ok_or("No sound output available")?
        };
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let voices = Arc::new(Mutex::new(Vec::with_capacity(16)));
        let error = Arc::new(Mutex::new(None));
        let requested = stream_config(&config);
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                output::<f32>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::F64 => {
                output::<f64>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::I8 => {
                output::<i8>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::I16 => {
                output::<i16>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::U16 => {
                output::<u16>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::I32 => {
                output::<i32>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::I64 => {
                output::<i64>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::U8 => {
                output::<u8>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::U32 => {
                output::<u32>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::U64 => {
                output::<u64>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::I24 => {
                output::<cpal::I24>(&device, &requested, voices.clone(), error.clone())
            }
            cpal::SampleFormat::U24 => {
                output::<cpal::U24>(&device, &requested, voices.clone(), error.clone())
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
        self.enqueue(
            Voice {
                clip,
                cursor: 0.0,
                gain: gain.clamp(0.0, 1.0),
                owner,
                cancelled: None,
            },
            replace,
        );
    }

    pub(crate) fn speak(
        &self,
        owner: bevy::prelude::Entity,
        clip: Arc<Clip>,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.enqueue(
            Voice {
                clip,
                cursor: 0.0,
                gain: 1.0,
                owner,
                cancelled: Some(cancelled),
            },
            true,
        );
    }

    fn enqueue(&self, voice: Voice, replace: bool) {
        let mut voices = self.voices.lock().unwrap();
        if replace {
            voices.retain(|current| current.owner != voice.owner);
        }
        if voices.len() < 16 {
            voices.push(voice);
        }
    }

    pub fn stop_all(&self) {
        self.voices.lock().unwrap().clear();
    }

    pub fn playing(&self, owner: bevy::prelude::Entity) -> bool {
        self.voices
            .lock()
            .unwrap()
            .iter()
            .any(|voice| voice.owner == owner)
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
    let rate = f64::from(config.sample_rate);
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                data.fill(T::from_sample(0.0));
                let Ok(mut voices) = voices.try_lock() else {
                    return;
                };
                voices.retain(|voice| {
                    !voice.cancelled.as_ref().is_some_and(|cancelled| {
                        cancelled.load(std::sync::atomic::Ordering::Acquire)
                    })
                });
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

#[cfg(test)]
mod tests {
    #[test]
    fn cancelled_listening_start_never_opens_an_audio_device() {
        let stopped = std::sync::atomic::AtomicBool::new(true);
        assert!(
            super::Capture::start_when_available(None, &stopped)
                .err()
                .is_some_and(|error| error.contains("cancelled"))
        );
    }

    #[test]
    fn audio_buffers_are_bounded_by_the_device_range_and_keep_unknown_defaults() {
        for (supported, expected) in [
            (
                cpal::SupportedBufferSize::Range {
                    min: 1,
                    max: 524288,
                },
                cpal::BufferSize::Fixed(512),
            ),
            (
                cpal::SupportedBufferSize::Range {
                    min: 1024,
                    max: 4096,
                },
                cpal::BufferSize::Fixed(1024),
            ),
            (
                cpal::SupportedBufferSize::Range { min: 64, max: 256 },
                cpal::BufferSize::Fixed(256),
            ),
            (
                cpal::SupportedBufferSize::Unknown,
                cpal::BufferSize::Default,
            ),
        ] {
            let source =
                cpal::SupportedStreamConfig::new(2, 48000, supported, cpal::SampleFormat::F32);
            let config = super::stream_config(&source);
            assert_eq!(config.channels, 2);
            assert_eq!(config.sample_rate, 48000);
            assert_eq!(config.buffer_size, expected);
        }
    }
}
