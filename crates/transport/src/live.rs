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
    sessions: Arc<tokio::sync::Semaphore>,
}

impl LiveHost {
    pub fn new(engine: Arc<Engine>, hub: Arc<LaneHub>) -> Arc<LiveHost> {
        Arc::new(LiveHost {
            engine,
            hub,
            sessions: Arc::new(tokio::sync::Semaphore::new(128)),
        })
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
        let Ok(_permit) = self.sessions.clone().try_acquire_owned() else {
            connection.close(0u32.into(), b"live sessions busy");
            return;
        };
        let closing = connection.clone();
        let connection_id = nucleus::new_uid("live");
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
        closing.close(0u32.into(), b"live session ended");
    }
}

async fn write_frame(
    send: &mut iroh::endpoint::SendStream,
    value: &ServerMessage,
) -> Result<(), String> {
    let body = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if body.len() > MAX_FRAME_BYTES as usize {
        return Err("Reply exceeds the frame limit".into());
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        send.write_all(&(body.len() as u32).to_be_bytes())
            .await
            .map_err(|error| format!("login write: {error}"))?;
        send.write_all(&body)
            .await
            .map_err(|error| format!("login write: {error}"))
    })
    .await
    .map_err(|_| "Peer stopped reading".to_string())?
}

async fn authenticate(
    engine: &Arc<Engine>,
    connection: &Connection,
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<engine::login::LoginSession, String> {
    let result = async {
        let bytes = read_frame_with_limit(recv, 8192)
            .await?
            .ok_or("Login required")?;
        let ClientMessage::LiveLogin { username, password } =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid login")?
        else {
            return Err("Login required".to_string());
        };
        let password = engine::private_password::PasswordInput::new(password.into_bytes())
            .map_err(|_| "Invalid login")?;
        engine
            .login_password(
                &username,
                password,
                Some(&connection.remote_id().to_string()),
            )
            .await
            .map_err(|_| "Invalid login".to_string())
    }
    .await;
    match result {
        Ok(login) => {
            write_frame(
                send,
                &ServerMessage::LiveLoginOk {
                    person: login.person_uid().into(),
                    organ: login.organ_uid().into(),
                },
            )
            .await?;
            Ok(login)
        }
        Err(error) => {
            let _ = write_frame(
                send,
                &ServerMessage::LiveLoginError {
                    message: "Invalid username or password".into(),
                },
            )
            .await;
            if send.finish().is_ok() {
                let _ =
                    tokio::time::timeout(std::time::Duration::from_secs(2), send.stopped()).await;
            }
            Err(error)
        }
    }
}

async fn read_frame(recv: &mut iroh::endpoint::RecvStream) -> Result<Option<Vec<u8>>, String> {
    read_frame_with_limit(recv, MAX_FRAME_BYTES).await
}

async fn read_frame_with_limit(
    recv: &mut iroh::endpoint::RecvStream,
    limit: u32,
) -> Result<Option<Vec<u8>>, String> {
    let mut len = [0u8; 4];
    match recv.read_exact(&mut len).await {
        Ok(()) => {}
        Err(_) => return Ok(None),
    }
    let len = u32::from_be_bytes(len);
    if len > limit {
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

    let login = match granted_person {
        Some(person) => {
            let login = engine
                .login_granted(&connection.remote_id().to_string())
                .await
                .map_err(|error| error.to_string())?;
            if login.person_uid() != person {
                return Err("Login grant changed".into());
            }
            login
        }
        None => tokio::time::timeout(
            std::time::Duration::from_secs(30),
            authenticate(&engine, &connection, &mut send, &mut recv),
        )
        .await
        .map_err(|_| "Login timed out")??,
    };
    let mut revoked = login.watch_revocation();
    let mut relays = tokio::task::JoinSet::new();

    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(32);
    let queued = out_tx.downgrade();
    let queue_connection = connection_id.clone();
    let _sync_queue = engine
        .sync_service
        .observe_queue(move || nucleus::sync::Queue {
            instance: nucleus::sync::Instance::interface(&queue_connection, "live-events"),
            incoming: 0,
            outgoing: queued.upgrade().map_or(0, |sender| {
                (sender.max_capacity() - sender.capacity()) as u64
            }),
        });
    let mut session = Session::authenticated(
        engine.clone(),
        hub.clone(),
        connection_id.clone(),
        login.clone(),
    );
    let challenge = session.initialize_action_intent().await;
    write_frame(&mut send, &challenge).await?;

    let mut sync_events = crate::SyncEvents::new(&engine).with_presence(session.presence_changes());
    let mut next_frame = Box::pin(read_frame(&mut recv));
    let mut rate_window = std::time::Instant::now();
    let mut message_count = 0;

    loop {
        tokio::select! {
            _ = revoked.changed() => break,
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(login.expires())) => break,
            frame = &mut next_frame => {
                drop(next_frame);
                next_frame = Box::pin(read_frame(&mut recv));
                let Some(bytes) = frame? else { break };
                if rate_window.elapsed() >= std::time::Duration::from_secs(10) { rate_window = std::time::Instant::now(); message_count = 0; }
                message_count += 1;
                if message_count > 120 { break; }
                if !session.subject_may_act().await { break; }
                match serde_json::from_slice::<ClientMessage>(&bytes) {
                    Ok(message) => {
                        let joined = match &message {
                            ClientMessage::LaneJoin { room } => Some(room.clone()),
                            _ => None,
                        };
                        let write = matches!(message, ClientMessage::Act { .. } | ClientMessage::SignedAct { .. } | ClientMessage::CollabUpdate { .. } | ClientMessage::SessionAuthenticate { .. });
                        login.run(&engine, write, async {
                            for reply in session.handle(message).await {
                                write_frame(&mut send, &reply).await.map_err(engine::EngineError::Consequence)?;
                            }
                            Ok(())
                        }).await.map_err(|error| error.to_string())?;
                        if let Some(room) = joined
                            && session.joined_rooms().contains(&room) && relays.len() < 64 {
                            relays.spawn(lane_relay(hub.clone(), room, connection_id.clone(), out_tx.clone(), engine.clone(), login.clone()));
                        }
                    }
                    Err(error) => {
                        write_frame(&mut send, &ServerMessage::Error {
                            id: "-".into(),
                            message: error.to_string(),
                            code: None,
                        }).await?;
                    }
                }
            }
            event = sync_events.next(session.has_ephemeral_subscriptions()) => {
                let Some(event) = event else { break };
                if !session.subject_may_act().await { break; }
                login.run(&engine, false, async {
                    for update in session.on_sync_event(event).await {
                        write_frame(&mut send, &update).await.map_err(engine::EngineError::Consequence)?;
                    }
                    Ok(())
                }).await.map_err(|error| error.to_string())?;
            }
            outgoing = out_rx.recv() => {
                let Some(message) = outgoing else { break };
                if !session.subject_may_act().await { break; }
                login.run(&engine, false, async {
                    if let ServerMessage::LaneEvent { room, .. } = &message
                        && (!session.joined_rooms().contains(room) || !session.may_use_lane().await) { return Ok(()); }
                    write_frame(&mut send, &message).await.map_err(engine::EngineError::Consequence)
                }).await.map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

async fn lane_relay(
    hub: Arc<LaneHub>,
    room: String,
    connection_id: String,
    out_tx: mpsc::Sender<ServerMessage>,
    engine: Arc<Engine>,
    login: engine::login::LoginSession,
) {
    let mut rx = hub.join(&room);
    while let Ok(event) = rx.recv().await {
        if login.require(&engine).await.is_err()
            || engine
                .require_permission(Some(login.person_uid()), "view:stream")
                .await
                .is_err()
        {
            break;
        }
        if event.from == connection_id {
            continue;
        }
        let from = match event.from_subject.as_deref() {
            Some(subject) => engine
                .may_read_record(Some(login.person_uid()), subject)
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
}
