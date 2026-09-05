use std::sync::Arc;

use engine::Engine;
use engine::wire::LiveSessions;
use iroh::endpoint::Connection;
use tokio::sync::mpsc;

use crate::lane::LaneHub;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::session::Session;

const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

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

async fn authenticate(
    engine: &Arc<Engine>,
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<String, String> {
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
        _ => return deny(send, "the peer skipped the login".to_string()).await,
    };

    let user = store::auth::user_by_username(&engine.store.pool, &username)
        .await
        .map_err(|error| error.to_string())?;
    let ok = match &user {
        Some(user) => {
            utils::auth::verify_password(&password, &user.password_hash).unwrap_or(false)
                && store::people::is_active(&engine.store.pool, &user.uid)
                    .await
                    .map_err(|error| error.to_string())?
        }
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
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| format!("no session stream: {error}"))?;

    let login_required = granted_person.is_none();
    write_frame(&mut send, &ServerMessage::LiveHello { login_required }).await?;

    let person_uid = match granted_person {
        Some(person) => person,
        None => match authenticate(&engine, &mut send, &mut recv).await {
            Ok(person) => person,
            Err(reason) => {
                let _ = send.finish();
                let _ =
                    tokio::time::timeout(std::time::Duration::from_secs(5), connection.closed())
                        .await;
                return Err(reason);
            }
        },
    };

    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(256);
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
