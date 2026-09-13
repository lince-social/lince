use std::time::Duration;

use axum::{
    extract::{
        State as Extract, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::HeaderMap,
    response::Response,
};
use cell::{ClientMessage, ServerMessage};
use serde_json::{Value, json};

use crate::{
    State,
    auth::{self, Failure},
    records,
};

pub(crate) async fn upgrade(
    Extract(state): Extract<State>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, Failure> {
    state.security.same_origin(&headers)?;
    let browser = auth::authenticated(&state, &headers).await?;
    let browser_permit = browser
        .connections
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "Too many connections for this login".into(),
            )
        })?;
    let permit = state.connections.clone().try_acquire_owned().map_err(|_| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Too many connections.".into(),
        )
    })?;
    Ok(ws
        .max_message_size(128 * 1024)
        .max_frame_size(128 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            let _browser_permit = browser_permit;
            drive(socket, state, headers, browser.login).await;
        }))
}

async fn send(socket: &mut WebSocket, value: &impl serde::Serialize) -> bool {
    let Ok(body) = serde_json::to_string(value) else {
        return false;
    };
    matches!(
        tokio::time::timeout(
            Duration::from_secs(10),
            socket.send(Message::Text(body.into()))
        )
        .await,
        Ok(Ok(()))
    )
}

async fn drive(
    mut socket: WebSocket,
    state: State,
    headers: HeaderMap,
    login: engine::login::LoginSession,
) {
    let subject = Some(login.person_uid().to_string());
    let mut session = cell::Session::authenticated(
        state.cell.engine.clone(),
        state.cell.lanes.clone(),
        uuid::Uuid::new_v4().to_string(),
        login.clone(),
    );
    let mut events = cell::SyncEvents::new(&state.cell.engine);
    let mut revoked = login.watch_revocation();
    let mut rate_window = std::time::Instant::now();
    let mut message_count = 0;
    let mut last_received = std::time::Instant::now();
    let mut stop = state.stop.clone();
    let mut timer = tokio::time::interval(Duration::from_secs(15));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut selected = String::new();
    let mut search = String::new();
    let mut previous = Value::Null;
    if !send(&mut socket, &session.initialize_action_intent().await).await {
        return;
    }
    loop {
        match auth::viewer(&state, &headers).await {
            Ok(user) if user.as_ref().map(|u| &u.uid) == subject.as_ref() => {}
            _ => break,
        };
        let snapshot = login
            .run(&state.cell.engine, false, async {
                let current = store::auth::user_by_uid(&state.cell.store.pool, login.person_uid())
                    .await?
                    .ok_or_else(|| engine::EngineError::Forbidden("Login unavailable".into()))?;
                let signals = records::snapshot(&state, Some(&current), &selected, &search)
                    .await
                    .map_err(|_| engine::EngineError::Forbidden("Cannot read this view".into()))?;
                if signals != previous {
                    if !login.is_live()
                        || !send(&mut socket, &json!({"type": "signals", "signals": signals})).await
                    {
                        return Err(engine::EngineError::Forbidden("Connection ended".into()));
                    }
                    previous = signals;
                }
                Ok(())
            })
            .await;
        match snapshot {
            Ok(()) => {}
            Err(_) => break,
        }
        tokio::select! {
            _ = stop.changed() => break,
            _ = revoked.changed() => break,
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(login.expires())) => break,
            _ = timer.tick() => { if last_received.elapsed() > Duration::from_secs(60) || !send_ping(&mut socket).await { break; } },
            event = events.next(session.has_ephemeral_subscriptions()) => {
                let Some(event) = event else { break; };
                if !session.subject_may_act().await { break; }
                let sent = login.run(&state.cell.engine, false, async {
                    for message in session.on_sync_event(event).await {
                        if !send(&mut socket, &message).await { return Ok(false); }
                    }
                    Ok(true)
                }).await.unwrap_or(false);
                if !sent { break; }
            },
            incoming = socket.recv() => {
                let Some(Ok(message)) = incoming else { break; };
                last_received = std::time::Instant::now();
                if rate_window.elapsed() >= Duration::from_secs(10) { rate_window = last_received; message_count = 0; }
                message_count += 1;
                if message_count > 120 { break; }
                let Message::Text(text) = message else {
                    if matches!(message, Message::Close(_)) { break; }
                    continue;
                };
                match auth::viewer(&state, &headers).await {
                    Ok(user) if user.as_ref().map(|u| &u.uid) == subject.as_ref() => {},
                    _ => break,
                };
                let value = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
                login.touch();
                if value["type"] == "select" {
                    selected = value["uid"].as_str().filter(|s| s.is_empty() || nucleus::valid_uid(s, "r")).unwrap_or("").to_string();
                    continue;
                }
                if value["type"] == "search" {
                    search = value["text"].as_str().unwrap_or("").chars().take(200).collect();
                    continue;
                }
                let id = value["id"].as_str().unwrap_or("request").chars().take(100).collect::<String>();
                let sent = login.run(&state.cell.engine, true, async {
                    let result = process(&mut session, value).await;
                    let messages = result.unwrap_or_else(|message| vec![ServerMessage::Error { id, message, code: Some("invalid_request".into()) }]);
                    for message in messages {
                        if !send(&mut socket, &message).await { return Ok(false); }
                    }
                    Ok(true)
                }).await.unwrap_or(false);
                if !sent { break; }
            }
        }
    }
    let _ = send(&mut socket, &json!({"type":"logout"})).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Close(None))).await;
}

async fn send_ping(socket: &mut WebSocket) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(10),
            socket.send(Message::Ping(Vec::new().into()))
        )
        .await,
        Ok(Ok(()))
    )
}

async fn process(session: &mut cell::Session, value: Value) -> Result<Vec<ServerMessage>, String> {
    let message: ClientMessage =
        serde_json::from_value(value).map_err(|_| "Invalid request.".to_string())?;
    Ok(session.handle(message).await)
}
