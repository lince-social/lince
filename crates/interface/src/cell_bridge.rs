use crate::{app::CellHandle, wake::WakeSignal};
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
    task: JoinHandle<()>,
    closed: Arc<AtomicBool>,
    reported_closed: bool,
}

struct ConnectionEnded {
    closed: Arc<AtomicBool>,
    wake: WakeSignal,
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
    let (outgoing, mut requests) = mpsc::channel::<ClientMessage>(64);
    let (responses, incoming) = mpsc::channel(64);
    let mut facts = runtime.engine.subscribe();
    let mut query_changes = runtime.engine.watch_query_changes();
    let mut session = runtime.local_session();
    let closed = Arc::new(AtomicBool::new(false));
    let ended = ConnectionEnded {
        closed: closed.clone(),
        wake: wake.clone(),
    };
    let task = tokio::spawn(async move {
        let _ended = ended;
        let mut lanes = HashMap::<String, tokio::task::AbortHandle>::new();
        let mut lane_tasks = tokio::task::JoinSet::new();
        let mut ephemeral = tokio::time::interval(std::time::Duration::from_secs(3));
        ephemeral.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let messages = tokio::select! {
                request = requests.recv() => {
                    let Some(request) = request else { break };
                    match &request {
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
                fact = facts.recv() => match fact {
                    Ok(fact) => session.on_fact(&fact).await,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        facts = facts.resubscribe();
                        session.refresh().await
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = ephemeral.tick(), if session.has_ephemeral_subscriptions() => {
                    session.tick_ephemeral().await
                },
                changed = query_changes.changed() => {
                    if changed.is_err() { break; }
                    session.refresh().await
                },
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
        task,
        closed,
        reported_closed: false,
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
                drain.in_set(ReceiveCell).run_if(crate::laboratory::normal),
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
                store: engine.store.clone(),
                engine,
                lanes: Arc::new(cell::LaneHub::new()),
                wire: Default::default(),
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
        crate::laboratory::isolate(app.world_mut());
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
        assert!(properties["quantity"].is_number());
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

    crate::laboratory_cases! {
        async stopped_bridge_wakes_the_interface_and_reports_disconnect_once,
        async subscription_action_and_external_changes_wake_the_interface,
        async lanes_share_the_wake_channel_and_invalid_actions_return_errors,
        async nearby_changes_arrive_without_facts_and_stop_after_unsubscribe,
        async facts_deliver_live_protein_results_without_a_socket,
        async backend_actions_without_facts_refresh_protein_views,
        async assertions_deliver_live_results_and_acknowledge_writes,
        async a_slow_interface_recovers_current_data_after_the_fact_bus_overflows,
    }
}
