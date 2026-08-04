//! Live sessions over iroh (Ontology §11 "live mode").
//!
//! The same `Session` state machine the websocket driver runs, driven over a
//! QUIC stream instead. A contact Organ with a login granted opens one against
//! this Cell and gets exactly what a local browser tab gets — Protein
//! subscriptions, Actions, lanes, collab — as the Person their login binds
//! them to, with every read gated by that Person's visibility.
//!
//! Why this rides iroh rather than HTTPS, which is the whole point: a session
//! authenticated by KEY has no hostname to go stale and no certificate bound
//! to one. Change network mid-sentence and QUIC migrates the path under a
//! connection that stays open. The alternative needed a hostname, a
//! certificate and a reverse proxy, and still broke the moment the address
//! changed.
//!
//! Framing is one JSON value per QUIC datagram-sized message, length-prefixed:
//! QUIC gives ordered bytes, not messages, so the boundary has to be written
//! down somewhere.

use std::sync::Arc;

use engine::Engine;
use engine::wire::LiveSessions;
use iroh::endpoint::Connection;
use tokio::sync::mpsc;

use crate::lane::LaneHub;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::session::Session;

/// Ceiling on one frame. A live peer is authenticated but not therefore
/// trusted with unbounded memory.
const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

/// Installs into `Wire` and serves each accepted live connection.
pub struct LiveHost {
    engine: Arc<Engine>,
    hub: Arc<LaneHub>,
}

impl LiveHost {
    pub fn new(engine: Arc<Engine>, hub: Arc<LaneHub>) -> Arc<LiveHost> {
        Arc::new(LiveHost { engine, hub })
    }
}

#[async_trait::async_trait]
impl LiveSessions for LiveHost {
    async fn serve(&self, organ_uid: String, person_uid: String, connection: Connection) {
        // The subject is the Person their LOGIN says they act as — resolved
        // from the handshake-proven Organ, never from anything they sent. This
        // one value is what every visibility decision downstream rests on.
        let connection_id = format!("live:{organ_uid}");
        if let Err(error) = drive(
            self.engine.clone(),
            self.hub.clone(),
            connection_id,
            person_uid,
            connection,
        )
        .await
        {
            tracing::debug!(%organ_uid, %error, "live session ended");
        }
    }
}

async fn read_frame(recv: &mut iroh::endpoint::RecvStream) -> Result<Option<Vec<u8>>, String> {
    let mut len = [0u8; 4];
    match recv.read_exact(&mut len).await {
        Ok(()) => {}
        // A closed stream is how a session ends, not a failure.
        Err(_) => return Ok(None),
    }
    let len = u32::from_be_bytes(len);
    if len > MAX_FRAME_BYTES {
        return Err(format!("frame of {len} bytes is over the cap"));
    }
    let mut buf = vec![0u8; len as usize];
    recv.read_exact(&mut buf)
        .await
        .map_err(|error| format!("short frame: {error}"))?;
    Ok(Some(buf))
}

async fn drive(
    engine: Arc<Engine>,
    hub: Arc<LaneHub>,
    connection_id: String,
    person_uid: String,
    connection: Connection,
) -> Result<(), String> {
    // One bidirectional stream for the whole session: the browser end is one
    // websocket, and multiplexing would buy nothing while making ordering a
    // question that currently has no answer to get wrong.
    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .map_err(|error| format!("no session stream: {error}"))?;

    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(256);
    // Kept beside the session because the lane relay needs it too: it is the
    // viewer whose permission decides whether a cursor gets a name.
    let viewer = person_uid.clone();
    let mut session = Session::new(
        engine.clone(),
        hub.clone(),
        connection_id.clone(),
        Some(person_uid),
    );
    let challenge = session.initialize_action_intent().await;
    let _ = out_tx.send(challenge).await;

    let mut bus = engine.subscribe();
    let mut ephemeral = tokio::time::interval(std::time::Duration::from_secs(3));
    ephemeral.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            frame = read_frame(&mut recv) => {
                let Some(bytes) = frame? else { break };
                match serde_json::from_slice::<ClientMessage>(&bytes) {
                    Ok(ClientMessage::LaneJoin { room }) => {
                        session.handle(ClientMessage::LaneJoin { room: room.clone() }).await;
                        spawn_lane_relay(&hub, &room, &connection_id, out_tx.clone(), engine.clone(), Some(viewer.clone()));
                    }
                    Ok(message) => {
                        for reply in session.handle(message).await {
                            if out_tx.send(reply).await.is_err() { break; }
                        }
                    }
                    Err(error) => {
                        let _ = out_tx.send(ServerMessage::Error {
                            id: "-".into(),
                            message: error.to_string(),
                            code: None,
                        }).await;
                    }
                }
            }
            fact = bus.recv() => {
                let Ok(fact) = fact else { continue };
                for update in session.on_fact(&fact).await {
                    if out_tx.send(update).await.is_err() { break; }
                }
            }
            _ = ephemeral.tick(), if session.has_ephemeral_subscriptions() => {
                for update in session.tick_ephemeral().await {
                    if out_tx.send(update).await.is_err() { break; }
                }
            }
            outgoing = out_rx.recv() => {
                let Some(message) = outgoing else { break };
                let body = serde_json::to_vec(&message)
                    .map_err(|error| format!("unserializable reply: {error}"))?;
                send.write_all(&(body.len() as u32).to_be_bytes())
                    .await
                    .map_err(|error| format!("write: {error}"))?;
                send.write_all(&body)
                    .await
                    .map_err(|error| format!("write: {error}"))?;
            }
        }
    }
    Ok(())
}

/// Cursors and other ephemeral events, relayed to this live peer.
///
/// Identity resolution is the websocket driver's, unchanged and for the same
/// reason: a cursor's POSITION is shared with the room, but WHOSE it is only
/// reaches a viewer allowed to know that Person.
fn spawn_lane_relay(
    hub: &Arc<LaneHub>,
    room: &str,
    connection_id: &str,
    out_tx: mpsc::Sender<ServerMessage>,
    engine: Arc<Engine>,
    viewer: Option<String>,
) {
    let mut rx = hub.join(room);
    let room = room.to_string();
    let connection_id = connection_id.to_string();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if event.from == connection_id {
                continue;
            }
            let from = match event.from_subject.as_deref() {
                Some(subject) => engine
                    .may_read_record(viewer.as_deref(), subject)
                    .await
                    .unwrap_or(false)
                    .then(|| subject.to_string()),
                None => None,
            };
            if out_tx
                .send(ServerMessage::LaneEvent {
                    room: room.clone(),
                    from: event.from,
                    payload: event.payload,
                    identity: from,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });
}
