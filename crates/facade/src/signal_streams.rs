use crate::{
    State,
    auth::{self, Failure},
};
use axum::{
    extract::{
        Path, State as Extract, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use engine::signal_streams::{
    MAX_PAYLOAD,
    ingress::{Credential, PublisherSession},
};
use serde_json::json;
use std::time::{Duration, Instant};

fn denied() -> Failure {
    (StatusCode::FORBIDDEN, "Signal stream access refused".into())
}

fn token(headers: &HeaderMap) -> Result<String, Failure> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(denied)?;
    if values.next().is_some() {
        return Err(denied());
    }
    Ok(value.into())
}

pub(crate) async fn publish(
    Extract(state): Extract<State>,
    Path(signal): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, Failure> {
    state.security.request(&headers)?;
    if headers.contains_key(header::ORIGIN) {
        state.security.same_origin(&headers)?;
    }
    let value = token(&headers)?;
    let endpoint = state
        .cell
        .engine
        .stream_token(&signal, &value)
        .await
        .map_err(|_| denied())?;
    let credential = Credential::Token {
        value,
        epoch: endpoint.credential_epoch,
    };
    let session = PublisherSession::open(&state.cell.engine, &signal, credential)
        .await
        .map_err(open_failure)?;
    let permit = state
        .connections
        .clone()
        .try_acquire_owned()
        .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "Too many connections".into()))?;
    Ok(ws
        .max_message_size(MAX_PAYLOAD)
        .max_frame_size(MAX_PAYLOAD)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            run_publisher(state, session, socket).await;
        }))
}

pub(crate) async fn ailuros_publish(
    Extract(state): Extract<State>,
    Path(signal): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, Failure> {
    if !state.security.public || headers.contains_key(header::AUTHORIZATION) {
        return Err(denied());
    }
    state.security.request(&headers)?;
    if headers.contains_key(header::ORIGIN) {
        state.security.same_origin(&headers)?;
    }
    let ingress = state.signal_endpoint_ingress.admit().await?;
    let permit = state
        .connections
        .clone()
        .try_acquire_owned()
        .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "Too many connections".into()))?;
    let (audience, challenge) = state
        .cell
        .engine
        .ailuros_stream_challenge()
        .await
        .map_err(|_| denied())?;
    Ok(ws.max_message_size(engine::ailuros::STREAM_MAX_PAYLOAD)
        .max_frame_size(engine::ailuros::STREAM_MAX_PAYLOAD)
        .on_upgrade(move |mut socket| async move {
            let _permit = permit;
            if !send(&mut socket, json!({"type":"challenge", "audience":audience,
                "challenge":challenge, "expires_in":engine::ailuros::AUTH_TIMEOUT_SECONDS,
                "protocol":"lince-ailuros-stream-v1", "server_time":nucleus::execution::now().timestamp()})).await { return; }
            let proof = tokio::time::timeout(Duration::from_secs(engine::ailuros::AUTH_TIMEOUT_SECONDS), async {
                let Some(Ok(Message::Text(text))) = socket.recv().await else { return None; };
                if text.len() > engine::ailuros::AUTH_MAX_PAYLOAD { return None; }
                let proof: engine::ailuros::StreamProof = serde_json::from_str(&text).ok()?;
                state.cell.engine.authenticate_ailuros_stream(&proof, &challenge, &signal).await.ok()
            }).await.ok().flatten();
            drop(ingress);
            let Some(identity) = proof else {
                let _ = send_message(&mut socket, Message::Close(None)).await;
                return;
            };
            let Ok(session) = PublisherSession::open(&state.cell.engine, &signal, Credential::Device(identity)).await else {
                let _ = send(&mut socket, json!({"type":"error","message":"Signal producer unavailable"})).await;
                let _ = send_message(&mut socket, Message::Close(None)).await;
                return;
            };
            if !send(&mut socket, json!({"type":"authenticated", "max_frame_bytes":engine::ailuros::STREAM_MAX_PAYLOAD,
                "max_fps":engine::ailuros::STREAM_MAX_FPS, "max_width":1920, "max_height":1080})).await { return; }
            run_publisher(state, session, socket).await;
        }))
}

fn open_failure(error: engine::EngineError) -> Failure {
    match error {
        engine::EngineError::Conflict {
            code: "signal_producer_busy",
            ..
        } => (StatusCode::CONFLICT, "Signal already has a producer".into()),
        _ => denied(),
    }
}

async fn run_publisher(state: State, mut session: PublisherSession, mut socket: WebSocket) {
    let mut demand = session.demand_changes();
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    let mut stop = state.stop.clone();
    let mut authority = state.cell.engine.watch_query_changes();
    loop {
        match session.demand() {
            Ok(Some(message)) => {
                if !send(&mut socket, message).await {
                    break;
                }
            }
            Ok(None) => {}
            Err(_) => break,
        }
        tokio::select! {
            _ = stop.changed() => break,
            _ = authority.changed() => {
                if session.authorize(&state.cell.engine).await.is_err() { break; }
            },
            _ = demand.changed() => {},
            _ = timer.tick() => {
                if session.expired() { break; }
                if session.authorize(&state.cell.engine).await.is_err() { break; }
                if !send_message(&mut socket, Message::Ping(Vec::new().into())).await { break; }
            }
            incoming = socket.recv() => {
                let Some(Ok(message)) = incoming else { break; };
                if session.received(Instant::now()).is_err() { break; }
                match message {
                    Message::Text(text) => {
                        if session.metadata(&text).is_err() { break; }
                    }
                    Message::Binary(bytes) => {
                        match session.publish(&state.cell.engine, &bytes).await {
                            Ok(receipt) => { if !send(&mut socket, receipt).await { break; } }
                            Err(_) => { let _ = send(&mut socket, json!({"type":"error","message":"Invalid frame, type, epoch or sequence"})).await; break; }
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(bytes) => { if !send_message(&mut socket, Message::Pong(bytes)).await { break; } }
                    _ => {},
                }
            }
        }
    }
    let _ = send_message(&mut socket, Message::Close(None)).await;
}

pub(crate) async fn view(
    Extract(state): Extract<State>,
    Path(signal): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, Failure> {
    state.security.same_origin(&headers)?;
    let browser = auth::authenticated(&state, &headers).await?;
    let actor = browser.login.person_uid().to_string();
    let channel = browser
        .login
        .run(
            &state.cell.engine,
            false,
            state.cell.engine.stream_channel(&signal, Some(&actor)),
        )
        .await
        .map_err(|_| denied())?;
    let mut subscription = channel.subscribe().map_err(|_| denied())?;
    let permit = state
        .connections
        .clone()
        .try_acquire_owned()
        .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "Too many connections".into()))?;
    let browser_permit = browser
        .connections
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many connections for login".into(),
            )
        })?;
    Ok(ws.max_message_size(4096).max_frame_size(4096).on_upgrade(move |mut socket| async move {
        let _permit = permit;
        let _browser_permit = browser_permit;
        let mut stop = state.stop.clone();
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        let mut received = Instant::now();
        let mut authority = state.cell.engine.watch_query_changes();
        let mut sent_value: Option<(String, u64)> = None;
        loop {
            let current = browser.login.run(&state.cell.engine, false, state.cell.engine.stream_channel(&signal, Some(&actor))).await;
            if !current.is_ok_and(|current| std::sync::Arc::ptr_eq(&current, &channel)) { break; }
            let snapshot = subscription.updates.borrow_and_update().clone();
            if snapshot.status == "Configuration changed" { break; }
            if !send(&mut socket, json!({"type":"status","epoch":snapshot.epoch,"status":snapshot.status})).await { break; }
            if let Some(value) = snapshot.value {
                let key = (value.metadata.epoch.clone(), value.metadata.sequence);
                if sent_value.as_ref() != Some(&key) {
                let Ok(metadata) = serde_json::to_string(&value.metadata) else { break; };
                if !send_message(&mut socket, Message::Text(metadata.into())).await || !send_message(&mut socket, Message::Binary(value.bytes.to_vec().into())).await { break; }
                sent_value = Some(key);
                }
            }
            tokio::select! {
                _ = stop.changed() => break,
                _ = authority.changed() => {},
                result = subscription.updates.changed() => { if result.is_err() { break; } },
                _ = timer.tick() => {
                    if received.elapsed() > Duration::from_secs(30) || !send_message(&mut socket, Message::Ping(Vec::new().into())).await { break; }
                },
                message = socket.recv() => {
                    match message {
                        Some(Ok(Message::Pong(_))) => received = Instant::now(),
                        Some(Ok(Message::Ping(bytes))) => { received = Instant::now(); if !send_message(&mut socket, Message::Pong(bytes)).await { break; } },
                        _ => break,
                    }
                }
            }
        }
        let _ = send_message(&mut socket, Message::Close(None)).await;
    }))
}

async fn send(socket: &mut WebSocket, value: serde_json::Value) -> bool {
    send_message(socket, Message::Text(value.to_string().into())).await
}

async fn send_message(socket: &mut WebSocket, message: Message) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(2), socket.send(message)).await,
        Ok(Ok(()))
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ailuros_tests;
