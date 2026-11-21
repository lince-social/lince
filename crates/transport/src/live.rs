use std::{
    collections::{HashMap, VecDeque},
    fmt,
    pin::pin,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use engine::{
    Engine, EngineError, login::LoginSession, private_password::PasswordInput, wire::LiveSessions,
};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use nucleus::{
    description_asset, new_uid,
    sync::{Instance, Queue},
};
use serde_json::{from_slice, to_vec};
use tokio::{
    sync::{Semaphore, broadcast, mpsc},
    task::JoinSet,
    time::{sleep_until, timeout},
};

use crate::{
    ClientMessage, LaneEvent, LaneHub, ServerMessage, Session, SyncEvents,
    connection_task::ConnectionTask,
};

const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;
const MAX_LOGIN_BYTES: u32 = 8192;
const MAX_SESSIONS: usize = 128;
const OUTGOING_CAPACITY: usize = 32;
const MAX_WAITING_CALLS: usize = 64;
const RATE_WINDOW: Duration = Duration::from_secs(10);
const MAX_MESSAGES_PER_WINDOW: u32 = 120;
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Hosts authenticated live sessions on a Cell's Iroh connection handler.
pub struct LiveHost {
    engine: Arc<Engine>,
    hub: Arc<LaneHub>,
    sessions: Arc<Semaphore>,
}

impl LiveHost {
    /// Creates a live host with its own limit on active sessions.
    pub fn new(engine: Arc<Engine>, hub: Arc<LaneHub>) -> Arc<Self> {
        Arc::new(Self {
            engine,
            hub,
            sessions: Arc::new(Semaphore::new(MAX_SESSIONS)),
        })
    }
}

impl fmt::Debug for LiveHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveHost").finish_non_exhaustive()
    }
}

#[async_trait]
impl LiveSessions for LiveHost {
    async fn serve(
        &self,
        organ_uid: String,
        granted_person: Option<String>,
        connection: Connection,
    ) {
        let Ok(_permit) = Arc::clone(&self.sessions).try_acquire_owned() else {
            connection.close(0u32.into(), b"live sessions busy");
            return;
        };
        let closing = connection.clone();
        let connection_id = new_uid("live");
        if let Err(error) = drive(
            Arc::clone(&self.engine),
            Arc::clone(&self.hub),
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

async fn write_frame(send: &mut SendStream, value: &ServerMessage) -> Result<(), String> {
    let body = to_vec(value).map_err(|error| error.to_string())?;
    let size = u32::try_from(body.len())
        .ok()
        .filter(|&size| size <= MAX_FRAME_BYTES)
        .ok_or("Reply exceeds the frame limit")?;
    timeout(WRITE_TIMEOUT, async {
        send.write_all(&size.to_be_bytes())
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
    engine: &Engine,
    connection: &Connection,
    send: &mut SendStream,
    recv: &mut RecvStream,
) -> Result<LoginSession, String> {
    // Keep JSON, password, and login-provider details out of connection diagnostics.
    let result = async {
        let bytes = read_frame_with_limit(recv, MAX_LOGIN_BYTES)
            .await?
            .ok_or("Login required")?;
        let ClientMessage::LiveLogin { username, password } =
            from_slice(&bytes).map_err(|_| "Invalid login")?
        else {
            return Err("Login required".to_string());
        };
        let password = PasswordInput::new(password.into_bytes()).map_err(|_| "Invalid login")?;
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
                let _ = timeout(Duration::from_secs(2), send.stopped()).await;
            }
            Err(error)
        }
    }
}

async fn read_frame(mut recv: RecvStream) -> Result<(RecvStream, Option<Vec<u8>>), String> {
    let bytes = read_frame_with_limit(&mut recv, MAX_FRAME_BYTES).await?;
    Ok((recv, bytes))
}

async fn read_frame_with_limit(
    recv: &mut RecvStream,
    limit: u32,
) -> Result<Option<Vec<u8>>, String> {
    let mut len = [0u8; 4];
    if recv.read_exact(&mut len).await.is_err() {
        return Ok(None);
    }
    let len = u32::from_be_bytes(len);
    if len > limit {
        return Err(format!("frame of {len} bytes is over the cap"));
    }
    let size = usize::try_from(len).map_err(|_| format!("frame of {len} bytes is over the cap"))?;
    let mut buf = vec![0; size];
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
        None => timeout(
            LOGIN_TIMEOUT,
            authenticate(&engine, &connection, &mut send, &mut recv),
        )
        .await
        .map_err(|_| "Login timed out")??,
    };
    let mut revoked = login.watch_revocation();
    let mut relays = HashMap::<String, ConnectionTask>::new();
    let mut calls = JoinSet::new();
    let mut call_queue = VecDeque::new();

    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(OUTGOING_CAPACITY);
    let queued = out_tx.downgrade();
    let relay_connection = Arc::<str>::from(connection_id.as_str());
    let queue_connection = Arc::clone(&relay_connection);
    let _sync_queue = engine.sync_service.observe_queue(move || Queue {
        instance: Instance::interface(&queue_connection, "live-events"),
        incoming: 0,
        outgoing: queued.upgrade().map_or(0, |sender| {
            u64::try_from(sender.max_capacity() - sender.capacity()).unwrap_or_default()
        }),
    });
    let mut session = Session::authenticated(
        Arc::clone(&engine),
        Arc::clone(&hub),
        connection_id,
        login.clone(),
    );
    let challenge = session.initialize_action_intent().await;
    write_frame(&mut send, &challenge).await?;

    let mut sync_events = SyncEvents::new(&engine).with_presence(session.presence_changes());
    // Keep partial frames alive while other connection events are handled.
    let mut next_frame = pin!(read_frame(recv));
    let mut rate_window = Instant::now();
    let mut message_count = 0;

    loop {
        if calls.is_empty()
            && let Some(message) = call_queue.pop_front()
        {
            let mut call = session.fork_call();
            calls.spawn(async move { call.handle(message).await });
        }
        tokio::select! {
            _ = revoked.changed() => break,
            _ = sleep_until(tokio::time::Instant::from_std(login.expires())) => break,
            frame = &mut next_frame => {
                let (recv, Some(bytes)) = frame? else { break };
                next_frame.set(read_frame(recv));
                if rate_window.elapsed() >= RATE_WINDOW {
                    rate_window = Instant::now();
                    message_count = 0;
                }
                message_count += 1;
                if message_count > MAX_MESSAGES_PER_WINDOW { break; }
                if !session.subject_may_act().await { break; }
                match from_slice::<ClientMessage>(&bytes) {
                    Ok(message) => {
                        if let ClientMessage::Call { id, .. } | ClientMessage::CallContext { id, .. } = &message {
                            if call_queue.len() >= MAX_WAITING_CALLS {
                                write_frame(&mut send, &ServerMessage::Error { id: id.clone(), message: "Call controls are busy; try again".into(), code: Some("call_busy".into()) }).await?;
                            } else {
                                call_queue.push_back(message);
                            }
                            continue;
                        }
                        let membership = match &message {
                            ClientMessage::LaneJoin { room } => Some(Membership::Join(room.clone())),
                            ClientMessage::LaneLeave { room } => Some(Membership::Leave(room.clone())),
                            _ => None,
                        };
                        let write = matches!(message, ClientMessage::Act { .. } | ClientMessage::SignedAct { .. } | ClientMessage::CollabUpdate { .. } | ClientMessage::SessionAuthenticate { .. } | ClientMessage::DescriptionAsset { request: description_asset::Request::Put { .. }, .. });
                        let preview = message.runs_karma_preview();
                        let operation = async {
                            let replies = session.handle(message).await;
                            let admitted = replies.is_empty();
                            write_replies(&mut send, replies).await?;
                            Ok(admitted)
                        };
                        let admitted = if preview {
                            login.run_unlocked(&engine, operation).await
                        } else {
                            login.run(&engine, write, operation).await
                        }.map_err(|error| error.to_string())?;
                        if admitted {
                            match membership {
                                Some(Membership::Join(room))
                                    if session.joined_rooms().contains(&room)
                                        && relays.get(&room).is_none_or(ConnectionTask::is_finished) =>
                                {
                                    let receiver = hub.join(&room);
                                    relays.insert(room, ConnectionTask::spawn(lane_relay(
                                        receiver, Arc::clone(&relay_connection), out_tx.clone(),
                                        Arc::clone(&engine), login.clone(),
                                    )));
                                }
                                Some(Membership::Leave(room)) => {
                                    if let Some(relay) = relays.remove(&room) {
                                        relay.cancel().await;
                                        hub.prune(&room);
                                    }
                                }
                                _ => {}
                            }
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
                    write_replies(&mut send, session.on_sync_event(event).await).await
                }).await.map_err(|error| error.to_string())?;
            }
            outgoing = out_rx.recv() => {
                let Some(message) = outgoing else { break };
                if !session.subject_may_act().await { break; }
                login.run(&engine, false, async {
                    if let ServerMessage::LaneEvent { room, .. } = &message
                        && (!session.joined_rooms().contains(room) || !session.may_use_lane().await) { return Ok(()); }
                    write_frame(&mut send, &message).await.map_err(EngineError::Consequence)
                }).await.map_err(|error| error.to_string())?;
            }
            Some(result) = calls.join_next(), if !calls.is_empty() => {
                if !session.subject_may_act().await { break; }
                if let Ok(messages) = result { for message in messages { write_frame(&mut send, &message).await?; } }
            }
        }
    }
    for (room, relay) in relays {
        relay.cancel().await;
        hub.prune(&room);
    }
    Ok(())
}

enum Membership {
    Join(String),
    Leave(String),
}

async fn write_replies(
    send: &mut SendStream,
    replies: Vec<ServerMessage>,
) -> Result<(), EngineError> {
    for reply in replies {
        write_frame(send, &reply)
            .await
            .map_err(EngineError::Consequence)?;
    }
    Ok(())
}

async fn lane_relay(
    mut rx: broadcast::Receiver<LaneEvent>,
    connection_id: Arc<str>,
    out_tx: mpsc::Sender<ServerMessage>,
    engine: Arc<Engine>,
    login: LoginSession,
) {
    while let Ok(event) = rx.recv().await {
        if login.require(&engine).await.is_err()
            || engine
                .require_permission(Some(login.person_uid()), "view:stream")
                .await
                .is_err()
        {
            break;
        }
        if event.from == connection_id.as_ref() {
            continue;
        }
        let from = if let Some(subject) = event.from_subject
            && matches!(
                engine
                    .may_read_record(Some(login.person_uid()), &subject)
                    .await,
                Ok(true)
            ) {
            Some(subject)
        } else {
            None
        };
        if out_tx
            .send(ServerMessage::LaneEvent {
                room: event.room,
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
