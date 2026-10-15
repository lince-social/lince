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
use engine::signal_streams::{MAX_PAYLOAD, Metadata};
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
    let credential = token(&headers)?;
    let endpoint = state
        .cell
        .engine
        .stream_token(&signal, &credential)
        .await
        .map_err(|_| denied())?;
    let channel = state
        .cell
        .engine
        .stream_channel(&signal, None)
        .await
        .map_err(|_| denied())?;
    if !channel.config.source_url.is_empty() {
        return Err(denied());
    }
    let producer = channel
        .publisher()
        .map_err(|_| (StatusCode::CONFLICT, "Signal already has a producer".into()))?;
    let permit = state
        .connections
        .clone()
        .try_acquire_owned()
        .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "Too many connections".into()))?;
    Ok(ws.max_message_size(MAX_PAYLOAD).max_frame_size(MAX_PAYLOAD).on_upgrade(move |mut socket| async move {
        let _permit = permit;
        let mut producer = producer;
        let mut metadata: Option<Metadata> = None;
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        let mut last_received = Instant::now();
        let mut window = Instant::now();
        let mut count = 0;
        let mut demand_key = (String::new(), usize::MAX);
        let mut stop = state.stop.clone();
        let mut authority = state.cell.engine.watch_query_changes();
        loop {
            let snapshot = producer.snapshot();
            if snapshot.status == "Configuration changed" { break; }
            let key = (snapshot.epoch.clone(), snapshot.consumers);
            if demand_key != key {
                if !send(&mut socket, json!({"type":"demand", "epoch":snapshot.epoch,"consumers":snapshot.consumers,"active":snapshot.consumers > 0})).await { break; }
                if demand_key.0 != key.0 { metadata = None; }
                demand_key = key;
            }
            tokio::select! {
                _ = stop.changed() => break,
                _ = authority.changed() => {
                    if !state.cell.engine.stream_token(&signal, &credential).await.is_ok_and(|current| current.credential_epoch == endpoint.credential_epoch) { break; }
                },
                _ = producer.demand.changed() => {},
                _ = timer.tick() => {
                    if last_received.elapsed() > Duration::from_secs(30) { break; }
                    let valid = state.cell.engine.stream_token(&signal, &credential).await;
                    if !valid.is_ok_and(|current| current.credential_epoch == endpoint.credential_epoch) { break; }
                    if !send_message(&mut socket, Message::Ping(Vec::new().into())).await { break; }
                }
                incoming = socket.recv() => {
                    let Some(Ok(message)) = incoming else { break; };
                    last_received = Instant::now();
                    if window.elapsed() >= Duration::from_secs(1) { window = Instant::now(); count = 0; }
                    count += 1;
                    if count > 150 { break; }
                    match message {
                        Message::Text(text) => {
                            if text.len() > 4096 || metadata.is_some() { break; }
                            let Ok(value) = serde_json::from_str::<Metadata>(&text) else { break; };
                            metadata = Some(value);
                        }
                        Message::Binary(bytes) => {
                            let Some(value) = metadata.take() else { break; };
                            if value.content_type == "text/decimal" && engine::signal_streams::validate_scalar_endpoint(&endpoint, &bytes).is_err() { break; }
                            let result = state.cell.engine.access_scope(false, async {
                                let current = state.cell.engine.stream_token(&signal, &credential).await?;
                                if current.credential_epoch != endpoint.credential_epoch { return Err(engine::signal_streams::invalid("Credential changed")); }
                                if producer.snapshot().consumers == 0 { return Ok(false); }
                                producer.publish(value, bytes.to_vec()).await
                            }).await;
                            match result {
                                Ok(retained) => { if !send(&mut socket, json!({"type":"published","retained":retained})).await { break; } }
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
    }))
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
