use ailuros_protocol::{Bytes, Codec, Frame, HEADER_BYTES, Kind, MAX_DATA, Phase};
use anyhow::{Context, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, future::Future, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::{Instant, MissedTickBehavior},
};
use tokio_util::codec::{FramedRead, FramedWrite};

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const ACK_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_IN_FLIGHT: usize = 16;
const MAX_PENDING_BYTES: usize = 1024 * 1024;
const MAX_BYTES_PER_SECOND: usize = 1536 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub audience: String,
    pub device: String,
    pub signal: String,
    pub key_epoch: i64,
}

fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn uid(value: &str) -> bool {
    value.strip_prefix("r_").is_some_and(|body| {
        body.len() == 26
            && body
                .bytes()
                .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
    })
}

impl Setup {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            hexadecimal(&self.audience, 64)
                && uid(&self.device)
                && uid(&self.signal)
                && self.key_epoch > 0,
            "Invalid provisioned Cell or Signal identity"
        );
        Ok(())
    }

    fn payload(&self, challenge: &str) -> String {
        format!(
            "lince-ailuros-stream-v1\n{}\n{}\n{}\n{}\n{}\npublish\n",
            self.audience, self.device, self.key_epoch, self.signal, challenge
        )
    }
}

pub trait Identity {
    fn confirm_time(&mut self, server_time: i64) -> anyhow::Result<()>;
    fn sign(&mut self, payload: &str) -> anyhow::Result<String>;
}

pub struct CapturedFrame {
    pub bytes: Bytes,
    pub captured_at: i64,
    pub age: Duration,
}

#[derive(Clone, Default, Serialize)]
pub struct Statistics {
    pub interval_ms: u128,
    pub capture_attempts: u64,
    pub sent: u64,
    pub receipts: u64,
    pub capacity_skips: u64,
    pub dropped: u64,
    pub capture_wait_ms: u128,
    pub write_wait_ms: u128,
    pub pending: usize,
    pub pending_bytes: usize,
    pub receipt_wait_ms: u128,
    pub receipt_max_ms: u128,
    pub oldest_pending_ms: u128,
}

pub trait Camera {
    fn set_active(&mut self, active: bool) -> impl Future<Output = anyhow::Result<()>>;
    fn capture(
        &mut self,
        maximum: usize,
    ) -> impl Future<Output = anyhow::Result<Option<CapturedFrame>>>;
    fn report(&mut self, _: Statistics) {}
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Challenge {
        audience: String,
        challenge: String,
        protocol: String,
        expires_in: u64,
        server_time: i64,
    },
    Authenticated {
        max_frame_bytes: usize,
        max_fps: u32,
        max_width: u32,
        max_height: u32,
    },
    Demand {
        epoch: String,
        consumers: usize,
        active: bool,
    },
    Published {
        epoch: String,
        sequence: u64,
        retained: bool,
    },
    Ping,
}

fn control(frame: Frame) -> anyhow::Result<Control> {
    ensure!(frame.kind == Kind::Control, "Expected receiver control");
    serde_json::from_slice(&frame.payload).context("Invalid receiver control")
}

async fn send<W: AsyncWrite + Unpin>(
    writer: &mut FramedWrite<W, Codec>,
    frame: Frame,
) -> anyhow::Result<()> {
    tokio::time::timeout(IO_TIMEOUT, writer.send(frame))
        .await
        .context("Receiver stopped reading")??;
    Ok(())
}

async fn read<R: AsyncRead + Unpin>(reader: &mut FramedRead<R, Codec>) -> anyhow::Result<Control> {
    control(reader.next().await.context("Receiver disconnected")??)
}

struct Pending {
    epoch: String,
    sequence: u64,
    sent: Instant,
    bytes: usize,
}

struct Streaming {
    epoch: String,
    active: bool,
    sequence: u64,
    pending: VecDeque<Pending>,
    pending_bytes: usize,
    maximum: usize,
    window: Instant,
    bytes: usize,
    received: Instant,
    reported: Instant,
    statistics: Statistics,
}

impl Streaming {
    fn new(maximum: usize) -> Self {
        Self {
            epoch: String::new(),
            active: false,
            sequence: 0,
            pending: VecDeque::with_capacity(MAX_IN_FLIGHT),
            pending_bytes: 0,
            maximum,
            window: Instant::now(),
            bytes: 0,
            received: Instant::now(),
            reported: Instant::now(),
            statistics: Statistics::default(),
        }
    }

    fn report(&mut self, camera: &mut impl Camera) {
        let elapsed = self.reported.elapsed();
        if elapsed >= Duration::from_secs(5) {
            self.statistics.interval_ms = elapsed.as_millis();
            self.statistics.pending = self.pending.len();
            self.statistics.pending_bytes = self.pending_bytes;
            self.statistics.oldest_pending_ms = self
                .pending
                .front()
                .map(|frame| frame.sent.elapsed().as_millis())
                .unwrap_or_default();
            camera.report(std::mem::take(&mut self.statistics));
            self.reported = Instant::now();
        }
    }

    fn demand(&mut self, epoch: String, consumers: usize, active: bool) -> anyhow::Result<()> {
        ensure!(
            !epoch.is_empty()
                && epoch.len() < 96
                && epoch.bytes().all(|byte| byte.is_ascii_graphic())
                && active == (consumers > 0),
            "Invalid receiver demand"
        );
        if epoch != self.epoch {
            self.epoch = epoch;
            self.sequence = 0;
        }
        self.active = active;
        Ok(())
    }

    fn acknowledge(&mut self, epoch: &str, sequence: u64) -> anyhow::Result<()> {
        let pending = self
            .pending
            .pop_front()
            .context("Unexpected frame receipt")?;
        ensure!(
            pending.epoch == epoch && pending.sequence == sequence,
            "Stale or out-of-order frame receipt"
        );
        let elapsed = pending.sent.elapsed().as_millis();
        self.statistics.receipt_wait_ms += elapsed;
        self.statistics.receipt_max_ms = self.statistics.receipt_max_ms.max(elapsed);
        self.pending_bytes = self
            .pending_bytes
            .checked_sub(pending.bytes)
            .context("Invalid pending frame budget")?;
        Ok(())
    }

    fn has_capacity(&self) -> bool {
        self.pending.len() < MAX_IN_FLIGHT
            && self.maximum <= MAX_PENDING_BYTES.saturating_sub(self.pending_bytes)
    }

    fn check(&self) -> anyhow::Result<()> {
        ensure!(
            self.received.elapsed() <= IDLE_TIMEOUT,
            "Receiver heartbeat timed out"
        );
        ensure!(
            self.pending
                .front()
                .is_none_or(|frame| frame.sent.elapsed() <= ACK_TIMEOUT),
            "Frame receipt timed out"
        );
        Ok(())
    }

    fn reserve(&mut self, frame: &CapturedFrame) -> anyhow::Result<Option<u64>> {
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.bytes = 0;
        }
        if self.pending.len() >= MAX_IN_FLIGHT
            || frame.bytes.is_empty()
            || frame.bytes.len() > self.maximum
            || frame.age > Duration::from_secs(5)
            || frame.captured_at < 1_700_000_000_000
            || frame.captured_at > 4_102_444_800_000
            || frame.bytes.len() > MAX_BYTES_PER_SECOND.saturating_sub(self.bytes)
            || frame.bytes.len() > MAX_PENDING_BYTES.saturating_sub(self.pending_bytes)
        {
            return Ok(None);
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("Frame sequence exhausted")?;
        self.bytes += frame.bytes.len();
        self.pending_bytes += frame.bytes.len();
        self.pending.push_back(Pending {
            epoch: self.epoch.clone(),
            sequence: self.sequence,
            sent: Instant::now(),
            bytes: frame.bytes.len(),
        });
        Ok(Some(self.sequence))
    }
}

pub async fn publish<R, W, I, C>(
    receive: R,
    send_stream: W,
    setup: &Setup,
    fps: u32,
    identity: &mut I,
    camera: &mut C,
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    I: Identity,
    C: Camera,
{
    setup.validate()?;
    ensure!((1..=15).contains(&fps), "Invalid camera frame rate");
    let result = session(receive, send_stream, setup, fps, identity, camera).await;
    let cleanup = tokio::time::timeout(IO_TIMEOUT, camera.set_active(false))
        .await
        .context("Camera cleanup timed out")?;
    result.and(cleanup)
}

async fn session<R, W, I, C>(
    receive: R,
    send_stream: W,
    setup: &Setup,
    fps: u32,
    identity: &mut I,
    camera: &mut C,
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    I: Identity,
    C: Camera,
{
    let mut reader = FramedRead::with_capacity(receive, Codec::default(), HEADER_BYTES);
    let mut writer = FramedWrite::new(send_stream, Codec::default());
    let (maximum, fps) = tokio::time::timeout(AUTH_TIMEOUT, async {
        let Control::Challenge {
            audience,
            challenge,
            protocol,
            expires_in,
            server_time,
        } = read(&mut reader).await?
        else {
            bail!("Receiver challenge required");
        };
        ensure!(
            audience == setup.audience
                && hexadecimal(&challenge, 64)
                && protocol == "lince-ailuros-stream-v1"
                && (1..=5).contains(&expires_in)
                && (1_700_000_000..=4_102_444_800).contains(&server_time),
            "Receiver challenge does not match the enrolled Cell"
        );
        identity.confirm_time(server_time)?;
        let signature = identity.sign(&setup.payload(&challenge))?;
        ensure!(hexadecimal(&signature, 128), "Invalid device signature");
        let proof = serde_json::to_vec(&serde_json::json!({
            "audience":setup.audience,"device":setup.device,"signal":setup.signal,
            "key_epoch":setup.key_epoch,"challenge":challenge,"signature":signature,
        }))?;
        send(&mut writer, Frame::new(Kind::Control, proof)).await?;
        let Control::Authenticated {
            max_frame_bytes,
            max_fps,
            max_width,
            max_height,
        } = read(&mut reader).await?
        else {
            bail!("Receiver authentication required");
        };
        ensure!(
            (1..=MAX_DATA).contains(&max_frame_bytes)
                && (1..=15).contains(&max_fps)
                && (1..=1920).contains(&max_width)
                && (1..=1080).contains(&max_height),
            "Invalid receiver stream limits"
        );
        Ok::<_, anyhow::Error>((max_frame_bytes.min(128 * 1024), fps.min(max_fps)))
    })
    .await
    .context("Receiver authentication timed out")??;
    reader.decoder_mut().set_phase(Phase::Streaming);
    writer.encoder_mut().set_phase(Phase::Streaming);
    let mut state = Streaming::new(maximum);
    let mut timer = tokio::time::interval(Duration::from_secs_f64(1.0 / f64::from(fps)));
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        state.report(camera);
        state.check()?;
        tokio::select! {
            biased;
            message = read(&mut reader) => {
                let message = message?;
                state.received = Instant::now();
                match message {
                    Control::Demand { epoch, consumers, active } => {
                        state.demand(epoch, consumers, active)?;
                        tokio::time::timeout(IO_TIMEOUT, camera.set_active(active))
                            .await.context("Camera demand update timed out")??;
                    }
                    Control::Published { epoch, sequence, retained } => {
                        state.acknowledge(&epoch, sequence)?;
                        state.statistics.receipts += 1;
                        let _ = retained;
                    }
                    Control::Ping => send(&mut writer, Frame::new(Kind::Control, Bytes::from_static(br#"{"type":"pong"}"#))).await?,
                    _ => bail!("Unexpected receiver control"),
                }
            }
            _ = timer.tick() => {
                if !state.active { continue; }
                if !state.has_capacity() {
                    state.statistics.capacity_skips += 1;
                    continue;
                }
                state.statistics.capture_attempts += 1;
                let started = Instant::now();
                let frame = tokio::time::timeout(IO_TIMEOUT, camera.capture(state.maximum))
                    .await.context("Camera capture timed out")??;
                state.statistics.capture_wait_ms += started.elapsed().as_millis();
                let Some(frame) = frame else {
                    state.statistics.dropped += 1;
                    continue;
                };
                let Some(sequence) = state.reserve(&frame)? else {
                    state.statistics.dropped += 1;
                    continue;
                };
                let metadata = serde_json::to_vec(&serde_json::json!({
                    "epoch":state.epoch,"sequence":sequence,"captured_at":frame.captured_at,
                    "content_type":"image/jpeg",
                }))?;
                let started = Instant::now();
                send(&mut writer, Frame::new(Kind::Metadata, metadata)).await?;
                send(&mut writer, Frame::new(Kind::Data, frame.bytes)).await?;
                state.statistics.write_wait_ms += started.elapsed().as_millis();
                state.statistics.sent += 1;
            }
        }
    }
}
