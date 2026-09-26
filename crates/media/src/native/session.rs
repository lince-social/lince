use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use super::{
    capture::{AudioCapture, AudioInput, EchoProcessor, ScreenCapture},
    peer::{Network, Peer, PeerConnectionState, Received, Signal, Source, Sources},
    playback::{Playback, Speaker},
    preview::{CameraWorker, Command as Capture},
};
use crate::{
    MediaError, Result,
    video::{LatestVideo, VideoFrame},
};

pub enum Command {
    Attendance(Vec<(String, bool)>),
    Signal { peer: String, signal: Signal },
    Capture(Capture),
    Renew,
    Stop,
}

pub enum Event {
    Signal {
        peer: String,
        signal: Signal,
    },
    Tracks {
        microphone: bool,
        camera: bool,
        screen: bool,
        shared_audio: bool,
    },
    Connection {
        peer: String,
        state: String,
    },
    Error(String),
    Ended,
}

pub struct Session {
    sender: mpsc::SyncSender<Command>,
    events: Mutex<mpsc::Receiver<Event>>,
    running: Arc<AtomicBool>,
    videos: Arc<Mutex<BTreeMap<String, LatestVideo>>>,
}

impl Session {
    pub fn spawn(network: Network) -> Result<Self> {
        let (sender, commands) = mpsc::sync_channel(128);
        let (output, events) = mpsc::sync_channel(128);
        let running = Arc::new(AtomicBool::new(true));
        let active = running.clone();
        let videos = Arc::new(Mutex::new(BTreeMap::new()));
        let frames = videos.clone();
        std::thread::Builder::new()
            .name("lince-call-media".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(super::error)?;
                    runtime.block_on(async {
                        let speaker = Speaker::default();
                        speaker.set_volume(1.0);
                        let mut worker = Worker {
                            sources: Sources::default(),
                            network,
                            peers: BTreeMap::new(),
                            microphone: None,
                            shared: None,
                            camera: None,
                            screen: None,
                            playback: None,
                            speaker,
                            echo: EchoProcessor::default(),
                            videos: frames,
                            output: output.clone(),
                            renewed: Instant::now(),
                            last_screen: Instant::now(),
                        };
                        if let Err(error) = worker.capture(Capture::Speaker(None)) {
                            worker.emit(Event::Error(error.to_string()))?;
                        }
                        while active.load(Ordering::Acquire)
                            && worker.renewed.elapsed() < Duration::from_secs(30)
                        {
                            for _ in 0..32 {
                                match commands.try_recv() {
                                    Ok(Command::Stop) | Err(mpsc::TryRecvError::Disconnected) => {
                                        return Ok(());
                                    }
                                    Ok(command) => {
                                        if let Err(error) = worker.command(command).await {
                                            worker.emit(Event::Error(error.to_string()))?;
                                        }
                                    }
                                    Err(mpsc::TryRecvError::Empty) => break,
                                }
                            }
                            worker.poll().await?;
                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                        Ok(())
                    })
                })();
                if let Err(error) = result {
                    let _ = output.try_send(Event::Error(error.to_string()));
                }
                active.store(false, Ordering::Release);
                let _ = output.try_send(Event::Ended);
            })
            .map_err(super::error)?;
        Ok(Self {
            sender,
            events: Mutex::new(events),
            running,
            videos,
        })
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.sender
            .try_send(command)
            .map_err(|_| MediaError("Call media controls are busy or stopped".into()))
    }

    pub fn event(&self) -> Option<Event> {
        self.events.lock().ok()?.try_recv().ok()
    }
    pub fn active(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
    pub fn frames(&self) -> Vec<(String, VideoFrame)> {
        self.videos
            .lock()
            .map(|videos| {
                videos
                    .iter()
                    .filter_map(|(key, frame)| frame.take().map(|frame| (key.clone(), frame)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let _ = self.sender.try_send(Command::Stop);
    }
}

struct Connection {
    peer: Peer,
    audio: Vec<String>,
    video: Vec<String>,
    state: PeerConnectionState,
    initiator: bool,
    restart: Instant,
    restarting: bool,
}

struct Worker {
    sources: Sources,
    network: Network,
    peers: BTreeMap<String, Connection>,
    microphone: Option<AudioCapture>,
    shared: Option<AudioCapture>,
    camera: Option<CameraWorker>,
    screen: Option<ScreenCapture>,
    playback: Option<Playback>,
    speaker: Speaker,
    echo: EchoProcessor,
    videos: Arc<Mutex<BTreeMap<String, LatestVideo>>>,
    output: mpsc::SyncSender<Event>,
    renewed: Instant,
    last_screen: Instant,
}

impl Worker {
    fn emit(&self, event: Event) -> Result<()> {
        self.output
            .try_send(event)
            .map_err(|_| MediaError("Call controls stopped receiving events".into()))
    }

    fn tracks(&self) -> Result<()> {
        self.emit(Event::Tracks {
            microphone: self.microphone.is_some(),
            camera: self.camera.is_some(),
            screen: self.screen.is_some(),
            shared_audio: self.shared.is_some(),
        })
    }

    async fn command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Renew => self.renewed = Instant::now(),
            Command::Attendance(peers) => {
                if peers.len() > 5 {
                    return Err(MediaError("Calls support at most six devices".into()));
                }
                let departed: Vec<_> = self
                    .peers
                    .keys()
                    .filter(|key| !peers.iter().any(|(peer, _)| peer == *key))
                    .cloned()
                    .collect();
                for key in departed {
                    if let Some(connection) = self.peers.remove(&key) {
                        for track in &connection.audio {
                            self.speaker.remove(track);
                        }
                        if let Ok(mut videos) = self.videos.lock() {
                            for track in &connection.video {
                                videos.remove(track);
                            }
                        }
                    }
                }
                for (key, initiator) in &peers {
                    if self.peers.contains_key(key) {
                        continue;
                    }
                    let mut peer = Peer::new(&self.sources, &self.network)?;
                    if *initiator {
                        self.emit(Event::Signal {
                            peer: key.clone(),
                            signal: peer.offer(false).await?,
                        })?;
                    }
                    self.peers.insert(
                        key.clone(),
                        Connection {
                            peer,
                            audio: Vec::new(),
                            video: Vec::new(),
                            state: PeerConnectionState::New,
                            initiator: *initiator,
                            restart: Instant::now(),
                            restarting: false,
                        },
                    );
                }
                for connection in self.peers.values() {
                    connection.peer.set_attendance((peers.len() + 1).max(2))?;
                }
            }
            Command::Signal { peer, signal } => {
                let connection = self
                    .peers
                    .get_mut(&peer)
                    .ok_or_else(|| MediaError("Signal from a device outside the call".into()))?;
                if matches!(signal, Signal::Offer(_)) && connection.initiator {
                    return Err(MediaError("Unexpected media offer".into()));
                }
                if let Some(signal) = connection.peer.signal(signal).await? {
                    self.emit(Event::Signal { peer, signal })?;
                }
            }
            Command::Capture(command) => {
                let result = self.capture(command);
                self.tracks()?;
                result?;
            }
            Command::Stop => {}
        }
        Ok(())
    }

    fn capture(&mut self, command: Capture) -> Result<()> {
        match command {
            Capture::Microphone(device) => {
                self.microphone = None;
                self.sources.set_enabled(Source::Microphone, false);
                self.microphone = Some(AudioCapture::start(AudioInput::Microphone(device))?);
                self.sources.set_enabled(Source::Microphone, true);
            }
            Capture::SharedAudio(input) => {
                if matches!(input, AudioInput::Microphone(_)) {
                    return Err(MediaError("Choose system or application audio".into()));
                }
                self.shared = None;
                self.sources.set_enabled(Source::SharedAudio, false);
                self.shared = Some(AudioCapture::start(input)?);
                self.sources.set_enabled(Source::SharedAudio, true);
            }
            Capture::Camera(id) => {
                self.camera = None;
                self.sources.set_enabled(Source::Camera, false);
                self.camera = Some(CameraWorker::start(id)?);
                self.sources.set_enabled(Source::Camera, true);
            }
            Capture::Screen(id) => {
                if id.is_none() && !super::preview::screen_uses_picker() {
                    return Err(MediaError("Select a screen from the device list".into()));
                }
                self.screen = None;
                self.sources.set_enabled(Source::Screen, false);
                self.screen = Some(ScreenCapture::start(id)?);
                self.sources.set_enabled(Source::Screen, true);
            }
            Capture::Speaker(device) => {
                self.playback = None;
                self.playback = Some(Playback::with_speaker(
                    device.as_deref(),
                    self.speaker.clone(),
                )?);
            }
            Capture::Volume(volume) => self.speaker.set_volume(volume),
            Capture::StopMicrophone => {
                self.microphone = None;
                self.sources.set_enabled(Source::Microphone, false);
            }
            Capture::StopSharedAudio => {
                self.shared = None;
                self.sources.set_enabled(Source::SharedAudio, false);
            }
            Capture::StopCamera => {
                self.camera = None;
                self.sources.set_enabled(Source::Camera, false);
                self.videos
                    .lock()
                    .map_err(super::error)?
                    .remove("local/camera");
            }
            Capture::StopScreen => {
                self.screen = None;
                self.sources.set_enabled(Source::Screen, false);
                self.videos
                    .lock()
                    .map_err(super::error)?
                    .remove("local/screen");
            }
            _ => {
                return Err(MediaError(
                    "This control is only available in the local media test".into(),
                ));
            }
        }
        Ok(())
    }

    fn local_video(&self, source: Source, frame: VideoFrame) -> Result<()> {
        self.sources.video(source, &frame)?;
        self.videos
            .lock()
            .map_err(super::error)?
            .entry(format!("local/{}", source.label()))
            .or_default()
            .publish(frame);
        Ok(())
    }

    async fn poll(&mut self) -> Result<()> {
        if let Some(error) = self.speaker.take_error() {
            self.emit(Event::Error(format!("Speaker interrupted: {error}")))?;
            self.playback = None;
        }
        for _ in 0..6 {
            let Some(frame) = self.speaker.take_played() else {
                break;
            };
            self.echo.speaker(&frame)?;
        }
        for source in [Source::Microphone, Source::SharedAudio] {
            for _ in 0..3 {
                let input = if source == Source::Microphone {
                    &mut self.microphone
                } else {
                    &mut self.shared
                };
                let Some(input) = input else { break };
                match input.poll() {
                    Ok(Some(frames)) => {
                        for frame in frames {
                            let samples = if source == Source::Microphone {
                                self.echo.microphone(&frame, 20)?
                            } else {
                                frame.pcm()
                            };
                            self.sources.audio(source, samples).await?;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        self.capture(if source == Source::Microphone {
                            Capture::StopMicrophone
                        } else {
                            Capture::StopSharedAudio
                        })?;
                        self.tracks()?;
                        self.emit(Event::Error(error.to_string()))?;
                        break;
                    }
                }
            }
        }
        if let Some(camera) = &self.camera {
            let failure = camera
                .status
                .try_recv()
                .ok()
                .and_then(std::result::Result::err);
            let frame = camera.frames.take();
            if let Some(message) = failure {
                self.capture(Capture::StopCamera)?;
                self.tracks()?;
                self.emit(Event::Error(message))?;
            } else if let Some(frame) = frame {
                self.local_video(Source::Camera, frame)?;
            }
        }
        if self.last_screen.elapsed() >= Duration::from_millis(100) {
            self.last_screen = Instant::now();
            if let Some(screen) = &mut self.screen {
                match screen.frame() {
                    Ok(Some(frame)) => self.local_video(Source::Screen, frame)?,
                    Ok(None) => {}
                    Err(error) => {
                        self.capture(Capture::StopScreen)?;
                        self.tracks()?;
                        self.emit(Event::Error(error.to_string()))?;
                    }
                }
            }
        }
        let mut outgoing = Vec::new();
        for (key, connection) in &mut self.peers {
            while let Some(signal) = connection.peer.poll_signal()? {
                outgoing.push(Event::Signal {
                    peer: key.clone(),
                    signal,
                });
            }
            while let Some((source, track)) = connection.peer.poll_track() {
                let id = format!("{key}/{}", source.label());
                match track {
                    Received::Audio(frame) => {
                        if !connection.audio.contains(&id) {
                            self.speaker.add(&id)?;
                            connection.audio.push(id.clone());
                        }
                        self.speaker.push(&id, frame)?;
                    }
                    Received::Video(frame) => {
                        let mut videos = self.videos.lock().map_err(super::error)?;
                        if !connection.video.contains(&id) {
                            connection.video.push(id.clone());
                        }
                        videos.entry(id).or_default().publish(frame);
                    }
                }
            }
            let state = connection.peer.state();
            if state != connection.state {
                outgoing.push(Event::Connection {
                    peer: key.clone(),
                    state: format!("{state:?}"),
                });
                connection.state = state;
                connection.restart = Instant::now();
                if state == PeerConnectionState::Connected {
                    connection.restarting = false;
                }
            }
            if connection.initiator
                && !connection.restarting
                && matches!(
                    state,
                    PeerConnectionState::Disconnected | PeerConnectionState::Failed
                )
                && connection.restart.elapsed() > Duration::from_secs(3)
            {
                outgoing.push(Event::Signal {
                    peer: key.clone(),
                    signal: connection.peer.offer(true).await?,
                });
                connection.restarting = true;
            }
        }
        for event in outgoing {
            self.emit(event)?;
        }
        Ok(())
    }
}
