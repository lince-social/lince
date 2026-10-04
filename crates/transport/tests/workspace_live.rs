use engine::trust::Signer;
use engine::wire::{ALPN_LIVE, Reach, Wire};
use engine::{
    Engine,
    actions::Action,
    workspace_sync::{Change, Client, Command, Element, Request},
};
use iroh::{EndpointAddr, SecretKey};
use nucleus::{
    canvas::{Component, Geometry},
    component::ComponentState,
};
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{sync::mpsc, task::JoinHandle};
use transport::{ClientMessage, LaneHub, ServerMessage, live::LiveHost};

struct View {
    requests: mpsc::Sender<ClientMessage>,
    responses: mpsc::Receiver<ServerMessage>,
    task: JoinHandle<Result<(), String>>,
    snapshot: Option<Value>,
}

impl View {
    async fn receive(&mut self) -> ServerMessage {
        let message = tokio::time::timeout(Duration::from_secs(15), self.responses.recv())
            .await
            .unwrap()
            .expect("Live session remains open");
        if let ServerMessage::Workspace { workspace, .. } = &message {
            self.snapshot = Some(workspace.clone());
        }
        if let ServerMessage::Error { message, code, .. } = &message {
            panic!("Unexpected live refusal {code:?}: {message}");
        }
        message
    }

    async fn revision(&mut self, revision: i64) -> Value {
        loop {
            if let Some(snapshot) = &self.snapshot
                && snapshot["revision"].as_i64().unwrap() >= revision
            {
                return snapshot.clone();
            }
            self.receive().await;
        }
    }

    async fn act(&mut self, id: &str, command: Command) -> Value {
        self.requests
            .send(ClientMessage::Act {
                id: id.into(),
                action: Action::Workspace {
                    request: Request {
                        client: Client::default(),
                        command,
                    },
                },
            })
            .await
            .unwrap();
        loop {
            if let ServerMessage::ActionOk {
                id: reply, data, ..
            } = self.receive().await
                && reply == id
            {
                return data.unwrap();
            }
        }
    }
}

async fn cell(seed: u8) -> (Arc<Engine>, Wire, String) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    engine
        .set_signer(Signer::generate(&organ, "workspace-test"))
        .await
        .unwrap();
    let wire = Wire::bind_with_discovery(
        engine.clone(),
        SecretKey::from_bytes(&[seed; 32]),
        Reach::Local,
        None,
        false,
    )
    .await
    .unwrap();
    (engine, wire, organ)
}

async fn grant(host: &Engine, wire: &Wire, organ: &str) -> String {
    store::organs::add_contact(
        &host.store.pool,
        organ,
        None,
        "Workspace participant",
        "",
        0,
    )
    .await
    .unwrap();
    store::organs::set_node_id(&host.store.pool, organ, Some(&wire.node_id().to_string()))
        .await
        .unwrap();
    store::organs::set_trust(&host.store.pool, organ, "known")
        .await
        .unwrap();
    let person = host
        .act(
            Action::GrantOrganLogin {
                organ: organ.into(),
                person_name: "Workspace participant".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let role = store::auth::ensure_role(&host.store.pool, "Workspace participant")
        .await
        .unwrap();
    for action in ["read", "update"] {
        let permission = store::auth::ensure_permission(&host.store.pool, "workspace", action)
            .await
            .unwrap();
        store::auth::grant(&host.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::set_user_role(&host.store.pool, &person, role)
        .await
        .unwrap();
    person
}

async fn connect(host: &Wire, guest: &Wire, person: &str, workspace: &str) -> View {
    let port = host.endpoint().bound_sockets()[0].port();
    let address =
        EndpointAddr::new(host.node_id()).with_ip_addr(SocketAddr::from(([127, 0, 0, 1], port)));
    let connection = guest.endpoint().connect(address, ALPN_LIVE).await.unwrap();
    let (requests, outgoing) = mpsc::channel(16);
    let (responses, incoming) = mpsc::channel(64);
    let task = tokio::spawn(transport::live_client::drive(
        connection,
        outgoing,
        responses,
        || {},
    ));
    let mut view = View {
        requests,
        responses: incoming,
        task,
        snapshot: None,
    };
    assert!(
        matches!(view.receive().await, ServerMessage::SessionAuthenticated { person: actor, .. } if actor == person)
    );
    view.requests
        .send(ClientMessage::WorkspaceSubscribe {
            id: "workspace".into(),
            workspace: workspace.into(),
            client: Client::default(),
        })
        .await
        .unwrap();
    view.revision(1).await;
    view
}

fn placement(text: &str) -> Element {
    Element {
        id: nucleus::new_uid("placement"),
        component: Component::Builtin {
            state: ComponentState::Text { text: text.into() },
        },
        geometry: Geometry {
            position: [0.0; 2],
            size: [100.0; 2],
        },
    }
}

fn proposal(workspace: &str, request_id: &str, base_revision: i64, change: Change) -> Command {
    Command::Propose {
        workspace: workspace.into(),
        request_id: request_id.into(),
        base_revision,
        change,
    }
}

#[tokio::test]
async fn network_workspace_merges_signed_edits_reconnects_retries_and_revokes() {
    let (host, host_wire, _) = cell(91).await;
    let (_, first_wire, first_organ) = cell(92).await;
    let (_, second_wire, second_organ) = cell(93).await;
    let first_person = grant(&host, &first_wire, &first_organ).await;
    let second_person = grant(&host, &second_wire, &second_organ).await;
    let workspace = host.act(Action::Workspace { request: Request { client: Client::default(), command: Command::Create { name: "Network workspace".into(), policy: json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[]}}) } } }, None).await.unwrap().created.unwrap();
    let host_wire = Arc::new(host_wire);
    host_wire.set_live_handler(LiveHost::new(host.clone(), Arc::new(LaneHub::new())));
    let serving = {
        let wire = host_wire.clone();
        tokio::spawn(async move { wire.serve().await })
    };
    let mut first = connect(&host_wire, &first_wire, &first_person, &workspace).await;
    let mut second = connect(&host_wire, &second_wire, &second_person, &workspace).await;
    let first_element = placement("First participant");
    let second_element = placement("Second participant");
    assert_eq!(
        first
            .act(
                "first-add",
                proposal(
                    &workspace,
                    &nucleus::new_uid("request"),
                    1,
                    Change::Add {
                        element: first_element.clone()
                    }
                )
            )
            .await["state"],
        "applied"
    );
    second.revision(2).await;
    assert_eq!(
        second
            .act(
                "second-add",
                proposal(
                    &workspace,
                    &nucleus::new_uid("request"),
                    2,
                    Change::Add {
                        element: second_element.clone()
                    }
                )
            )
            .await["state"],
        "applied"
    );
    first.revision(3).await;
    second.revision(3).await;
    let first_move = Change::Move {
        element: first_element.id.clone(),
        position: [10.0, 20.0],
    };
    let second_move = Change::Move {
        element: second_element.id.clone(),
        position: [30.0, 40.0],
    };
    let first_request = nucleus::new_uid("request");
    let (first_result, second_result) = tokio::join!(
        first.act(
            "first-move",
            proposal(&workspace, &first_request, 3, first_move.clone())
        ),
        second.act(
            "second-move",
            proposal(&workspace, &nucleus::new_uid("request"), 3, second_move)
        )
    );
    assert_eq!(first_result["state"], "applied");
    assert_eq!(second_result["state"], "applied");
    let first_snapshot = first.revision(5).await;
    let second_snapshot = second.revision(5).await;
    assert_eq!(first_snapshot["layout"], second_snapshot["layout"]);
    for (id, position) in [
        (&first_element.id, [10.0, 20.0]),
        (&second_element.id, [30.0, 40.0]),
    ] {
        assert_eq!(
            first_snapshot["layout"]["elements"]
                .as_array()
                .unwrap()
                .iter()
                .find(|element| element["id"] == *id)
                .unwrap()["geometry"]["position"],
            json!(position)
        );
    }
    first.task.abort();
    let _ = first.task.await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while host.workspace_presence.participants(&workspace).len() != 1 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut first = connect(&host_wire, &first_wire, &first_person, &workspace).await;
    assert_eq!(first.revision(5).await["layout"], first_snapshot["layout"]);
    let retried = first
        .act(
            "retry-first-move",
            proposal(&workspace, &first_request, 3, first_move),
        )
        .await;
    assert_eq!(retried, first_result);
    let current = host
        .act(
            Action::Workspace {
                request: Request {
                    client: Client::default(),
                    command: Command::Inspect {
                        workspace: workspace.clone(),
                        permitted_view: false,
                    },
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(current["revision"], 5);
    host.act(Action::RevokeOrganLogin { organ: first_organ }, None)
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(15), &mut first.task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(
        second
            .act(
                "remaining-participant",
                proposal(
                    &workspace,
                    &nucleus::new_uid("request"),
                    5,
                    Change::Move {
                        element: second_element.id,
                        position: [50.0, 60.0]
                    }
                )
            )
            .await["state"],
        "applied"
    );
    assert_eq!(second.revision(6).await["revision"], 6);
    second.task.abort();
    let _ = second.task.await;
    host_wire.shutdown().await;
    first_wire.shutdown().await;
    second_wire.shutdown().await;
    serving.abort();
}

#[tokio::test]
async fn capacity_load_merges_concurrent_edits_and_revokes_all_live_readers() {
    let started = std::time::Instant::now();
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let role = store::auth::ensure_role(&engine.store.pool, "Load participants")
        .await
        .unwrap();
    let mut read_permission = 0;
    for operation in ["read", "update"] {
        let permission = store::auth::ensure_permission(&engine.store.pool, "workspace", operation)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
        if operation == "read" {
            read_permission = permission;
        }
    }
    let mut logins = Vec::new();
    for index in 0..2 {
        let username = format!("load-participant-{index}");
        engine
            .act(
                Action::CreateUser {
                    username: username.clone(),
                    name: username.clone(),
                    password: "workspace-load-test-password".into(),
                    role: "Load participants".into(),
                },
                None,
            )
            .await
            .unwrap();
        logins.push(
            engine
                .login_password(
                    &username,
                    engine::private_password::PasswordInput::new(
                        b"workspace-load-test-password".to_vec(),
                    )
                    .unwrap(),
                    None,
                )
                .await
                .unwrap(),
        );
    }
    let workspace = engine.act(Action::Workspace { request: Request { client: Client::default(), command: Command::Create { name: "Capacity workspace".into(), policy: json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[]}}) } } }, None).await.unwrap().created.unwrap();
    let mut elements = Vec::new();
    for index in 0..256 {
        let element = placement(&format!("Placement {index}"));
        let result = engine
            .act(
                Action::Workspace {
                    request: Request {
                        client: Client::default(),
                        command: proposal(
                            &workspace,
                            &nucleus::new_uid("request"),
                            index + 1,
                            Change::Add {
                                element: element.clone(),
                            },
                        ),
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert_eq!(result["state"], "applied");
        elements.push(element);
    }
    let hub = Arc::new(LaneHub::new());
    let mut views = Vec::new();
    for index in 0..64 {
        let mut view = transport::Session::authenticated(
            engine.clone(),
            hub.clone(),
            format!("load-{index}"),
            logins[index % 2].clone(),
        );
        let messages = view
            .handle(ClientMessage::WorkspaceSubscribe {
                id: "workspace".into(),
                workspace: workspace.clone(),
                client: Client::default(),
            })
            .await;
        assert!(messages.iter().any(|message| matches!(message, ServerMessage::Workspace { workspace, .. } if workspace["revision"] == 257)));
        views.push(view);
    }
    assert_eq!(engine.workspace_presence.participants(&workspace).len(), 64);
    let mut overflow =
        transport::Session::authenticated(engine.clone(), hub, "overflow", logins[0].clone());
    let refused = overflow
        .handle(ClientMessage::WorkspaceSubscribe {
            id: "workspace".into(),
            workspace: workspace.clone(),
            client: Client::default(),
        })
        .await;
    assert!(
        refused
            .iter()
            .any(|message| matches!(message, ServerMessage::Error { .. }))
    );
    assert!(
        !refused
            .iter()
            .any(|message| matches!(message, ServerMessage::Workspace { .. }))
    );
    let editing_started = std::time::Instant::now();
    let mut edits = tokio::task::JoinSet::new();
    for (index, element) in elements.iter().take(64).enumerate() {
        let engine = engine.clone();
        let actor = logins[index % 2].person_uid().to_owned();
        let command = proposal(
            &workspace,
            &nucleus::new_uid("request"),
            257,
            Change::Move {
                element: element.id.clone(),
                position: [index as f64 + 1.0, 20.0],
            },
        );
        edits.spawn(async move {
            engine
                .act(
                    Action::Workspace {
                        request: Request {
                            client: Client::default(),
                            command,
                        },
                    },
                    Some(actor),
                )
                .await
                .unwrap()
                .data
                .unwrap()
        });
    }
    while let Some(result) = edits.join_next().await {
        assert_eq!(result.unwrap()["state"], "applied");
    }
    let editing_seconds = editing_started.elapsed().as_secs_f64();
    let refresh_started = std::time::Instant::now();
    for view in &mut views {
        let messages = view.refresh().await;
        let snapshot = messages
            .iter()
            .find_map(|message| match message {
                ServerMessage::Workspace { workspace, .. } => Some(workspace),
                _ => None,
            })
            .unwrap();
        assert_eq!(snapshot["revision"], 321);
        assert_eq!(
            snapshot["layout"]["elements"].as_array().unwrap().len(),
            256
        );
        for (index, element) in elements.iter().take(64).enumerate() {
            let placement = snapshot["layout"]["elements"]
                .as_array()
                .unwrap()
                .iter()
                .find(|placement| placement["id"] == element.id)
                .unwrap();
            assert_eq!(
                placement["geometry"]["position"],
                json!([index as f64 + 1.0, 20.0])
            );
        }
    }
    let refresh_seconds = refresh_started.elapsed().as_secs_f64();
    store::auth::revoke(&engine.store.pool, role, read_permission)
        .await
        .unwrap();
    let revocation_started = std::time::Instant::now();
    for view in &mut views {
        let messages = view.refresh().await;
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, ServerMessage::Error { .. }))
        );
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, ServerMessage::Workspace { .. }))
        );
        assert!(
            !view
                .refresh()
                .await
                .iter()
                .any(|message| matches!(message, ServerMessage::Workspace { .. }))
        );
    }
    assert!(
        engine
            .workspace_presence
            .participants(&workspace)
            .is_empty()
    );
    eprintln!(
        "Workspace capacity load: 256 placements, 64 live sessions, 64 concurrent independent edits; edits={editing_seconds:.3}s, full snapshot refresh={refresh_seconds:.3}s, revoke all={:.3}s, total={:.3}s",
        revocation_started.elapsed().as_secs_f64(),
        started.elapsed().as_secs_f64()
    );
}

fn resident_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|number| number.parse().ok())
        })
        .unwrap_or(0)
}

#[tokio::test]
async fn sustained_network_consequences_tolerate_slow_readers_and_connection_churn() {
    let (host, host_wire, _) = cell(101).await;
    let (_, first_wire, first_organ) = cell(102).await;
    let (_, second_wire, second_organ) = cell(103).await;
    let first_person = grant(&host, &first_wire, &first_organ).await;
    let second_person = grant(&host, &second_wire, &second_organ).await;
    for operation in ["read", "update"] {
        let permission = store::auth::ensure_permission(&host.store.pool, "record", operation)
            .await
            .unwrap();
        let role = store::auth::ensure_role(&host.store.pool, "Workspace participant")
            .await
            .unwrap();
        store::auth::grant(&host.store.pool, role, permission)
            .await
            .unwrap();
    }
    let mut records = vec![];
    let mut elements = vec![];
    for index in 0..8 {
        let record = store::records::create(
            &host.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: &format!("Network Record {index}"),
                body: &"Measured content ".repeat(128),
                quantity: store::exact::zero(),
            },
        )
        .await
        .unwrap()
        .uid;
        records.push(record.clone());
        elements.push(Element {
            id: nucleus::new_uid("placement"),
            component: Component::Builtin {
                state: ComponentState::Record {
                    record,
                    mode: Default::default(),
                    start_call: None,
                },
            },
            geometry: Geometry {
                position: [2000.0, 0.0],
                size: [100.0; 2],
            },
        });
    }
    let area = nucleus::new_uid("placement");
    let mut layout = engine::workspace_sync::Layout {
        elements: elements.clone(),
        ..Default::default()
    };
    layout.elements.push(Element {
        id: area.clone(),
        component: Component::Builtin {
            state: ComponentState::Area {
                immunity: Default::default(),
                strength: 0,
            },
        },
        geometry: Geometry {
            position: [0.0; 2],
            size: [1000.0; 2],
        },
    });
    layout.areas.insert(
        area,
        engine::area_transition::RecordChanges {
            quantity: Some("+=1".into()),
            ..Default::default()
        },
    );
    let selectors = records
        .iter()
        .map(|record| json!({"uid_eq":record}))
        .collect::<Vec<_>>();
    let policy = json!({"required_capabilities":["record:read","record:update"],"ceiling":{"read":{"all":[{"kind_eq":"plain"},{"any":selectors}]},"grants":[{"operation":"update","selector":{"all":[{"kind_eq":"plain"},{"any":selectors}]},"properties":["quantity"],"assertions_add":[],"assertions_remove":[]}]}});
    let workspace = host
        .act(
            Action::Workspace {
                request: Request {
                    client: Client::default(),
                    command: Command::Publish {
                        name: "Measured network".into(),
                        policy,
                        layout: serde_json::to_value(layout).unwrap(),
                    },
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let host_wire = Arc::new(host_wire);
    host_wire.set_live_handler(LiveHost::new(host.clone(), Arc::new(LaneHub::new())));
    let serving = {
        let wire = host_wire.clone();
        tokio::spawn(async move { wire.serve().await })
    };
    let mut readers = vec![];
    for _ in 0..4 {
        readers.push(connect(&host_wire, &first_wire, &first_person, &workspace).await);
    }
    let mut slow = connect(&host_wire, &second_wire, &second_person, &workspace).await;
    let rss_before = resident_kib();
    let start = std::time::Instant::now();
    let mut latencies = vec![];
    let mut revision = 1;
    for round in 0..32 {
        let base = revision;
        let tasks = readers
            .into_iter()
            .enumerate()
            .map(|(index, mut reader)| {
                let workspace = workspace.clone();
                let element = elements[index].id.clone();
                tokio::spawn(async move {
                    let start = std::time::Instant::now();
                    let result = reader
                        .act(
                            &format!("load-{round}-{index}"),
                            proposal(
                                &workspace,
                                &nucleus::new_uid("request"),
                                base,
                                Change::Move {
                                    element,
                                    position: [
                                        if round % 2 == 0 { 0.0 } else { 2000.0 },
                                        f64::from(round),
                                    ],
                                },
                            ),
                        )
                        .await;
                    assert_eq!(result["state"], "applied");
                    (reader, start.elapsed())
                })
            })
            .collect::<Vec<_>>();
        readers = vec![];
        for task in tasks {
            let (reader, latency) = task.await.unwrap();
            readers.push(reader);
            latencies.push(latency);
        }
        revision += 4;
        for reader in &mut readers {
            assert_eq!(reader.revision(revision).await["revision"], revision);
        }
        if round % 4 == 3 {
            let churn = connect(&host_wire, &second_wire, &second_person, &workspace).await;
            assert_eq!(churn.snapshot.unwrap()["revision"], revision);
            churn.task.abort();
            let _ = churn.task.await;
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
    assert_eq!(slow.revision(revision).await["revision"], revision);
    for record in records.iter().take(4) {
        assert_eq!(
            store::records::get(&host.store.pool, record)
                .await
                .unwrap()
                .unwrap()
                .quantity
                .to_string(),
            "16"
        );
    }
    latencies.sort();
    let median = latencies[latencies.len() / 2];
    let p95 = latencies[latencies.len() * 95 / 100];
    let rss_after = resident_kib();
    assert!(
        p95 < Duration::from_secs(10),
        "Healthy clients must keep making progress"
    );
    assert!(
        rss_after.saturating_sub(rss_before) < 512 * 1024,
        "Bounded workload must not retain unbounded queued snapshots"
    );
    println!(
        "Network load: {} signed edits over {:?}; 4 concurrent editors, 1 delayed reader, 8 reconnects, Record/Area consequences and compound policy; median {:?}, p95 {:?}, RSS {} -> {} KiB",
        latencies.len(),
        start.elapsed(),
        median,
        p95,
        rss_before,
        rss_after
    );
    host.act(
        Action::RevokeOrganLogin {
            organ: second_organ,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(15), &mut slow.task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    for reader in readers {
        reader.task.abort();
        let _ = reader.task.await;
    }
    host_wire.shutdown().await;
    first_wire.shutdown().await;
    second_wire.shutdown().await;
    serving.abort();
}
