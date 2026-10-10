use super::*;
use crate::actions::Action;

struct Offline;

struct Linked {
    node: String,
    organ: String,
    peer: Weak<Engine>,
    online: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait::async_trait]
impl Network for Linked {
    fn node_id(&self) -> String {
        self.node.clone()
    }

    async fn location_request(&self, _: &str, request: PeerRequest) -> Result<Value, EngineError> {
        if !self.online.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(invalid("offline"));
        }
        self.peer
            .upgrade()
            .ok_or_else(|| invalid("disconnected"))?
            .location_peer(&self.organ, &self.node, request)
            .await
    }
}

#[async_trait::async_trait]
impl Network for Offline {
    fn node_id(&self) -> String {
        "a".repeat(64)
    }
    async fn location_request(&self, _: &str, _: PeerRequest) -> Result<Value, EngineError> {
        Err(invalid("offline"))
    }
}

struct Fixture {
    engine: Engine,
    _network: Arc<dyn Network>,
    controller: String,
    recipient: String,
    stranger: String,
    record: String,
    settings: Settings,
}

impl Fixture {
    async fn new() -> Self {
        let engine = Engine::open_memory().await.unwrap();
        let network: Arc<dyn Network> = Arc::new(Offline);
        engine.attach_location_network(network.clone());
        let role = store::auth::ensure_role(&engine.store.pool, "location-test")
            .await
            .unwrap();
        for action in ["read", "update"] {
            let permission = store::auth::ensure_permission(&engine.store.pool, "record", action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        let controller = store::auth::create_person_login(
            &engine.store.pool,
            "Controller",
            "controller",
            "hash",
            role,
        )
        .await
        .unwrap();
        let recipient = store::auth::create_person_login(
            &engine.store.pool,
            "Recipient",
            "recipient",
            "hash",
            role,
        )
        .await
        .unwrap();
        let stranger = store::auth::create_person_login(
            &engine.store.pool,
            "Stranger",
            "stranger",
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
                    head: "Location subject".into(),
                    body: String::new(),
                    quantity: 1.0,
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
            source_node_id: network.node_id(),
            source_kind: SourceKind::Device,
            duration_seconds: DEFAULT_DURATION_SECONDS,
            recipients: Vec::new(),
            transfer_uid: None,
        };
        Self {
            engine,
            _network: network,
            controller,
            recipient,
            stranger,
            record,
            settings,
        }
    }

    async fn start(&self, settings: Settings) -> SourceLease {
        self.engine
            .location_request(Command::Configure { settings }, None)
            .await
            .unwrap();
        self.engine
            .location_request(
                Command::Start {
                    person: self.controller.clone(),
                    record_uid: self.record.clone(),
                },
                None,
            )
            .await
            .unwrap();
        self.engine
            .location_request(
                Command::Approve {
                    person: self.controller.clone(),
                    record_uid: self.record.clone(),
                },
                None,
            )
            .await
            .unwrap();
        self.engine
            .location_sources()
            .await
            .into_iter()
            .next()
            .unwrap()
    }

    fn fix(&self, sequence: u64) -> Fix {
        Fix {
            sequence,
            latitude: -23.5505,
            longitude: -46.6333,
            accuracy_metres: Some(8.0),
            captured_at_ms: now_ms(),
        }
    }
}

#[tokio::test]
async fn organ_exclusions_override_named_recipients_and_policies_require_permission() {
    use nucleus::visibility::{Command as VisibilityCommand, Condition, Data, Policy};
    let fixture = Fixture::new().await;
    let mut settings = fixture.settings.clone();
    settings.recipients.push(fixture.recipient.clone());
    let lease = fixture.start(settings).await;
    fixture
        .engine
        .location_publish_source(&lease, fixture.fix(1))
        .await
        .unwrap();
    assert!(
        fixture
            .engine
            .location_view(&fixture.recipient, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
    let organ = store::organs::local(&fixture.engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let policy = Policy {
        include: vec![Condition::default()],
        exclude: vec![Condition {
            organs: vec![organ],
            ..Default::default()
        }],
    };
    let command = VisibilityCommand::Save {
        record_uid: fixture.record.clone(),
        data: Data::LiveLocation,
        policy: policy.clone(),
        expected_revision: 0,
    };
    assert!(
        fixture
            .engine
            .visibility_request(command.clone(), Some(&fixture.controller))
            .await
            .is_err()
    );
    fixture
        .engine
        .visibility_request(command, None)
        .await
        .unwrap();
    assert!(
        fixture
            .engine
            .location_view(&fixture.recipient, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
    assert!(
        fixture
            .engine
            .visibility_request(
                VisibilityCommand::Context {
                    record_uid: fixture.record.clone(),
                    data: Data::LiveLocation
                },
                Some(&fixture.recipient)
            )
            .await
            .is_err()
    );
    fixture
        .engine
        .visibility_request(
            VisibilityCommand::Save {
                record_uid: fixture.record.clone(),
                data: Data::LiveLocation,
                policy: Policy::default(),
                expected_revision: 1,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        fixture
            .engine
            .location_view(&fixture.recipient, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
}

#[tokio::test]
async fn another_device_requires_local_approval_and_stops_acquiring_while_offline() {
    use crate::roster::{CellEntry, Roster};
    use std::sync::atomic::{AtomicBool, Ordering};

    let fixture = Fixture::new().await;
    let mut settings = fixture.settings.clone();
    settings.recipients.push(fixture.recipient.clone());
    let controller = fixture.controller.clone();
    let stranger = fixture.stranger.clone();
    let recipient = fixture.recipient.clone();
    let record = fixture.record.clone();
    let authority = Arc::new(fixture.engine);
    let source = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&authority.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::sqlx::query("UPDATE record SET slug=NULL WHERE slug='local-organ'")
        .execute(&source.store.pool)
        .await
        .unwrap();
    for uid in [&organ, &controller, &stranger, &record] {
        let row = store::records::get(&authority.store.pool, uid)
            .await
            .unwrap()
            .unwrap();
        store::sqlx::query("INSERT INTO record(uid,slug,kind,head,body,quantity_mantissa,quantity_scale,organ_uid,created_at,updated_at) VALUES(?,?,?,?,?,'1',0,?,?,?)")
            .bind(&row.uid).bind(row.slug).bind(row.kind).bind(row.head).bind(row.body)
            .bind(row.organ_uid).bind(row.created_at).bind(row.updated_at)
            .execute(&source.store.pool).await.unwrap();
    }
    store::sqlx::query("UPDATE record SET organ_uid=? WHERE slug='local-cell'")
        .bind(&organ)
        .execute(&source.store.pool)
        .await
        .unwrap();
    let source_cell = store::cells::local(&source.store.pool)
        .await
        .unwrap()
        .unwrap();
    settings.source_cell_uid = source_cell.uid.clone();
    settings.source_node_id = "b".repeat(64);
    let signer = crate::trust::Signer::generate(&organ, "location-test-root");
    let roster = Roster {
        organ_uid: organ.clone(),
        root_key: signer.public_key_b64(),
        version: 1,
        not_after: (nucleus::execution::now() + chrono::Duration::days(1)).to_rfc3339(),
        cells: vec![
            CellEntry {
                cell_uid: fixture.settings.source_cell_uid,
                node_id: "a".repeat(64),
                label: "Authority".into(),
                operational_key: signer.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: crate::roster::full_capabilities(),
            },
            CellEntry {
                cell_uid: source_cell.uid,
                node_id: "b".repeat(64),
                label: "Phone".into(),
                operational_key: signer.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: crate::roster::full_capabilities(),
            },
        ],
        pickup: Vec::new(),
    };
    let stored = store::roster::StoredRoster {
        organ_uid: organ.clone(),
        root_key: roster.root_key.clone(),
        version: roster.version,
        not_after: roster.not_after.clone(),
        payload: serde_json::to_string(&roster).unwrap(),
        signature: signer.sign_bytes(&crate::roster::roster_signing_payload(&roster).unwrap()),
    };
    store::roster::put(&authority.store.pool, &stored)
        .await
        .unwrap();
    store::roster::put(&source.store.pool, &stored)
        .await
        .unwrap();
    store::roster::project_local_capabilities(
        &authority.store.pool,
        &crate::roster::full_capabilities(),
    )
    .await
    .unwrap();
    store::roster::project_local_capabilities(
        &source.store.pool,
        &crate::roster::full_capabilities(),
    )
    .await
    .unwrap();
    let online = Arc::new(AtomicBool::new(true));
    let authority_network: Arc<dyn Network> = Arc::new(Linked {
        node: "a".repeat(64),
        organ: organ.clone(),
        peer: Arc::downgrade(&source),
        online: online.clone(),
    });
    let source_network: Arc<dyn Network> = Arc::new(Linked {
        node: "b".repeat(64),
        organ: organ.clone(),
        peer: Arc::downgrade(&authority),
        online: online.clone(),
    });
    authority.attach_location_network(authority_network.clone());
    source.attach_location_network(source_network.clone());
    authority
        .act(
            Action::Location {
                request: Command::Configure { settings },
            },
            None,
        )
        .await
        .unwrap();
    authority
        .act(
            Action::Location {
                request: Command::Start {
                    person: controller.clone(),
                    record_uid: record.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    assert!(authority.location_sources().await.is_empty());
    assert!(source.location_sources().await.is_empty());
    assert_eq!(source.location_pending_sources().await.len(), 1);
    assert!(
        source
            .act(
                Action::Location {
                    request: Command::Approve {
                        person: stranger,
                        record_uid: record.clone()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        source
            .act(
                Action::Location {
                    request: Command::Approve {
                        person: controller.clone(),
                        record_uid: record.clone()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let mut connection = authority.store.pool.acquire().await.unwrap();
    let credential = store::session_access::password_on(&mut connection, "controller")
        .await
        .unwrap()
        .unwrap();
    store::session_access::register_device_on(
        &mut connection,
        credential.authentication(),
        &"b".repeat(64),
    )
    .await
    .unwrap();
    drop(connection);
    source
        .act(
            Action::Location {
                request: Command::Approve {
                    person: controller.clone(),
                    record_uid: record.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(source.location_sources().await.len(), 1);
    source
        .publish_device_location(-23.5505, -46.6333, Some(8.0), now_ms(), 1)
        .await
        .unwrap();
    assert!(
        authority
            .location_view(&recipient, &record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
    let visibility = nucleus::visibility::Command::Save {
        record_uid: record.clone(),
        data: nucleus::visibility::Data::LiveLocation,
        policy: nucleus::visibility::Policy {
            include: vec![],
            exclude: vec![nucleus::visibility::Condition {
                organs: vec![organ.clone()],
                ..Default::default()
            }],
        },
        expected_revision: 0,
    };
    source.visibility_request(visibility, None).await.unwrap();
    assert!(
        store::data_visibility::policy(
            &authority.store.pool,
            &record,
            nucleus::visibility::Data::LiveLocation
        )
        .await
        .unwrap()
        .is_some()
    );
    assert!(
        authority
            .location_view(&recipient, &record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    source
        .visibility_request(
            nucleus::visibility::Command::Save {
                record_uid: record.clone(),
                data: nucleus::visibility::Data::LiveLocation,
                policy: Default::default(),
                expected_revision: 1,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        authority
            .location_view(&recipient, &record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
    let first_session = source.location_sources().await[0].session_uid.clone();
    online.store(false, Ordering::SeqCst);
    assert!(
        source
            .act(
                Action::Location {
                    request: Command::StopAll {
                        person: controller.clone()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert!(source.location_sources().await.is_empty());
    online.store(true, Ordering::SeqCst);
    authority
        .act(
            Action::Location {
                request: Command::Stop {
                    person: controller.clone(),
                    record_uid: record.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    authority
        .act(
            Action::Location {
                request: Command::Start {
                    person: controller.clone(),
                    record_uid: record.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    source
        .act(
            Action::Location {
                request: Command::Approve {
                    person: controller,
                    record_uid: record.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    assert_ne!(
        source.location_sources().await[0].session_uid,
        first_session
    );
    assert!(
        authority
            .location_peer(
                &organ,
                &"b".repeat(64),
                PeerRequest::Publish {
                    record_uid: record,
                    session_uid: first_session,
                    fix: Fix {
                        sequence: 99,
                        latitude: 1.0,
                        longitude: 1.0,
                        accuracy_metres: None,
                        captured_at_ms: now_ms()
                    },
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn private_defaults_and_explicit_recipients_do_not_inherit_organ_access() {
    let fixture = Fixture::new().await;
    let lease = fixture.start(fixture.settings.clone()).await;
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
    assert!(
        fixture
            .engine
            .location_view(&fixture.recipient, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    assert!(
        fixture
            .engine
            .location_view(&fixture.stranger, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    let mut settings = fixture.settings.clone();
    settings.recipients.push(fixture.recipient.clone());
    fixture
        .engine
        .location_request(
            Command::Configure {
                settings: settings.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    let lease = fixture.start(settings).await;
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(2),
        )
        .await
        .unwrap();
    let view = fixture
        .engine
        .location_view(&fixture.recipient, &fixture.record)
        .await
        .unwrap();
    assert!(view.fix.is_some());
    assert!(view.session_uid.is_none());
    assert!(
        fixture
            .engine
            .location_view(&fixture.stranger, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
}

#[tokio::test]
async fn approval_replay_and_source_identity_are_enforced() {
    let fixture = Fixture::new().await;
    fixture
        .engine
        .location_request(
            Command::Configure {
                settings: fixture.settings.clone(),
            },
            None,
        )
        .await
        .unwrap();
    fixture
        .engine
        .location_request(
            Command::Start {
                person: fixture.controller.clone(),
                record_uid: fixture.record.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let lease = fixture
        .engine
        .location_pending_sources()
        .await
        .pop()
        .unwrap();
    assert!(fixture.engine.location_sources().await.is_empty());
    assert!(
        fixture
            .engine
            .location_accept_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                fixture.fix(1)
            )
            .await
            .is_err()
    );
    assert!(
        fixture
            .engine
            .location_request(
                Command::Approve {
                    person: fixture.stranger.clone(),
                    record_uid: fixture.record.clone()
                },
                None
            )
            .await
            .is_err()
    );
    fixture
        .engine
        .location_request(
            Command::Approve {
                person: fixture.controller.clone(),
                record_uid: fixture.record.clone(),
            },
            Some(&fixture.controller),
        )
        .await
        .unwrap();
    assert!(
        fixture
            .engine
            .location_accept_fix(
                &"b".repeat(64),
                &fixture.record,
                &lease.session_uid,
                fixture.fix(1)
            )
            .await
            .is_err()
    );
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    assert!(
        fixture
            .engine
            .location_accept_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                fixture.fix(1)
            )
            .await
            .is_err()
    );
    assert!(
        fixture
            .engine
            .location_request(
                Command::Start {
                    person: fixture.controller.clone(),
                    record_uid: fixture.record.clone()
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn old_fixes_expire_without_being_refreshed_by_viewing() {
    let fixture = Fixture::new().await;
    let lease = fixture.start(fixture.settings.clone()).await;
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    {
        let mut state = fixture.engine.location.state.lock().await;
        let observation = state
            .sessions
            .get_mut(&fixture.record)
            .unwrap()
            .latest
            .as_mut()
            .unwrap();
        observation.received = Instant::now() - Duration::from_secs(61);
    }
    fixture.engine.location_expire().await.unwrap();
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    assert!(
        fixture
            .engine
            .location_accept_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                fixture.fix(1)
            )
            .await
            .is_err()
    );
    let mut future = fixture.fix(2);
    future.captured_at_ms += 1000;
    assert!(future.validate(now_ms()).is_err());
    let mut old = fixture.fix(2);
    old.captured_at_ms -= 60_001;
    assert!(old.validate(now_ms()).is_err());
}

#[tokio::test]
async fn stop_expiry_and_restart_keep_settings_without_coordinates() {
    let fixture = Fixture::new().await;
    let lease = fixture.start(fixture.settings.clone()).await;
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    fixture
        .engine
        .location_request(
            Command::Stop {
                person: fixture.controller.clone(),
                record_uid: fixture.record.clone(),
            },
            Some(&fixture.controller),
        )
        .await
        .unwrap();
    assert!(fixture.engine.location_sources().await.is_empty());
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    assert!(
        store::location::settings(&fixture.engine.store.pool, &fixture.record)
            .await
            .unwrap()
            .is_some()
    );
    let restarted = Engine::new(fixture.engine.store.clone()).await.unwrap();
    restarted.attach_location_network(fixture._network.clone());
    assert!(restarted.location_sources().await.is_empty());
    let lease = fixture.start(fixture.settings.clone()).await;
    fixture
        .engine
        .location
        .state
        .lock()
        .await
        .sessions
        .get_mut(&fixture.record)
        .unwrap()
        .deadline = Instant::now() - Duration::from_secs(1);
    fixture.engine.location_tick().await.unwrap();
    assert!(
        fixture
            .engine
            .location_accept_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                fixture.fix(2)
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn authenticated_people_cannot_impersonate_the_controller() {
    let fixture = Fixture::new().await;
    fixture.start(fixture.settings.clone()).await;
    assert!(
        fixture
            .engine
            .location_request(
                Command::Stop {
                    person: fixture.controller.clone(),
                    record_uid: fixture.record.clone()
                },
                Some(&fixture.stranger)
            )
            .await
            .is_err()
    );
    assert!(
        fixture
            .engine
            .location_request(
                Command::Stop {
                    person: fixture.stranger.clone(),
                    record_uid: fixture.record.clone()
                },
                Some(&fixture.stranger)
            )
            .await
            .is_err()
    );
    let organ = store::organs::local(&fixture.engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    assert!(
        fixture
            .engine
            .location_peer(
                &organ.uid,
                &"b".repeat(64),
                PeerRequest::Control {
                    command: Command::View {
                        person: fixture.controller.clone(),
                        record_uid: fixture.record.clone()
                    }
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn observations_are_bounded_and_never_write_record_history_or_place() {
    let fixture = Fixture::new().await;
    let lease = fixture.start(fixture.settings.clone()).await;
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&fixture.engine.store.pool)
        .await
        .unwrap();
    for sequence in 1..=2000 {
        fixture
            .engine
            .location_accept_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                fixture.fix(sequence),
            )
            .await
            .unwrap();
    }
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&fixture.engine.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let state = fixture.engine.location.state.lock().await;
    assert_eq!(state.sessions.len(), 1);
    assert_eq!(
        state.sessions[&fixture.record]
            .latest
            .as_ref()
            .unwrap()
            .fix
            .sequence,
        2000
    );
    drop(state);
    assert!(
        store::records::get(&fixture.engine.store.pool, &fixture.record)
            .await
            .unwrap()
            .unwrap()
            .place_uid
            .is_none()
    );
    let raw: String =
        store::sqlx::query_scalar("SELECT settings_json FROM location_settings WHERE record_uid=?")
            .bind(&fixture.record)
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
    assert!(!raw.contains("latitude"));
    assert!(!raw.contains("longitude"));
}

#[tokio::test]
async fn concurrent_starts_keep_one_source_and_permission_loss_stops_it() {
    let fixture = Fixture::new().await;
    fixture
        .engine
        .act(
            Action::Location {
                request: Command::Configure {
                    settings: fixture.settings.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    let command = Command::Start {
        person: fixture.controller.clone(),
        record_uid: fixture.record.clone(),
    };
    let (first, second) = tokio::join!(
        fixture.engine.act(
            Action::Location {
                request: command.clone()
            },
            None
        ),
        fixture
            .engine
            .act(Action::Location { request: command }, None),
    );
    assert_ne!(first.is_ok(), second.is_ok());
    assert_eq!(fixture.engine.location_pending_sources().await.len(), 1);
    fixture
        .engine
        .act(
            Action::Location {
                request: Command::Approve {
                    person: fixture.controller.clone(),
                    record_uid: fixture.record.clone(),
                },
            },
            None,
        )
        .await
        .unwrap();
    let lease = fixture.engine.location_sources().await.remove(0);
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    let role = store::auth::ensure_role(&fixture.engine.store.pool, "location-read-only")
        .await
        .unwrap();
    store::auth::set_user_role(&fixture.engine.store.pool, &fixture.controller, role)
        .await
        .unwrap();
    fixture.engine.location_tick().await.unwrap();
    assert!(fixture.engine.location_sources().await.is_empty());
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
}

#[tokio::test]
async fn invalid_native_fixes_cannot_replace_the_current_position() {
    let fixture = Fixture::new().await;
    let lease = fixture.start(fixture.settings.clone()).await;
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    let mut invalid = Vec::new();
    let mut fix = fixture.fix(2);
    fix.latitude = f64::NAN;
    invalid.push(fix);
    let mut fix = fixture.fix(2);
    fix.longitude = 181.0;
    invalid.push(fix);
    let mut fix = fixture.fix(2);
    fix.accuracy_metres = Some(-1.0);
    invalid.push(fix);
    let mut fix = fixture.fix(2);
    fix.captured_at_ms += 60_000;
    invalid.push(fix);
    let mut fix = fixture.fix(2);
    fix.captured_at_ms -= 60_001;
    invalid.push(fix);
    for fix in invalid {
        assert!(
            fixture
                .engine
                .location_accept_fix(
                    &lease.settings.source_node_id,
                    &fixture.record,
                    &lease.session_uid,
                    fix
                )
                .await
                .is_err()
        );
    }
    assert_eq!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .unwrap()
            .sequence,
        1
    );
}

#[tokio::test]
async fn ended_transfer_stops_live_location() {
    let fixture = Fixture::new().await;
    let transfer = store::transfers::create(
        &fixture.engine.store.pool,
        store::transfers::NewTransfer {
            slug: None,
            head: "Ride",
            agreement_type: "all",
            agreement_pct: None,
            satiation: None,
            source_uid: None,
            reserve_default: None,
            require_confirmation: false,
        },
    )
    .await
    .unwrap();
    store::sqlx::query("UPDATE record SET quantity_mantissa='1' WHERE uid=?")
        .bind(&transfer)
        .execute(&fixture.engine.store.pool)
        .await
        .unwrap();
    let mut settings = fixture.settings.clone();
    settings.transfer_uid = Some(transfer.clone());
    let lease = fixture.start(settings).await;
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
    store::sqlx::query("UPDATE record SET quantity_mantissa='0' WHERE uid=?")
        .bind(&transfer)
        .execute(&fixture.engine.store.pool)
        .await
        .unwrap();
    fixture.engine.location_tick().await.unwrap();
    assert!(fixture.engine.location_sources().await.is_empty());
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
}

#[tokio::test]
async fn signed_manual_updates_keep_replay_protection_without_durable_intents() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    use ed25519_dalek::Signer;
    use nucleus::action_intent::{ActionIntentSessionProof, SignedActionIntent};
    let fixture = Fixture::new().await;
    let mut settings = fixture.settings.clone();
    settings.source_kind = SourceKind::Manual;
    let lease = fixture.start(settings).await;
    let key = ed25519_dalek::SigningKey::from_bytes(&[7_u8; 32]);
    let mut session = fixture
        .engine
        .begin_action_intent_session(&fixture.controller)
        .await
        .unwrap();
    let mut proof = ActionIntentSessionProof {
        session_id: session.session_id().into(),
        session_challenge: session.challenge().into(),
        person_uid: fixture.controller.clone(),
        key_id: "location-test-key".into(),
        public_key_base64: B64.encode(key.verifying_key().as_bytes()),
        signature: String::new(),
    };
    proof.signature = B64.encode(key.sign(&proof.signing_bytes()).to_bytes());
    fixture
        .engine
        .authenticate_action_intent_session(&mut session, proof)
        .await
        .unwrap();
    let action = Action::Location {
        request: Command::Publish {
            person: fixture.controller.clone(),
            record_uid: fixture.record.clone(),
            session_uid: lease.session_uid,
            fix: fixture.fix(1),
        },
    };
    let mut intent = SignedActionIntent {
        session_id: session.session_id().into(),
        session_challenge: session.challenge().into(),
        sequence: 1,
        message_id: "private-manual-fix".into(),
        action_base64: B64.encode(serde_json::to_vec(&action).unwrap()),
        signature: String::new(),
    };
    intent.signature = B64.encode(key.sign(&intent.signing_bytes()).to_bytes());
    let verified = fixture
        .engine
        .verify_action_intent(&mut session, intent.clone())
        .await
        .unwrap();
    fixture.engine.act_verified_intent(verified).await.unwrap();
    assert!(
        fixture
            .engine
            .verify_action_intent(&mut session, intent)
            .await
            .is_err()
    );
    assert!(
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
    let durable: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM signed_action_intent")
        .fetch_one(&fixture.engine.store.pool)
        .await
        .unwrap();
    assert_eq!(durable, 0);
}

#[tokio::test]
async fn narrow_peer_view_requires_current_person_device_admission() {
    let fixture = Fixture::new().await;
    let mut settings = fixture.settings.clone();
    settings.recipients.push(fixture.recipient.clone());
    let lease = fixture.start(settings).await;
    fixture
        .engine
        .location_accept_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    let node = "b".repeat(64);
    let mut connection = fixture.engine.store.pool.acquire().await.unwrap();
    let credential = store::session_access::password_on(&mut connection, "recipient")
        .await
        .unwrap()
        .unwrap();
    store::session_access::register_device_on(&mut connection, credential.authentication(), &node)
        .await
        .unwrap();
    drop(connection);
    let request = PeerRequest::Control {
        command: Command::View {
            person: fixture.recipient.clone(),
            record_uid: fixture.record.clone(),
        },
    };
    let view = fixture
        .engine
        .location_peer("other-organ", &node, request.clone())
        .await
        .unwrap();
    assert!(view["fix"].is_object());
    assert!(view.get("settings").is_none());
    assert!(view.get("people").is_none());
    assert!(view.get("transfer_uid").is_none());
    store::sqlx::query(
        "UPDATE person_device SET revoked=1,revision=revision+1 WHERE person_uid=? AND node_id=?",
    )
    .bind(&fixture.recipient)
    .bind(&node)
    .execute(&fixture.engine.store.pool)
    .await
    .unwrap();
    assert!(
        fixture
            .engine
            .location_peer("other-organ", &node, request)
            .await
            .is_err()
    );
}
