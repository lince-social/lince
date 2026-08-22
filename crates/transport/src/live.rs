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
    async fn serve(
        &self,
        organ_uid: String,
        granted_person: Option<String>,
        connection: Connection,
    ) {
        // The subject is the Person they act as — either resolved from the
        // handshake-proven Organ, or proved with a password below. Never read
        // out of anything the peer merely asserted. This one value is what
        // every visibility decision downstream rests on.
        let connection_id = format!("live:{organ_uid}");
        if let Err(error) = drive(
            self.engine.clone(),
            self.hub.clone(),
            connection_id,
            granted_person,
            connection,
        )
        .await
        {
            tracing::debug!(%organ_uid, %error, "live session ended");
        }
    }
}

async fn write_frame(
    send: &mut iroh::endpoint::SendStream,
    value: &ServerMessage,
) -> Result<(), String> {
    let body = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    send.write_all(&(body.len() as u32).to_be_bytes())
        .await
        .map_err(|error| format!("login write: {error}"))?;
    send.write_all(&body)
        .await
        .map_err(|error| format!("login write: {error}"))
}

/// Prove a username and password, and answer with the Person they name.
///
/// This is the device-INDEPENDENT way in. Nothing about the peer's keys is
/// consulted: a Lince installed a minute ago can reach this Cell from any
/// network and get in with the same credential its owner types into the login
/// page. The credential is checked against `person_credential` by exactly the
/// code path the HTTP login uses, so there is one answer to "is this the right
/// password" rather than two that could drift.
///
/// Exactly ONE attempt is served per connection. Not a lockout — redialing is
/// cheap and this is not a rate limit — but it keeps a single connection from
/// becoming a password oracle, and the cost of guessing stays a full QUIC
/// handshake per guess. The refusal never says which half was wrong.
async fn authenticate(
    engine: &Arc<Engine>,
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<String, String> {
    // One identical refusal for every cause — unknown user, wrong password,
    // wrong frame. A message that distinguishes them tells whoever is guessing
    // which half they already have right.
    async fn deny(send: &mut iroh::endpoint::SendStream, reason: String) -> Result<String, String> {
        let _ = write_frame(
            send,
            &ServerMessage::LiveLoginError {
                message: "Invalid username or password".to_string(),
            },
        )
        .await;
        Err(reason)
    }

    let Some(bytes) = read_frame(recv).await? else {
        return Err("the peer hung up before logging in".to_string());
    };
    let (username, password) = match serde_json::from_slice::<ClientMessage>(&bytes) {
        Ok(ClientMessage::LiveLogin { username, password }) => {
            (username.trim().to_string(), password)
        }
        // Anything else is refused and the session ends. A peer that skips the
        // login cannot try again on this connection.
        _ => return deny(send, "the peer skipped the login".to_string()).await,
    };

    let user = store::auth::user_by_username(&engine.store.pool, &username)
        .await
        .map_err(|error| error.to_string())?;
    let ok = match &user {
        // Password AND standing, and the refusal below says neither which
        // failed nor that the name exists — the same sentence for a wrong
        // password, an unknown name and someone who no longer uses this Organ.
        Some(user) => {
            utils::auth::verify_password(&password, &user.password_hash).unwrap_or(false)
                && store::people::is_active(&engine.store.pool, &user.uid)
                    .await
                    .map_err(|error| error.to_string())?
        }
        // No fake verify on a missing user, and not pretended otherwise:
        // timing here is observable to anyone who already reached the
        // endpoint. What is guaranteed is that the ANSWER carries nothing.
        None => false,
    };
    if !ok {
        return deny(send, format!("login refused for `{username}`")).await;
    }
    let person = user.expect("verified above").uid;
    let organ = store::organs::local(&engine.store.pool)
        .await
        .ok()
        .flatten()
        .map(|organ| organ.uid)
        .unwrap_or_default();
    write_frame(
        send,
        &ServerMessage::LiveLoginOk {
            person: person.clone(),
            organ,
        },
    )
    .await?;
    Ok(person)
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
    granted_person: Option<String>,
    connection: Connection,
) -> Result<(), String> {
    // One bidirectional stream for the whole session: the browser end is one
    // websocket, and multiplexing would buy nothing while making ordering a
    // question that currently has no answer to get wrong.
    //
    // The HOST opens it, because the host now speaks first. QUIC does not
    // deliver a stream to the far side until something is written on it, so a
    // guest-opened stream would leave the guest waiting for a hello the host
    // could not yet see it had to send — and the guest cannot write first,
    // since what it must write is exactly what the hello tells it.
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| format!("no session stream: {error}"))?;

    // The login gate. Announced first so the guest never has to guess whether
    // this Cell wants a password: a granted device is told it is already in,
    // and everyone else is told to log in and gets NOTHING until they do.
    let login_required = granted_person.is_none();
    write_frame(&mut send, &ServerMessage::LiveHello { login_required }).await?;

    let person_uid = match granted_person {
        Some(person) => person,
        None => match authenticate(&engine, &mut send, &mut recv).await {
            Ok(person) => person,
            Err(reason) => {
                // Let the refusal actually ARRIVE. Returning here drops the
                // Connection, and a dropped QUIC connection discards whatever
                // was still buffered — so the peer would see the link die and
                // have no idea it was their password rather than the network.
                let _ = send.finish();
                let _ =
                    tokio::time::timeout(std::time::Duration::from_secs(5), connection.closed())
                        .await;
                return Err(reason);
            }
        },
    };

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

    // Deliberately NOT subscribed to `watch_notifications` the way `ws::serve`
    // is. Notifications are the host's inbox — who is asking THEM for a
    // conversation — and a guest acting inside their Cell has no business
    // reading it. A live guest sees the host's data through their granted
    // Person's visibility; pending invites are not data, they are the host's
    // unanswered mail.
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
                    organ: event.organ,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });
}
