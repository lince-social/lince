use ailuros_protocol::{Codec, Frame, HEADER_BYTES, Kind, Phase};
use engine::{
    Engine,
    actions::Action,
    ailuros::{StreamProof, receiver},
    signal_endpoints::{Request, hex},
    signal_streams::Metadata,
    wire::{ALPN_AILUROS, ALPN_SYNC, Wire},
};
use futures::{SinkExt, StreamExt};
use iroh::{
    Endpoint,
    endpoint::{Connection, RecvStream, SendStream},
};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::codec::{FramedRead, FramedWrite};

struct Fixture {
    engine: Arc<Engine>,
    wire: Wire,
    client: Endpoint,
    key: SigningKey,
    grant: store::signal_endpoints::Endpoint,
    serving: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

impl Fixture {
    async fn new(enabled: bool) -> Self {
        Self::with_builder(enabled, false).await
    }

    async fn with_builder(enabled: bool, customized: bool) -> Self {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let organ = store::organs::ensure_local(&engine.store.pool, "http://hardware.test")
            .await
            .unwrap();
        store::cells::ensure_local(&engine.store.pool, &organ.uid, "Hardware test")
            .await
            .unwrap();
        settings(&engine, enabled).await;
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        let saved = engine
            .act(
                Action::SignalEndpoints {
                    request: Request::Save {
                        endpoint: None,
                        expected_revision: None,
                        name: "Camera".into(),
                        public_key: hex(key.verifying_key().to_encoded_point(true).as_bytes()),
                        signal_slug: "camera".into(),
                        enabled: false,
                        cooldown_seconds: 1,
                        minimum: "0".into(),
                        maximum: "100".into(),
                        expires_at: (nucleus::execution::now() + chrono::Duration::days(1))
                            .to_rfc3339(),
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let mut grant: store::signal_endpoints::Endpoint =
            serde_json::from_value(saved["endpoints"][0].clone()).unwrap();
        let enabled = engine
            .act(
                Action::SignalEndpoints {
                    request: Request::SetEnabled {
                        endpoint: grant.uid.clone(),
                        expected_revision: grant.revision,
                        enabled: true,
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        grant = serde_json::from_value(enabled["endpoints"][0].clone()).unwrap();
        engine
            .configure_stream(&grant.signal_uid, Some("jpeg"), "", None, None)
            .await
            .unwrap();
        let secret = iroh::SecretKey::generate();
        let expected = secret.public();
        let wire = if customized {
            Wire::bind_with_endpoint_builder(
                engine.clone(),
                secret,
                engine::wire::Reach::Internet,
                Endpoint::builder(iroh::endpoint::presets::Minimal)
                    .clear_relay_transports()
                    .clear_ip_transports()
                    .secret_key(iroh::SecretKey::generate())
                    .alpns(vec![b"not-lince".to_vec()])
                    .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
                    .unwrap(),
            )
            .await
            .unwrap()
        } else {
            Wire::bind_loopback(engine.clone(), secret).await.unwrap()
        };
        assert_eq!(wire.endpoint().id(), expected);
        let server = wire.clone();
        let serving = tokio::spawn(async move { server.serve().await });
        let client = Endpoint::builder(iroh::endpoint::presets::Minimal)
            .relay_mode(iroh::RelayMode::Disabled)
            .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .unwrap()
            .bind()
            .await
            .unwrap();
        Self {
            engine,
            wire,
            client,
            key,
            grant,
            serving,
        }
    }

    async fn connection(&self) -> Connection {
        tokio::time::timeout(
            Duration::from_secs(5),
            self.client
                .connect(self.wire.endpoint().addr(), ALPN_AILUROS),
        )
        .await
        .unwrap()
        .unwrap()
    }

    async fn stream(&self) -> Stream {
        let connection = self.connection().await;
        let (send, receive) = tokio::time::timeout(Duration::from_secs(5), connection.accept_bi())
            .await
            .unwrap()
            .unwrap();
        Stream {
            connection,
            writer: FramedWrite::new(send, Codec::default()),
            reader: FramedRead::with_capacity(receive, Codec::default(), HEADER_BYTES),
        }
    }
}

async fn settings(engine: &Engine, enabled: bool) {
    engine
        .act(
            Action::SetCellConfig {
                namespace: receiver::NAMESPACE.into(),
                fds: json!({"enabled":enabled,"iroh":true,"https":false}),
            },
            None,
        )
        .await
        .unwrap();
}

struct Stream {
    connection: Connection,
    writer: FramedWrite<SendStream, Codec>,
    reader: FramedRead<RecvStream, Codec>,
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.connection.close(0u32.into(), b"test finished");
    }
}

impl Stream {
    async fn send(&mut self, kind: Kind, value: Vec<u8>) {
        self.writer.send(Frame::new(kind, value)).await.unwrap();
    }

    async fn control(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let frame = self.reader.next().await.unwrap().unwrap();
                assert_eq!(frame.kind, Kind::Control);
                let value: Value = serde_json::from_slice(&frame.payload).unwrap();
                if value["type"] == "ping" {
                    self.send(Kind::Control, br#"{"type":"pong"}"#.to_vec())
                        .await;
                } else {
                    return value;
                }
            }
        })
        .await
        .unwrap()
    }

    async fn authenticate(&mut self, fixture: &Fixture) -> StreamProof {
        let challenge = self.control().await;
        assert_eq!(challenge["type"], "challenge");
        let signed = proof(&challenge, fixture);
        self.send(Kind::Control, serde_json::to_vec(&signed).unwrap())
            .await;
        assert_eq!(self.control().await["type"], "authenticated");
        self.reader.decoder_mut().set_phase(Phase::Streaming);
        self.writer.encoder_mut().set_phase(Phase::Streaming);
        signed
    }

    async fn frame(&mut self, epoch: &str, sequence: u64) {
        self.frame_at(
            epoch,
            sequence,
            nucleus::execution::now().timestamp_millis(),
            true,
        )
        .await;
    }

    async fn write_frame(&mut self, epoch: &str, sequence: u64, captured_at: i64) {
        let metadata = Metadata {
            epoch: epoch.into(),
            sequence,
            captured_at,
            content_type: "image/jpeg".into(),
        };
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode(&[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        self.send(Kind::Metadata, serde_json::to_vec(&metadata).unwrap())
            .await;
        self.send(Kind::Data, jpeg).await;
    }

    async fn frame_at(&mut self, epoch: &str, sequence: u64, captured_at: i64, retained: bool) {
        self.write_frame(epoch, sequence, captured_at).await;
        let published = self.control().await;
        assert_eq!(published["type"], "published");
        assert_eq!(published["sequence"], sequence);
        assert_eq!(published["retained"], retained);
    }
}

#[tokio::test]
async fn custom_builder_keeps_auth_identity_and_socket_scope() {
    let fixture = Fixture::with_builder(true, true).await;
    assert!(
        fixture
            .wire
            .endpoint()
            .bound_sockets()
            .iter()
            .all(|address| address.ip().is_loopback())
    );
    let channel = fixture
        .engine
        .stream_channel(&fixture.grant.signal_uid, None)
        .await
        .unwrap();
    let _viewer = channel.subscribe().unwrap();
    let mut stream = fixture.stream().await;
    stream.authenticate(&fixture).await;
    let demand = stream.control().await;
    stream.frame(demand["epoch"].as_str().unwrap(), 1).await;
    assert!(channel.snapshot().value.is_some());
    settings(&fixture.engine, false).await;
    tokio::time::timeout(Duration::from_secs(5), stream.connection.closed())
        .await
        .unwrap();
    assert!(channel.snapshot().value.is_none());
}

fn proof(challenge: &Value, fixture: &Fixture) -> StreamProof {
    let mut proof = StreamProof {
        audience: challenge["audience"].as_str().unwrap().into(),
        challenge: challenge["challenge"].as_str().unwrap().into(),
        device: fixture.grant.uid.clone(),
        signal: fixture.grant.signal_uid.clone(),
        key_epoch: fixture.grant.credential_epoch,
        signature: String::new(),
    };
    let signature: Signature = fixture.key.sign(&proof.signing_payload().unwrap());
    proof.signature = hex(&signature.to_bytes());
    proof
}

#[tokio::test]
async fn expired_camera_frames_keep_fresh_state_and_sequence_guards() {
    let fixture = Fixture::new(true).await;
    let channel = fixture
        .engine
        .stream_channel(&fixture.grant.signal_uid, None)
        .await
        .unwrap();
    let _viewer = channel.subscribe().unwrap();
    let mut stream = fixture.stream().await;
    stream.authenticate(&fixture).await;
    let demand = stream.control().await;
    let epoch = demand["epoch"].as_str().unwrap();
    stream
        .frame_at(
            epoch,
            1,
            nucleus::execution::now().timestamp_millis() - 10000,
            false,
        )
        .await;
    assert!(channel.snapshot().value.is_none());
    stream.frame(epoch, 2).await;
    stream
        .frame_at(
            epoch,
            3,
            nucleus::execution::now().timestamp_millis() - 10000,
            false,
        )
        .await;
    assert_eq!(channel.snapshot().value.unwrap().metadata.sequence, 2);
    stream.frame(epoch, 4).await;
    assert_eq!(channel.snapshot().value.unwrap().metadata.sequence, 4);
    stream
        .frame_at(
            epoch,
            5,
            nucleus::execution::now().timestamp_millis() - 10000,
            false,
        )
        .await;
    stream
        .write_frame(epoch, 5, nucleus::execution::now().timestamp_millis())
        .await;
    tokio::time::timeout(Duration::from_secs(5), stream.connection.closed())
        .await
        .unwrap();
}

#[tokio::test]
async fn device_stream_tracks_viewers_clears_epochs_and_revokes_without_organ_enrollment() {
    let fixture = Fixture::new(true).await;
    let channel = fixture
        .engine
        .stream_channel(&fixture.grant.signal_uid, None)
        .await
        .unwrap();
    let first = channel.subscribe().unwrap();
    let mut stream = fixture.stream().await;
    stream.authenticate(&fixture).await;
    let initial = stream.control().await;
    assert_eq!(initial["consumers"], 1);
    let epoch = initial["epoch"].as_str().unwrap();
    stream.frame(epoch, 1).await;
    assert!(channel.snapshot().value.is_some());
    let second = channel.subscribe().unwrap();
    assert_eq!(stream.control().await["consumers"], 2);
    drop(first);
    let remaining = stream.control().await;
    assert_eq!(remaining["epoch"], epoch);
    assert_eq!(remaining["consumers"], 1);
    drop(second);
    let stopped = stream.control().await;
    assert_eq!(stopped["active"], false);
    assert_ne!(stopped["epoch"], epoch);
    assert!(channel.snapshot().value.is_none());
    let _reopened = channel.subscribe().unwrap();
    let reopened = stream.control().await;
    stream.frame(reopened["epoch"].as_str().unwrap(), 1).await;
    assert!(
        store::organs::contact_by_node_id(
            &fixture.engine.store.pool,
            &fixture.client.id().to_string()
        )
        .await
        .unwrap()
        .is_none()
    );
    let sync = fixture
        .client
        .connect(fixture.wire.endpoint().addr(), ALPN_SYNC)
        .await;
    if let Ok(sync) = sync {
        tokio::time::timeout(Duration::from_secs(5), sync.closed())
            .await
            .unwrap();
    }
    fixture
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: fixture.grant.uid.clone(),
                    expected_revision: fixture.grant.revision,
                    enabled: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), stream.connection.closed())
        .await
        .unwrap();
    assert!(channel.snapshot().value.is_none());
    assert!(channel.publisher().is_ok());
}

#[tokio::test]
async fn replayed_proofs_and_preauth_data_never_reserve_a_producer() {
    let fixture = Fixture::new(true).await;
    let channel = fixture
        .engine
        .stream_channel(&fixture.grant.signal_uid, None)
        .await
        .unwrap();
    let mut first = fixture.stream().await;
    let challenge = first.control().await;
    let signed = proof(&challenge, &fixture);
    drop(first);
    let mut replay = fixture.stream().await;
    let _challenge = replay.control().await;
    replay
        .send(Kind::Control, serde_json::to_vec(&signed).unwrap())
        .await;
    tokio::time::timeout(Duration::from_secs(5), replay.connection.closed())
        .await
        .unwrap();
    assert!(channel.publisher().is_ok());
    let mut malformed = fixture.stream().await;
    let _challenge = malformed.control().await;
    malformed
        .writer
        .get_mut()
        .write_all(&[Kind::Data as u8, 0, 0, 0, 1])
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), malformed.connection.closed())
        .await
        .unwrap();
    assert!(channel.publisher().is_ok());
}

#[tokio::test]
async fn saved_settings_gate_connections_and_disable_a_running_receiver() {
    let fixture = Fixture::new(false).await;
    let denied = tokio::time::timeout(
        Duration::from_secs(5),
        fixture
            .client
            .connect(fixture.wire.endpoint().addr(), ALPN_AILUROS),
    )
    .await
    .unwrap();
    if let Ok(connection) = denied {
        assert!(
            tokio::time::timeout(Duration::from_secs(5), connection.accept_bi())
                .await
                .unwrap()
                .is_err()
        );
    }
    settings(&fixture.engine, true).await;
    let mut stream = fixture.stream().await;
    stream.authenticate(&fixture).await;
    assert_eq!(stream.control().await["active"], false);
    settings(&fixture.engine, false).await;
    tokio::time::timeout(Duration::from_secs(5), stream.connection.closed())
        .await
        .unwrap();
    assert!(
        !fixture
            .engine
            .ailuros_receiver_settings()
            .await
            .unwrap()
            .enabled
    );
}

#[tokio::test]
async fn receiver_settings_survive_reopening_the_cell_and_invalid_updates_preserve_them() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("receiver.db").display()
    );
    let engine = Engine::open(&url).await.unwrap();
    let organ = store::organs::ensure_local(&engine.store.pool, "http://hardware.test")
        .await
        .unwrap();
    store::cells::ensure_local(&engine.store.pool, &organ.uid, "Hardware test")
        .await
        .unwrap();
    settings(&engine, true).await;
    assert!(
        engine
            .act(
                Action::SetCellConfig {
                    namespace: receiver::NAMESPACE.into(),
                    fds: json!({"enabled":true,"iroh":false,"https":false}),
                },
                None
            )
            .await
            .is_err()
    );
    assert!(engine.ailuros_receiver_settings().await.unwrap().enabled);
    engine.store.pool.close().await;
    drop(engine);
    let reopened = Engine::open(&url).await.unwrap();
    assert_eq!(
        reopened.ailuros_receiver_settings().await.unwrap(),
        receiver::Settings {
            enabled: true,
            iroh: true,
            https: false,
        }
    );
    reopened.store.pool.close().await;
}
