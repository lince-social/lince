use engine::{Engine, actions::Action};
use nucleus::{
    RecordKind,
    sync::{Destination, Outcome, Retention},
};
use std::{sync::Arc, time::Duration};
use transport::{ClientMessage, LaneHub, ServerMessage, Session, SyncEvents};

async fn guest(engine: &Engine) -> String {
    let role = store::auth::ensure_role(&engine.store.pool, "guest")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::auth::create_person_login(&engine.store.pool, "Guest", "guest", "unused", role)
        .await
        .unwrap()
        .to_string()
}

fn query(id: &str) -> ClientMessage {
    ClientMessage::Subscribe {
        id: id.into(),
        protein: protein::Protein {
            source: protein::Source::Record,
            filter: Vec::new(),
            fields: Some(vec!["uid".into(), "head".into()]),
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: None,
        },
    }
}

#[tokio::test]
async fn local_status_uses_the_common_history_and_remote_users_cannot_read_or_clear_it() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let hub = Arc::new(LaneHub::new());
    let mut local = Session::local(engine.clone(), hub.clone(), "local");
    local
        .handle(ClientMessage::SyncHistoryPolicy {
            id: "enable".into(),
            retention: Retention {
                enabled: true,
                ..Default::default()
            },
        })
        .await;
    local.handle(query("records")).await;
    let messages = local
        .handle(ClientMessage::SyncInspect {
            id: "status".into(),
            before: None,
        })
        .await;
    let ServerMessage::SyncStatus { overview, .. } = &messages[0] else {
        panic!("missing sync status")
    };
    assert!(overview.history.iter().any(|change| matches!(
        change.activity.instance.destination,
        Destination::Interface { .. }
    ) && change.summary.outcome
        == Outcome::Refreshed));
    let history = overview.history.clone();
    let person = guest(&engine).await;
    let mut remote = Session::new(engine.clone(), hub, "remote", Some(person));
    for message in [
        ClientMessage::SyncInspect {
            id: "status".into(),
            before: None,
        },
        ClientMessage::SyncForgetHistory { id: "clear".into() },
        ClientMessage::SyncHistoryPolicy {
            id: "limits".into(),
            retention: Retention {
                enabled: true,
                seconds: 60,
                max_entries: 1,
            },
        },
    ] {
        assert!(
            matches!(&remote.handle(message).await[0], ServerMessage::Error { code: Some(code), .. } if code == "sync_local_only")
        );
    }
    assert_eq!(engine.sync_overview(None).await.unwrap().history, history);
    let mut anonymous = Session::new(engine.clone(), Arc::new(LaneHub::new()), "anonymous", None);
    assert!(
        matches!(&anonymous.handle(ClientMessage::SyncInspect { id: "anonymous".into(), before: None }).await[0], ServerMessage::Error { code: Some(code), .. } if code == "sync_local_only")
    );
    assert!(
        matches!(&local.handle(ClientMessage::SyncForgetHistory { id: "clear".into() }).await[0], ServerMessage::SyncStatus { overview, .. } if overview.history.is_empty())
    );
}

#[tokio::test]
async fn shared_events_refresh_changes_without_facts_and_stop_when_the_engine_closes() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let mut events = SyncEvents::new(&engine);
    let mut session = Session::new(engine.clone(), Arc::new(LaneHub::new()), "local", None);
    let mut subscription = match query("concepts") {
        ClientMessage::Subscribe { protein, .. } => protein,
        _ => unreachable!(),
    };
    subscription.source = protein::Source::Concept;
    session
        .handle(ClientMessage::Subscribe {
            id: "concepts".into(),
            protein: subscription,
        })
        .await;
    let created = engine
        .act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: "shared-event".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(created.facts.is_empty());
    let event = tokio::time::timeout(Duration::from_secs(2), events.next(false))
        .await
        .unwrap()
        .unwrap();
    let messages = session.on_sync_event(event).await;
    assert!(
        matches!(&messages[0], ServerMessage::Snapshot { rows, .. } if rows.iter().any(|row| row["uid"] == created.created.as_ref().unwrap().as_str()))
    );
    drop(session);
    drop(engine);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), events.next(false))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn proteins_receive_user_file_and_karma_changes_with_history_disabled() {
    let storage = tempfile::tempdir().unwrap();
    let directory = storage.path().join("files");
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("task.lingua");
    std::fs::write(&path, "Task (@daily-task: 0, #task) {\nOriginal\n}\n").unwrap();
    let engine = Arc::new(
        Engine::open(&format!(
            "sqlite://{}",
            storage.path().join("cell.sqlite").display()
        ))
        .await
        .unwrap(),
    );
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    engine
        .act(
            Action::ConfigureFileSync {
                protein: String::new(),
                path: directory.to_string_lossy().into(),
                format: engine::file_sync::FileFormat::Lingua,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
    let mut files = engine::file_sync::FileSyncState::new();
    engine
        .file_sync_tick(&directory, &organ, &mut files)
        .await
        .unwrap();
    let target = store::records::resolve(&engine.store.pool, "daily-task")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let at = nucleus::execution::now() + Duration::from_secs(86_400);
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("protein-test".into())
                .unwrap(),
        )
        .unwrap();
    engine
        .act(
            Action::CreateFrequency {
                slug: "daily".into(),
                head: None,
                every: nucleus::karma::CadenceStep {
                    days: 1,
                    ..Default::default()
                },
                anchor_at: Some(at.to_rfc3339()),
                request_id: Some("protein-frequency".into()),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::SaveKarmaRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: ["-1 * freq(@daily)", "!=0", "@daily-task"].map(|source| {
                    nucleus::karma::rule_field::RuleFieldInput::Text {
                        source: source.into(),
                    }
                }),
                request_id: "protein-rule".into(),
            },
            None,
        )
        .await
        .unwrap();
    let frequency = store::records::resolve(&engine.store.pool, "daily")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let handle = store::karma::frequencies::get_handle(&engine.store.pool, &frequency)
        .await
        .unwrap()
        .unwrap();
    engine
        .act(
            Action::ActivateKarmaFrequency {
                request_id: "activate-protein-frequency".into(),
                frequency_uid: frequency,
                expected_handle_revision: handle.handle_revision,
                revision_hash: handle.head_revision_hash,
                parameter_overrides: Default::default(),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .file_sync_tick(&directory, &organ, &mut files)
        .await
        .unwrap();
    let mut session = Session::local(engine.clone(), Arc::new(LaneHub::new()), "local");
    session
        .handle(ClientMessage::Subscribe {
            id: "task".into(),
            protein: protein::Protein {
                source: protein::Source::Record,
                filter: vec![protein::Predicate::SlugEq("daily-task".into())],
                fields: Some(vec!["uid".into(), "head".into(), "quantity".into()]),
                include: Default::default(),
                aggregate: None,
                order: Vec::new(),
                limit: None,
            },
        })
        .await;
    let mut events = SyncEvents::new(&engine);
    engine.advance_karma_time(at).await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), events.next(false))
        .await
        .unwrap()
        .unwrap();
    let updates = session.on_sync_event(event).await;
    assert!(updates.iter().any(|message| matches!(message, ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } if id == "task" && rows.iter().any(|row| row["uid"] == target && row["quantity"] == "-1"))), "{updates:?}");
    files = engine::file_sync::FileSyncState::new();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut files)
        .await
        .unwrap();
    assert!(report.conflicts.is_empty(), "{report:?}");
    assert_eq!(
        store::records::get(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "-1"
    );
    let source = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Original", "Changed on disk")
        .replace("Task (@", "File task (@");
    std::fs::write(&path, source).unwrap();
    let mut events = SyncEvents::new(&engine);
    engine
        .file_sync_tick(&directory, &organ, &mut files)
        .await
        .unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), events.next(false))
        .await
        .unwrap()
        .unwrap();
    let updates = session.on_sync_event(event).await;
    assert!(updates.iter().any(|message| matches!(message, ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } if id == "task" && rows.iter().any(|row| row["head"] == "File task" && row["quantity"] == "-1"))), "{updates:?}");
    let mut events = SyncEvents::new(&engine);
    engine
        .act(
            Action::SetQuantityExact {
                target: target.clone(),
                amount: "0".into(),
            },
            None,
        )
        .await
        .unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), events.next(false))
        .await
        .unwrap()
        .unwrap();
    let updates = session.on_sync_event(event).await;
    assert!(updates.iter().any(|message| matches!(message, ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } if id == "task" && rows.iter().any(|row| row["quantity"] == "0"))), "{updates:?}");
    let ((), ()) = tokio::join!(
        async {
            for _ in 0..8 {
                let report = engine
                    .file_sync_tick(&directory, &organ, &mut files)
                    .await
                    .unwrap();
                assert!(report.conflicts.is_empty(), "{report:?}");
            }
        },
        async {
            for amount in 1..=8 {
                engine
                    .act(
                        Action::SetQuantityExact {
                            target: target.clone(),
                            amount: amount.to_string(),
                        },
                        None,
                    )
                    .await
                    .unwrap();
            }
        }
    );
    engine
        .file_sync_tick(&directory, &organ, &mut files)
        .await
        .unwrap();
    let latest = session.refresh().await;
    assert!(latest.iter().any(|message| matches!(message, ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } if id == "task" && rows.iter().any(|row| row["quantity"] == "8"))), "{latest:?}");
    let overview = engine.sync_overview(None).await.unwrap();
    assert!(!overview.retention.enabled);
    assert!(overview.history.is_empty());
    engine.store.pool.close().await;
}

#[tokio::test]
async fn a_joined_document_is_rechecked_before_broadcasting_new_text() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let uid = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Shared",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let person = guest(&engine).await;
    let grant = store::visibility::grant(&engine.store.pool, "actor", Some(&person), &uid)
        .await
        .unwrap();
    let mut session = Session::new(
        engine.clone(),
        Arc::new(LaneHub::new()),
        "remote",
        Some(person),
    );
    assert!(matches!(
        &session
            .handle(ClientMessage::CollabJoin {
                id: "join".into(),
                record_uid: uid.clone()
            })
            .await[0],
        ServerMessage::CollabState { .. }
    ));
    store::sqlx::query("DELETE FROM visibility_rule WHERE uid = ?")
        .bind(grant)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    let facts = engine.append_user(&uid, 1.0).await.unwrap();
    assert!(
        !session
            .on_fact(&facts[0])
            .await
            .iter()
            .any(|message| matches!(message, ServerMessage::CollabChange { .. }))
    );
    assert!(
        !session
            .refresh()
            .await
            .iter()
            .any(|message| matches!(message, ServerMessage::CollabChange { .. }))
    );
}
