//! Reference websocket driver over the transport-agnostic `Session`
//! (blueprint VII.3). This is the only place that knows about a socket; all the
//! logic lives in `Session`. Enabled by the `axum` feature.
//!
//! One connection multiplexes: inbound client messages (subscriptions,
//! actions, lane sends), outbound subscription snapshots/updates driven by the
//! engine `fact_bus`, and outbound lane events from joined rooms.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use engine::Engine;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::lane::LaneHub;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::session::Session;

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

    let mut session = Session::new(engine.clone(), hub.clone(), connection_id.clone(), subject);
    let mut bus = engine.subscribe();

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
                        spawn_lane_forwarder(&hub, &room, &connection_id, out_tx.clone());
                    }
                    Ok(msg) => {
                        for reply in session.handle(msg).await {
                            if out_tx.send(reply).await.is_err() { break; }
                        }
                    }
                    Err(e) => {
                        let _ = out_tx
                            .send(ServerMessage::Error { id: "-".into(), message: e.to_string() })
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
        }
    }
    writer.abort();
}

fn spawn_lane_forwarder(
    hub: &Arc<LaneHub>,
    room: &str,
    connection_id: &str,
    out_tx: mpsc::Sender<ServerMessage>,
) {
    let mut rx = hub.join(room);
    let me = connection_id.to_string();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if event.from == me {
                continue; // don't echo presence back to the sender
            }
            let msg = ServerMessage::LaneEvent {
                room: event.room,
                from: event.from,
                payload: event.payload,
            };
            if out_tx.send(msg).await.is_err() {
                break;
            }
        }
    });
}
