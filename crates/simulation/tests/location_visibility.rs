use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};

use engine::{Engine, EngineError, actions::Action, location::Network};
use nucleus::{
    execution::Execution,
    location::{Command, PeerRequest, Settings, SourceKind, Status, View},
    visibility::{Bound, Condition, Data, Policy},
};
use serde_json::Value;
use simulation::{scenario::Cell, world::World};

struct Link {
    node: String,
    organ: String,
    peers: BTreeMap<String, (Weak<Engine>, Execution)>,
}

#[async_trait::async_trait]
impl Network for Link {
    fn node_id(&self) -> String {
        self.node.clone()
    }

    async fn location_request(
        &self,
        node: &str,
        request: PeerRequest,
    ) -> Result<Value, EngineError> {
        let (engine, execution) = self
            .peers
            .get(node)
            .ok_or_else(|| EngineError::Consequence("Offline".into()))?;
        let engine = engine
            .upgrade()
            .ok_or_else(|| EngineError::Consequence("Offline".into()))?;
        execution
            .scope(engine.location_peer(&self.organ, &self.node, request))
            .await
    }
}

fn node_id(seed: u64, name: &str) -> String {
    let hex =
        nucleus::fact::sha256_hex(format!("lince.simulation.v1:{seed}:{name}:node").as_bytes());
    let secret: [u8; 32] = std::array::from_fn(|index| {
        u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap()
    });
    nucleus::fact::sha256_hex(&secret)
}

async fn save(
    engine: &Engine,
    record: &str,
    data: Data,
    policy: Policy,
    revision: u64,
) -> nucleus::visibility::Context {
    let result = engine
        .act(
            Action::Visibility {
                request: nucleus::visibility::Command::Save {
                    record_uid: record.into(),
                    data,
                    policy,
                    expected_revision: revision,
                },
            },
            None,
        )
        .await
        .unwrap();
    serde_json::from_value(result.data.unwrap()).unwrap()
}

async fn read(
    network: &Arc<dyn Network>,
    authority: &str,
    record: &str,
    data: Data,
) -> Result<Value, EngineError> {
    network
        .location_request(
            authority,
            PeerRequest::Read {
                record_uid: record.into(),
                data,
            },
        )
        .await
}

#[test]
fn four_linces_share_record_locations_during_a_map_free_transfer() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(run());
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn run() {
    let directory = tempfile::tempdir().unwrap();
    let mut case = simulation::fixtures::transfer::sale();
    case.name = "location-visibility-handoff".into();
    for input in &mut case.inputs {
        if let simulation::scenario::Event::Action { invocation } = &mut input.event
            && let Action::CreateTransferDraft { promises, head, .. } = &mut invocation.action
        {
            promises.retain(|promise| promise.uid.as_deref() == Some("bike-give"));
            *head = "Map-free bike handoff".into();
        }
    }
    for name in ["family", "outsider"] {
        case.cells.push(Cell {
            name: name.into(),
            database: None,
            lingua: vec![],
            seed: vec![],
        });
    }
    let pause_at = case
        .inputs
        .iter()
        .find(|input| input.id == "delivery")
        .unwrap()
        .at_ms;
    let restart_at = case
        .inputs
        .iter()
        .find(|input| input.id == "restart")
        .unwrap()
        .at_ms;
    let mut world = World::open(case, directory.path()).await.unwrap();
    let send_at = world
        .scenario
        .inputs
        .iter()
        .find(|input| input.id == "send")
        .unwrap()
        .at_ms;
    while world.next_ms().is_some_and(|time| time < pause_at) {
        assert!(world.step().await.unwrap());
    }
    assert_eq!(world.nodes.len(), 4);
    let mut organs = BTreeMap::new();
    let mut peers = BTreeMap::new();
    for (name, node) in &world.nodes {
        organs.insert(
            name.clone(),
            store::organs::local(&node.engine().store.pool)
                .await
                .unwrap()
                .unwrap()
                .uid,
        );
        peers.insert(
            node_id(world.scenario.seed, name),
            (
                Arc::downgrade(&node.cell.runtime().engine),
                node.execution.clone(),
            ),
        );
    }
    let mut links = BTreeMap::new();
    for (name, node) in &world.nodes {
        let network: Arc<dyn Network> = Arc::new(Link {
            node: node_id(world.scenario.seed, name),
            organ: organs[name].clone(),
            peers: peers.clone(),
        });
        node.engine().attach_location_network(network.clone());
        links.insert(name.clone(), network);
    }
    let source = &world.nodes["a"];
    let engine = source.engine();
    let record = world.resolve_reference("$bike");
    let person = world.resolve_reference("$ana");
    let transfer = world.resolve_reference("$sale");
    let authority = links["a"].node_id();
    source
        .execution
        .scope(async {
            let role = store::auth::ensure_role(&engine.store.pool, "location-controller")
                .await
                .unwrap();
            for action in ["read", "update"] {
                let permission =
                    store::auth::ensure_permission(&engine.store.pool, "record", action)
                        .await
                        .unwrap();
                store::auth::grant(&engine.store.pool, role, permission)
                    .await
                    .unwrap();
            }
            store::auth::set_user_role(&engine.store.pool, &person, role)
                .await
                .unwrap();
            for (name, proximity) in [("b", 1), ("family", 6), ("outsider", 2)] {
                store::organs::add_contact(
                    &engine.store.pool,
                    &organs[name],
                    None,
                    name,
                    "",
                    proximity,
                )
                .await
                .unwrap();
                store::organs::set_trust(&engine.store.pool, &organs[name], "known")
                    .await
                    .unwrap();
                store::organs::set_proximity(&engine.store.pool, &organs[name], proximity)
                    .await
                    .unwrap();
            }
            engine
                .act(
                    Action::SetPlace {
                        target: record.clone(),
                        lat: -23.5,
                        lon: -46.6,
                        address: None,
                    },
                    None,
                )
                .await
                .unwrap();
            assert!(engine.location_sources().await.is_empty());
            assert!(
                read(&links["b"], &authority, &record, Data::Place)
                    .await
                    .is_err()
            );
            let saved = save(
                engine,
                &record,
                Data::Record,
                Policy {
                    include: vec![Condition::default()],
                    exclude: vec![],
                },
                0,
            )
            .await;
            assert_eq!(saved.saved.unwrap().revision, 1);
            let projection = read(&links["b"], &authority, &record, Data::Record)
                .await
                .unwrap();
            assert_eq!(projection["record"]["uid"], record);
            assert!(projection["place"].is_null());
            assert!(
                store::visibility::hidden_from_organ(&engine.store.pool, &organs["b"])
                    .await
                    .unwrap()
                    .contains(&record)
            );
            let places = Policy {
                include: vec![Condition {
                    organs: vec![organs["b"].clone()],
                    ..Default::default()
                }],
                exclude: vec![],
            };
            save(engine, &record, Data::Place, places, 1).await;
            assert_eq!(
                read(&links["b"], &authority, &record, Data::Place)
                    .await
                    .unwrap()["place"]["latitude"],
                -23.5
            );
            assert!(
                read(&links["family"], &authority, &record, Data::Place)
                    .await
                    .is_err()
            );
            assert!(
                !store::visibility::hidden_from_organ(&engine.store.pool, &organs["b"])
                    .await
                    .unwrap()
                    .contains(&record)
            );
            let cell = store::cells::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let settings = Settings {
                record_uid: record.clone(),
                controller_uid: person.clone(),
                source_cell_uid: cell.uid,
                source_node_id: authority.clone(),
                source_kind: SourceKind::Device,
                duration_seconds: 3600,
                recipients: vec![],
                transfer_uid: Some(transfer.clone()),
            };
            for command in [
                Command::Configure { settings },
                Command::Start {
                    person: person.clone(),
                    record_uid: record.clone(),
                },
                Command::Approve {
                    person: person.clone(),
                    record_uid: record.clone(),
                },
            ] {
                engine.location_request(command, None).await.unwrap();
            }
            let facts: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
                .fetch_one(&engine.store.pool)
                .await
                .unwrap();
            for index in 0_u32..500 {
                engine
                    .publish_device_location(
                        -23.55 + f64::from(index) / 100_000.0,
                        -46.63,
                        Some(8.0),
                        nucleus::execution::now().timestamp_millis(),
                        u64::from(index) + 1,
                    )
                    .await
                    .unwrap();
            }
            assert_eq!(
                facts,
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fact")
                    .fetch_one(&engine.store.pool)
                    .await
                    .unwrap()
            );
            assert!(
                read(&links["b"], &authority, &record, Data::LiveLocation)
                    .await
                    .is_err()
            );
            let policy = Policy {
                include: vec![
                    Condition {
                        upper: Some(Bound {
                            value: 3,
                            inclusive: false,
                        }),
                        ..Default::default()
                    },
                    Condition {
                        organs: vec![organs["family"].clone()],
                        lower: Some(Bound {
                            value: 5,
                            inclusive: true,
                        }),
                        upper: Some(Bound {
                            value: 7,
                            inclusive: true,
                        }),
                    },
                ],
                exclude: vec![Condition {
                    organs: vec![organs["outsider"].clone()],
                    lower: Some(Bound {
                        value: 1,
                        inclusive: false,
                    }),
                    upper: Some(Bound {
                        value: 3,
                        inclusive: false,
                    }),
                }],
            };
            let context = save(engine, &record, Data::LiveLocation, policy.clone(), 0).await;
            assert_eq!(
                context
                    .organs
                    .iter()
                    .find(|organ| organ.uid == organs["family"])
                    .unwrap()
                    .proximity,
                Some(6)
            );
            for name in ["b", "family"] {
                let view: View = serde_json::from_value(
                    read(&links[name], &authority, &record, Data::LiveLocation)
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(view.status, Status::Live);
                assert_eq!(view.fix.unwrap().sequence, 500);
                assert!(view.session_uid.is_none());
                let observer = &world.nodes[name];
                let result = observer
                    .execution
                    .scope(observer.engine().location_request(
                        Command::Observe {
                            person: String::new(),
                            record_uid: record.clone(),
                            node_id: authority.clone(),
                        },
                        None,
                    ))
                    .await
                    .unwrap();
                let view: View = serde_json::from_value(result.data.unwrap()).unwrap();
                assert!(view.fix.is_some());
            }
            assert!(
                read(&links["outsider"], &authority, &record, Data::LiveLocation)
                    .await
                    .is_err()
            );
            store::organs::set_proximity(&engine.store.pool, &organs["b"], 3)
                .await
                .unwrap();
            assert!(
                read(&links["b"], &authority, &record, Data::LiveLocation)
                    .await
                    .is_err()
            );
            store::organs::set_proximity(&engine.store.pool, &organs["family"], 8)
                .await
                .unwrap();
            assert!(
                read(&links["family"], &authority, &record, Data::LiveLocation)
                    .await
                    .is_err()
            );
            let stale = engine
                .act(
                    Action::Visibility {
                        request: nucleus::visibility::Command::Save {
                            record_uid: record.clone(),
                            data: Data::LiveLocation,
                            policy: Policy::default(),
                            expected_revision: 0,
                        },
                    },
                    None,
                )
                .await;
            assert!(stale.is_err());
            assert_eq!(
                store::data_visibility::policy(&engine.store.pool, &record, Data::LiveLocation)
                    .await
                    .unwrap()
                    .unwrap()
                    .policy,
                policy
            );
            store::organs::set_proximity(&engine.store.pool, &organs["b"], 1)
                .await
                .unwrap();
            save(engine, &record, Data::LiveLocation, Policy::default(), 1).await;
            assert!(
                read(&links["b"], &authority, &record, Data::LiveLocation)
                    .await
                    .is_err()
            );
            save(engine, &record, Data::LiveLocation, policy, 2).await;
            store::organs::set_trust(&engine.store.pool, &organs["b"], "blocked")
                .await
                .unwrap();
            assert!(
                read(&links["b"], &authority, &record, Data::LiveLocation)
                    .await
                    .is_err()
            );
            store::organs::set_trust(&engine.store.pool, &organs["b"], "known")
                .await
                .unwrap();
            let raw: String = store::sqlx::query_scalar(
                "SELECT GROUP_CONCAT(value,' ') FROM sync_op WHERE value IS NOT NULL",
            )
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            assert!(!raw.contains("captured_at_ms"));
            assert!(!raw.contains("-23.54501"));
            assert_eq!(
                read(&links["b"], &authority, &record, Data::Place)
                    .await
                    .unwrap()["place"]["latitude"],
                -23.5
            );
            save(engine, &record, Data::Place, Policy::default(), 2).await;
        })
        .await;
    let passenger = &world.nodes["b"];
    let passenger_record = passenger
        .execution
        .scope(async {
            let engine = passenger.engine();
            let role = store::auth::ensure_role(&engine.store.pool, "passenger-location")
                .await
                .unwrap();
            for action in ["read", "update"] {
                let permission =
                    store::auth::ensure_permission(&engine.store.pool, "record", action)
                        .await
                        .unwrap();
                store::auth::grant(&engine.store.pool, role, permission)
                    .await
                    .unwrap();
            }
            let controller = store::auth::create_person_login(
                &engine.store.pool,
                "Passenger",
                "passenger",
                "hash",
                role,
            )
            .await
            .unwrap();
            let record = engine
                .act(
                    Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: "Passenger journey".into(),
                        body: String::new(),
                        quantity: -1.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap();
            let cell = store::cells::local(&engine.store.pool)
                .await
                .unwrap()
                .unwrap();
            let settings = Settings {
                record_uid: record.clone(),
                controller_uid: controller.clone(),
                source_cell_uid: cell.uid,
                source_node_id: links["b"].node_id(),
                source_kind: SourceKind::Device,
                duration_seconds: 3600,
                recipients: vec![],
                transfer_uid: None,
            };
            for command in [
                Command::Configure { settings },
                Command::Start {
                    person: controller.clone(),
                    record_uid: record.clone(),
                },
                Command::Approve {
                    person: controller,
                    record_uid: record.clone(),
                },
            ] {
                engine.location_request(command, None).await.unwrap();
            }
            engine
                .publish_device_location(
                    -23.56,
                    -46.64,
                    None,
                    nucleus::execution::now().timestamp_millis(),
                    1,
                )
                .await
                .unwrap();
            save(
                engine,
                &record,
                Data::LiveLocation,
                Policy {
                    include: vec![Condition {
                        organs: vec![organs["a"].clone()],
                        ..Default::default()
                    }],
                    exclude: vec![],
                },
                0,
            )
            .await;
            let view: View = serde_json::from_value(
                read(
                    &links["a"],
                    &links["b"].node_id(),
                    &record,
                    Data::LiveLocation,
                )
                .await
                .unwrap(),
            )
            .unwrap();
            assert_eq!(view.fix.unwrap().latitude, -23.56);
            assert!(
                read(
                    &links["family"],
                    &links["b"].node_id(),
                    &record,
                    Data::LiveLocation
                )
                .await
                .is_err()
            );
            record
        })
        .await;
    while world.next_ms().is_some_and(|time| time < send_at) {
        assert!(world.step().await.unwrap());
    }
    let sender = &world.nodes["a"];
    sender
        .execution
        .scope(async {
            let queued = store::transfer_delivery::outbox_due(
                &sender.engine().store.pool,
                nucleus::execution::now(),
                10,
            )
            .await
            .unwrap();
            assert_eq!(queued.len(), 1);
            assert!(!queued[0].payload.contains("-23.5"));
            assert!(!queued[0].payload.contains("captured_at_ms"));
            assert!(!queued[0].payload.contains("data_visibility"));
            save(
                sender.engine(),
                &transfer,
                Data::Record,
                Policy::default(),
                0,
            )
            .await;
            assert!(
                protein::transfer_delivery_projection(
                    &sender.engine().store,
                    &transfer,
                    &world.resolve_reference("$carlos"),
                    &organs["b"]
                )
                .await
                .is_err()
            );
            assert!(
                cell::transfer::prepare_envelope(sender.cell.runtime(), &queued[0])
                    .await
                    .unwrap_err()
                    .contains("visibility")
            );
            save(
                sender.engine(),
                &transfer,
                Data::Record,
                Policy {
                    include: vec![Condition {
                        organs: vec![organs["b"].clone()],
                        ..Default::default()
                    }],
                    exclude: vec![],
                },
                1,
            )
            .await;
            assert!(
                cell::transfer::prepare_envelope(sender.cell.runtime(), &queued[0])
                    .await
                    .is_ok()
            );
        })
        .await;
    while world.next_ms().is_some_and(|time| time < restart_at) {
        assert!(world.step().await.unwrap());
    }
    let source = &world.nodes["a"];
    source
        .execution
        .scope(async {
            source.engine().location_tick().await.unwrap();
            assert!(source.engine().location_sources().await.is_empty());
            let view: View = serde_json::from_value(
                read(&links["b"], &authority, &record, Data::LiveLocation)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(view.status, Status::Stopped);
            assert!(view.fix.is_none());
            assert!(
                store::location::transfer_ended(&source.engine().store.pool, &transfer)
                    .await
                    .unwrap()
            );
            assert_eq!(
                store::records::get(&source.engine().store.pool, &record)
                    .await
                    .unwrap()
                    .unwrap()
                    .quantity
                    .to_f64(),
                0.0
            );
        })
        .await;
    assert_eq!(world.nodes["b"].engine().location_sources().await.len(), 1);
    while world.step().await.unwrap() {}
    let passenger = &world.nodes["b"];
    assert!(passenger.engine().location_sources().await.is_empty());
    assert!(
        store::location::settings(&passenger.engine().store.pool, &passenger_record)
            .await
            .unwrap()
            .is_some()
    );
}
