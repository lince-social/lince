use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType};
use scrcap::CaptureDescriptor;
use sonora::{AudioProcessing, Config, StreamConfig};

use super::error;
use crate::audio::{AudioFrame, FRAME_SAMPLES, SAMPLE_RATE};
use crate::video::{LatestVideo, MAX_HEIGHT, MAX_WIDTH, VideoFrame};
use crate::{MediaError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioInput {
    Microphone(Option<String>),
    System(Option<String>),
    Application(u32),
}

pub fn audio_config(input: &AudioInput) -> Result<flexaudio::StreamConfig> {
    let mut config = flexaudio::StreamConfig {
        chunk_ms: 20,
        ring_capacity_chunks: 3,
        exclude_self: true,
        output: flexaudio::OutputFormat {
            sample_rate: SAMPLE_RATE,
            channels: 1,
        },
        ..Default::default()
    };
    match input {
        AudioInput::Microphone(device) => {
            config.kind = flexaudio::SourceKind::Mic;
            config.device_id = device.clone();
        }
        AudioInput::System(device) => {
            config.kind = flexaudio::SourceKind::SystemLoopback;
            config.device_id = device.clone();
        }
        AudioInput::Application(pid) => {
            if *pid == 0 || *pid == std::process::id() {
                return Err(MediaError("Choose another application's audio".into()));
            }
            config.kind = flexaudio::SourceKind::ProcessLoopback;
            config.target_pid = Some(*pid);
        }
    }
    Ok(config)
}

pub struct AudioCapture {
    stream: flexaudio::Stream,
}

impl AudioCapture {
    pub fn start(input: AudioInput) -> Result<Self> {
        let mut stream = flexaudio::open(audio_config(&input)?).map_err(error)?;
        stream.start().map_err(error)?;
        Ok(Self { stream })
    }

    pub fn poll(&mut self) -> Result<Option<[AudioFrame; 2]>> {
        for _ in 0..16 {
            match self.stream.poll_event() {
                Some(flexaudio::Event::PermissionDenied) => {
                    return Err(MediaError("Audio capture permission was denied".into()));
                }
                Some(flexaudio::Event::DeviceLost) => {
                    return Err(MediaError(
                        "The audio device was removed; select another device".into(),
                    ));
                }
                Some(flexaudio::Event::StreamStalled) => {
                    return Err(MediaError(
                        "Audio capture stopped; select the source again".into(),
                    ));
                }
                Some(flexaudio::Event::Error(message)) => return Err(MediaError(message)),
                None => break,
                _ => {}
            }
        }
        let Some(chunk) = self.stream.poll_chunk() else {
            return Ok(None);
        };
        if chunk.data.len() != FRAME_SAMPLES * 2 {
            return Err(MediaError("Unexpected audio capture format".into()));
        }
        let mut frames = [AudioFrame::silence(), AudioFrame::silence()];
        frames[0].0.copy_from_slice(&chunk.data[..FRAME_SAMPLES]);
        frames[1].0.copy_from_slice(&chunk.data[FRAME_SAMPLES..]);
        Ok(Some(frames))
    }

    pub fn event(&mut self) -> Option<flexaudio::Event> {
        self.stream.poll_event()
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stream.stop();
    }
}

pub struct EchoProcessor(AudioProcessing);

impl Default for EchoProcessor {
    fn default() -> Self {
        let config = Config {
            echo_canceller: Some(Default::default()),
            noise_suppression: Some(Default::default()),
            gain_controller2: Some(Default::default()),
            ..Default::default()
        };
        let stream = StreamConfig::new(SAMPLE_RATE, 1);
        Self(
            AudioProcessing::builder()
                .config(config)
                .capture_config(stream)
                .render_config(stream)
                .build(),
        )
    }
}

impl EchoProcessor {
    pub fn speaker(&mut self, frame: &AudioFrame) -> Result<()> {
        let mut output = [0.0; FRAME_SAMPLES];
        self.0
            .process_render_f32(&[&frame.0], &mut [&mut output])
            .map_err(error)
    }

    pub fn microphone(
        &mut self,
        frame: &AudioFrame,
        delay_ms: u32,
    ) -> Result<[i16; FRAME_SAMPLES]> {
        self.0
            .set_stream_delay_ms(delay_ms.min(500) as i32)
            .map_err(error)?;
        let mut output = AudioFrame::silence();
        self.0
            .process_capture_f32(&[&frame.0], &mut [&mut output.0])
            .map_err(error)?;
        Ok(output.pcm())
    }
}

pub fn cameras() -> Result<Vec<(CameraIndex, String)>> {
    nokhwa::query(ApiBackend::Auto)
        .map_err(error)
        .map(|devices| {
            devices
                .into_iter()
                .map(|device| (device.index().clone(), device.human_name()))
                .collect()
        })
}

pub struct CameraCapture(nokhwa::Camera);

impl CameraCapture {
    pub fn start(index: CameraIndex) -> Result<Self> {
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
            nokhwa::utils::CameraFormat::new(
                nokhwa::utils::Resolution::new(640, 360),
                nokhwa::utils::FrameFormat::YUYV,
                15,
            ),
        ));
        let mut camera = nokhwa::Camera::new(index, requested).map_err(error)?;
        let resolution = camera.resolution();
        if resolution.width() > MAX_WIDTH || resolution.height() > MAX_HEIGHT {
            return Err(MediaError(
                "Choose a camera mode at or below 1920 × 1080".into(),
            ));
        }
        camera.open_stream().map_err(error)?;
        Ok(Self(camera))
    }

    pub fn frame(&mut self) -> Result<VideoFrame> {
        let buffer = self.0.frame().map_err(error)?;
        let image = buffer.decode_image::<RgbFormat>().map_err(error)?;
        let mut rgba = Vec::with_capacity(image.width() as usize * image.height() as usize * 4);
        for rgb in image.as_raw().chunks_exact(3) {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        VideoFrame::new(image.width(), image.height(), rgba)
    }
}

impl Drop for CameraCapture {
    fn drop(&mut self) {
        let _ = self.0.stop_stream();
    }
}

pub struct ScreenCapture {
    running: Arc<AtomicBool>,
    frames: LatestVideo,
    failure: Arc<Mutex<Option<MediaError>>>,
    started: Instant,
}

impl ScreenCapture {
    pub fn start(id: Option<u64>) -> Result<Self> {
        let frames = LatestVideo::default();
        let output = frames.clone();
        let failure = Arc::new(Mutex::new(None));
        let errors = failure.clone();
        let running = Arc::new(AtomicBool::new(true));
        let active = running.clone();
        std::thread::Builder::new()
            .name("lince-screen".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    #[cfg(target_os = "linux")]
                    if super::x11::available() {
                        let capture = super::x11::Capture::start(id)?;
                        while active.load(Ordering::Acquire) {
                            output.publish(capture.frame()?);
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        return Ok(());
                    }
                    #[cfg(target_os = "windows")]
                    let target = scrcap::Target::Monitor(
                        id.ok_or_else(|| MediaError("Select a screen".into()))? as isize,
                    );
                    #[cfg(not(target_os = "windows"))]
                    let target = {
                        if id.is_some() {
                            return Err(MediaError("Choose a source in the system picker".into()));
                        }
                        scrcap::Target::Pick
                    };
                    let capture = scrcap::CaptureConfig {
                        video: scrcap::VideoConfig {
                            channel_capacity: 1,
                            hide: Vec::new(),
                            target,
                            fps: Some(10),
                        },
                        audio: None,
                    }
                    .create()
                    .map_err(error)?;
                    let result = (|| -> Result<()> {
                        while active.load(Ordering::Acquire) {
                            match capture
                                .video()
                                .recv_timeout(std::time::Duration::from_millis(100))
                            {
                                Ok(frame) => output.publish(screen_frame(frame)?),
                                Err(err) if err.is_timeout() => {}
                                Err(_) => {
                                    return Err(MediaError(
                                        "Screen sharing was cancelled or interrupted".into(),
                                    ));
                                }
                            }
                        }
                        Ok(())
                    })();
                    capture.terminate();
                    result
                })();
                if let Err(err) = result {
                    *errors.lock().unwrap_or_else(|e| e.into_inner()) = Some(err);
                }
            })
            .map_err(error)?;
        Ok(Self {
            running,
            frames,
            failure,
            started: Instant::now(),
        })
    }

    pub fn frame(&mut self) -> Result<Option<VideoFrame>> {
        if let Some(error) = self
            .failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            return Err(error);
        }
        let frame = self.frames.take();
        if frame.is_some() {
            self.started = Instant::now();
        }
        if self.started.elapsed() > std::time::Duration::from_secs(30) {
            return Err(MediaError(
                "Screen selection timed out; choose a screen again".into(),
            ));
        }
        Ok(frame)
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.started.elapsed()
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

pub fn screens() -> Result<Vec<(u64, String)>> {
    #[cfg(target_os = "linux")]
    if super::x11::available() {
        return super::x11::screens();
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::{LPARAM, RECT};
        use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
        use windows_sys::core::BOOL;
        unsafe extern "system" fn count(_: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            unsafe {
                *(data as *mut usize) += 1;
            }
            1
        }
        let mut total = 0usize;
        let success = unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(count),
                (&mut total as *mut usize) as LPARAM,
            )
        };
        if success == 0 {
            return Err(MediaError("Screen enumeration failed".into()));
        }
        Ok((0..total.min(32))
            .map(|index| (index as u64, format!("Screen {}", index + 1)))
            .collect())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

fn screen_frame(frame: scrcap::VideoFrame) -> Result<VideoFrame> {
    use scrcap::PixFmt;
    let (width, height) = frame.size;
    let mut data = frame.vframe;
    let order = match frame.pix_fmt {
        PixFmt::Bgra | PixFmt::Bgr0 => [0, 1, 2],
        PixFmt::Rgba | PixFmt::Rgb0 => [2, 1, 0],
        PixFmt::Argb => [3, 2, 1],
        PixFmt::Abgr => [1, 2, 3],
        _ => return Err(MediaError("Unsupported screen pixel format".into())),
    };
    for pixel in data.chunks_exact_mut(4) {
        let original = [pixel[0], pixel[1], pixel[2], pixel[3]];
        for i in 0..3 {
            pixel[i] = original[order[i]];
        }
    }
    desktop_frame(width as i32, height as i32, width.saturating_mul(4), &data)
}

fn desktop_frame(width: i32, height: i32, stride: u32, data: &[u8]) -> Result<VideoFrame> {
    if width <= 0
        || height <= 0
        || width > 16384
        || height > 16384
        || stride < width as u32 * 4
        || data.len() < stride as usize * height as usize
    {
        return Err(MediaError("Invalid screen capture frame".into()));
    }
    let ratio = (MAX_WIDTH as f64 / width as f64)
        .min(MAX_HEIGHT as f64 / height as f64)
        .min(1.0);
    let out_width = ((width as f64 * ratio) as u32).max(1);
    let out_height = ((height as f64 * ratio) as u32).max(1);
    let mut rgba = vec![0; out_width as usize * out_height as usize * 4];
    for y in 0..out_height as usize {
        let source_y = y * height as usize / out_height as usize;
        for x in 0..out_width as usize {
            let source_x = x * width as usize / out_width as usize;
            let source = source_y * stride as usize + source_x * 4;
            let target = (y * out_width as usize + x) * 4;
            rgba[target..target + 4].copy_from_slice(&[
                data[source + 2],
                data[source + 1],
                data[source],
                255,
            ]);
        }
    }
    VideoFrame::new(out_width, out_height, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_audio_excludes_lince_and_never_mixes_the_microphone() {
        let config = audio_config(&AudioInput::System(None)).unwrap();
        assert!(config.exclude_self);
        assert_eq!(config.kind, flexaudio::SourceKind::SystemLoopback);
        assert_eq!(config.ring_capacity_chunks * config.chunk_ms as usize, 60);
        assert!(audio_config(&AudioInput::Application(std::process::id())).is_err());
    }

    #[test]
    fn desktop_conversion_respects_padded_rows_and_channel_order() {
        let frame =
            desktop_frame(1, 2, 8, &[1, 2, 3, 4, 0, 0, 0, 0, 5, 6, 7, 8, 0, 0, 0, 0]).unwrap();
        assert_eq!(frame.rgba, [3, 2, 1, 255, 7, 6, 5, 255]);
        assert!(desktop_frame(-1, 2, 8, &[]).is_err());
        assert!(desktop_frame(2, 2, 4, &[0; 8]).is_err());
    }
}
