use nucleus::location::{Command, Settings, SourceKind};
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
        Err(engine::EngineError::Consequence(
            "This local sharing has no remote transport".into(),
        ))
    }
}

pub(super) async fn start(
    engine: &engine::Engine,
    transfer: &str,
) -> (Arc<dyn engine::location::Network>, String, String) {
    let network: Arc<dyn engine::location::Network> = Arc::new(Local);
    engine.attach_location_network(network.clone());
    let role = store::auth::ensure_role(&engine.store.pool, "cancellation-location")
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
        "Location controller",
        "cancellation.location",
        "hash",
        role,
    )
    .await
    .unwrap();
    let record = super::plain(engine, "cancellation.location-record", 1.0).await;
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let settings = Settings {
        record_uid: record.clone(),
        controller_uid: controller.clone(),
        source_cell_uid: cell.uid,
        source_node_id: network.node_id(),
        source_kind: SourceKind::Manual,
        duration_seconds: 3600,
        recipients: vec![],
        transfer_uid: Some(transfer.into()),
    };
    for command in [
        Command::Configure { settings },
        Command::Start {
            person: controller.clone(),
            record_uid: record.clone(),
        },
    ] {
        engine.location_request(command, None).await.unwrap();
    }
    let epoch = engine
        .location_pending_sources()
        .await
        .into_iter()
        .find(|source| source.settings.record_uid == record)
        .unwrap()
        .session_uid;
    engine
        .location_request(
            Command::Approve {
                person: controller.clone(),
                record_uid: record.clone(),
                expected_session_uid: epoch,
            },
            None,
        )
        .await
        .unwrap();
    let context = engine
        .location_request(
            Command::Context {
                person: controller.clone(),
                record_uid: record.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    engine
        .location_request(
            Command::Publish {
                person: controller.clone(),
                record_uid: record.clone(),
                session_uid: context["view"]["session_uid"].as_str().unwrap().into(),
                fix: nucleus::location::Fix {
                    sequence: 1,
                    latitude: 1.0,
                    longitude: 2.0,
                    accuracy_metres: None,
                    captured_at_ms: nucleus::execution::now().timestamp_millis(),
                    capture_age_ms: 0,
                },
            },
            None,
        )
        .await
        .unwrap();
    (network, record, controller)
}

pub(super) async fn assert_live(engine: &engine::Engine, record: &str, person: &str) {
    let view = engine
        .location_request(
            Command::View {
                person: person.into(),
                record_uid: record.into(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert!(view["fix"].is_object());
}

pub(super) async fn assert_stopped(engine: &engine::Engine, record: &str, person: &str) {
    let view = engine
        .location_request(
            Command::View {
                person: person.into(),
                record_uid: record.into(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(view["status"], "stopped");
    assert!(view["fix"].is_null());
    assert!(
        !engine
            .location_sources()
            .await
            .iter()
            .any(|source| source.settings.record_uid == record)
    );
}
