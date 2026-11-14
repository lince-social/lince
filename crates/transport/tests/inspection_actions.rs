use engine::{Engine, actions::Action};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use transport::{ClientMessage, LaneHub, ServerMessage, Session};

async fn inspect(session: &mut Session, action: Action) -> Value {
    match session
        .handle(ClientMessage::Act {
            id: "inspection".into(),
            action,
        })
        .await
        .as_slice()
    {
        [
            ServerMessage::ActionOk {
                data: Some(data), ..
            },
        ] => data.clone(),
        messages => panic!("Inspection failed: {messages:?}"),
    }
}

#[tokio::test]
async fn inspections_preserve_fresh_results_without_broadcasting_data_changes() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let target = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Inspection fixture".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let person = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Authority inspection".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut changes = engine.watch_query_changes();
    let sequence = store::sync_ops::max_seq(&engine.store.pool).await.unwrap();
    let mut session = Session::local(engine.clone(), Arc::new(LaneHub::new()), "inspection");
    for action in [
        Action::RosterStatus,
        Action::InspectCanvases,
        Action::MailboxSavedStatus,
        Action::MailboxOutbound,
        Action::FileSyncStatus { organ },
        Action::InspectFioteActivations {
            target: target.clone(),
        },
        Action::InspectKarmaRuleHistory {
            rule: target.clone(),
            limit: 5,
        },
        Action::InspectKarmaCommands,
        Action::InspectRecordAuthority {
            person,
            record_uid: target,
        },
    ] {
        let name = format!("{action:?}");
        let data = inspect(&mut session, action).await;
        assert!(data.is_object(), "{name}");
        assert!(
            !changes.has_changed().unwrap(),
            "Read invalidated subscriptions: {name}"
        );
        assert_eq!(
            store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
            sequence
        );
    }
    let before = inspect(&mut session, Action::RosterStatus).await;
    assert_eq!(before["karma"]["local_running"], true);
    engine
        .act(
            Action::SetCellConfig {
                namespace: "lince.karma-runtime".into(),
                fds: json!({"running": false}),
            },
            None,
        )
        .await
        .unwrap();
    assert!(changes.has_changed().unwrap());
    changes.borrow_and_update();
    let after = inspect(&mut session, Action::RosterStatus).await;
    assert_eq!(after["karma"]["local_running"], false);
    assert!(!changes.has_changed().unwrap());
    let concepts = engine
        .act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: "A real change without a Fact".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(concepts.facts.is_empty());
    assert!(concepts.created.is_some());
    assert!(changes.has_changed().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspection_shares_read_access_and_mutations_still_wait_for_readers() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let held = engine.clone();
    let reader = tokio::spawn(async move {
        held.access_scope(false, async {
            entered.send(()).unwrap();
            released.await.unwrap();
            Ok(())
        })
        .await
        .unwrap();
    });
    ready.await.unwrap();
    let current = engine.clone();
    let mut inspection = tokio::spawn(async move {
        let mut session = Session::local(current, Arc::new(LaneHub::new()), "reader");
        inspect(&mut session, Action::RosterStatus).await
    });
    let result = tokio::time::timeout(Duration::from_secs(2), &mut inspection).await;
    if result.is_err() {
        release.send(()).unwrap();
        reader.await.unwrap();
        inspection.await.unwrap();
        panic!("A status read requested exclusive access");
    }
    assert!(result.unwrap().unwrap().is_object());
    let mut session = Session::local(engine.clone(), Arc::new(LaneHub::new()), "writer");
    let mutation = session.handle(ClientMessage::Act {
        id: "write".into(),
        action: Action::SetCellConfig {
            namespace: "lince.karma-runtime".into(),
            fds: json!({"running": false}),
        },
    });
    tokio::pin!(mutation);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), mutation.as_mut())
            .await
            .is_err()
    );
    release.send(()).unwrap();
    reader.await.unwrap();
    let messages = tokio::time::timeout(Duration::from_secs(2), mutation)
        .await
        .unwrap();
    assert!(matches!(
        messages.as_slice(),
        [ServerMessage::ActionOk { .. }]
    ));
}

#[tokio::test]
async fn failed_and_unauthorized_inspections_do_not_trigger_refreshes() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let person = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "No inspection permission".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let changes = engine.watch_query_changes();
    assert!(
        engine
            .act(Action::RosterStatus, Some(person))
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::FileSyncStatus {
                    organ: "missing-record".into()
                },
                None
            )
            .await
            .is_err()
    );
    assert!(!changes.has_changed().unwrap());
}

#[tokio::test]
async fn status_operations_that_mutate_data_remain_exclusive_and_publish_changes() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    store::mailbox::register(
        &engine.store.pool,
        "recipient",
        "fixture-key",
        "Fixture",
        1024,
    )
    .await
    .unwrap();
    store::mailbox::deposit(
        &engine.store.pool,
        &store::mailbox::HeldBundle {
            uid: "expired-fixture".into(),
            to_organ: "recipient".into(),
            from_organ: "sender".into(),
            from_cell: "cell".into(),
            from_node: "node".into(),
            body: "private fixture".into(),
            bytes: 15,
            received_at: "2000-01-01T00:00:00Z".into(),
            expires_at: "2000-01-02T00:00:00Z".into(),
        },
    )
    .await
    .unwrap();
    let changes = engine.watch_query_changes();
    let result = engine
        .access_scope(false, engine.act(Action::MailboxStatus, None))
        .await;
    assert!(matches!(result, Err(engine::EngineError::Forbidden(_))));
    assert_eq!(
        store::mailbox::held_bytes(&engine.store.pool, "recipient")
            .await
            .unwrap(),
        15
    );
    assert!(!changes.has_changed().unwrap());
    let mut session = Session::local(engine.clone(), Arc::new(LaneHub::new()), "mailbox");
    let status = inspect(&mut session, Action::MailboxStatus).await;
    assert_eq!(status["swept_just_now"], 1);
    assert_eq!(
        store::mailbox::held_bytes(&engine.store.pool, "recipient")
            .await
            .unwrap(),
        0
    );
    assert!(changes.has_changed().unwrap());
    assert_eq!(
        store::mailbox::pending_notices(&engine.store.pool)
            .await
            .unwrap()
            .len(),
        1
    );
}
