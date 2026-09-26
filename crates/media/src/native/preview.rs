use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use super::{
    capture::{self, AudioCapture, AudioInput, CameraCapture, EchoProcessor, ScreenCapture},
    error,
    playback::{self, Playback},
};
use crate::audio::{AudioFrame, FRAME_SAMPLES};
use crate::video::{LatestVideo, VideoFrame};
use crate::{MediaError, Result};

#[derive(Clone, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Devices {
    pub microphones: Vec<Choice>,
    pub speakers: Vec<Choice>,
    pub cameras: Vec<Choice>,
    pub screens: Vec<Choice>,
    pub applications: Vec<Choice>,
}

#[derive(Clone)]
pub enum Command {
    Devices,
    Microphone(Option<String>),
    SharedAudio(AudioInput),
    Camera(String),
    Screen(Option<u64>),
    Speaker(Option<String>),
    Volume(f32),
    StopMicrophone,
    StopSharedAudio,
    StopCamera,
    StopScreen,
    Pattern,
    Stop,
}

#[derive(Clone, Default)]
pub struct Status {
    pub devices: Devices,
    pub message: String,
    pub microphone: bool,
    pub shared_audio: bool,
    pub camera: bool,
    pub screen: bool,
    pub microphone_peak: f32,
    pub shared_audio_peak: f32,
}

pub struct Preview {
    commands: mpsc::SyncSender<Command>,
    status: Arc<Mutex<Status>>,
    running: Arc<AtomicBool>,
    pub camera: LatestVideo,
    pub screen: LatestVideo,
}

impl Preview {
    pub fn spawn() -> Result<Self> {
        let (commands, incoming) = mpsc::sync_channel(16);
        let status = Arc::new(Mutex::new(Status {
            message: "Choose a source to start a local preview".into(),
            ..Default::default()
        }));
        let running = Arc::new(AtomicBool::new(true));
        let camera = LatestVideo::default();
        let screen = LatestVideo::default();
        let worker_status = status.clone();
        let worker_running = running.clone();
        let worker_camera = camera.clone();
        let worker_screen = screen.clone();
        std::thread::Builder::new()
            .name("lince-media-preview".into())
            .spawn(move || {
                let mut worker = Worker {
                    microphone: None,
                    shared: None,
                    camera: None,
                    screen: None,
                    playback: None,
                    echo: EchoProcessor::default(),
                    status: worker_status,
                    camera_frames: worker_camera,
                    screen_frames: worker_screen,
                    tone_until: None,
                    tone_position: 0,
                    last_tone: Instant::now(),
                    last_screen: Instant::now(),
                };
                while worker_running.load(Ordering::Acquire) {
                    match incoming.recv_timeout(Duration::from_millis(5)) {
                        Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Ok(command) => {
                            if let Err(error) = worker.command(command) {
                                worker.message(error.to_string());
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if let Err(error) = worker.poll() {
                        worker.message(error.to_string());
                    }
                }
            })
            .map_err(error)?;
        Ok(Self {
            commands,
            status,
            running,
            camera,
            screen,
        })
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|_| MediaError("Media controls are busy; try again".into()))
    }

    pub fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let _ = self.commands.try_send(Command::Stop);
    }
}

pub(crate) struct CameraWorker {
    running: Arc<AtomicBool>,
    pub(crate) status: mpsc::Receiver<std::result::Result<(), String>>,
    pub(crate) frames: LatestVideo,
}

impl CameraWorker {
    pub(crate) fn start(id: String) -> Result<Self> {
        let frames = LatestVideo::default();
        let output = frames.clone();
        let running = Arc::new(AtomicBool::new(true));
        let active = running.clone();
        let (sender, status) = mpsc::sync_channel(2);
        std::thread::Builder::new()
            .name("lince-camera".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    let (permission, granted) = mpsc::sync_channel(1);
                    nokhwa::nokhwa_initialize(move |allowed| {
                        let _ = permission.try_send(allowed);
                    });
                    loop {
                        if !active.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        match granted.recv_timeout(Duration::from_millis(100)) {
                            Ok(true) => break,
                            Ok(false) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                                return Err(MediaError("Camera access was denied".into()));
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                    }
                    let index = capture::cameras()?
                        .into_iter()
                        .find(|(index, _)| index.to_string() == id)
                        .ok_or_else(|| MediaError("The selected camera is unavailable".into()))?
                        .0;
                    let mut camera = CameraCapture::start(index)?;
                    let _ = sender.try_send(Ok(()));
                    while active.load(Ordering::Acquire) {
                        let frame = camera.frame()?;
                        if active.load(Ordering::Acquire) {
                            output.publish(frame);
                        }
                        std::thread::sleep(Duration::from_millis(30));
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = sender.try_send(Err(error.to_string()));
                }
            })
            .map_err(error)?;
        Ok(Self {
            running,
            status,
            frames,
        })
    }
}

impl Drop for CameraWorker {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

struct Worker {
    microphone: Option<AudioCapture>,
    shared: Option<AudioCapture>,
    camera: Option<CameraWorker>,
    screen: Option<ScreenCapture>,
    playback: Option<Playback>,
    echo: EchoProcessor,
    status: Arc<Mutex<Status>>,
    camera_frames: LatestVideo,
    screen_frames: LatestVideo,
    tone_until: Option<Instant>,
    tone_position: usize,
    last_tone: Instant,
    last_screen: Instant,
}

pub fn screen_uses_picker() -> bool {
    #[cfg(target_os = "linux")]
    if super::x11::available() {
        return false;
    }
    !cfg!(target_os = "windows")
}

impl Worker {
    fn message(&self, message: String) {
        self.status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .message = message;
    }

    fn command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Devices => {
                let mut devices = Devices::default();
                let mut failures = Vec::new();
                match flexaudio::devices() {
                    Ok(inputs) => {
                        devices.microphones = inputs
                            .into_iter()
                            .filter(|input| input.source_kind == flexaudio::SourceKind::Mic)
                            .map(|input| Choice {
                                id: input.id,
                                label: input.name,
                            })
                            .collect()
                    }
                    Err(error) => failures.push(error.to_string()),
                }
                match playback::devices() {
                    Ok(outputs) => {
                        devices.speakers = outputs
                            .into_iter()
                            .map(|id| Choice {
                                label: id.clone(),
                                id,
                            })
                            .collect()
                    }
                    Err(error) => failures.push(error.to_string()),
                }
                match capture::cameras() {
                    Ok(cameras) => {
                        devices.cameras = cameras
                            .into_iter()
                            .map(|(index, label)| Choice {
                                id: index.to_string(),
                                label,
                            })
                            .collect()
                    }
                    Err(error) => failures.push(error.to_string()),
                }
                match capture::screens() {
                    Ok(screens) => {
                        devices.screens = screens
                            .into_iter()
                            .map(|(id, label)| Choice {
                                id: id.to_string(),
                                label,
                            })
                            .collect()
                    }
                    Err(error) => failures.push(error.to_string()),
                }
                match flexaudio::processes() {
                    Ok(processes) => {
                        devices.applications = processes
                            .into_iter()
                            .filter(|p| p.pid != std::process::id())
                            .map(|p| Choice {
                                id: p.pid.to_string(),
                                label: p.name,
                            })
                            .take(128)
                            .collect()
                    }
                    Err(error) => failures.push(error.to_string()),
                }
                self.status.lock().map_err(error)?.devices = devices;
                self.message(if failures.is_empty() {
                    "Choose a source to preview".into()
                } else {
                    failures.join("; ")
                });
            }
            Command::Microphone(device) => {
                self.microphone = None;
                self.status.lock().map_err(error)?.microphone = false;
                self.microphone = Some(AudioCapture::start(AudioInput::Microphone(device))?);
                self.status.lock().map_err(error)?.microphone = true;
                self.message("Microphone preview is active".into());
            }
            Command::SharedAudio(input) => {
                if matches!(input, AudioInput::Microphone(_)) {
                    return Err(MediaError("Choose system or application audio".into()));
                }
                self.shared = None;
                self.status.lock().map_err(error)?.shared_audio = false;
                self.shared = Some(AudioCapture::start(input)?);
                self.status.lock().map_err(error)?.shared_audio = true;
                self.message("Shared audio preview is active".into());
            }
            Command::Camera(id) => {
                self.camera = None;
                self.status.lock().map_err(error)?.camera = false;
                self.camera_frames.clear();
                self.camera = Some(CameraWorker::start(id)?);
                self.message("Waiting for camera access…".into());
            }
            Command::Screen(id) => {
                if id.is_none() && !screen_uses_picker() {
                    return Err(MediaError("Select a screen from the device list".into()));
                }
                self.screen = None;
                self.status.lock().map_err(error)?.screen = false;
                self.screen_frames.clear();
                self.screen = Some(ScreenCapture::start(id)?);
                self.message("Choose a screen in the system picker".into());
            }
            Command::Speaker(device) => {
                self.playback = None;
                let playback = Playback::start(device.as_deref())?;
                playback.speaker.add("test-tone")?;
                self.playback = Some(playback);
                self.tone_until = Some(Instant::now() + Duration::from_secs(1));
                self.message("Playing a one-second speaker test".into());
            }
            Command::Volume(volume) => {
                if let Some(playback) = &self.playback {
                    playback.speaker.set_volume(volume);
                }
            }
            Command::StopMicrophone => {
                self.microphone = None;
                let mut state = self.status.lock().map_err(error)?;
                state.microphone = false;
                state.microphone_peak = 0.0;
                state.message = "Microphone stopped".into();
            }
            Command::StopSharedAudio => {
                self.shared = None;
                let mut state = self.status.lock().map_err(error)?;
                state.shared_audio = false;
                state.shared_audio_peak = 0.0;
                state.message = "Shared audio stopped".into();
            }
            Command::StopCamera => {
                self.camera = None;
                self.camera_frames.clear();
                self.status.lock().map_err(error)?.camera = false;
                self.message("Camera stopped".into());
            }
            Command::StopScreen => {
                self.screen = None;
                self.screen_frames.clear();
                self.status.lock().map_err(error)?.screen = false;
                self.message("Screen preview stopped".into());
            }
            Command::Pattern => {
                self.camera_frames.publish(VideoFrame::new(
                    320,
                    180,
                    [50, 140, 220, 255].repeat(320 * 180),
                )?);
                self.message("Synthetic video preview".into());
            }
            Command::Stop => {}
        }
        Ok(())
    }

    fn poll(&mut self) -> Result<()> {
        if let Some(playback) = &self.playback {
            if let Some(error) = playback.speaker.take_error() {
                self.message(format!("Speaker interrupted: {error}"));
            }
            while let Some(frame) = playback.speaker.take_played() {
                self.echo.speaker(&frame)?;
            }
            if self.tone_until.is_some_and(|until| Instant::now() < until)
                && self.last_tone.elapsed() >= Duration::from_millis(10)
            {
                let frame = AudioFrame(std::array::from_fn(|index| {
                    ((self.tone_position + index) as f32 * 440.0 * std::f32::consts::TAU / 48000.0)
                        .sin()
                        * 0.08
                }));
                self.tone_position += FRAME_SAMPLES;
                playback.speaker.push("test-tone", frame)?;
                self.last_tone = Instant::now();
            }
        }
        for microphone in [true, false] {
            let capture = if microphone {
                &mut self.microphone
            } else {
                &mut self.shared
            };
            let Some(capture) = capture else { continue };
            while let Some(frames) = capture.poll()? {
                let mut peak = 0.0_f32;
                for frame in frames {
                    let pcm = if microphone {
                        self.echo.microphone(&frame, 20)?
                    } else {
                        frame.pcm()
                    };
                    peak = peak.max(
                        pcm.iter()
                            .map(|sample| sample.unsigned_abs() as f32 / 32768.0)
                            .fold(0.0, f32::max),
                    );
                }
                let mut state = self.status.lock().map_err(error)?;
                if microphone {
                    state.microphone_peak = peak;
                } else {
                    state.shared_audio_peak = peak;
                }
            }
            if let Some(event) = capture.event() {
                self.status.lock().map_err(error)?.message = format!("Audio device: {event:?}");
            }
        }
        if let Some(camera) = &self.camera {
            if let Some(frame) = camera.frames.take() {
                self.camera_frames.publish(frame);
            }
            match camera.status.try_recv() {
                Ok(Ok(())) => {
                    self.status.lock().map_err(error)?.camera = true;
                    self.message("Camera preview is active".into());
                }
                Ok(Err(message)) => {
                    self.camera = None;
                    self.status.lock().map_err(error)?.camera = false;
                    self.message(message);
                }
                Err(_) => {}
            }
        }
        if self.last_screen.elapsed() >= Duration::from_millis(100) {
            self.last_screen = Instant::now();
            if let Some(screen) = &mut self.screen {
                match screen.frame() {
                    Ok(Some(frame)) => {
                        self.screen_frames.publish(frame);
                        let mut state = self.status.lock().map_err(error)?;
                        if !state.screen {
                            state.message = "Screen preview is active".into();
                            state.screen = true;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.screen = None;
                        self.status.lock().map_err(super::error)?.screen = false;
                        self.message(error.to_string());
                    }
                }
            }
        }
        Ok(())
    }
}
