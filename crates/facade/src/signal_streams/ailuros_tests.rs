use super::*;
use engine::{
    actions::Action,
    ailuros::StreamProof,
    signal_endpoints::{Request, hex},
    signal_streams::Metadata,
};
use futures::{SinkExt, StreamExt};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as WireMessage, client::IntoClientRequest},
};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn next_json(socket: &mut Socket) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                WireMessage::Text(text) => return serde_json::from_str(&text).unwrap(),
                WireMessage::Ping(bytes) => socket.send(WireMessage::Pong(bytes)).await.unwrap(),
                other => panic!("Expected JSON, got {other:?}"),
            }
        }
    })
    .await
    .unwrap()
}

async fn closed(socket: &mut Socket) {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match socket.next().await {
                None | Some(Err(_)) | Some(Ok(WireMessage::Close(_))) => return,
                Some(Ok(WireMessage::Ping(bytes))) => {
                    if socket.send(WireMessage::Pong(bytes)).await.is_err() {
                        return;
                    }
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
}

async fn fixture() -> (
    State,
    crate::Facade,
    SigningKey,
    store::signal_endpoints::Endpoint,
) {
    let state = crate::tests::fixture().await;
    let key = SigningKey::from_slice(&[7; 32]).unwrap();
    let saved = state
        .cell
        .engine
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
                    expires_at: (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339(),
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let row: store::signal_endpoints::Endpoint =
        serde_json::from_value(saved["endpoints"][0].clone()).unwrap();
    let enabled = state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: row.uid,
                    expected_revision: row.revision,
                    enabled: true,
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let row: store::signal_endpoints::Endpoint =
        serde_json::from_value(enabled["endpoints"][0].clone()).unwrap();
    state
        .cell
        .engine
        .configure_stream(&row.signal_uid, Some("jpeg"), "", None, None)
        .await
        .unwrap();
    let facade =
        crate::Facade::start_public(state.cell.clone(), "127.0.0.1:0", "https://lince.test")
            .await
            .unwrap();
    (state, facade, key, row)
}

async fn connect(facade: &crate::Facade, signal: &str) -> Socket {
    let url = format!("ws://{}/ailuros/streams/{signal}", facade.address);
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("host", "lince.test".parse().unwrap());
    request
        .headers_mut()
        .insert("x-forwarded-proto", "https".parse().unwrap());
    connect_async(request).await.unwrap().0
}

fn proof(
    challenge: &serde_json::Value,
    row: &store::signal_endpoints::Endpoint,
    key: &SigningKey,
) -> StreamProof {
    let mut proof = StreamProof {
        audience: challenge["audience"].as_str().unwrap().into(),
        challenge: challenge["challenge"].as_str().unwrap().into(),
        device: row.uid.clone(),
        key_epoch: row.credential_epoch,
        signal: row.signal_uid.clone(),
        signature: String::new(),
    };
    let signature: Signature = key.sign(&proof.signing_payload().unwrap());
    proof.signature = hex(&signature.to_bytes());
    proof
}

async fn authenticate(
    socket: &mut Socket,
    row: &store::signal_endpoints::Endpoint,
    key: &SigningKey,
) -> StreamProof {
    let challenge = next_json(socket).await;
    assert_eq!(challenge["type"], "challenge");
    let proof = proof(&challenge, row, key);
    socket
        .send(WireMessage::Text(
            serde_json::to_string(&proof).unwrap().into(),
        ))
        .await
        .unwrap();
    let authenticated = next_json(socket).await;
    assert_eq!(authenticated["type"], "authenticated");
    assert_eq!(
        authenticated["max_frame_bytes"],
        engine::ailuros::STREAM_MAX_PAYLOAD
    );
    proof
}

async fn frame(socket: &mut Socket, epoch: &str, sequence: u64, bytes: Vec<u8>) {
    let metadata = Metadata {
        epoch: epoch.into(),
        sequence,
        captured_at: chrono::Utc::now().timestamp_millis(),
        content_type: "image/jpeg".into(),
    };
    socket
        .send(WireMessage::Text(
            serde_json::to_string(&metadata).unwrap().into(),
        ))
        .await
        .unwrap();
    socket
        .send(WireMessage::Binary(bytes.into()))
        .await
        .unwrap();
}

#[tokio::test]
async fn shared_session_checks_authority_before_reserving_the_signal_producer() {
    let (state, _facade, key, row) = fixture().await;
    let engine = &state.cell.engine;
    let (audience, challenge) = engine.ailuros_stream_challenge().await.unwrap();
    let signed = proof(
        &json!({"audience":audience,"challenge":challenge}),
        &row,
        &key,
    );
    let identity = engine
        .authenticate_ailuros_stream(&signed, &signed.challenge, &row.signal_uid)
        .await
        .unwrap();
    let wrong = PublisherSession::open(
        engine,
        &nucleus::new_uid("r"),
        Credential::Device(identity.clone()),
    )
    .await;
    assert!(
        matches!(wrong, Err(engine::EngineError::Conflict { message, .. }) if message == "Device identity belongs to a different Signal")
    );
    let session = PublisherSession::open(
        engine,
        &row.signal_uid,
        Credential::Device(identity.clone()),
    )
    .await
    .unwrap();
    let duplicate = PublisherSession::open(
        engine,
        &row.signal_uid,
        Credential::Device(identity.clone()),
    )
    .await;
    assert!(matches!(
        duplicate,
        Err(engine::EngineError::Conflict {
            code: "signal_producer_busy",
            ..
        })
    ));
    drop(session);
    drop(
        PublisherSession::open(
            engine,
            &row.signal_uid,
            Credential::Device(identity.clone()),
        )
        .await
        .unwrap(),
    );
    let revision = engine
        .stream_channel(&row.signal_uid, None)
        .await
        .unwrap()
        .config
        .revision;
    let external_configuration = engine
        .configure_stream(
            &row.signal_uid,
            Some("jpeg"),
            "ws://127.0.0.1:9/stream",
            Some(revision),
            None,
        )
        .await
        .unwrap()
        .unwrap();
    let external = PublisherSession::open(
        engine,
        &row.signal_uid,
        Credential::Device(identity.clone()),
    )
    .await;
    assert!(
        matches!(external, Err(engine::EngineError::Forbidden(message)) if message == "Signal uses an external source")
    );
    engine
        .configure_stream(
            &row.signal_uid,
            Some("jpeg"),
            "",
            Some(external_configuration.revision),
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: row.uid,
                    expected_revision: row.revision,
                    enabled: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        PublisherSession::open(engine, &row.signal_uid, Credential::Device(identity))
            .await
            .is_err()
    );
    assert!(
        engine
            .stream_channel(&row.signal_uid, None)
            .await
            .unwrap()
            .publisher()
            .is_ok()
    );
}

#[tokio::test]
async fn signed_camera_stream_reuses_live_signal_and_revokes_without_persisting_frames() {
    let (state, facade, key, row) = fixture().await;
    let channel = state
        .cell
        .engine
        .stream_channel(&row.signal_uid, None)
        .await
        .unwrap();
    let consumer = channel.subscribe().unwrap();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&state.cell.store.pool)
        .await
        .unwrap();
    let mut socket = connect(&facade, &row.signal_uid).await;
    authenticate(&mut socket, &row, &key).await;
    let demand = next_json(&mut socket).await;
    assert_eq!(demand["type"], "demand");
    assert_eq!(demand["active"], true);
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut bytes)
        .encode(&[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    frame(
        &mut socket,
        demand["epoch"].as_str().unwrap(),
        1,
        bytes.clone(),
    )
    .await;
    let ack = next_json(&mut socket).await;
    assert_eq!(ack["retained"], true);
    assert_eq!(ack["epoch"], demand["epoch"]);
    assert_eq!(ack["sequence"], 1);
    assert_eq!(
        channel.snapshot().value.unwrap().bytes.as_ref(),
        bytes.as_slice()
    );
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&state.cell.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    drop(consumer);
    let idle = next_json(&mut socket).await;
    assert_eq!(idle["active"], false);
    assert_ne!(idle["epoch"], demand["epoch"]);
    assert!(channel.snapshot().value.is_none());
    state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: row.uid,
                    expected_revision: row.revision,
                    enabled: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    closed(&mut socket).await;
    assert!(channel.publisher().is_ok());
}

#[tokio::test]
async fn unauthenticated_connections_never_reserve_producers_and_proofs_cannot_move_between_connections()
 {
    let (state, facade, key, row) = fixture().await;
    let channel = state
        .cell
        .engine
        .stream_channel(&row.signal_uid, None)
        .await
        .unwrap();
    let mut pending = connect(&facade, &row.signal_uid).await;
    let challenge = next_json(&mut pending).await;
    let old_proof = proof(&challenge, &row, &key);
    assert!(channel.publisher().is_ok());
    let mut active = connect(&facade, &row.signal_uid).await;
    authenticate(&mut active, &row, &key).await;
    assert_eq!(next_json(&mut active).await["active"], false);
    assert!(channel.publisher().is_err());
    active.close(None).await.unwrap();
    closed(&mut active).await;
    let mut replay = connect(&facade, &row.signal_uid).await;
    let new_challenge = next_json(&mut replay).await;
    assert_ne!(challenge["challenge"], new_challenge["challenge"]);
    replay
        .send(WireMessage::Text(
            serde_json::to_string(&old_proof).unwrap().into(),
        ))
        .await
        .unwrap();
    closed(&mut replay).await;
    pending
        .send(WireMessage::Binary(vec![0; 64].into()))
        .await
        .unwrap();
    closed(&mut pending).await;
    assert!(channel.publisher().is_ok());
}

#[tokio::test]
async fn ailuros_stream_rejects_forged_proof_unknown_fields_and_unverified_transport() {
    let (state, facade, _, row) = fixture().await;
    let url = format!("ws://{}/ailuros/streams/{}", facade.address, row.signal_uid);
    assert!(connect_async(url.as_str()).await.is_err());
    let mut forged = connect(&facade, &row.signal_uid).await;
    let challenge = next_json(&mut forged).await;
    let wrong_key = SigningKey::from_slice(&[8; 32]).unwrap();
    let bad_proof = proof(&challenge, &row, &wrong_key);
    forged
        .send(WireMessage::Text(
            serde_json::to_string(&bad_proof).unwrap().into(),
        ))
        .await
        .unwrap();
    closed(&mut forged).await;
    let mut unknown = connect(&facade, &row.signal_uid).await;
    let challenge = next_json(&mut unknown).await;
    let mut bad_proof = serde_json::to_value(proof(&challenge, &row, &wrong_key)).unwrap();
    bad_proof["command"] = json!("run");
    unknown
        .send(WireMessage::Text(bad_proof.to_string().into()))
        .await
        .unwrap();
    closed(&mut unknown).await;
    let local = crate::Facade::start(state.cell, "127.0.0.1:0")
        .await
        .unwrap();
    assert!(
        connect_async(format!(
            "ws://{}/ailuros/streams/{}",
            local.address, row.signal_uid
        ))
        .await
        .is_err()
    );
}

#[tokio::test]
async fn ailuros_stream_closes_for_repeated_frames_and_authentication_timeout() {
    let (state, facade, key, row) = fixture().await;
    let channel = state
        .cell
        .engine
        .stream_channel(&row.signal_uid, None)
        .await
        .unwrap();
    let _consumer = channel.subscribe().unwrap();
    let mut socket = connect(&facade, &row.signal_uid).await;
    authenticate(&mut socket, &row, &key).await;
    let demand = next_json(&mut socket).await;
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut bytes)
        .encode(&[0, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    frame(
        &mut socket,
        demand["epoch"].as_str().unwrap(),
        1,
        bytes.clone(),
    )
    .await;
    assert_eq!(next_json(&mut socket).await["retained"], true);
    frame(&mut socket, demand["epoch"].as_str().unwrap(), 1, bytes).await;
    assert_eq!(next_json(&mut socket).await["type"], "error");
    closed(&mut socket).await;
    let mut pending = connect(&facade, &row.signal_uid).await;
    assert_eq!(next_json(&mut pending).await["type"], "challenge");
    closed(&mut pending).await;
    assert!(channel.publisher().is_ok());
}
