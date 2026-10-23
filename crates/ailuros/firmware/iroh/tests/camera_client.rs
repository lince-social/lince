use ailuros_protocol::{Bytes, Codec, Frame, HEADER_BYTES, Kind, Phase};
use futures_util::{SinkExt, StreamExt};
use lince_ailuros_esp_iroh::client::{self, Camera, CapturedFrame, Identity, Setup};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{DuplexStream, ReadHalf, WriteHalf};
use tokio_util::codec::{FramedRead, FramedWrite};

#[derive(Default)]
struct Observations {
    active: bool,
    changes: Vec<bool>,
    captures: usize,
    signed: Vec<String>,
    clock: Option<i64>,
    bytes: Option<usize>,
    age: Duration,
    reports: Vec<client::Statistics>,
}

struct Hardware(Arc<Mutex<Observations>>);

impl Identity for Hardware {
    fn confirm_time(&mut self, time: i64) -> anyhow::Result<()> {
        self.0.lock().unwrap().clock = Some(time);
        Ok(())
    }

    fn sign(&mut self, payload: &str) -> anyhow::Result<String> {
        self.0.lock().unwrap().signed.push(payload.into());
        Ok("ab".repeat(64))
    }
}

impl Camera for Hardware {
    fn report(&mut self, statistics: client::Statistics) {
        self.0.lock().unwrap().reports.push(statistics);
    }

    async fn set_active(&mut self, active: bool) -> anyhow::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.active = active;
        state.changes.push(active);
        Ok(())
    }

    async fn capture(&mut self, _: usize) -> anyhow::Result<Option<CapturedFrame>> {
        let mut state = self.0.lock().unwrap();
        assert!(state.active, "Capture without active viewer demand");
        state.captures += 1;
        Ok(Some(CapturedFrame {
            bytes: Bytes::from(vec![7; state.bytes.unwrap_or(64)]),
            captured_at: 1_800_000_000_000,
            age: state.age,
        }))
    }
}

struct Receiver {
    reader: FramedRead<ReadHalf<DuplexStream>, Codec>,
    writer: FramedWrite<WriteHalf<DuplexStream>, Codec>,
    observations: Arc<Mutex<Observations>>,
    client: tokio::task::JoinHandle<anyhow::Result<()>>,
}

fn setup() -> Setup {
    Setup {
        audience: "11".repeat(32),
        device: "r_00000000000000000000000001".into(),
        signal: "r_00000000000000000000000002".into(),
        key_epoch: 1,
    }
}

impl Receiver {
    fn new() -> Self {
        let (device, receiver) = tokio::io::duplex(4096);
        let (receive, send) = tokio::io::split(device);
        let (input, output) = tokio::io::split(receiver);
        let observations = Arc::new(Mutex::new(Observations::default()));
        let device = observations.clone();
        let client = tokio::spawn(async move {
            client::publish(
                receive,
                send,
                &setup(),
                5,
                &mut Hardware(device.clone()),
                &mut Hardware(device),
            )
            .await
        });
        Self {
            reader: FramedRead::with_capacity(input, Codec::default(), HEADER_BYTES),
            writer: FramedWrite::new(output, Codec::default()),
            observations,
            client,
        }
    }

    async fn send(&mut self, message: Value) {
        self.writer
            .send(Frame::new(
                Kind::Control,
                serde_json::to_vec(&message).unwrap(),
            ))
            .await
            .unwrap();
    }

    fn challenge() -> Value {
        json!({"type":"challenge","audience":setup().audience,"challenge":"22".repeat(32),
            "protocol":"lince-ailuros-stream-v1","server_time":1_800_000_000,"expires_in":5})
    }

    async fn authenticate(&mut self) {
        self.send(Self::challenge()).await;
        let frame = self.reader.next().await.unwrap().unwrap();
        assert_eq!(frame.kind, Kind::Control);
        let proof: Value = serde_json::from_slice(&frame.payload).unwrap();
        assert_eq!(proof["device"], setup().device);
        assert_eq!(proof["signal"], setup().signal);
        assert_eq!(proof["key_epoch"], 1);
        assert_eq!(proof["challenge"], "22".repeat(32));
        assert_eq!(proof["signature"], "ab".repeat(64));
        self.send(json!({"type":"authenticated","max_frame_bytes":524288,
            "max_fps":15,"max_width":1920,"max_height":1080}))
            .await;
        self.writer.encoder_mut().set_phase(Phase::Streaming);
        self.reader.decoder_mut().set_phase(Phase::Streaming);
    }

    async fn demand(&mut self, epoch: &str, viewers: usize) {
        self.send(json!({"type":"demand","epoch":epoch,"consumers":viewers,"active":viewers>0}))
            .await;
    }

    async fn frame(&mut self) -> Value {
        let frame = tokio::time::timeout(Duration::from_secs(2), self.reader.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(frame.kind, Kind::Metadata);
        let metadata: Value = serde_json::from_slice(&frame.payload).unwrap();
        let frame = self.reader.next().await.unwrap().unwrap();
        assert_eq!(frame.kind, Kind::Data);
        assert_eq!(
            frame.payload.len(),
            self.observations.lock().unwrap().bytes.unwrap_or(64)
        );
        assert_eq!(metadata["content_type"], "image/jpeg");
        metadata
    }

    async fn receipt(&mut self, metadata: &Value) {
        self.receipt_with_retention(metadata, true).await;
    }

    async fn receipt_with_retention(&mut self, metadata: &Value, retained: bool) {
        self.send(json!({"type":"published","epoch":metadata["epoch"],
            "sequence":metadata["sequence"],"retained":retained}))
            .await;
    }

    async fn finish(self) {
        let Self {
            reader,
            writer,
            observations,
            client,
        } = self;
        drop(reader);
        drop(writer);
        assert!(
            tokio::time::timeout(Duration::from_secs(1), client)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        let state = observations.lock().unwrap();
        assert!(!state.active);
        assert_eq!(state.changes.last(), Some(&false));
    }
}

#[tokio::test]
async fn demand_controls_capture_and_receipts_bound_in_flight_frames_across_epochs() {
    let mut receiver = Receiver::new();
    receiver.authenticate().await;
    receiver.demand("first", 0).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(receiver.observations.lock().unwrap().captures, 0);
    receiver.demand("first", 2).await;
    let mut pending = Vec::new();
    for sequence in 1..=16 {
        let metadata = receiver.frame().await;
        assert_eq!(metadata["sequence"], sequence);
        pending.push(metadata);
    }
    let captures = receiver.observations.lock().unwrap().captures;
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert_eq!(receiver.observations.lock().unwrap().captures, captures);
    receiver.receipt(&pending.remove(0)).await;
    let resumed = receiver.frame().await;
    assert_eq!(resumed["sequence"], 17);
    pending.push(resumed);
    let captures = receiver.observations.lock().unwrap().captures;
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert_eq!(receiver.observations.lock().unwrap().captures, captures);
    receiver.demand("first", 1).await;
    receiver.demand("second", 0).await;
    for metadata in pending {
        receiver.receipt(&metadata).await;
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(receiver.observations.lock().unwrap().captures, captures);
    receiver.demand("second", 1).await;
    let reopened = receiver.frame().await;
    assert_eq!(reopened["epoch"], "second");
    assert_eq!(reopened["sequence"], 1);
    receiver.receipt(&reopened).await;
    receiver.send(json!({"type":"ping"})).await;
    let pong = receiver.reader.next().await.unwrap().unwrap();
    assert_eq!(&pong.payload[..], br#"{"type":"pong"}"#);
    let expected = format!(
        "lince-ailuros-stream-v1\n{}\n{}\n1\n{}\n{}\npublish\n",
        setup().audience,
        setup().device,
        setup().signal,
        "22".repeat(32)
    );
    assert_eq!(receiver.observations.lock().unwrap().signed, vec![expected]);
    receiver.finish().await;
}

#[tokio::test(start_paused = true)]
async fn delayed_receipts_preserve_frame_rate_without_unbounded_capture() {
    let mut receiver = Receiver::new();
    receiver.authenticate().await;
    receiver.demand("epoch", 1).await;
    let mut pending = std::collections::VecDeque::new();
    let mut first = None;
    for sequence in 1..=40 {
        let metadata = receiver.frame().await;
        assert_eq!(metadata["sequence"], sequence);
        let now = tokio::time::Instant::now();
        let started = *first.get_or_insert(now);
        assert_eq!(
            now.duration_since(started),
            Duration::from_millis((sequence - 1) * 200)
        );
        pending.push_back((now, metadata));
        assert!(pending.len() <= 16);
        while pending
            .front()
            .is_some_and(|(sent, _)| now.duration_since(*sent) >= Duration::from_millis(3000))
        {
            receiver.receipt(&pending.pop_front().unwrap().1).await;
        }
    }
    {
        let state = receiver.observations.lock().unwrap();
        assert_eq!(state.reports.len(), 1);
        let report = &state.reports[0];
        assert_eq!(report.interval_ms, 5000);
        assert!(report.sent >= 20);
        assert_eq!(report.capture_attempts, report.sent);
        assert_eq!(report.sent - report.receipts, report.pending as u64);
        assert!(report.pending <= 16);
        assert_eq!(report.pending_bytes, report.pending * 64);
        assert_eq!(report.capacity_skips, 0);
        assert_eq!(report.dropped, 0);
        assert_eq!(report.capture_wait_ms, 0);
        assert_eq!(report.write_wait_ms, 0);
        assert_eq!(report.receipt_max_ms, 3000);
        assert_eq!(report.receipt_wait_ms, u128::from(report.receipts) * 3000);
        assert!(report.oldest_pending_ms <= 3200);
    }
    receiver.finish().await;
}

#[tokio::test]
async fn payload_budget_limits_large_frames_before_the_frame_count_limit() {
    for retained in [true, false] {
        let mut receiver = Receiver::new();
        receiver.observations.lock().unwrap().bytes = Some(128 * 1024);
        receiver.authenticate().await;
        receiver.demand("epoch", 1).await;
        let mut pending = Vec::new();
        for sequence in 1..=8 {
            let metadata = receiver.frame().await;
            assert_eq!(metadata["sequence"], sequence);
            pending.push(metadata);
        }
        let captures = receiver.observations.lock().unwrap().captures;
        assert_eq!(captures, 8);
        tokio::time::sleep(Duration::from_millis(450)).await;
        assert_eq!(receiver.observations.lock().unwrap().captures, captures);
        receiver.receipt_with_retention(&pending[0], retained).await;
        let resumed = receiver.frame().await;
        assert_eq!(resumed["sequence"], 9);
        tokio::time::sleep(Duration::from_millis(450)).await;
        assert_eq!(receiver.observations.lock().unwrap().captures, captures + 1);
        receiver.finish().await;
    }
}

#[tokio::test]
async fn wrong_cell_malformed_challenge_and_pre_auth_data_never_sign_or_capture() {
    for modification in 0..5 {
        let mut receiver = Receiver::new();
        let mut challenge = Receiver::challenge();
        match modification {
            0 => challenge["audience"] = json!("99".repeat(32)),
            1 => challenge["challenge"] = json!("not-a-nonce"),
            2 => challenge["server_time"] = json!(0),
            3 => challenge["protocol"] = json!("other-protocol"),
            _ => challenge["extra"] = json!(true),
        }
        receiver.send(challenge).await;
        let observations = receiver.observations.clone();
        receiver.finish().await;
        let state = observations.lock().unwrap();
        assert!(state.signed.is_empty());
        assert!(state.clock.is_none());
        assert_eq!(state.captures, 0);
    }
    let mut receiver = Receiver::new();
    receiver.writer.encoder_mut().set_phase(Phase::Streaming);
    receiver
        .writer
        .send(Frame::new(Kind::Data, Bytes::from_static(b"x")))
        .await
        .unwrap();
    receiver.finish().await;
}

#[tokio::test(start_paused = true)]
async fn a_receiver_that_stops_reading_cannot_hold_a_camera_buffer_indefinitely() {
    let mut receiver = Receiver::new();
    receiver.observations.lock().unwrap().bytes = Some(128 * 1024);
    receiver.authenticate().await;
    receiver.demand("epoch", 1).await;
    for _ in 0..20 {
        if receiver.observations.lock().unwrap().captures > 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(receiver.observations.lock().unwrap().captures, 1);
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    assert!(receiver.client.is_finished());
    assert!(!receiver.observations.lock().unwrap().active);
    receiver.finish().await;
}

#[tokio::test]
async fn stale_receipts_close_the_session_and_release_camera() {
    let mut receiver = Receiver::new();
    receiver.authenticate().await;
    receiver.demand("epoch", 1).await;
    let metadata = receiver.frame().await;
    receiver.send(json!({"type":"published","epoch":"wrong", "sequence":metadata["sequence"],"retained":true})).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(receiver.client.is_finished());
    receiver.finish().await;
}

#[tokio::test]
async fn oversized_and_aged_frames_are_dropped_without_consuming_sequences() {
    for (length, age) in [
        (128 * 1024 + 1, Duration::ZERO),
        (64, Duration::from_secs(6)),
    ] {
        let mut receiver = Receiver::new();
        {
            let mut state = receiver.observations.lock().unwrap();
            state.bytes = Some(length);
            state.age = age;
        }
        receiver.authenticate().await;
        receiver.demand("epoch", 1).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(450), receiver.reader.next())
                .await
                .is_err()
        );
        assert!(receiver.observations.lock().unwrap().captures > 0);
        {
            let mut state = receiver.observations.lock().unwrap();
            state.bytes = Some(64);
            state.age = Duration::ZERO;
        }
        let metadata = receiver.frame().await;
        assert_eq!(metadata["sequence"], 1);
        receiver.receipt(&metadata).await;
        receiver.finish().await;
    }
}

#[tokio::test(start_paused = true)]
async fn authentication_and_unacknowledged_frames_have_deadlines() {
    let receiver = Receiver::new();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(6)).await;
    receiver.finish().await;
    let mut receiver = Receiver::new();
    receiver.authenticate().await;
    receiver.demand("epoch", 1).await;
    receiver.frame().await;
    tokio::time::advance(Duration::from_secs(11)).await;
    tokio::task::yield_now().await;
    assert!(receiver.client.is_finished());
    receiver.finish().await;
}
