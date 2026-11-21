//! WebSocket driver for a first-party Transport session.

use std::{collections::HashMap, sync::Arc};

use axum::extract::ws::{Message, WebSocket};
use engine::Engine;
use futures::{SinkExt, StreamExt};
use nucleus::sync::{Instance, Queue};
use serde_json::{from_str, to_string};
use tokio::sync::{broadcast, mpsc};

use crate::{
    ClientMessage, LaneEvent, LaneHub, ServerMessage, Session, SyncEvents,
    connection_task::ConnectionTask,
    terminal::{TerminalHost, pty_size},
};

const OUTGOING_CAPACITY: usize = 256;
const TERMINAL_DONE_CAPACITY: usize = 32;

/// Drives an upgraded connection for the supplied session identity.
pub async fn serve(
    engine: Arc<Engine>,
    hub: Arc<LaneHub>,
    connection_id: String,
    subject: Option<String>,
    socket: WebSocket,
) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(OUTGOING_CAPACITY);
    let queued = out_tx.downgrade();
    let relay_connection = Arc::<str>::from(connection_id.as_str());
    let queue_connection = Arc::clone(&relay_connection);
    let _sync_queue = engine.sync_service.observe_queue(move || Queue {
        instance: Instance::interface(&queue_connection, "websocket"),
        incoming: 0,
        outgoing: queued.upgrade().map_or(0, |sender| {
            u64::try_from(sender.max_capacity() - sender.capacity()).unwrap_or_default()
        }),
    });
    let (terminal_done_tx, mut terminal_done_rx) = mpsc::channel::<String>(TERMINAL_DONE_CAPACITY);

    let writer = ConnectionTask::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let Ok(text) = to_string(&msg) else {
                continue;
            };
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let viewer = subject.as_deref().map(Arc::<str>::from);
    let mut session = Session::new(
        Arc::clone(&engine),
        Arc::clone(&hub),
        connection_id,
        subject,
    );
    let challenge = session.initialize_action_intent().await;
    if out_tx.send(challenge).await.is_err() {
        return;
    }
    let mut notifications = engine.watch_notifications();
    if let Ok(items) = engine.notifications().await
        && out_tx
            .send(ServerMessage::Notifications { items })
            .await
            .is_err()
    {
        return;
    }
    let mut terminals = TerminalHost::new();
    let mut sync_events = SyncEvents::new(&engine).with_presence(session.presence_changes());
    let mut lane_tasks = HashMap::<String, ConnectionTask>::new();

    loop {
        tokio::select! {
            () = out_tx.closed() => break,
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                let text = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => break,
                    _ => continue,
                };
                if !session.subject_may_act().await { break; }
                match from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::LaneJoin { room }) => {
                        let replies = session.handle(ClientMessage::LaneJoin { room: room.clone() }).await;
                        if replies.is_empty()
                            && session.joined_rooms().contains(&room)
                            && lane_tasks.get(&room).is_none_or(ConnectionTask::is_finished)
                        {
                            let receiver = hub.join(&room);
                            lane_tasks.insert(room, spawn_lane_forwarder(
                                receiver,
                                Arc::clone(&relay_connection),
                                out_tx.clone(),
                                Arc::clone(&engine),
                                viewer.clone(),
                            ));
                        }
                        if !send_replies(&out_tx, replies).await { break; }
                    }
                    Ok(ClientMessage::LaneLeave { room }) => {
                        if let Some(task) = lane_tasks.remove(&room) {
                            task.cancel().await;
                        }
                        let replies = session.handle(ClientMessage::LaneLeave { room }).await;
                        if !send_replies(&out_tx, replies).await { break; }
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
                        if !send_replies(&out_tx, session.handle(msg).await).await { break; }
                    }
                    Err(e) => {
                        let _ = out_tx
                            .send(ServerMessage::Error { id: "-".into(), message: e.to_string(), code: None })
                            .await;
                    }
                }
            }
            event = sync_events.next(session.has_ephemeral_subscriptions()) => {
                let Some(event) = event else { break };
                if !session.subject_may_act().await { break; }
                if !send_replies(&out_tx, session.on_sync_event(event).await).await { break; }
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
    for (room, task) in lane_tasks {
        task.cancel().await;
        hub.prune(&room);
    }
    terminals.shutdown().await;
    drop(writer);
}

async fn send_replies(out_tx: &mpsc::Sender<ServerMessage>, replies: Vec<ServerMessage>) -> bool {
    for reply in replies {
        if out_tx.send(reply).await.is_err() {
            return false;
        }
    }
    true
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
    mut rx: broadcast::Receiver<LaneEvent>,
    connection_id: Arc<str>,
    out_tx: mpsc::Sender<ServerMessage>,
    engine: Arc<Engine>,
    viewer: Option<Arc<str>>,
) -> ConnectionTask {
    ConnectionTask::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if event.from == connection_id.as_ref() {
                continue;
            }
            if engine
                .require_permission(viewer.as_deref(), "view:stream")
                .await
                .is_err()
            {
                break;
            }
            let identity = if let Some(subject) = event.from_subject
                && matches!(
                    engine.may_read_record(viewer.as_deref(), &subject).await,
                    Ok(true)
                ) {
                Some(subject)
            } else {
                None
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
    })
}
