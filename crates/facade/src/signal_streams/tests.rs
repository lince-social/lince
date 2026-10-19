use super::*;
use engine::signal_streams::Metadata;
use engine::{actions::Action, signal_endpoints::Request};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as WireMessage, client::IntoClientRequest},
};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn receive(socket: &mut Socket, kind: &str) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                WireMessage::Text(text) => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if value["type"] == kind {
                        return value;
                    }
                }
                WireMessage::Ping(bytes) => socket.send(WireMessage::Pong(bytes)).await.unwrap(),
                message => panic!("Unexpected message {message:?}"),
            }
        }
    })
    .await
    .unwrap()
}

async fn enrollment(state: &State) -> (String, String, String, i64) {
    let saved = state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::Save {
                    endpoint: None,
                    expected_revision: None,
                    name: "Stream".into(),
                    public_key: String::new(),
                    signal_slug: "stream".into(),
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
    let endpoint = saved["endpoints"][0]["uid"].as_str().unwrap().to_string();
    let signal = saved["endpoints"][0]["signal_uid"]
        .as_str()
        .unwrap()
        .to_string();
    let issued = state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::IssueToken {
                    endpoint: endpoint.clone(),
                    expected_revision: 1,
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let credential = issued["issued_token"]["token"]
        .as_str()
        .unwrap()
        .to_string();
    let revision = issued["endpoints"][0]["revision"].as_i64().unwrap();
    let enabled = state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: endpoint.clone(),
                    expected_revision: revision,
                    enabled: true,
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    (
        signal,
        credential,
        endpoint,
        enabled["endpoints"][0]["revision"].as_i64().unwrap(),
    )
}

#[tokio::test]
async fn websocket_frames_follow_demand_and_revocation_without_persistence() {
    let state = crate::tests::fixture().await;
    let (signal, credential, endpoint, revision) = enrollment(&state).await;
    state
        .cell
        .engine
        .configure_stream(&signal, Some("jpeg"), "", None, None)
        .await
        .unwrap();
    let facade = crate::Facade::start(state.cell.clone(), "127.0.0.1:0")
        .await
        .unwrap();
    let origin = format!("http://{}", facade.address);
    let url = format!("ws://{}/signals/{signal}/stream", facade.address);
    assert!(connect_async(url.as_str()).await.is_err());
    let mut request = url.as_str().into_client_request().unwrap();
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {credential}").parse().unwrap(),
    );
    let (mut producer, _) = connect_async(request).await.unwrap();
    let idle = receive(&mut producer, "demand").await;
    assert_eq!(idle["active"], false);
    let response = reqwest::Client::new()
        .post(format!("{origin}/login"))
        .header("Origin", &origin)
        .json(&json!({"username":"fixture-admin","password":"fixture-password"}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let view_url = format!("ws://{}/signals/{signal}/stream/view", facade.address);
    let mut request = view_url.as_str().into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", origin.parse().unwrap());
    request
        .headers_mut()
        .insert("Cookie", cookie.parse().unwrap());
    let (mut viewer, _) = connect_async(request).await.unwrap();
    let demand = receive(&mut producer, "demand").await;
    assert_eq!(demand["consumers"], 1);
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&state.cell.store.pool)
        .await
        .unwrap();
    let mut frame = Vec::new();
    let jpeg = image_fixture();
    frame.extend_from_slice(&jpeg);
    let metadata = Metadata {
        epoch: demand["epoch"].as_str().unwrap().into(),
        sequence: 1,
        captured_at: chrono::Utc::now().timestamp_millis(),
        content_type: "image/jpeg".into(),
    };
    producer
        .send(WireMessage::Text(
            serde_json::to_string(&metadata).unwrap().into(),
        ))
        .await
        .unwrap();
    producer
        .send(WireMessage::Binary(frame.clone().into()))
        .await
        .unwrap();
    assert_eq!(receive(&mut producer, "published").await["retained"], true);
    let received = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match viewer.next().await.unwrap().unwrap() {
                WireMessage::Binary(bytes) => return bytes,
                WireMessage::Ping(bytes) => viewer.send(WireMessage::Pong(bytes)).await.unwrap(),
                WireMessage::Text(_) => {}
                message => panic!("Unexpected {message:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(received.as_ref(), frame.as_slice());
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&state.cell.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    viewer.close(None).await.unwrap();
    let ended = receive(&mut producer, "demand").await;
    assert_eq!(ended["consumers"], 0);
    assert_ne!(ended["epoch"], demand["epoch"]);
    assert!(
        state
            .cell
            .engine
            .stream_channel(&signal, None)
            .await
            .unwrap()
            .snapshot()
            .value
            .is_none()
    );
    state
        .cell
        .engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint,
                    expected_revision: revision,
                    enabled: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match producer.next().await {
                None | Some(Ok(WireMessage::Close(_))) | Some(Err(_)) => break,
                Some(Ok(WireMessage::Ping(bytes))) => {
                    producer.send(WireMessage::Pong(bytes)).await.unwrap()
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
}

fn image_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut bytes)
        .encode(&[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

#[tokio::test]
async fn http_memory_scalar_retains_only_current_value_and_honors_endpoint_bounds() {
    let state = crate::tests::fixture().await;
    let (signal, credential, _, _) = enrollment(&state).await;
    state
        .cell
        .engine
        .configure_stream(&signal, Some("scalar"), "", None, None)
        .await
        .unwrap();
    let channel = state
        .cell
        .engine
        .stream_channel(&signal, None)
        .await
        .unwrap();
    let consumer = channel.subscribe().unwrap();
    let facade =
        crate::Facade::start_public(state.cell.clone(), "127.0.0.1:0", "https://lince.test")
            .await
            .unwrap();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&state.cell.store.pool)
        .await
        .unwrap();
    let url = format!("http://{}/signals/{signal}/readings", facade.address);
    let client = reqwest::Client::new();
    let post = |sequence, value| {
        client.post(&url).header("host", "lince.test").header("x-forwarded-proto", "https")
        .bearer_auth(&credential).json(&json!({"sequence":sequence,"issued_at":chrono::Utc::now().timestamp(),"value":value}))
    };
    let accepted = post(1, "37").send().await.unwrap();
    assert!(accepted.status().is_success());
    assert_eq!(
        accepted.json::<serde_json::Value>().await.unwrap()["storage"],
        "memory"
    );
    assert_eq!(
        state
            .cell
            .engine
            .signal_streams
            .scalar(&signal)
            .unwrap()
            .to_string(),
        "37"
    );
    assert_eq!(
        post(2, "101").send().await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(1, "37").send().await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        state
            .cell
            .engine
            .signal_streams
            .scalar(&signal)
            .unwrap()
            .to_string(),
        "37"
    );
    drop(consumer);
    let ignored = post(2, "12")
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(ignored["retained"], false);
    assert!(channel.snapshot().value.is_none());
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&state.cell.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}
