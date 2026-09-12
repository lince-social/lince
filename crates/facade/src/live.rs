use std::time::Duration;

use axum::{
    extract::{
        State as Extract, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::HeaderMap,
    response::Response,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
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
    auth::same_origin(&headers)?;
    let user = auth::viewer(&state, &headers).await?;
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
            drive(socket, state, headers, user).await;
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
    initial_user: Option<store::auth::AuthUser>,
) {
    let subject = initial_user.as_ref().map(|u| u.uid.clone());
    let mut session = cell::Session::new(
        state.cell.engine.clone(),
        state.cell.lanes.clone(),
        uuid::Uuid::new_v4().to_string(),
        subject.clone(),
    );
    let mut facts = state.cell.engine.subscribe();
    let mut changes = state.cell.engine.watch_query_changes();
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
        let current = match auth::viewer(&state, &headers).await {
            Ok(user) if user.as_ref().map(|u| &u.uid) == subject.as_ref() => user,
            _ => {
                let _ = send(&mut socket, &json!({"type": "logout"})).await;
                break;
            }
        };
        match records::snapshot(&state, current.as_ref(), &selected, &search).await {
            Ok(signals) => {
                if signals != previous {
                    if !send(&mut socket, &json!({"type": "signals", "signals": signals})).await {
                        break;
                    }
                    previous = signals;
                }
            }
            Err(_) => {
                let _ = send(&mut socket, &json!({"type": "logout"})).await;
                break;
            }
        }
        tokio::select! {
            _ = stop.changed() => break,
            _ = timer.tick() => { if !send_ping(&mut socket).await { break; } },
            _ = changes.changed() => {},
            fact = facts.recv() => {
                if matches!(fact, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; }
            },
            incoming = socket.recv() => {
                let Some(Ok(message)) = incoming else { break; };
                let Message::Text(text) = message else {
                    if matches!(message, Message::Close(_)) { break; }
                    continue;
                };
                let user = match auth::viewer(&state, &headers).await {
                    Ok(user) if user.as_ref().map(|u| &u.uid) == subject.as_ref() => user,
                    _ => break,
                };
                let value = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
                if value["type"] == "select" {
                    selected = value["uid"].as_str().filter(|s| s.is_empty() || nucleus::valid_uid(s, "r")).unwrap_or("").to_string();
                    continue;
                }
                if value["type"] == "search" {
                    search = value["text"].as_str().unwrap_or("").chars().take(200).collect();
                    continue;
                }
                let id = value["id"].as_str().unwrap_or("request").chars().take(100).collect::<String>();
                let result = process(&state, user.as_ref(), &selected, &mut session, value).await;
                let messages = result.unwrap_or_else(|message| vec![ServerMessage::Error { id, message, code: Some("facade_request_rejected".into()) }]);
                for message in messages {
                    if !send(&mut socket, &message).await { return; }
                }
            }
        }
    }
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

async fn process(
    state: &State,
    user: Option<&store::auth::AuthUser>,
    selected: &str,
    session: &mut cell::Session,
    value: Value,
) -> Result<Vec<ServerMessage>, String> {
    let message: ClientMessage =
        serde_json::from_value(value).map_err(|_| "Invalid request.".to_string())?;
    let action = match &message {
        ClientMessage::SessionAuthenticate { .. } => return Ok(session.handle(message).await),
        ClientMessage::SignedAct { action_base64, .. } => {
            let bytes = STANDARD
                .decode(action_base64)
                .map_err(|_| "Invalid Action.".to_string())?;
            serde_json::from_slice(&bytes).map_err(|_| "Invalid Action.".to_string())?
        }
        ClientMessage::Act { action, .. } if user.is_none() => action.clone(),
        _ => return Err("This request is not available in Facade.".into()),
    };
    let _management = if matches!(
        &action,
        engine::actions::Action::CreateUser { .. }
            | engine::actions::Action::CreateRole { .. }
            | engine::actions::Action::AssignRole { .. }
            | engine::actions::Action::GrantPermission { .. }
            | engine::actions::Action::RevokePermission { .. }
            | engine::actions::Action::SetPersonReadFilter { .. }
            | engine::actions::Action::SetRoleReadRules { .. }
    ) {
        Some(state.auth.management.lock().await)
    } else {
        None
    };
    let current = if let Some(user) = user {
        Some(
            store::auth::user_by_uid(&state.cell.store.pool, &user.uid)
                .await
                .map_err(|_| "Could not check permissions.".to_string())?
                .ok_or_else(|| "Please log in.".to_string())?,
        )
    } else {
        None
    };
    records::allow(state, current.as_ref(), selected, &action)
        .await
        .map_err(|(_, message)| message)?;
    Ok(session.handle(message).await)
}
