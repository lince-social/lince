use engine::{
    Engine,
    sync::{Delivery, OpBatch},
};
use nucleus::{
    RecordKind,
    sync::{
        Activity, Content, Destination, Direction, Instance, Outcome, Retention, Summary, Update,
    },
};
use std::sync::Arc;

fn activity() -> Activity {
    Activity {
        instance: Instance::interface("test", "records"),
        direction: Direction::Outgoing,
        update: Update::Full,
    }
}

async fn cell() -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&engine.store.pool, "")
        .await
        .unwrap()
        .uid;
    (engine, organ)
}

async fn record(engine: &Engine) -> String {
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Sync subject",
            body: "Private body",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn pair(engine: &Engine, organ: &str) {
    store::organs::add_contact(&engine.store.pool, organ, None, "Peer", "", 0)
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, organ, "known")
        .await
        .unwrap();
    store::organs::set_sync_policy(&engine.store.pool, organ, true, true)
        .await
        .unwrap();
}

#[tokio::test]
async fn history_expiry_and_clearing_do_not_delete_pending_work_or_content() {
    let (engine, _) = cell().await;
    let (_, peer) = cell().await;
    pair(&engine, &peer).await;
    let uid = record(&engine).await;
    store::organs::quarantine(
        &engine.store.pool,
        &peer,
        "Invalid signature",
        "private rejected payload",
    )
    .await
    .unwrap();
    let before = engine.sync_overview(None).await.unwrap();
    assert!(before.outgoing > 0);
    assert_eq!(before.held, 1);
    let now = chrono::Utc::now().timestamp();
    store::sync_activity::set_retention(
        &engine.store.pool,
        Retention {
            seconds: 60,
            max_entries: 2,
        },
        now,
    )
    .await
    .unwrap();
    for at in [now - 61, now - 2, now - 1, now] {
        store::sync_activity::append(
            &engine.store.pool,
            &activity(),
            &Summary::new(Outcome::Applied, 1),
            at,
        )
        .await
        .unwrap();
    }
    assert_eq!(engine.sync_overview(None).await.unwrap().history.len(), 2);
    store::sync_activity::prune(&engine.store.pool, now + 60)
        .await
        .unwrap();
    assert!(
        store::sync_activity::recent(&engine.store.pool, None, now + 60)
            .await
            .unwrap()
            .is_empty()
    );
    store::sync_activity::clear(&engine.store.pool)
        .await
        .unwrap();
    let after = engine.sync_overview(None).await.unwrap();
    assert_eq!((after.outgoing, after.held), (before.outgoing, before.held));
    assert!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .is_some()
    );
    let overview = serde_json::to_string(&after).unwrap();
    assert!(!overview.contains("private rejected payload"));
    assert!(!overview.contains("Private body"));
}

#[tokio::test]
async fn history_is_bounded_pageable_and_survives_restart() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("sync-history"));
    std::fs::create_dir_all(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    let engine = Engine::open(&url).await.unwrap();
    let (_, peer) = cell().await;
    pair(&engine, &peer).await;
    let uid = record(&engine).await;
    let queued = engine.sync_overview(None).await.unwrap().outgoing;
    assert!(queued > 0);
    let now = chrono::Utc::now().timestamp();
    let policy = Retention {
        seconds: 3600,
        max_entries: 120,
    };
    store::sync_activity::set_retention(&engine.store.pool, policy, now)
        .await
        .unwrap();
    let summary = Summary {
        subjects: vec!["x".repeat(512); 100],
        message: Some("x".repeat(4096)),
        ..Summary::new(Outcome::Refreshed, 1)
    };
    for _ in 0..130 {
        store::sync_activity::append(&engine.store.pool, &activity(), &summary, now)
            .await
            .unwrap();
    }
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    let first = engine.sync_overview(None).await.unwrap();
    assert_eq!(first.retention, policy);
    assert_eq!(first.outgoing, queued);
    assert!(
        first
            .pending
            .iter()
            .any(|pending| pending.subject.as_deref() == Some(&uid))
    );
    assert_eq!(first.history.len(), 100);
    let last = first.history.last().unwrap().seq;
    let second = engine.sync_overview(Some(last)).await.unwrap();
    assert_eq!(second.history.len(), 20);
    assert!(second.history.iter().all(|change| change.seq < last));
    let summary = &first.history[0].summary;
    assert_eq!(summary.subjects.len(), 32);
    assert_eq!(summary.subjects[0].len(), 256);
    assert_eq!(summary.message.as_ref().unwrap().len(), 1024);
    assert!(
        store::sync_activity::set_retention(
            &engine.store.pool,
            Retention {
                seconds: u64::MAX,
                max_entries: 0
            },
            now
        )
        .await
        .is_err()
    );
    assert_eq!(
        store::sync_activity::retention(&engine.store.pool)
            .await
            .unwrap(),
        policy
    );
    engine.store.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn failed_delivery_retries_through_the_existing_outbox_and_rechecks_permissions() {
    let (sender, sender_uid) = cell().await;
    let (receiver, receiver_uid) = cell().await;
    pair(&sender, &receiver_uid).await;
    pair(&receiver, &sender_uid).await;
    let uid = record(&sender).await;
    sender
        .drain_outbox(|_, _, _| async { Delivery::Failed("offline".into()) })
        .await
        .unwrap();
    let pending = sender.sync_overview(None).await.unwrap();
    assert!(
        pending
            .pending
            .iter()
            .any(|item| item.subject.as_deref() == Some(&uid) && item.attempts > 0)
    );
    assert!(
        pending
            .history
            .iter()
            .any(|change| change.summary.outcome == Outcome::Failed)
    );
    sender
        .drain_outbox(|_, _, batch| {
            let receiver = &receiver;
            async move {
                receiver.import_op_batch(&batch).await.unwrap();
                Delivery::Sent
            }
        })
        .await
        .unwrap();
    assert_eq!(sender.sync_overview(None).await.unwrap().outgoing, 0);
    let incoming = receiver.sync_overview(None).await.unwrap();
    assert!(
        incoming
            .history
            .iter()
            .any(|change| change.activity.direction == Direction::Incoming
                && change.summary.outcome == Outcome::Applied)
    );
    assert_eq!(
        store::records::get(&receiver.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Private body"
    );
    let (ops, _) = sender.ops_after(0, 100_000).await.unwrap();
    assert_eq!(
        receiver
            .import_op_batch(&OpBatch {
                from_organ: sender_uid,
                ops
            })
            .await
            .unwrap(),
        0
    );
    let blocked_uid = record(&sender).await;
    store::organs::set_trust(&sender.store.pool, &receiver_uid, "blocked")
        .await
        .unwrap();
    sender
        .drain_outbox(|_, _, _| async { panic!("blocked destination must not be called") })
        .await
        .unwrap();
    assert!(
        store::records::get(&receiver.store.pool, &blocked_uid)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn file_workflow_uses_shared_history_without_logging_idle_polls() {
    let (engine, organ) = cell().await;
    let uid = record(&engine).await;
    let directory = std::env::temp_dir().join(nucleus::new_uid("sync-files"));
    let mut state = engine::file_sync::FileSyncState::new();
    let report = engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert!(!report.written_to_disk.is_empty());
    let first = engine.sync_overview(None).await.unwrap();
    assert!(matches!(
        first.history[0].activity.instance.destination,
        Destination::Directory { .. }
    ));
    assert!(first.history[0].summary.subjects.contains(&uid));
    engine
        .file_sync_tick(&directory, &organ, &mut state)
        .await
        .unwrap();
    assert_eq!(
        engine.sync_overview(None).await.unwrap().history,
        first.history
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn cancelled_work_releases_activity_and_history_failure_does_not_replay_work() {
    let (engine, _) = cell().await;
    let engine = Arc::new(engine);
    let (started, waiting) = tokio::sync::oneshot::channel();
    let task = {
        let engine = engine.clone();
        tokio::spawn(async move {
            engine
                .sync_service
                .run(
                    &engine.store,
                    activity(),
                    async {
                        started.send(()).unwrap();
                        std::future::pending::<Result<(), String>>().await
                    },
                    |_| Summary::new(Outcome::Applied, 1),
                )
                .await
        })
    };
    waiting.await.unwrap();
    assert_eq!(engine.sync_service.active().len(), 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(engine.sync_service.active().is_empty());
    store::sqlx::query("CREATE TRIGGER fail_sync_history BEFORE INSERT ON sync_activity BEGIN SELECT RAISE(FAIL, 'history unavailable'); END")
        .execute(&engine.store.pool).await.unwrap();
    let value = engine
        .sync_service
        .run(
            &engine.store,
            activity(),
            async { Ok::<_, String>(42) },
            |_| Summary::new(Outcome::Applied, 1),
        )
        .await
        .unwrap();
    assert_eq!(value, 42);
    assert!(
        engine
            .sync_overview(None)
            .await
            .unwrap()
            .history_error
            .is_some()
    );
    assert!(engine.sync_service.active().is_empty());
}

#[test]
fn workspace_description_does_not_depend_on_interface_or_crdt_types() {
    let instance = Instance {
        content: Content::Workspace(nucleus::sync::WorkspaceSync {
            workspace_uid: "workspace".into(),
        }),
        destination: Destination::Organ {
            uid: "organ".into(),
        },
        capabilities: nucleus::sync::Capabilities {
            incremental: true,
            durable_queue: true,
            review: true,
        },
    };
    assert_eq!(
        serde_json::from_str::<Instance>(&serde_json::to_string(&instance).unwrap()).unwrap(),
        instance
    );
}

#[tokio::test]
async fn transient_queues_are_observed_without_copying_or_keeping_messages_alive() {
    let (engine, _) = cell().await;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let queued = sender.downgrade();
    let registration = engine
        .sync_service
        .observe_queue(move || nucleus::sync::Queue {
            instance: Instance::interface("local", "test"),
            incoming: 0,
            outgoing: queued.upgrade().map_or(0, |sender| {
                (sender.max_capacity() - sender.capacity()) as u64
            }),
        });
    sender.send("keep this message").await.unwrap();
    assert_eq!(engine.sync_overview(None).await.unwrap().outgoing, 1);
    assert_eq!(receiver.recv().await, Some("keep this message"));
    assert_eq!(engine.sync_overview(None).await.unwrap().outgoing, 0);
    drop(sender);
    assert_eq!(receiver.recv().await, None);
    drop(registration);
    assert!(engine.sync_overview(None).await.unwrap().queues.is_empty());
}
