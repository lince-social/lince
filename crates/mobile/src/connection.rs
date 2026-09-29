use cell::{ClientMessage, ServerMessage};
use lince_interface::wake::WakeSignal;
use std::{path::PathBuf, sync::mpsc, thread};

pub enum Event {
    Ready(String, bool, crate::session::Identity),
    LoginFailed(String),
    Message(ServerMessage),
    Failed(String),
    Stopped,
}

enum Request {
    Message(Box<ClientMessage>),
    Stop,
    Login(String, engine::private_password::PasswordInput),
    Logout,
}

pub struct Connection {
    outgoing: tokio::sync::mpsc::Sender<Request>,
    pub incoming: mpsc::Receiver<Event>,
}

impl Connection {
    pub fn open(directory: PathBuf, wake: WakeSignal) -> std::io::Result<Self> {
        let (outgoing, mut requests) = tokio::sync::mpsc::channel(64);
        let (responses, incoming) = mpsc::sync_channel(64);
        thread::Builder::new()
            .name("lince-mobile-cell".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let run = || -> std::io::Result<()> {
                    let executor = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .worker_threads(2)
                        .thread_stack_size(32 * 1024 * 1024)
                        .build()?;
                    executor.block_on(async {
                        let cell = cell::Cell::open_mobile(cell::CellOptions {
                            data_dir: Some(directory),
                            peer_port: cfg!(all(feature = "android-smoke", debug_assertions)).then_some(0),
                            ..Default::default()
                        })
                        .await?;
                        let runtime = cell.runtime();
                        let locked = store::cells::config(&runtime.store.pool, "lince.mobile.auth").await.map_err(std::io::Error::other)?.is_some_and(|value| value["require_login"] == true);
                        let mut session = (!locked).then(|| runtime.local_session());
                        let mut signing: Option<crate::session::Signing> = None;
                        let mut events = cell::SyncEvents::new(&runtime.engine);
                        let mut organ = store::organs::local(&runtime.store.pool)
                            .await
                            .map_err(std::io::Error::other)?
                            .ok_or_else(|| std::io::Error::other("Local Organ is unavailable"))?;
                        let mut setup = runtime
                            .engine
                            .roster_of(&organ.uid)
                            .await
                            .map_err(std::io::Error::other)?
                            .is_none();
                        let live = responses
                            .send(Event::Ready(organ.uid.clone(), setup, if locked { crate::session::Identity::Locked } else { crate::session::Identity::Owner }))
                            .is_ok();
                        wake.ring();
                        if live {
                            loop {
                                let messages = tokio::select! {
                                    request = requests.recv() => {
                                        match request {
                                            Some(Request::Message(request)) => {
                                                let Some(session) = session.as_mut() else { continue; };
                                                let request = match signing.as_mut() {
                                                    Some(signing) => signing.sign(*request).map_err(std::io::Error::other)?,
                                                    None => *request,
                                                };
                                                session.handle(request).await
                                            }
                                            Some(Request::Login(username, password)) => {
                                                match crate::session::Signing::open(&runtime, &username, password).await {
                                                    Ok((next, credentials)) => {
                                                        store::cells::set_config(&runtime.store.pool, "lince.mobile.auth", &serde_json::json!({"require_login": true})).await.map_err(std::io::Error::other)?;
                                                        let identity = crate::session::Identity::Person(credentials.login.person_uid().into());
                                                        session = Some(next);
                                                        signing = Some(credentials);
                                                        events = cell::SyncEvents::new(&runtime.engine);
                                                        let _ = responses.send(Event::Ready(organ.uid.clone(), setup, identity));
                                                    }
                                                    Err(error) => { let _ = responses.send(Event::LoginFailed(error)); }
                                                }
                                                wake.ring();
                                                continue;
                                            }
                                            Some(Request::Logout) => {
                                                session = None;
                                                signing = None;
                                                events = cell::SyncEvents::new(&runtime.engine);
                                                let _ = responses.send(Event::Ready(organ.uid.clone(), setup, crate::session::Identity::Locked));
                                                wake.ring();
                                                continue;
                                            }
                                            Some(Request::Stop) | None => break,
                                        }
                                    }
                                    event = events.next(session.as_ref().is_some_and(cell::Session::has_ephemeral_subscriptions)), if session.is_some() => {
                                        let Some(event) = event else { break };
                                        session.as_mut().expect("active session").on_sync_event(event).await
                                    }
                                };
                                if messages.iter().any(|message| matches!(message, ServerMessage::Error { code: Some(code), .. } if code == "session_expired")) {
                                    session = None;
                                    signing = None;
                                    let _ = responses.send(Event::Ready(organ.uid.clone(), setup, crate::session::Identity::Locked));
                                    wake.ring();
                                    continue;
                                }
                                let mut connected = true;
                                for message in messages {
                                    if responses.send(Event::Message(message)).is_err() {
                                        connected = false;
                                        break;
                                    }
                                    wake.ring();
                                }
                                if !connected {
                                    break;
                                }
                                let identity = store::organs::local(&runtime.store.pool)
                                    .await
                                    .map_err(std::io::Error::other)?
                                    .ok_or_else(|| {
                                        std::io::Error::other("Local Organ is unavailable")
                                    })?;
                                setup = runtime.engine.roster_of(&identity.uid).await.map_err(std::io::Error::other)?.is_none();
                                if identity.uid != organ.uid {
                                    organ = identity;
                                    session = Some(runtime.local_session());
                                    signing = None;
                                    events = cell::SyncEvents::new(&runtime.engine);
                                    if responses
                                        .send(Event::Ready(organ.uid.clone(), false, crate::session::Identity::Owner))
                                        .is_err()
                                    {
                                        break;
                                    }
                                    wake.ring();
                                }
                            }
                        }
                        cell.shutdown().await;
                        let _ = responses.send(Event::Stopped);
                        wake.ring();
                        Ok(())
                    })
                };
                if let Err(error) = run() {
                    let _ = responses.send(Event::Failed(error.to_string()));
                    wake.ring();
                }
            })?;
        Ok(Self { outgoing, incoming })
    }

    pub fn send(&self, request: ClientMessage) -> Result<(), String> {
        self.outgoing
            .try_send(Request::Message(Box::new(request)))
            .map_err(|error| match error {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    "Lince is busy. Your edit is still here; try again.".into()
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    "The connection stopped. Your edit is still here; reopen Lince.".into()
                }
            })
    }

    pub fn login(&self, username: String, password: String) -> Result<(), String> {
        let password = engine::private_password::PasswordInput::new(password.into_bytes())
            .map_err(|error| error.to_string())?;
        self.outgoing
            .try_send(Request::Login(username, password))
            .map_err(|_| "Wait for this device to finish its current request".into())
    }

    pub fn logout(&self) -> Result<(), String> {
        self.outgoing
            .try_send(Request::Logout)
            .map_err(|_| "Wait for this device to finish its current request".into())
    }

    pub fn stop(&self) -> Result<(), String> {
        self.outgoing
            .try_send(Request::Stop)
            .map_err(|_| "Wait for current changes to finish before switching profiles".into())
    }
}
