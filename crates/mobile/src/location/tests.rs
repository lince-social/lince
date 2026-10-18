use super::*;
use nucleus::location::{Command, Settings, SourceLease};
use std::sync::Arc;

struct Local;
#[async_trait::async_trait]
impl engine::location::Network for Local {
    fn node_id(&self) -> String {
        "a".repeat(64)
    }
    async fn location_request(
        &self,
        _: &str,
        _: nucleus::location::PeerRequest,
    ) -> Result<serde_json::Value, engine::EngineError> {
        Err(engine::EngineError::Consequence("Offline".into()))
    }
}

async fn start(
    engine: &engine::Engine,
    person: &str,
    record: &str,
    kind: SourceKind,
) -> SourceLease {
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    engine
        .location_request(
            Command::Configure {
                settings: Settings {
                    record_uid: record.into(),
                    controller_uid: person.into(),
                    source_cell_uid: cell.uid,
                    source_node_id: "a".repeat(64),
                    source_kind: kind,
                    duration_seconds: 3600,
                    recipients: vec![],
                    transfer_uid: None,
                },
            },
            None,
        )
        .await
        .unwrap();
    engine
        .location_request(
            Command::Start {
                person: person.into(),
                record_uid: record.into(),
            },
            None,
        )
        .await
        .unwrap();
    let lease = engine
        .location_pending_sources()
        .await
        .into_iter()
        .find(|source| source.settings.record_uid == record)
        .unwrap();
    engine
        .location_request(
            Command::Approve {
                person: person.into(),
                record_uid: record.into(),
                expected_session_uid: lease.session_uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .location_sources()
        .await
        .into_iter()
        .find(|source| source.settings.record_uid == record)
        .unwrap()
}

#[tokio::test]
async fn denied_revoked_and_old_android_epochs_only_affect_their_selected_record_sessions() {
    let engine = engine::Engine::open_memory().await.unwrap();
    let network = Arc::new(Local);
    engine.attach_location_network(network.clone());
    let role = store::auth::ensure_role(&engine.store.pool, "Capture controller")
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
    let person = store::auth::create_person_login(
        &engine.store.pool,
        "Controller",
        "capture.controller",
        "hash",
        role,
    )
    .await
    .unwrap();
    let mut records = Vec::new();
    for name in ["Tracked Record", "Independent manual"] {
        records.push(
            engine
                .act(
                    engine::actions::Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: name.into(),
                        body: String::new(),
                        quantity: 0.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    let old = start(&engine, &person, &records[0], SourceKind::Device).await;
    let manual = start(&engine, &person, &records[1], SourceKind::Manual).await;
    let mut capture = Capture {
        epoch: 200,
        until: Some(Instant::now() + std::time::Duration::from_secs(7200)),
        sources: vec![old.clone()],
        sequence: 0,
    };
    ACTIVE_CAPTURE.store(200, Ordering::Release);
    sample(Sample {
        epoch: 200,
        received: Instant::now(),
        result: Ok((-23.5, -46.6, Some(1500.0), 0)),
        stopped: false,
    });
    capture.tick(&engine).await;
    let data = engine
        .location_request(
            Command::Context {
                person: person.clone(),
                record_uid: records[0].clone(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let context: nucleus::location::Context = serde_json::from_value(data).unwrap();
    assert_eq!(context.view.fix.unwrap().accuracy_metres, Some(1500.0));
    sample(Sample {
        epoch: 200,
        received: Instant::now(),
        result: Err("Permission revoked".into()),
        stopped: false,
    });
    capture.tick(&engine).await;
    let remaining = engine.location_sources().await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].session_uid, manual.session_uid);
    let successor = start(&engine, &person, &records[0], SourceKind::Device).await;
    let mut current = Capture {
        epoch: 201,
        until: Some(Instant::now() + std::time::Duration::from_secs(7200)),
        sources: vec![successor.clone()],
        sequence: 0,
    };
    ACTIVE_CAPTURE.store(201, Ordering::Release);
    sample(Sample {
        epoch: 200,
        received: Instant::now(),
        result: Err("Old notification Stop".into()),
        stopped: true,
    });
    current.tick(&engine).await;
    assert!(
        engine
            .location_sources()
            .await
            .iter()
            .any(|source| source.session_uid == successor.session_uid)
    );
    capture.sources = vec![old];
    capture.epoch = 200;
    capture.stop(&engine).await;
    assert_eq!(ACTIVE_CAPTURE.load(Ordering::Acquire), 201);
    assert!(
        engine
            .location_sources()
            .await
            .iter()
            .any(|source| source.session_uid == successor.session_uid)
    );
    sample(Sample {
        epoch: 201,
        received: Instant::now(),
        result: Err("Notification Stop".into()),
        stopped: true,
    });
    current.tick(&engine).await;
    assert_eq!(
        engine.location_sources().await[0].session_uid,
        manual.session_uid
    );
    assert_eq!(engine.location_sources().await.len(), 1);
    assert_eq!(ACTIVE_CAPTURE.load(Ordering::Acquire), 0);
    assert!(SAMPLE.lock().unwrap().is_none());
    let denied = start(&engine, &person, &records[0], SourceKind::Device).await;
    let mut denied_capture = Capture {
        epoch: 202,
        until: Some(Instant::now() + std::time::Duration::from_secs(7200)),
        sources: vec![denied],
        sequence: 0,
    };
    ACTIVE_CAPTURE.store(202, Ordering::Release);
    sample(Sample {
        epoch: 202,
        received: Instant::now(),
        result: Err("OS permission denied".into()),
        stopped: false,
    });
    denied_capture.tick(&engine).await;
    assert_eq!(engine.location_sources().await.len(), 1);
}
