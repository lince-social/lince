use cell::{ClientMessage, ServerMessage};
use lince_interface::wake::WakeSignal;
use std::{path::PathBuf, sync::mpsc, thread};

pub enum Event {
    Ready(String),
    Message(ServerMessage),
    Failed(String),
}

pub struct Connection {
    outgoing: tokio::sync::mpsc::Sender<ClientMessage>,
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
                            ..Default::default()
                        })
                        .await?;
                        let runtime = cell.runtime();
                        let mut session = runtime.local_session();
                        let mut events = cell::SyncEvents::new(&runtime.engine)
                            .with_presence(session.presence_changes());
                        let organ = store::organs::local(&runtime.store.pool)
                            .await
                            .map_err(std::io::Error::other)?
                            .ok_or_else(|| std::io::Error::other("Local Organ is unavailable"))?;
                        let live = responses.send(Event::Ready(organ.uid)).is_ok();
                        wake.ring();
                        if live {
                            loop {
                                let messages = tokio::select! {
                                    request = requests.recv() => {
                                        let Some(request) = request else { break };
                                        session.handle(request).await
                                    }
                                    event = events.next(session.has_ephemeral_subscriptions()) => {
                                        let Some(event) = event else { break };
                                        session.on_sync_event(event).await
                                    }
                                };
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
                            }
                        }
                        cell.shutdown().await;
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
            .try_send(request)
            .map_err(|error| match error {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    "Lince is busy. Your edit is still here; try again.".into()
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    "The connection stopped. Your edit is still here; reopen Lince.".into()
                }
            })
    }
}
