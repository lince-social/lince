//! Reference websocket driver over the transport-agnostic `Session`
//! (blueprint VII.3). This is the only place that knows about a socket; all the
//! logic lives in `Session`. Enabled by the `axum` feature.
//!
//! One connection multiplexes: inbound client messages (subscriptions,
//! actions, lane sends, terminal capability frames), outbound subscription
//! snapshots/updates driven by the engine `fact_bus`, and outbound ephemeral
//! events/streams.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use engine::Engine;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::lane::LaneHub;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::session::Session;
use crate::terminal::{TerminalHost, pty_size};

/// How often a subscription reading process state is re-run. Discovery on a
/// LAN moves on a human timescale — a device is carried into a room, not
/// teleported — so this trades a couple of seconds of staleness for a socket
/// that stays silent almost always.
const EPHEMERAL_TICK: std::time::Duration = std::time::Duration::from_secs(3);

/// Drive one connection to completion. `subject = None` is the local Cell;
/// `Some(id)` applies that subject's visibility to every read.
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

    // Writer task: drain outbound messages to the socket.
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

    // The viewer identity is needed twice: by the Session as its visibility
    // subject, and by each lane forwarder to decide whether a cursor is named.
    let viewer = subject.clone();
    let mut session = Session::new(engine.clone(), hub.clone(), connection_id.clone(), subject);
    // The challenge is always the first application frame. Remote clients
    // must bind their mapped Person key before any Action; local trusted mode
    // is announced explicitly so clients never infer it from missing fields.
    let challenge = session.initialize_action_intent().await;
    if out_tx.send(challenge).await.is_err() {
        writer.abort();
        return;
    }
    // Subscribed BEFORE the snapshot is read, not after. `subscribe()` marks
    // the current value seen, so ordering it first is free — and the other
    // order drops an invite that lands between the read and the subscribe,
    // leaving the board wrong until something unrelated happens to change the
    // list again.
    let mut notifications = engine.watch_notifications();
    // The snapshot. A client that only ever received pushes would learn about
    // invites that arrived while it was connected and nothing else — so an
    // invite waiting since before the board opened would stay invisible.
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
    // Sources that read process state (who is on the LAN) commit no Facts, so
    // the bus can never wake them. This tick is their only refresh path — and
    // the session pushes only when the answer changed, so a quiet network
    // costs one query per interval and no traffic.
    let mut ephemeral = tokio::time::interval(EPHEMERAL_TICK);
    ephemeral.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            // inbound client message
            incoming = stream.next() => {
                // Stream ended or errored -> the connection is gone.
                let Some(Ok(message)) = incoming else { break };
                // Real browsers send Ping/Pong (keep-alive) and Close frames on
                // the same stream. Only Text carries a ClientMessage; ignore
                // control/binary frames (breaking on them would drop the socket
                // mid-session and starve live updates), and close on Close.
                let text = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => break,
                    _ => continue,
                };
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
            // a committed fact: push live updates for affected subscriptions
            fact = bus.recv() => {
                let Ok(fact) = fact else { continue };
                for update in session.on_fact(&fact).await {
                    if out_tx.send(update).await.is_err() { break; }
                }
            }
            // Guarded rather than always-armed: a session with no ephemeral
            // subscription must not run a timer at all.
            _ = ephemeral.tick(), if session.has_ephemeral_subscriptions() => {
                for update in session.tick_ephemeral().await {
                    if out_tx.send(update).await.is_err() { break; }
                }
            }
            // An invite arrived or was answered. Nothing here is per-session:
            // notifications are a property of the Cell, so every open board
            // gets the same list.
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
                continue; // dont echo presence back to the sender
            }
            // Presence has two halves. The cursor POSITION goes to everyone in
            // the room; WHO it belongs to is disclosed only when this viewer
            // may read that user. Without the permission the sand still
            // renders the cursor, unnamed — which is the designed behaviour,
            // not a degraded one.
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
