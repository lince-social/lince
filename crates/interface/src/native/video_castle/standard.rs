use super::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    collections::VecDeque,
    io::Read,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Cancellation(Arc<AtomicBool>);
impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn decoder(input: &str, start: u64, end: Option<u64>) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-max_alloc",
            "268435456",
            "-rw_timeout",
            "10000000",
            "-ss",
        ])
        .arg(format!("{:.3}", start as f64 / 1000.0))
        .args(["-i", input]);
    if let Some(end) = end {
        command.args([
            "-t",
            &format!("{:.3}", end.saturating_sub(start) as f64 / 1000.0),
        ]);
    }
    command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped());
    command
}

fn stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    samples: Arc<Mutex<VecDeque<f32>>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                if let Ok(mut samples) = samples.try_lock() {
                    for value in output {
                        *value = T::from_sample(samples.pop_front().unwrap_or(0.0));
                    }
                } else {
                    output.fill(T::from_sample(0.0));
                }
            },
            |_| {},
            None,
        )
        .map_err(|error| error.to_string())
}

fn audio(
    input: String,
    start: u64,
    end: Option<u64>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("Audio output is unavailable")?;
    let config = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let rate = config.sample_rate();
    let channels = config.channels();
    if channels == 0 || channels > 8 || rate > 192000 {
        return Err("Choose a supported audio output".into());
    }
    let samples = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let playback = match config.sample_format() {
        cpal::SampleFormat::F32 => stream::<f32>(&device, config.config(), samples.clone()),
        cpal::SampleFormat::I16 => stream::<i16>(&device, config.config(), samples.clone()),
        cpal::SampleFormat::U16 => stream::<u16>(&device, config.config(), samples.clone()),
        _ => Err("Unsupported audio output format".into()),
    }?;
    let mut command = decoder(&input, start, end);
    command.args([
        "-vn",
        "-f",
        "f32le",
        "-acodec",
        "pcm_f32le",
        "-ar",
        &rate.to_string(),
        "-ac",
        &channels.to_string(),
        "pipe:1",
    ]);
    let mut process = Process(
        command
            .spawn()
            .map_err(|error| format!("FFmpeg audio: {error}"))?,
    );
    let mut source = process
        .0
        .stdout
        .take()
        .ok_or("The audio decoder has no output")?;
    let mut bytes = [0; 4096];
    let mut started = false;
    let mut pending = Vec::new();
    while !cancel.load(Ordering::Relaxed) {
        let count = source.read(&mut bytes).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        pending.extend_from_slice(&bytes[..count]);
        while samples.lock().map_err(|error| error.to_string())?.len()
            > rate as usize * channels as usize / 4
        {
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let complete = pending.len() / 4 * 4;
        samples.lock().map_err(|error| error.to_string())?.extend(
            pending[..complete].chunks_exact(4).map(|sample| {
                let value = f32::from_le_bytes(sample.try_into().unwrap());
                if value.is_finite() {
                    value.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            }),
        );
        pending.drain(..complete);
        if !started {
            playback.play().map_err(|error| error.to_string())?;
            started = true;
        }
    }
    while !cancel.load(Ordering::Relaxed)
        && !samples
            .lock()
            .map_err(|error| error.to_string())?
            .is_empty()
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

pub(super) async fn play(
    settings: Settings,
    output: Reply,
    wake: Option<crate::native::wake::WakeSignal>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || play_blocking(settings, output, wake, cancel))
        .await
        .map_err(|error| error.to_string())?
}

fn play_blocking(
    settings: Settings,
    output: Reply,
    wake: Option<crate::native::wake::WakeSignal>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let _cancellation = Cancellation(cancel.clone());
    let library = engine::sources::library().map_err(|error| error.to_string())?;
    let resource = if let Some(resource) = settings.resource.clone() {
        resource
    } else {
        lince_resources::Resource {
            uri: lince_resources::address(&settings.source)?.to_string(),
            media_type: "video/*".into(),
            ..default()
        }
    };
    let retained = lince_resources::video::retained(&library, &resource)?;
    let server = if retained.is_none() {
        Some(lince_resources::MediaInput::open(&library, &resource)?)
    } else {
        None
    };
    let origin = resource.locator.start_ms.unwrap_or(0);
    let end = resource.locator.end_ms;
    let mut start = settings.position_ms.max(origin);
    if end.is_some_and(|end| start >= end) {
        start = origin;
    }
    let base = if retained.is_some() { origin } else { 0 };
    let input = retained.as_ref().map_or_else(
        || server.as_ref().unwrap().uri.clone(),
        |entry| entry.path.to_string_lossy().into_owned(),
    );
    let failure = Arc::new(Mutex::new(None));
    let audio_worker = if settings.muted {
        None
    } else {
        let uri = input.clone();
        let cancel = cancel.clone();
        let failure = failure.clone();
        Some(std::thread::spawn(move || {
            if let Err(error) = audio(uri, start - base, end.map(|end| end - base), cancel) {
                *failure.lock().unwrap() = Some(error);
            }
        }))
    };
    let mut command = decoder(&input, start - base, end.map(|end| end - base));
    command.args([
        "-an",
        "-vf",
        "scale=640:360:force_original_aspect_ratio=decrease,pad=640:360:(ow-iw)/2:(oh-ih)/2,fps=24",
        "-pix_fmt",
        "rgba",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    let mut process = Process(
        command
            .spawn()
            .map_err(|error| format!("FFmpeg is required for standard video: {error}"))?,
    );
    let mut source = process
        .0
        .stdout
        .take()
        .ok_or("The video decoder has no output")?;
    let clock = std::time::Instant::now();
    let mut index = 0u64;
    while !cancel.load(Ordering::Relaxed) {
        let mut pixels = vec![0; 640 * 360 * 4];
        match source.read_exact(&mut pixels) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.to_string()),
        }
        let at = Duration::from_micros(index * 1_000_000 / 24);
        while clock.elapsed() < at && !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(
                at.saturating_sub(clock.elapsed())
                    .min(Duration::from_millis(10)),
            );
        }
        let audio = failure
            .lock()
            .unwrap()
            .as_ref()
            .map(|error| format!(" · {error}"))
            .unwrap_or_default();
        publish(
            &output,
            Frame {
                pixels: Some(crate::native::media_sand::Pixels {
                    width: 640,
                    height: 360,
                    rgba: pixels,
                }),
                status: format!(
                    "Video · {:.1}s{audio}",
                    start as f64 / 1000.0 + at.as_secs_f64()
                ),
                position_ms: Some(start + at.as_millis() as u64),
                skipped: 0,
            },
            &wake,
        );
        index += 1;
    }
    cancel.store(true, Ordering::Relaxed);
    if let Some(worker) = audio_worker {
        let _ = worker.join();
    }
    if index == 0 {
        return Err("The video source has no readable frames".into());
    }
    publish(
        &output,
        Frame {
            pixels: None,
            status: "End of referenced video interval".into(),
            position_ms: end.or(Some(start + index * 1000 / 24)),
            skipped: 0,
        },
        &wake,
    );
    Ok(())
}
