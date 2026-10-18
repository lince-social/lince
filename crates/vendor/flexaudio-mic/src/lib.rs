use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat};

use flexaudio_core::backend::{CaptureBackend, RawSink};
use flexaudio_core::clock::monotonic_now_ns;
use flexaudio_core::types::{DeviceInfo, Error, Result, SourceKind};

#[cfg(windows)]
mod windows_keeper;

const FALLBACK_FORMAT: (u32, u16) = (48_000, 1);

fn cpal_default_host() -> Result<cpal::Host> {
    #[cfg(windows)]
    windows_keeper::ensure()?;

    Ok(cpal::default_host())
}

pub struct CpalMicBackend {
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    native: (u32, u16),
    device_id: Option<String>,
}

impl CpalMicBackend {
    pub fn new(device_id: Option<String>) -> Self {
        let native = query_native_format(device_id.as_deref()).unwrap_or(FALLBACK_FORMAT);
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
            native,
            device_id,
        }
    }
}

impl Default for CpalMicBackend {
    fn default() -> Self {
        Self::new(None)
    }
}

fn resolve_input_device(host: &cpal::Host, device_id: Option<&str>) -> Result<Device> {
    match device_id {
        None => host.default_input_device().ok_or(Error::DeviceNotFound),
        Some(id) => {
            let devices = host.input_devices().map_err(|_| Error::DeviceNotFound)?;
            for device in devices {
                if let Ok(name) = device
                    .description()
                    .map(|description| description.name().to_owned())
                {
                    if name == id {
                        return Ok(device);
                    }
                }
            }
            Err(Error::DeviceNotFound)
        }
    }
}

fn query_native_format(device_id: Option<&str>) -> Option<(u32, u16)> {
    let host = cpal_default_host().ok()?;
    let device = resolve_input_device(&host, device_id).ok()?;
    let config = device.default_input_config().ok()?;
    Some((config.sample_rate(), config.channels()))
}

pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal_default_host()?;

    let default_name = host.default_input_device().and_then(|d| {
        d.description()
            .map(|description| description.name().to_owned())
            .ok()
    });

    let devices = match host.input_devices() {
        Ok(it) => it,
        Err(_) => return Ok(Vec::new()),
    };

    let mut out = Vec::new();
    for device in devices {
        let Ok(name) = device
            .description()
            .map(|description| description.name().to_owned())
        else {
            continue;
        };
        let Ok(config) = device.default_input_config() else {
            continue;
        };
        let is_default = default_name.as_deref() == Some(name.as_str());
        out.push(DeviceInfo {
            id: name.clone(),
            name,
            source_kind: SourceKind::Mic,
            sample_rate: config.sample_rate(),
            channels: config.channels(),
            is_loopback: false,
            is_default,
        });
    }
    Ok(out)
}

impl CaptureBackend for CpalMicBackend {
    fn native_format(&self) -> (u32, u16) {
        self.native
    }

    fn start(&mut self, sink: RawSink) -> Result<()> {
        if self.handle.is_some() {
            return Ok(());
        }
        self.stop_flag.store(false, Ordering::SeqCst);

        let stop_flag = self.stop_flag.clone();
        let device_id = self.device_id.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

        let handle = thread::Builder::new()
            .name("flexaudio-mic-cpal".into())
            .spawn(move || {
                run_capture_thread(sink, device_id, stop_flag, ready_tx);
            })
            .map_err(|e| Error::Backend(format!("spawn cpal mic thread: {e}")))?;

        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.handle = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                let _ = handle.join();
                Err(Error::Backend(
                    "cpal mic thread exited before reporting readiness".into(),
                ))
            }
        }
    }

    fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            h.thread().unpark();
            let _ = h.join();
        }
    }
}

impl Drop for CpalMicBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_capture_thread(
    sink: RawSink,
    device_id: Option<String>,
    stop_flag: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<Result<()>>,
) {
    let stream = match build_stream(sink, device_id.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.send(Err(e));
            return;
        }
    };

    if let Err(e) = stream.play() {
        let _ = ready_tx.send(Err(Error::Backend(format!("cpal play: {e}"))));
        return;
    }

    let _ = ready_tx.send(Ok(()));

    while !stop_flag.load(Ordering::SeqCst) {
        thread::park();
    }
    drop(stream);
}

const MAX_SCRATCH_SECONDS: usize = 1;

#[inline]
fn fill_scratch<T: Copy>(scratch: &mut Vec<f32>, data: &[T], convert: impl Fn(T) -> f32) {
    let n = data.len();
    if n > scratch.capacity() {
        scratch.reserve(n - scratch.capacity());
    }
    scratch.clear();
    for &s in data {
        scratch.push(convert(s));
    }
}

const PRIMING_PEAK_LIMIT: f32 = 1.001;

struct TransientGuard {
    latched: bool,
}

impl TransientGuard {
    fn new() -> Self {
        Self { latched: false }
    }

    fn should_drop(&mut self, data: &[f32]) -> bool {
        if self.latched || data.is_empty() {
            self.latched = true;
            return false;
        }
        let mut peak = 0.0f32;
        for &s in data {
            let a = s.abs();
            if a > peak {
                peak = a;
            }
        }
        let is_transient = peak > PRIMING_PEAK_LIMIT;
        if !is_transient {
            self.latched = true;
        }
        is_transient
    }
}

fn build_stream(sink: RawSink, device_id: Option<&str>) -> Result<cpal::Stream> {
    let host = cpal_default_host()?;
    let device = resolve_input_device(&host, device_id)?;
    let supported = device
        .default_input_config()
        .map_err(|_| Error::DeviceNotFound)?;
    let format = supported.sample_format();
    let mut config = supported.config();
    if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
        if *min > 0 && min <= max {
            config.buffer_size = cpal::BufferSize::Fixed(512_u32.clamp(*min, *max));
        }
    }
    let stream = match format {
        SampleFormat::F32 => input::<f32>(&device, config, sink),
        SampleFormat::F64 => input::<f64>(&device, config, sink),
        SampleFormat::I8 => input::<i8>(&device, config, sink),
        SampleFormat::I16 => input::<i16>(&device, config, sink),
        SampleFormat::I24 => input::<cpal::I24>(&device, config, sink),
        SampleFormat::I32 => input::<i32>(&device, config, sink),
        SampleFormat::I64 => input::<i64>(&device, config, sink),
        SampleFormat::U8 => input::<u8>(&device, config, sink),
        SampleFormat::U16 => input::<u16>(&device, config, sink),
        SampleFormat::U24 => input::<cpal::U24>(&device, config, sink),
        SampleFormat::U32 => input::<u32>(&device, config, sink),
        SampleFormat::U64 => input::<u64>(&device, config, sink),
        other => {
            return Err(Error::Backend(format!(
                "unsupported cpal sample format: {other:?}"
            )));
        }
    };
    stream.map_err(|error| Error::Backend(format!("build_input_stream: {error}")))
}

fn input<T>(
    device: &Device,
    config: cpal::StreamConfig,
    mut sink: RawSink,
) -> std::result::Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let capacity = (config.sample_rate as usize)
        .saturating_mul(config.channels as usize)
        .saturating_mul(MAX_SCRATCH_SECONDS)
        .max(1);
    let mut scratch = Vec::with_capacity(capacity);
    let mut guard = TransientGuard::new();
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                fill_scratch(&mut scratch, data, <f32 as cpal::Sample>::from_sample);
                if !guard.should_drop(&scratch) {
                    sink.push(&scratch, monotonic_now_ns());
                }
            }));
        },
        |_| {},
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use flexaudio_core::raw_ring;

    fn make_buf(frames: usize, peak: f32) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = if i % 2 == 0 { peak } else { -peak };
            v.push(s);
            v.push(s);
        }
        v
    }

    #[test]
    fn transient_guard_drops_priming_then_latches_open() {
        let frames = 1024;
        let mut g = TransientGuard::new();

        assert!(g.should_drop(&make_buf(frames, 3.3)));
        assert!(g.should_drop(&make_buf(frames, 1.5)));
        assert!(!g.should_drop(&make_buf(frames, 0.88)));
        assert!(!g.should_drop(&make_buf(frames, 3.3)));
    }

    #[test]
    fn transient_guard_passes_clean_audio_from_the_start() {
        let frames = 1024;
        let mut g = TransientGuard::new();
        assert!(!g.should_drop(&make_buf(frames, 1.0)));
        assert!(!g.should_drop(&make_buf(frames, 0.5)));
        assert!(!g.should_drop(&vec![0.0f32; frames * 2]));
    }

    #[test]
    fn transient_guard_handles_empty_buffer() {
        let mut g = TransientGuard::new();
        assert!(!g.should_drop(&[]));
        assert!(!g.should_drop(&make_buf(1024, 3.3)));
    }

    #[test]
    fn fill_scratch_no_realloc_in_steady_state() {
        let cap = 480 * 2;
        let mut scratch: Vec<f32> = Vec::with_capacity(cap);
        let before = scratch.capacity();

        let data: Vec<i16> = (0..cap as i16).collect();
        for _ in 0..100 {
            fill_scratch(&mut scratch, &data, |s| s as f32 / -(i16::MIN as f32));
            assert_eq!(scratch.len(), data.len());
            assert_eq!(scratch.capacity(), before, "定常状態で容量拡大しない");
        }
        let mut one = Vec::with_capacity(1);
        fill_scratch(&mut one, &[i16::MIN], |s| s as f32 / -(i16::MIN as f32));
        assert_eq!(one[0], -1.0);
    }

    #[test]
    fn new_and_native_format_do_not_panic() {
        let backend = CpalMicBackend::new(None);
        let (rate, channels) = backend.native_format();
        assert!(rate > 0);
        assert!(channels > 0);

        let backend = CpalMicBackend::new(Some("__no_such_device__".into()));
        let (rate, channels) = backend.native_format();
        assert_eq!((rate, channels), FALLBACK_FORMAT);
    }

    #[test]
    fn start_with_unknown_device_id_yields_device_not_found() {
        let mut backend = CpalMicBackend::new(Some("__no_such_device__".into()));
        let (rate, channels) = backend.native_format();
        let cap = (rate as usize * channels as usize).max(1);
        let (prod, _cons) = raw_ring(cap);
        let sink = RawSink::new(prod, rate, channels);

        match backend.start(sink) {
            Err(Error::DeviceNotFound) => {}
            other => panic!("unknown device_id は DeviceNotFound であるべき: {other:?}"),
        }
    }

    #[test]
    fn list_devices_never_panics_and_is_consistent() {
        let devices = list_devices().expect("list_devices は Err を返さない設計");
        for d in &devices {
            assert_eq!(d.source_kind, SourceKind::Mic);
            assert!(!d.is_loopback, "マイクはループバックではない");
            assert_eq!(d.id, d.name);
            assert!(!d.id.is_empty(), "id（=name）は空でない");
            assert!(d.sample_rate > 0);
            assert!(d.channels > 0);
        }
        assert!(devices.iter().filter(|d| d.is_default).count() <= 1);
    }

    #[test]
    fn start_then_stop_tolerates_missing_device() {
        let mut backend = CpalMicBackend::new(None);
        let (rate, channels) = backend.native_format();
        let cap = (rate as usize * channels as usize).max(1);
        let (prod, _cons) = raw_ring(cap);
        let sink = RawSink::new(prod, rate, channels);

        match backend.start(sink) {
            Ok(()) => {
                backend.stop();
                backend.stop();
            }
            Err(Error::DeviceNotFound) => {}
            Err(other) => panic!("unexpected error from start(): {other:?}"),
        }
    }

    #[test]
    #[ignore = "実マイク必須。ラップトップで `cargo test -p flexaudio-mic -- --ignored` で実行"]
    fn end_to_end_captures_real_audio() {
        use std::time::Duration;

        let mut backend = CpalMicBackend::new(None);
        let (rate, channels) = backend.native_format();
        let cap = rate as usize * channels as usize * 2;
        let (prod, mut cons) = raw_ring(cap);
        let sink = RawSink::new(prod, rate, channels);

        backend
            .start(sink)
            .expect("start() should succeed with a real input device");

        thread::sleep(Duration::from_millis(500));
        backend.stop();

        let mut buf = vec![0.0f32; cap];
        let got = cons.pop_slice(&mut buf);
        assert!(got > 0, "expected captured samples, got none");
        assert!(buf[..got].iter().all(|&s| (-1.5..=1.5).contains(&s)));
    }
}
