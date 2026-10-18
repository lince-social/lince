use crate::native::{app::CellHandle, wake::WakeSignal};
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

mod speech_scope;

pub const RECORDS: &str = "interface-records";
pub const CONNECTION: &str = "interface-connection";

#[derive(Message, Clone)]
pub struct CellMessage(pub ServerMessage);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReceiveCell;

pub fn records_subscription() -> ClientMessage {
    ClientMessage::Subscribe {
        id: RECORDS.into(),
        protein: protein::Protein {
            source: protein::Source::Record,
            filter: Vec::new(),
            fields: Some(
                ["uid", "head", "slug", "kind", "quantity"]
                    .map(str::to_string)
                    .into(),
            ),
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: None,
        },
    }
}

pub struct CellBridge {
    pub outgoing: mpsc::Sender<ClientMessage>,
    pub incoming: mpsc::Receiver<ServerMessage>,
    pub(crate) speech_cancellation: SpeechCancellation,
    task: JoinHandle<()>,
    closed: Arc<AtomicBool>,
    reported_closed: bool,
    _sync_queue: engine::sync_service::QueueRegistration,
}

#[derive(Clone)]
pub(crate) struct SpeechCancellation {
    sender: mpsc::Sender<ClientMessage>,
    runtime: tokio::runtime::Handle,
}

impl SpeechCancellation {
    pub(crate) fn for_owner(world: &World, owner: Entity) -> Option<Self> {
        let sender = crate::native::practice_cells::sender(world, owner)?;
        let runtime = world
            .get_non_send::<CellBridge>()?
            .speech_cancellation
            .runtime
            .clone();
        Some(Self { sender, runtime })
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    pub(crate) fn new(sender: mpsc::Sender<ClientMessage>) -> Self {
        Self {
            sender,
            runtime: tokio::runtime::Handle::current(),
        }
    }
    pub(crate) fn send(&self, request: cell::speech::Request) -> Result<String, String> {
        if !matches!(
            &request,
            cell::speech::Request::Cancel { .. } | cell::speech::Request::StopListening { .. } | cell::speech::Request::PauseListening { .. }
                | cell::speech::Request::Voice { request: cell::fiote_communication::speech::voice::Request::Cancel { .. } }
        ) {
            return Err("This channel only cancels speech work.".into());
        }
        let id = nucleus::new_uid("speech-cancel");
        let message = ClientMessage::Speech {
            id: id.clone(),
            request,
        };
        match self.sender.try_send(message) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => {
                return Err("The speech connection closed.".into());
            }
            Err(mpsc::error::TrySendError::Full(message)) => {
                let sender = self.sender.clone();
                self.runtime.spawn(async move {
                    let _ = sender.send(message).await;
                });
            }
        }
        Ok(id)
    }
}

struct ConnectionEnded {
    closed: Arc<AtomicBool>,
    wake: WakeSignal,
    _speech: speech_scope::Scope,
}

impl Drop for ConnectionEnded {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        self.wake.ring();
    }
}

impl Drop for CellBridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn connect(runtime: cell::CellRuntime, wake: WakeSignal) -> CellBridge {
    let session = runtime.local_session();
    connect_session(runtime, wake, session)
}

fn connect_session(
    runtime: cell::CellRuntime,
    wake: WakeSignal,
    mut session: cell::Session,
) -> CellBridge {
    simulation::karma_preview::install(&runtime.engine).expect("Karma preview service");
    let (outgoing, mut requests) = mpsc::channel::<ClientMessage>(64);
    let speech_cancellation = SpeechCancellation::new(outgoing.clone());
    let (responses, incoming) = mpsc::channel(64);
    let mut components = runtime.engine.subscribe_components();
    let queue_connection = session.connection_id().to_owned();
    let queued_requests = outgoing.downgrade();
    let queued_responses = responses.downgrade();
    let sync_queue = runtime
        .engine
        .sync_service
        .observe_queue(move || nucleus::sync::Queue {
            instance: nucleus::sync::Instance::interface(&queue_connection, "channel"),
            incoming: queued_requests.upgrade().map_or(0, |sender| {
                (sender.max_capacity() - sender.capacity()) as u64
            }),
            outgoing: queued_responses.upgrade().map_or(0, |sender| {
                (sender.max_capacity() - sender.capacity()) as u64
            }),
        });
    let mut sync_events =
        cell::SyncEvents::new(&runtime.engine).with_presence(session.presence_changes());
    let closed = Arc::new(AtomicBool::new(false));
    let ended = ConnectionEnded {
        closed: closed.clone(),
        wake: wake.clone(),
        _speech: speech_scope::Scope::new(session.fork_call()),
    };
    let task = tokio::spawn(async move {
        let ended = ended;
        let mut lanes = HashMap::<String, tokio::task::AbortHandle>::new();
        let mut lane_tasks = tokio::task::JoinSet::new();
        let mut call_tasks = tokio::task::JoinSet::new();
        let mut speech_tasks = tokio::task::JoinSet::new();
        let mut preview_tasks = tokio::task::JoinSet::new();
        let mut call_queue = std::collections::VecDeque::new();
        let mut terminals = cell::terminal::TerminalHost::new();
        let (terminal_output, mut terminal_messages) = mpsc::channel(64);
        let (terminal_done, mut terminal_exits) = mpsc::channel(16);
        loop {
            if call_tasks.is_empty() {
                if let Some(request) = call_queue.pop_front() {
                    let mut call = session.fork_call();
                    call_tasks.spawn(async move { call.handle(request).await });
                }
            }
            let messages = tokio::select! {
                request = requests.recv() => {
                    let Some(request) = request else { break };
                    if let ClientMessage::Act { id, action: engine::actions::Action::PreviewKarmaProposal { .. } } = &request {
                        if preview_tasks.len() >= 4 {
                            if !deliver(&responses, &wake, vec![ServerMessage::Error { id: id.clone(), message: "Four previews are already running; wait for a result".into(), code: Some("preview_busy".into()) }]).await { break; }
                        } else {
                            let mut preview = session.fork_call();
                            preview_tasks.spawn(async move { preview.handle(request).await });
                        }
                        continue;
                    }
                    if let ClientMessage::Speech { id, request: speech } = &request {
                        let cancellation = matches!(speech, cell::speech::Request::Cancel { .. } | cell::speech::Request::StopListening { .. } | cell::speech::Request::PauseListening { .. } | cell::speech::Request::Voice { request: cell::fiote_communication::speech::voice::Request::Cancel { .. } });
                        if speech_tasks.len() >= 8 && !cancellation {
                            if !deliver(&responses, &wake, vec![ServerMessage::Error { id: id.clone(), message: "Speech controls are busy; try again".into(), code: Some("speech_busy".into()) }]).await { break; }
                        } else {
                            let tracked = match ended._speech.begin(speech) {
                                Ok(tracked) => tracked,
                                Err(message) => {
                                    if !deliver(&responses, &wake, vec![ServerMessage::Error { id: id.clone(), message, code: Some("speech_busy".into()) }]).await { break; }
                                    continue;
                                }
                            };
                            let mut speech = session.fork_call();
                            speech_tasks.spawn(async move {
                                let messages = speech.handle(request).await;
                                if let Some(tracked) = tracked { tracked.complete(&messages); }
                                messages
                            });
                        }
                        continue;
                    }
                    if let ClientMessage::Call { id, .. } | ClientMessage::CallContext { id, .. } = &request {
                        if call_queue.len() >= 64 {
                            if !deliver(&responses, &wake, vec![ServerMessage::Error { id: id.clone(), message: "Call controls are busy; try again".into(), code: Some("call_busy".into()) }]).await { break; }
                        } else {
                            call_queue.push_back(request);
                        }
                        continue;
                    }
                    match &request {
                        ClientMessage::Command { id, request } => {
                            let result = runtime.commands.request(&runtime.engine, request.clone()).await;
                            let message = match result {
                                Ok(response) => ServerMessage::Command { id: id.clone(), response },
                                Err(message) => ServerMessage::Error { id: id.clone(), message, code: Some("command".into()) },
                            };
                            if !deliver(&responses, &wake, vec![message]).await { break; }
                            continue;
                        }
                        ClientMessage::TerminalOpen { id, cols, rows, pixel_width, pixel_height } => {
                            let result = terminals.open(id.clone(), cell::terminal::pty_size(*cols, *rows, *pixel_width, *pixel_height), terminal_output.clone(), terminal_done.clone()).await;
                            if let Err(message) = result {
                                if !deliver(&responses, &wake, vec![ServerMessage::Error { id: id.clone(), message, code: Some("terminal".into()) }]).await { break; }
                            }
                            continue;
                        }
                        ClientMessage::TerminalInput { id, data_base64 } => {
                            let result = terminals.input(id, data_base64).await;
                            if let Err(message) = result {
                                if !deliver(&responses, &wake, vec![ServerMessage::Error { id: id.clone(), message, code: Some("terminal".into()) }]).await { break; }
                            }
                            continue;
                        }
                        ClientMessage::TerminalResize { id, cols, rows, pixel_width, pixel_height } => {
                            let result = terminals.resize(id, cell::terminal::pty_size(*cols, *rows, *pixel_width, *pixel_height)).await;
                            if let Err(message) = result {
                                if !deliver(&responses, &wake, vec![ServerMessage::Error { id: id.clone(), message, code: Some("terminal".into()) }]).await { break; }
                            }
                            continue;
                        }
                        ClientMessage::TerminalClose { id } => {
                            let _ = terminals.close(id).await;
                            continue;
                        }
                        ClientMessage::LaneJoin { room } if !lanes.contains_key(room) => {
                            let mut events = runtime.lanes.join(room);
                            let responses = responses.clone();
                            let wake = wake.clone();
                            let task = lane_tasks.spawn(async move {
                                loop {
                                    match events.recv().await {
                                        Ok(event) => {
                                            let message = ServerMessage::LaneEvent {
                                                room: event.room, from: event.from, payload: event.payload,
                                                identity: event.from_subject, organ: event.organ,
                                            };
                                            if !deliver(&responses, &wake, vec![message]).await { break; }
                                        }
                                        Err(broadcast::error::RecvError::Lagged(_)) => {
                                            if !deliver(&responses, &wake, vec![ServerMessage::Error {
                                                id: "lane".into(), message: "Some live events were missed. Rejoin the view to refresh it.".into(), code: Some("lane_lagged".into()),
                                            }]).await { break; }
                                        }
                                        Err(broadcast::error::RecvError::Closed) => break,
                                    }
                                }
                            });
                            lanes.insert(room.clone(), task);
                        }
                        ClientMessage::LaneLeave { room } => {
                            if let Some(task) = lanes.remove(room) { task.abort(); }
                        }
                        _ => {}
                    }
                    session.handle(request).await
                }
                event = sync_events.next(session.has_ephemeral_subscriptions()) => {
                    let Some(event) = event else { break };
                    session.on_sync_event(event).await
                },
                Some(message) = terminal_messages.recv() => vec![message],
                result = components.recv() => match result {
                    Ok(presentation) => vec![ServerMessage::PresentComponent { presentation }],
                    Err(broadcast::error::RecvError::Lagged(_)) => vec![ServerMessage::Error { id: "interface-components".into(), message: "Some component pushes were missed".into(), code: Some("components_lagged".into()) }],
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                Some(result) = call_tasks.join_next(), if !call_tasks.is_empty() => result.unwrap_or_default(),
                Some(result) = speech_tasks.join_next(), if !speech_tasks.is_empty() => result.unwrap_or_default(),
                Some(result) = preview_tasks.join_next(), if !preview_tasks.is_empty() => result.unwrap_or_default(),
                Some(id) = terminal_exits.recv() => { terminals.forget(&id); Vec::new() },
                finished = lane_tasks.join_next(), if !lane_tasks.is_empty() => {
                    lanes.retain(|_, task| !task.is_finished());
                    match finished {
                        Some(Err(error)) if !error.is_cancelled() => vec![ServerMessage::Error {
                            id: "lane".into(), message: format!("A live view stopped. Rejoin the view to refresh it: {error}"), code: Some("lane_closed".into()),
                        }],
                        _ => Vec::new(),
                    }
                },
            };
            if !deliver(&responses, &wake, messages).await {
                break;
            }
        }
    });
    CellBridge {
        outgoing,
        incoming,
        speech_cancellation,
        task,
        closed,
        reported_closed: false,
        _sync_queue: sync_queue,
    }
}

async fn deliver(
    out: &mpsc::Sender<ServerMessage>,
    wake: &WakeSignal,
    messages: Vec<ServerMessage>,
) -> bool {
    for message in messages {
        if out.send(message).await.is_err() {
            return false;
        }
        wake.ring();
    }
    true
}

pub struct CellBridgePlugin;
impl Plugin for CellBridgePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CellMessage>()
            .add_systems(Startup, start)
            .add_systems(
                Update,
                drain
                    .in_set(ReceiveCell)
                    .run_if(crate::native::laboratory::normal),
            );
    }
}

fn drain(
    mut bridge: NonSendMut<CellBridge>,
    mut messages: MessageWriter<CellMessage>,
    wake: Res<WakeSignal>,
) {
    for _ in 0..64 {
        let message = match bridge.incoming.try_recv() {
            Ok(message) => message,
            Err(error) => {
                if (matches!(error, mpsc::error::TryRecvError::Disconnected)
                    || bridge.closed.load(Ordering::Acquire))
                    && !bridge.reported_closed
                {
                    bridge.reported_closed = true;
                    messages.write(CellMessage(ServerMessage::Error {
                        id: CONNECTION.into(),
                        message: "The connection to this Cell stopped. Your drafts are still here. Quit and reopen Lince to reconnect.".into(),
                        code: Some("connection_closed".into()),
                    }));
                    wake.ring();
                }
                return;
            }
        };
        messages.write(CellMessage(message));
    }
    wake.ring();
}

fn start(world: &mut World) {
    let runtime = world.resource::<CellHandle>().0.clone();
    let wake = world.resource::<WakeSignal>().clone();
    let bridge = connect(runtime, wake);
    if let Err(error) = bridge.outgoing.try_send(records_subscription()) {
        world.write_message(CellMessage(ServerMessage::Error {
            id: RECORDS.into(),
            message: format!("Could not request Records: {error}"),
            code: Some("connection_closed".into()),
        }));
    }
    world.insert_non_send(bridge);
}

pub(crate) mod tests {
    use super::*;
    use engine::{Engine, actions::Action};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[cfg(test)]
    #[tokio::test]
    async fn cancellation_waits_for_a_full_bounded_queue_even_outside_the_runtime_context() {
        let (sender, mut receiver) = mpsc::channel(1);
        sender
            .try_send(ClientMessage::Speech {
                id: "queued".into(),
                request: cell::speech::Request::Inspect,
            })
            .unwrap();
        let cancellation = SpeechCancellation::new(sender);
        let job = nucleus::new_uid("speech");
        let expected = job.clone();
        let id = std::thread::spawn(move || {
            cancellation
                .send(cell::speech::Request::Cancel { job })
                .unwrap()
        })
        .join()
        .unwrap();
        assert!(
            matches!(receiver.recv().await, Some(ClientMessage::Speech { id, .. }) if id == "queued")
        );
        let request = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(request, ClientMessage::Speech { id: observed, request: cell::speech::Request::Cancel { job } } if observed == id && job == expected)
        );
        assert!(receiver.try_recv().is_err());
    }

    #[cfg(test)]
    struct BusySpeech {
        host: cell::speech::Host,
        entered: Arc<AtomicUsize>,
        release: Arc<tokio::sync::Notify>,
    }

    #[cfg(test)]
    #[async_trait::async_trait]
    impl cell::speech::Service for BusySpeech {
        async fn handle(
            &self,
            request: cell::speech::Request,
        ) -> Result<cell::speech::Status, String> {
            if matches!(&request, cell::speech::Request::Inspect) {
                self.entered.fetch_add(1, Ordering::SeqCst);
                self.release.notified().await;
            }
            cell::speech::Service::handle(&self.host, request).await
        }
    }

    #[cfg(test)]
    struct OwnedSpeech {
        host: cell::speech::Host,
        listening: std::sync::Mutex<Option<String>>,
        cancelled: std::sync::Mutex<Vec<String>>,
        entered: tokio::sync::Notify,
        delay: bool,
    }

    #[cfg(test)]
    #[async_trait::async_trait]
    impl cell::speech::Service for OwnedSpeech {
        async fn handle(
            &self,
            request: cell::speech::Request,
        ) -> Result<cell::speech::Status, String> {
            match request {
                cell::speech::Request::StartListening { owner, .. } => {
                    *self.listening.lock().unwrap() = Some(owner.clone());
                    self.entered.notify_one();
                    if self.delay {
                        std::future::pending::<()>().await;
                    }
                    let mut status =
                        cell::speech::Service::handle(&self.host, cell::speech::Request::Inspect)
                            .await?;
                    status.listening.owner = Some(owner);
                    status.listening.enabled = true;
                    Ok(status)
                }
                cell::speech::Request::StopListening {
                    owner,
                    generation: 0,
                } => {
                    self.cancelled.lock().unwrap().push(owner.clone());
                    {
                        let mut listening = self.listening.lock().unwrap();
                        if listening.as_ref() == Some(&owner) {
                            *listening = None;
                        }
                    }
                    cell::speech::Service::handle(&self.host, cell::speech::Request::Inspect).await
                }
                request => cell::speech::Service::handle(&self.host, request).await,
            }
        }
    }

    #[cfg(test)]
    #[tokio::test]
    async fn disconnect_cancels_accepted_and_unacknowledged_owned_listening_sessions() {
        for delay in [false, true] {
            let (runtime, _) = fixture().await;
            let root = tempfile::tempdir().unwrap();
            let service = Arc::new(OwnedSpeech {
                host: cell::speech::Host::open(root.path().join("speech.json")).unwrap(),
                listening: Default::default(),
                cancelled: Default::default(),
                entered: tokio::sync::Notify::new(),
                delay,
            });
            let session = runtime.local_session().with_speech(service.clone());
            let mut bridge = connect_session(runtime, WakeSignal::new(|| {}), session);
            let owner = nucleus::new_uid("voice");
            bridge
                .outgoing
                .send(ClientMessage::Speech {
                    id: "own-start".into(),
                    request: cell::speech::Request::StartListening {
                        mode: cell::speech::ListeningMode::Model,
                        owner: owner.clone(),
                        target: None,
                        routing: false,
                    },
                })
                .await
                .unwrap();
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                service.entered.notified(),
            )
            .await
            .unwrap();
            if !delay {
                assert!(matches!(
                    next(&mut bridge).await,
                    ServerMessage::Speech { .. }
                ));
            }
            drop(bridge);
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                while !service.cancelled.lock().unwrap().contains(&owner) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(service.listening.lock().unwrap().is_none());
        }
    }

    #[cfg(test)]
    #[tokio::test]
    async fn disconnect_of_an_old_view_does_not_cancel_another_listener_owner() {
        let (runtime, _) = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let service = Arc::new(OwnedSpeech {
            host: cell::speech::Host::open(root.path().join("speech.json")).unwrap(),
            listening: Default::default(),
            cancelled: Default::default(),
            entered: tokio::sync::Notify::new(),
            delay: false,
        });
        let mut bridge = connect_session(
            runtime.clone(),
            WakeSignal::new(|| {}),
            runtime.local_session().with_speech(service.clone()),
        );
        let owner = nucleus::new_uid("voice");
        bridge
            .outgoing
            .send(ClientMessage::Speech {
                id: "old-start".into(),
                request: cell::speech::Request::StartListening {
                    mode: cell::speech::ListeningMode::Model,
                    owner: owner.clone(),
                    target: None,
                    routing: false,
                },
            })
            .await
            .unwrap();
        assert!(matches!(
            next(&mut bridge).await,
            ServerMessage::Speech { .. }
        ));
        let other = nucleus::new_uid("voice");
        *service.listening.lock().unwrap() = Some(other.clone());
        drop(bridge);
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !service.cancelled.lock().unwrap().contains(&owner) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            service.listening.lock().unwrap().as_deref(),
            Some(other.as_str())
        );
        assert!(!service.cancelled.lock().unwrap().contains(&other));
    }

    #[cfg(test)]
    #[tokio::test]
    #[ignore = "Requires installed Phonon weights and the independently referenced goforward recording"]
    async fn actual_disconnect_cancels_transcription_without_releasing_cpu_early_or_retaining_results()
     {
        use base64::Engine as _;
        use cell::speech::{Request, Service};
        let (mut runtime, _) = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let host = Arc::new(cell::speech::Host::open(root.path().join("speech.json")).unwrap());
        let model = host.handle(Request::Inspect).await.unwrap().local.unwrap();
        let source = std::path::PathBuf::from(
            std::env::var_os("LINCE_SPEECH_TEST_CACHE").expect("Choose installed model cache"),
        )
        .join(format!("{}-{}", model.model, model.revision))
        .join("phonon-2.bps.tar.zst");
        let setup = nucleus::new_uid("speech");
        host.handle(Request::InstallLocal {
            job: setup.clone(),
            source: Some(source),
        })
        .await
        .unwrap();
        async fn finished(host: &cell::speech::Host, job: &str) -> cell::speech::Job {
            tokio::time::timeout(std::time::Duration::from_secs(180), async {
                loop {
                    let result = host
                        .handle(Request::Poll { job: job.into() })
                        .await
                        .unwrap()
                        .job
                        .unwrap();
                    if !result.pending {
                        return result;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap()
        }
        assert!(finished(&host, &setup).await.error.is_none());
        runtime.speech = Some(host.clone());
        let audio = cell::FioteSecret(
            base64::engine::general_purpose::STANDARD.encode(
                std::fs::read(
                    std::env::var_os("LINCE_SPEECH_TEST_WAV").expect("Choose goforward WAV"),
                )
                .unwrap(),
            ),
        );
        let mut bridge = connect(runtime, WakeSignal::new(|| {}));
        let job = nucleus::new_uid("speech");
        bridge
            .outgoing
            .send(ClientMessage::Speech {
                id: "real-start".into(),
                request: Request::StartLocal {
                    job: job.clone(),
                    audio: audio.clone(),
                },
            })
            .await
            .unwrap();
        assert!(
            matches!(next(&mut bridge).await, ServerMessage::Speech { status, .. } if status.job.as_ref().is_some_and(|result| result.id == job && result.pending))
        );
        tokio::time::timeout(std::time::Duration::from_secs(180), async {
            loop {
                let local = host.handle(Request::Inspect).await.unwrap().local.unwrap();
                if local.loaded && local.busy {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        drop(bridge);
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while host
                .handle(Request::Poll { job: job.clone() })
                .await
                .is_ok()
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if host
            .handle(Request::Inspect)
            .await
            .unwrap()
            .local
            .unwrap()
            .busy
        {
            let refused = host
                .handle(Request::StartLocal {
                    job: nucleus::new_uid("speech"),
                    audio: audio.clone(),
                })
                .await
                .unwrap_err();
            assert!(refused.contains("finishing"), "{refused}");
        }
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            while host
                .handle(Request::Inspect)
                .await
                .unwrap()
                .local
                .unwrap()
                .busy
            {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(host.handle(Request::Poll { job }).await.is_err());
        let retry = nucleus::new_uid("speech");
        host.handle(Request::StartLocal {
            job: retry.clone(),
            audio,
        })
        .await
        .unwrap();
        let result = finished(&host, &retry).await;
        assert!(result.error.is_none(), "{result:?}");
        assert_eq!(
            result.text.unwrap().to_lowercase().trim_end_matches('.'),
            "go forward ten meters"
        );
        host.handle(Request::Cancel { job: retry }).await.unwrap();
    }

    #[cfg(test)]
    #[tokio::test]
    async fn stop_listening_is_accepted_while_eight_other_speech_controls_are_busy() {
        let (runtime, _) = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let entered = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(tokio::sync::Notify::new());
        let session = runtime.local_session().with_speech(Arc::new(BusySpeech {
            host: cell::speech::Host::open(root.path().join("speech.json")).unwrap(),
            entered: entered.clone(),
            release: release.clone(),
        }));
        let mut bridge = connect_session(runtime, WakeSignal::new(|| {}), session);
        for index in 0..8 {
            bridge
                .outgoing
                .send(ClientMessage::Speech {
                    id: format!("busy-{index}"),
                    request: cell::speech::Request::Inspect,
                })
                .await
                .unwrap();
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while entered.load(Ordering::SeqCst) != 8 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        bridge
            .outgoing
            .send(ClientMessage::Speech {
                id: "ninth".into(),
                request: cell::speech::Request::Inspect,
            })
            .await
            .unwrap();
        assert!(
            matches!(next(&mut bridge).await, ServerMessage::Error { id, code, .. } if id == "ninth" && code.as_deref() == Some("speech_busy"))
        );
        let id = bridge
            .speech_cancellation
            .send(cell::speech::Request::StopListening {
                owner: nucleus::new_uid("voice"),
                generation: 0,
            })
            .unwrap();
        assert!(
            matches!(next(&mut bridge).await, ServerMessage::Speech { id: observed, status } if observed == id && !status.listening.enabled)
        );
        release.notify_waiters();
    }

    async fn fixture() -> (cell::CellRuntime, String) {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let uid = engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Original".into(),
                    body: "Keep the body".into(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        (
            cell::CellRuntime {
                commands: Default::default(),
                store: engine.store.clone(),
                engine,
                lanes: Arc::new(cell::LaneHub::new()),
                wire: Default::default(),
                fiote: None,
                speech: None,
                information: None,
            },
            uid,
        )
    }

    async fn next(bridge: &mut CellBridge) -> ServerMessage {
        tokio::time::timeout(std::time::Duration::from_secs(5), bridge.incoming.recv())
            .await
            .unwrap()
            .unwrap()
    }

    #[cfg_attr(test, tokio::test)]
    async fn stopped_bridge_wakes_the_interface_and_reports_disconnect_once() {
        let (runtime, _) = fixture().await;
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        let wake = WakeSignal::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        });
        let bridge = connect(runtime, wake.clone());
        bridge.task.abort();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !bridge.closed.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(wakes.load(Ordering::SeqCst) > 0);
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_message::<CellMessage>()
            .insert_resource(wake)
            .add_systems(Update, drain);
        app.world_mut().insert_non_send(bridge);
        let mut cursor = bevy::ecs::message::MessageCursor::<CellMessage>::default();
        app.update();
        assert_eq!(cursor.read(app.world().resource::<Messages<CellMessage>>()).filter(|message| matches!(&message.0, ServerMessage::Error { id, .. } if id == CONNECTION)).count(), 1);
        app.update();
        assert_eq!(
            cursor
                .read(app.world().resource::<Messages<CellMessage>>())
                .count(),
            0
        );
    }

    #[cfg_attr(test, tokio::test)]
    async fn subscription_action_and_external_changes_wake_the_interface() {
        let (runtime, uid) = fixture().await;
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let mut bridge = connect(
            runtime.clone(),
            WakeSignal::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );
        bridge.outgoing.send(records_subscription()).await.unwrap();
        let message = next(&mut bridge).await;
        let ServerMessage::Snapshot { rows, .. } = message else {
            panic!("expected initial records, got {message:?}");
        };
        let properties = rows.iter().find(|row| row["uid"] == uid).unwrap();
        assert_eq!(properties["quantity"], "0");
        assert!(properties["kind"].is_string());
        assert!(properties.get("slug").is_some());
        assert_eq!(
            rows.iter().find(|row| row["uid"] == uid).unwrap()["head"],
            "Original"
        );
        bridge
            .outgoing
            .send(ClientMessage::Act {
                id: "edit".into(),
                action: Action::EditRecordText {
                    target: uid.clone(),
                    head: Some("Saved title".into()),
                    body: None,
                },
            })
            .await
            .unwrap();
        let mut acknowledged = false;
        let mut updated = false;
        while !acknowledged || !updated {
            match next(&mut bridge).await {
                ServerMessage::ActionOk { id, .. } => {
                    assert_eq!(id, "edit");
                    acknowledged = true;
                }
                ServerMessage::Update { rows, .. } => {
                    updated |= rows
                        .iter()
                        .any(|row| row["uid"] == uid && row["head"] == "Saved title");
                }
                message => panic!("unexpected reply: {message:?}"),
            }
        }
        runtime
            .engine
            .act(
                Action::EditRecordText {
                    target: uid.clone(),
                    head: Some("Another editor".into()),
                    body: None,
                },
                None,
            )
            .await
            .unwrap();
        loop {
            if let ServerMessage::Update { rows, .. } = next(&mut bridge).await
                && rows
                    .iter()
                    .any(|row| row["uid"] == uid && row["head"] == "Another editor")
            {
                break;
            }
        }
        assert!(wakes.load(Ordering::SeqCst) >= 4);
        let task = bridge.task.abort_handle();
        drop(bridge);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !task.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[cfg_attr(test, tokio::test)]
    async fn lanes_share_the_wake_channel_and_invalid_actions_return_errors() {
        let (runtime, _) = fixture().await;
        let mut bridge = connect(runtime.clone(), WakeSignal::new(|| {}));
        bridge
            .outgoing
            .send(ClientMessage::LaneJoin {
                room: "test-room".into(),
            })
            .await
            .unwrap();
        bridge.outgoing.send(records_subscription()).await.unwrap();
        assert!(matches!(
            next(&mut bridge).await,
            ServerMessage::Snapshot { .. }
        ));
        runtime.lanes.send(cell::LaneEvent {
            room: "test-room".into(),
            from: "other".into(),
            payload: serde_json::json!({"cursor": 3}),
            from_subject: None,
            organ: None,
        });
        assert!(
            matches!(next(&mut bridge).await, ServerMessage::LaneEvent { room, .. } if room == "test-room")
        );
        bridge
            .outgoing
            .send(ClientMessage::Act {
                id: "bad".into(),
                action: Action::EditRecordText {
                    target: "missing-record".into(),
                    head: Some("Rejected".into()),
                    body: None,
                },
            })
            .await
            .unwrap();
        assert!(matches!(next(&mut bridge).await, ServerMessage::Error { id, .. } if id == "bad"));
    }

    fn subscribe(id: &str, source: protein::Source) -> ClientMessage {
        let ClientMessage::Subscribe { mut protein, .. } = records_subscription() else {
            unreachable!()
        };
        protein.source = source;
        protein.fields = None;
        ClientMessage::Subscribe {
            id: id.into(),
            protein,
        }
    }

    #[cfg_attr(test, tokio::test)]
    async fn nearby_changes_arrive_without_facts_and_stop_after_unsubscribe() {
        let (runtime, _) = fixture().await;
        let nearby = engine::wire::Nearby::default();
        runtime.engine.attach_nearby(nearby.clone());
        let mut bridge = connect(runtime, WakeSignal::new(|| {}));
        bridge
            .outgoing
            .send(subscribe("nearby", protein::Source::Nearby))
            .await
            .unwrap();
        assert!(matches!(next(&mut bridge).await,
            ServerMessage::Snapshot { id, rows } if id == "nearby" && rows.is_empty()));

        nearby.observe("phone".into(), "fingerprint".into(), "Phone".into());
        assert!(matches!(next(&mut bridge).await,
            ServerMessage::Update { id, rows } if id == "nearby" && rows.len() == 1));
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(3100),
                bridge.incoming.recv()
            )
            .await
            .is_err()
        );

        bridge
            .outgoing
            .send(ClientMessage::Unsubscribe {
                id: "nearby".into(),
            })
            .await
            .unwrap();
        bridge.outgoing.send(records_subscription()).await.unwrap();
        assert!(
            matches!(next(&mut bridge).await, ServerMessage::Snapshot { id, .. } if id == RECORDS)
        );
        nearby.forget("phone");
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(3100),
                bridge.incoming.recv()
            )
            .await
            .is_err()
        );
    }

    #[cfg_attr(test, tokio::test)]
    async fn facts_deliver_live_protein_results_without_a_socket() {
        let (runtime, uid) = fixture().await;
        let mut bridge = connect(runtime.clone(), WakeSignal::new(|| {}));
        bridge
            .outgoing
            .send(subscribe("facts", protein::Source::Fact))
            .await
            .unwrap();
        let ServerMessage::Snapshot { rows, .. } = next(&mut bridge).await else {
            panic!("expected initial facts")
        };
        let previous = rows.len();
        runtime.engine.append_user(&uid, 1.0).await.unwrap();
        assert!(matches!(next(&mut bridge).await,
            ServerMessage::Update { id, rows } if id == "facts" && rows.len() > previous));
    }

    #[cfg_attr(test, tokio::test)]
    async fn backend_actions_without_facts_refresh_protein_views() {
        let (runtime, _) = fixture().await;
        let mut bridge = connect(runtime.clone(), WakeSignal::new(|| {}));
        bridge
            .outgoing
            .send(subscribe("concepts", protein::Source::Concept))
            .await
            .unwrap();
        assert!(matches!(
            next(&mut bridge).await,
            ServerMessage::Snapshot { .. }
        ));
        let created = runtime
            .engine
            .act(
                Action::CreateConcept {
                    lingua: "g_local".into(),
                    name: "new-concept".into(),
                    parents: Vec::new(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(created.facts.is_empty());
        let uid = created.created.unwrap();
        assert!(matches!(next(&mut bridge).await,
            ServerMessage::Snapshot { id, rows } if id == "concepts" && rows.iter().any(|row| row["uid"] == uid)));
        runtime
            .engine
            .act(
                Action::DeleteConcept {
                    concept: uid.clone(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(matches!(next(&mut bridge).await,
            ServerMessage::Snapshot { id, rows } if id == "concepts" && rows.iter().all(|row| row["uid"] != uid)));
    }

    #[cfg_attr(test, tokio::test)]
    async fn assertions_deliver_live_results_and_acknowledge_writes() {
        let (runtime, uid) = fixture().await;
        runtime
            .engine
            .act(
                Action::CreateConcept {
                    lingua: "g_local".into(),
                    name: "connected".into(),
                    parents: Vec::new(),
                },
                None,
            )
            .await
            .unwrap();
        let mut bridge = connect(runtime, WakeSignal::new(|| {}));
        bridge
            .outgoing
            .send(subscribe("assertions", protein::Source::Assertion))
            .await
            .unwrap();
        let ServerMessage::Snapshot { rows, .. } = next(&mut bridge).await else {
            panic!("expected initial assertions")
        };
        let previous = rows.len();
        bridge
            .outgoing
            .send(ClientMessage::Act {
                id: "assert".into(),
                action: Action::AssertRecord {
                    subject: uid,
                    predicate: "connected".into(),
                    object: None,
                    quantity: None,
                    unit: None,
                },
            })
            .await
            .unwrap();
        let mut acknowledged = false;
        let mut updated = false;
        while !acknowledged || !updated {
            match next(&mut bridge).await {
                ServerMessage::ActionOk { id, .. } if id == "assert" => acknowledged = true,
                ServerMessage::Update { id, rows } if id == "assertions" => {
                    updated |= rows.len() > previous
                }
                message => panic!("unexpected assertion response: {message:?}"),
            }
        }
    }

    #[cfg_attr(test, tokio::test)]
    async fn a_slow_interface_recovers_current_data_after_the_fact_bus_overflows() {
        let (runtime, uid) = fixture().await;
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let mut bridge = connect(
            runtime.clone(),
            WakeSignal::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );
        for _ in 0..65 {
            bridge.outgoing.send(records_subscription()).await.unwrap();
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while wakes.load(Ordering::SeqCst) < 64 || bridge.outgoing.capacity() != 64 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        for _ in 0..1030 {
            runtime.engine.append_user(&uid, 1.0).await.unwrap();
        }
        runtime
            .engine
            .act(
                Action::EditRecordText {
                    target: uid.clone(),
                    head: Some("After overflow".into()),
                    body: None,
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(wakes.load(Ordering::SeqCst), 64);
        for _ in 0..65 {
            assert!(matches!(
                next(&mut bridge).await,
                ServerMessage::Snapshot { .. }
            ));
        }
        assert!(matches!(next(&mut bridge).await,
            ServerMessage::Snapshot { rows, .. }
                if rows.iter().any(|row| row["uid"] == uid && row["head"] == "After overflow")));
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                bridge.incoming.recv()
            )
            .await
            .is_err()
        );
        runtime
            .engine
            .act(
                Action::EditRecordText {
                    target: uid.clone(),
                    head: Some("Still connected".into()),
                    body: None,
                },
                None,
            )
            .await
            .unwrap();
        assert!(matches!(next(&mut bridge).await,
            ServerMessage::Update { rows, .. }
                if rows.iter().any(|row| row["uid"] == uid && row["head"] == "Still connected")));
    }

    crate::native::laboratory_cases! {
        async stopped_bridge_wakes_the_interface_and_reports_disconnect_once,
        async subscription_action_and_external_changes_wake_the_interface,
        async lanes_share_the_wake_channel_and_invalid_actions_return_errors,
        async nearby_changes_arrive_without_facts_and_stop_after_unsubscribe,
        async facts_deliver_live_protein_results_without_a_socket,
        async backend_actions_without_facts_refresh_protein_views,
        async assertions_deliver_live_results_and_acknowledge_writes,
        async a_slow_interface_recovers_current_data_after_the_fact_bus_overflows,
    }

    #[cfg(test)]
    struct HeldPreview {
        ready: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }

    #[cfg(test)]
    impl engine::karma_preview::Runner for HeldPreview {
        fn run<'a>(
            &'a self,
            engine: &'a Engine,
            _actor: Option<String>,
            request: engine::karma_preview::Request,
            now: chrono::DateTime<chrono::Utc>,
        ) -> engine::karma_preview::PreviewFuture<'a> {
            Box::pin(async move {
                let source = engine.store.state_hash().await?.as_str().into();
                self.ready.notify_one();
                self.release.notified().await;
                Ok(engine::karma_preview::Report {
                    required_reads: Vec::new(),
                    draft: request.fingerprint()?,
                    source,
                    source_current: true,
                    start_ms: now.timestamp_millis(),
                    requested_until_ms: now.timestamp_millis(),
                    stopped_at_ms: now.timestamp_millis(),
                    stop: nucleus::simulation::Stop::HorizonReached {},
                    evaluations: 0,
                    final_values: Vec::new(),
                    first_failure: None,
                    cycles: Vec::new(),
                    coverage: Vec::new(),
                    incomplete: false,
                    unsupported: Vec::new(),
                    assumptions: Vec::new(),
                })
            })
        }
    }

    #[tokio::test]
    async fn native_karma_preview_keeps_live_editing_available_and_reports_stale_data() {
        let (runtime, uid) = fixture().await;
        let mut bridge = connect(runtime.clone(), WakeSignal::new(|| {}));
        let ready = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        runtime
            .engine
            .install_karma_preview_runner(Arc::new(HeldPreview {
                ready: ready.clone(),
                release: release.clone(),
            }))
            .unwrap();
        let request = engine::karma_preview::Request {
            proposals: vec![engine::karma_preview::ProposedRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: ["1", "always", &format!("@{uid}")].map(|source| {
                    nucleus::karma::rule_field::RuleFieldInput::Text {
                        source: source.into(),
                    }
                }),
            }],
            limits: Default::default(),
            inputs: Vec::new(),
            records: Vec::new(),
            quantity_basis: Default::default(),
            checks: Vec::new(),
            saved_checks: None,
            checks_start_ms: None,
            checking: Default::default(),
        };
        bridge
            .outgoing
            .send(ClientMessage::Act {
                id: "preview".into(),
                action: Action::PreviewKarmaProposal { request },
            })
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), ready.notified())
            .await
            .unwrap();
        bridge
            .outgoing
            .send(ClientMessage::Act {
                id: "edit".into(),
                action: Action::SetQuantityExact {
                    target: uid.clone(),
                    amount: "5".into(),
                },
            })
            .await
            .unwrap();
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), next(&mut bridge))
            .await
            .unwrap();
        assert!(matches!(reply, ServerMessage::ActionOk { id, .. } if id == "edit"));
        let reading = runtime
            .engine
            .act(
                Action::PreviewKarmaReading {
                    source: format!("@{uid}"),
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(reading.data.unwrap()["value"], "5");
        release.notify_one();
        let reply = next(&mut bridge).await;
        assert!(
            matches!(reply, ServerMessage::ActionOk { id, data: Some(data), .. } if id == "preview" && data["source_current"] == false)
        );
    }
}
