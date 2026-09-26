use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime};

use opus_rs::OpusDecoder;
use str0m::{
    Candidate, Event, IceConnectionState, Input, Output, Rtc,
    change::{SdpAnswer, SdpOffer, SdpPendingOffer},
    media::{Direction, MediaKind, MediaTime, Mid},
    net::{Protocol, Receive},
};

use super::{
    codec::{MAX_PACKET, VideoDecoder},
    error,
    ice::Ice,
    sources::SourceReader,
};
use crate::{
    MediaError, Result,
    audio::{AudioFrame, FRAME_SAMPLES},
    video::{LatestVideo, VideoFrame},
};

pub use super::sources::Sources;

pub const MAX_SIGNALS: usize = 64;
#[derive(Debug, Clone)]
pub enum Signal {
    Offer(String),
    Answer(String),
    Candidate {
        mid: String,
        line: i32,
        candidate: String,
    },
}

#[derive(Clone)]
pub struct TurnServer {
    pub urls: Vec<String>,
    pub username: String,
    pub password: String,
    pub expires_at: SystemTime,
}

#[derive(Clone, Default)]
pub struct Network {
    pub stun: Vec<String>,
    pub turn: Vec<TurnServer>,
    pub relay_only: bool,
}

impl Network {
    pub(crate) fn configuration(&self) -> Result<()> {
        if self.turn.len() > 4 || (self.relay_only && self.turn.is_empty()) {
            return Err(MediaError(
                "Relay-only calls require a configured TURN server".into(),
            ));
        }
        if self.stun.len() > 4
            || self.stun.iter().any(|url| {
                url.len() > 2048
                    || !url.starts_with("stun:")
                    || url.chars().any(char::is_whitespace)
            })
        {
            return Err(MediaError("Invalid STUN server".into()));
        }
        for server in &self.turn {
            if server.expires_at <= SystemTime::now() + Duration::from_secs(30)
                || server.username.is_empty()
                || server.username.len() > 512
                || server.password.len() > 512
                || server.password.is_empty()
                || server.urls.is_empty()
                || server.urls.len() > 4
                || server.urls.iter().any(|url| {
                    url.len() > 2048
                        || !(url.starts_with("turn:") || url.starts_with("turns:"))
                        || url.chars().any(char::is_whitespace)
                })
            {
                return Err(MediaError(
                    "TURN credentials are missing, invalid, or expired".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Source {
    Microphone,
    SharedAudio,
    Camera,
    Screen,
}

impl Source {
    pub const ALL: [Self; 4] = [
        Self::Microphone,
        Self::SharedAudio,
        Self::Camera,
        Self::Screen,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::SharedAudio => "shared-audio",
            Self::Camera => "camera",
            Self::Screen => "screen",
        }
    }
    pub(crate) fn audio(self) -> bool {
        matches!(self, Self::Microphone | Self::SharedAudio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerConnectionState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
}

pub enum Received {
    Audio(AudioFrame),
    Video(VideoFrame),
}

struct VideoReceiver {
    input: Arc<Mutex<[Option<Vec<u8>>; 2]>>,
    output: [LatestVideo; 2],
    active: Arc<AtomicBool>,
}

impl VideoReceiver {
    fn new() -> Result<Self> {
        let input = Arc::new(Mutex::new([None::<Vec<u8>>, None]));
        let output = [LatestVideo::default(), LatestVideo::default()];
        let active = Arc::new(AtomicBool::new(true));
        let pending = input.clone();
        let frames = output.clone();
        let running = active.clone();
        std::thread::Builder::new()
            .name("lince-video-decode".into())
            .spawn(move || {
                let mut decoders = [None, None];
                while running.load(Ordering::Acquire) {
                    for index in 0..2 {
                        let packet =
                            pending.lock().unwrap_or_else(|e| e.into_inner())[index].take();
                        let Some(packet) = packet else { continue };
                        if decoders[index].is_none() {
                            decoders[index] = VideoDecoder::new().ok();
                        }
                        if let Some(decoder) = &mut decoders[index] {
                            match decoder.decode(&packet) {
                                Ok(Some(frame)) => frames[index].publish(frame),
                                Ok(None) => {}
                                Err(_) => decoders[index] = None,
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
            .map_err(error)?;
        Ok(Self {
            input,
            output,
            active,
        })
    }
}

impl Drop for VideoReceiver {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

pub struct Peer {
    rtc: Rtc,
    ice: Ice,
    network: Network,
    timeout: Instant,
    state: PeerConnectionState,
    sources: SourceReader,
    mids: Vec<Mid>,
    pending: Option<SdpPendingOffer>,
    signals: VecDeque<Signal>,
    candidates: usize,
    local_candidates: Vec<Candidate>,
    restarting: bool,
    sent: [u64; 4],
    audio: [OpusDecoder; 2],
    received: [VecDeque<AudioFrame>; 2],
    video: Option<VideoReceiver>,
}

impl Peer {
    pub fn new(sources: &Sources, network: &Network) -> Result<Self> {
        let now = Instant::now();
        let mut ice = Ice::new(network)?;
        let mut rtc = Rtc::builder()
            .clear_codecs()
            .enable_opus(true)
            .enable_av1(true)
            .set_reordering_size_audio(3)
            .set_reordering_size_video(32)
            .set_send_buffer_audio(64)
            .set_send_buffer_video(256)
            .build(now);
        let mut local_candidates = Vec::new();
        while let Some(candidate) = ice.candidates.pop_front() {
            local_candidates.push(candidate.clone());
            rtc.add_local_candidate(candidate);
        }
        Ok(Self {
            rtc,
            ice,
            network: network.clone(),
            timeout: now,
            state: PeerConnectionState::New,
            sources: sources.reader(),
            mids: Vec::new(),
            pending: None,
            signals: VecDeque::new(),
            candidates: 0,
            local_candidates,
            restarting: false,
            sent: [0; 4],
            audio: [
                OpusDecoder::new(48_000, 1).map_err(error)?,
                OpusDecoder::new(48_000, 1).map_err(error)?,
            ],
            received: [VecDeque::new(), VecDeque::new()],
            video: None,
        })
    }

    pub fn state(&self) -> PeerConnectionState {
        self.state
    }

    pub fn poll_signal(&mut self) -> Result<Option<Signal>> {
        self.drive()?;
        Ok(self.signals.pop_front())
    }

    pub fn poll_track(&mut self) -> Option<(Source, Received)> {
        for (index, queue) in self.received.iter_mut().enumerate() {
            if let Some(frame) = queue.pop_front() {
                return Some((Source::ALL[index], Received::Audio(frame)));
            }
        }
        if let Some(video) = &self.video {
            for (index, output) in video.output.iter().enumerate() {
                if let Some(frame) = output.take() {
                    return Some((Source::ALL[index + 2], Received::Video(frame)));
                }
            }
        }
        None
    }

    pub async fn offer(&mut self, restart: bool) -> Result<Signal> {
        if self.pending.is_some() {
            return Err(MediaError("Media negotiation is already pending".into()));
        }
        if restart {
            self.regather()?;
            self.restarting = true;
        }
        let mut changes = self.rtc.sdp_api();
        if self.mids.is_empty() {
            for source in Source::ALL {
                self.mids.push(changes.add_media(
                    if source.audio() {
                        MediaKind::Audio
                    } else {
                        MediaKind::Video
                    },
                    Direction::SendRecv,
                    Some(source.label().into()),
                    Some(source.label().into()),
                    None,
                ));
            }
        }
        if restart {
            changes.ice_restart(false);
            self.candidates = 0;
        }
        let (offer, pending) = changes
            .apply()
            .ok_or_else(|| MediaError("No media negotiation changes".into()))?;
        self.pending = Some(pending);
        self.state = PeerConnectionState::Connecting;
        Ok(Signal::Offer(offer.to_sdp_string()))
    }

    pub async fn signal(&mut self, signal: Signal) -> Result<Option<Signal>> {
        match signal {
            Signal::Candidate {
                mid,
                line,
                candidate,
            } => {
                if candidate.len() > 4096
                    || mid.len() > 64
                    || !(0..4).contains(&line)
                    || self.candidates >= MAX_SIGNALS
                {
                    return Err(MediaError("Invalid or excessive network candidates".into()));
                }
                let candidate = Candidate::from_sdp_string(&candidate).map_err(error)?;
                self.rtc.add_remote_candidate(candidate);
                self.candidates += 1;
                Ok(None)
            }
            Signal::Offer(sdp) => {
                let mids = description(&sdp)?;
                if self.pending.is_some() {
                    return Err(MediaError("Unexpected concurrent media offer".into()));
                }
                if !self.mids.is_empty() {
                    self.regather()?;
                }
                let offer = SdpOffer::from_sdp_string(&sdp).map_err(error)?;
                let answer = self.rtc.sdp_api().accept_offer(offer).map_err(error)?;
                self.mids = mids;
                self.state = PeerConnectionState::Connecting;
                self.candidates = 0;
                Ok(Some(Signal::Answer(answer.to_sdp_string())))
            }
            Signal::Answer(sdp) => {
                let mids = description(&sdp)?;
                if mids != self.mids {
                    return Err(MediaError("Media answer changed source identities".into()));
                }
                let answer = SdpAnswer::from_sdp_string(&sdp).map_err(error)?;
                let pending = self
                    .pending
                    .take()
                    .ok_or_else(|| MediaError("Unexpected media answer".into()))?;
                self.rtc
                    .sdp_api()
                    .accept_answer(pending, answer)
                    .map_err(error)?;
                self.restarting = false;
                Ok(None)
            }
        }
    }

    pub fn set_attendance(&self, devices: usize) -> Result<()> {
        if !(2..=6).contains(&devices) {
            return Err(MediaError("Calls support two to six devices".into()));
        }
        self.sources.attendance(devices);
        Ok(())
    }

    fn drive(&mut self) -> Result<()> {
        let now = Instant::now();
        self.ice.poll(now)?;
        while !self.restarting {
            let Some(candidate) = self.ice.candidates.pop_front() else {
                break;
            };
            if self.signals.len() >= MAX_SIGNALS {
                return Err(MediaError("Too many local candidates".into()));
            }
            self.signals.push_back(Signal::Candidate {
                mid: self
                    .mids
                    .first()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                line: 0,
                candidate: candidate.to_sdp_string(),
            });
            self.local_candidates.push(candidate.clone());
            self.rtc.add_local_candidate(candidate);
        }
        while let Some(packet) = self.ice.received.pop_front() {
            let Ok(contents) = packet.data.as_slice().try_into() else {
                continue;
            };
            let input = Input::Receive(
                now,
                Receive {
                    proto: Protocol::Udp,
                    source: packet.source,
                    destination: packet.destination,
                    contents,
                },
            );
            if self.rtc.accepts(&input) {
                let _ = self.rtc.handle_input(input);
            }
        }
        if now >= self.timeout {
            self.rtc.handle_input(Input::Timeout(now)).map_err(error)?;
        }
        if self.rtc.is_connected() {
            for source in Source::ALL {
                let index = source as usize;
                let Some(&mid) = self.mids.get(index) else {
                    continue;
                };
                for packet in self.sources.packets(source, self.sent[index]) {
                    self.sent[index] = packet.sequence;
                    if packet.at.elapsed()
                        > Duration::from_millis(if source.audio() { 100 } else { 500 })
                    {
                        continue;
                    }
                    let Some(writer) = self.rtc.writer(mid) else {
                        continue;
                    };
                    let Some(pt) = writer.payload_params().next().map(|params| params.pt()) else {
                        continue;
                    };
                    let time = if source.audio() {
                        MediaTime::from_micros(packet.timestamp * 1_000_000 / 48_000)
                    } else {
                        MediaTime::from_90khz(packet.timestamp)
                    };
                    writer
                        .write(pt, packet.at, time, packet.data)
                        .map_err(error)?;
                }
            }
        }
        for _ in 0..512 {
            match self.rtc.poll_output().map_err(error)? {
                Output::Timeout(timeout) => {
                    self.timeout = timeout;
                    break;
                }
                Output::Transmit(transmit) => self.ice.send(transmit, now)?,
                Output::Event(Event::Connected) => {
                    self.state = PeerConnectionState::Connected;
                    self.sources.keyframe(Source::Camera);
                    self.sources.keyframe(Source::Screen);
                }
                Output::Event(Event::IceConnectionStateChange(state)) => {
                    if state == IceConnectionState::Disconnected {
                        self.state = PeerConnectionState::Disconnected;
                    } else if self.rtc.is_connected() {
                        self.state = PeerConnectionState::Connected;
                    }
                }
                Output::Event(Event::KeyframeRequest(request)) => {
                    if let Some(index) = self.mids.iter().position(|mid| *mid == request.mid) {
                        self.sources.keyframe(Source::ALL[index]);
                    }
                }
                Output::Event(Event::MediaData(data)) => {
                    let Some(index) = self.mids.iter().position(|mid| *mid == data.mid) else {
                        continue;
                    };
                    if index < 2 {
                        if data.data.len() > 1275 {
                            continue;
                        }
                        let mut frame = AudioFrame::silence();
                        if self.audio[index]
                            .decode(&data.data, FRAME_SAMPLES, &mut frame.0)
                            .ok()
                            != Some(FRAME_SAMPLES)
                        {
                            continue;
                        }
                        if self.received[index].len() >= 6 {
                            self.received[index].pop_front();
                        }
                        self.received[index].push_back(frame);
                    } else if data.data.len() <= MAX_PACKET {
                        if self.video.is_none() {
                            self.video = Some(VideoReceiver::new()?);
                        }
                        self.video.as_ref().unwrap().input.lock().map_err(error)?[index - 2] =
                            Some(data.data.to_vec());
                    }
                }
                Output::Event(_) => {}
            }
        }
        Ok(())
    }

    fn regather(&mut self) -> Result<()> {
        let ice = Ice::new(&self.network)?;
        for candidate in self.local_candidates.drain(..) {
            self.rtc.direct_api().invalidate_candidate(&candidate);
        }
        self.ice = ice;
        self.signals.clear();
        Ok(())
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.rtc.disconnect();
    }
}

fn description(sdp: &str) -> Result<Vec<Mid>> {
    if sdp.len() > 65536 {
        return Err(MediaError("Media description is too large".into()));
    }
    let mut kinds = Vec::new();
    let mut mids = Vec::new();
    let mut candidates = 0;
    for line in sdp.lines() {
        if let Some(media) = line.strip_prefix("m=") {
            kinds.push(media.split_whitespace().next().unwrap_or_default());
        }
        if let Some(mid) = line.strip_prefix("a=mid:") {
            if mid.is_empty() || mid.len() > 16 || mids.contains(&Mid::from(mid)) {
                return Err(MediaError("Invalid media source identity".into()));
            }
            mids.push(Mid::from(mid));
        }
        if line.starts_with("a=candidate:") {
            candidates += 1;
        }
    }
    if kinds != ["audio", "audio", "video", "video"] || mids.len() != 4 || candidates > MAX_SIGNALS
    {
        return Err(MediaError(
            "Calls require two audio and two video sources".into(),
        ));
    }
    Ok(mids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_defaults_do_not_contact_discovery_or_relay_servers() {
        let network = Network::default();
        assert!(network.configuration().is_ok());
        assert!(network.stun.is_empty() && network.turn.is_empty() && !network.relay_only);
        assert!(
            Network {
                relay_only: true,
                ..Default::default()
            }
            .configuration()
            .is_err()
        );
    }

    #[test]
    fn expired_relay_credentials_are_rejected() {
        let network = Network {
            turn: vec![TurnServer {
                urls: vec!["turn:127.0.0.1:3478".into()],
                username: "user".into(),
                password: "secret".into(),
                expires_at: SystemTime::now(),
            }],
            ..Default::default()
        };
        assert!(network.configuration().is_err());
    }

    #[test]
    fn media_descriptions_cannot_add_sources_or_repeat_identities() {
        assert!(description("m=application 9 UDP/DTLS/SCTP webrtc-datachannel\na=mid:0").is_err());
        assert!(description(&"x".repeat(65537)).is_err());
        assert!(
            description("m=audio\na=mid:0\nm=audio\na=mid:0\nm=video\na=mid:2\nm=video\na=mid:3")
                .is_err()
        );
    }
}
