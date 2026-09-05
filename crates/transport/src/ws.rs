use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use engine::Engine;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::lane::LaneHub;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::session::Session;
use crate::terminal::{TerminalHost, pty_size};

const EPHEMERAL_TICK: std::time::Duration = std::time::Duration::from_secs(3);

pub async fn serve(
    engine: Arc<Engine>,
    hub: Arc<LaneHub>,
    connection_id: String,
    subject: Option<String>,
    socket: WebSocket,
) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(256);
    let (terminal_done_tx, mut terminal_done_rx) = mpsc::channel::<String>(32);

    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let Ok(text) = serde_json::to_string(&msg) else {
                continue;
            };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let viewer = subject.clone();
    let mut session = Session::new(engine.clone(), hub.clone(), connection_id.clone(), subject);
    let challenge = session.initialize_action_intent().await;
    if out_tx.send(challenge).await.is_err() {
        writer.abort();
        return;
    }
    let mut notifications = engine.watch_notifications();
    if let Ok(items) = engine.notifications().await {
        if out_tx
            .send(ServerMessage::Notifications { items })
            .await
            .is_err()
        {
            writer.abort();
            return;
        }
    }
    let mut terminals = TerminalHost::new();
    let mut bus = engine.subscribe();
    let mut ephemeral = tokio::time::interval(EPHEMERAL_TICK);
    ephemeral.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                let text = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => break,
                    _ => continue,
                };
                if !session.subject_may_act().await { break; }
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::LaneJoin { room }) => {
                        session.handle(ClientMessage::LaneJoin { room: room.clone() }).await;
                        spawn_lane_forwarder(
                            &hub,
                            &room,
                            &connection_id,
                            out_tx.clone(),
                            engine.clone(),
                            viewer.clone(),
                        );
                    }
                    Ok(ClientMessage::TerminalOpen {
                        id,
                        cols,
                        rows,
                        pixel_width,
                        pixel_height,
                    }) => {
                        let result = terminals
                            .open(
                                id.clone(),
                                pty_size(cols, rows, pixel_width, pixel_height),
                                out_tx.clone(),
                                terminal_done_tx.clone(),
                            )
                            .await;
                        send_terminal_error(result, id, &out_tx).await;
                    }
                    Ok(ClientMessage::TerminalInput { id, data_base64 }) => {
                        let result = terminals.input(&id, &data_base64).await;
                        send_terminal_error(result, id, &out_tx).await;
                    }
                    Ok(ClientMessage::TerminalResize {
                        id,
                        cols,
                        rows,
                        pixel_width,
                        pixel_height,
                    }) => {
                        let result = terminals
                            .resize(&id, pty_size(cols, rows, pixel_width, pixel_height))
                            .await;
                        send_terminal_error(result, id, &out_tx).await;
                    }
                    Ok(ClientMessage::TerminalClose { id }) => {
                        let result = terminals.close(&id).await;
                        send_terminal_error(result, id, &out_tx).await;
                    }
                    Ok(msg) => {
                        for reply in session.handle(msg).await {
                            if out_tx.send(reply).await.is_err() { break; }
                        }
                    }
                    Err(e) => {
                        let _ = out_tx
                            .send(ServerMessage::Error { id: "-".into(), message: e.to_string(), code: None })
                            .await;
                    }
                }
            }
            fact = bus.recv() => {
                let Ok(fact) = fact else { continue };
                if !session.subject_may_act().await { break; }
                for update in session.on_fact(&fact).await {
                    if out_tx.send(update).await.is_err() { break; }
                }
            }
            _ = ephemeral.tick(), if session.has_ephemeral_subscriptions() => {
                if !session.subject_may_act().await { break; }
                for update in session.tick_ephemeral().await {
                    if out_tx.send(update).await.is_err() { break; }
                }
            }
            Ok(()) = notifications.changed() => {
                let Ok(items) = engine.notifications().await else { continue };
                if out_tx.send(ServerMessage::Notifications { items }).await.is_err() { break; }
            }
            Some(id) = terminal_done_rx.recv() => {
                terminals.forget(&id);
            }
        }
    }
    terminals.shutdown().await;
    writer.abort();
}

async fn send_terminal_error(
    result: Result<(), String>,
    id: String,
    out_tx: &mpsc::Sender<ServerMessage>,
) {
    if let Err(message) = result {
        let _ = out_tx
            .send(ServerMessage::Error {
                id,
                message,
                code: None,
            })
            .await;
    }
}

fn spawn_lane_forwarder(
    hub: &Arc<LaneHub>,
    room: &str,
    connection_id: &str,
    out_tx: mpsc::Sender<ServerMessage>,
    engine: Arc<Engine>,
    viewer: Option<String>,
) {
    let mut rx = hub.join(room);
    let me = connection_id.to_string();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if event.from == me {
                continue;
            }
            let identity = match &event.from_subject {
                Some(subject) => match engine.may_read_record(viewer.as_deref(), subject).await {
                    Ok(true) => Some(subject.clone()),
                    _ => None,
                },
                None => None,
            };
            let msg = ServerMessage::LaneEvent {
                room: event.room,
                from: event.from,
                payload: event.payload,
                identity,
                organ: event.organ,
            };
            if out_tx.send(msg).await.is_err() {
                break;
            }
        }
    });
}
