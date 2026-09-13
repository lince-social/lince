use engine::{Engine, actions::Action, private_password::PasswordInput};
use std::sync::Arc;
use transport::{ClientMessage, LaneHub, ServerMessage, Session};

async fn fixture() -> (Arc<Engine>, engine::login::LoginSession, String) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let role = store::auth::ensure_role(&engine.store.pool, "reader")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    let person = engine
        .act(
            Action::CreateUser {
                username: "reader".into(),
                name: "Reader".into(),
                password: "session-password".into(),
                role: "reader".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let record = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Shared".into(),
                body: "Protected".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&person), &record)
        .await
        .unwrap();
    let login = engine
        .login_password(
            "reader",
            PasswordInput::new(b"session-password".to_vec()).unwrap(),
            None,
        )
        .await
        .unwrap();
    (engine, login, record)
}

fn query() -> ClientMessage {
    serde_json::from_value(serde_json::json!({"type":"subscribe", "id":"records", "protein":{"source":"record", "where":[]}})).unwrap()
}

#[tokio::test]
async fn revocation_stops_queries_collaboration_and_actions_but_keeps_local_owner_access() {
    let (engine, login, record) = fixture().await;
    let mut session = Session::authenticated(
        engine.clone(),
        Arc::new(LaneHub::new()),
        "remote",
        login.clone(),
    );
    let rows = session.handle(query()).await;
    assert!(matches!(&rows[0], ServerMessage::Snapshot { rows, .. } if rows.len() == 1));
    let denied = session
        .handle(ClientMessage::CollabUpdate {
            id: "edit".into(),
            record_uid: record.clone(),
            update_base64: String::new(),
        })
        .await;
    assert!(
        matches!(&denied[0], ServerMessage::Error { code: Some(code), .. } if code == "forbidden")
    );
    login.revoke();
    for message in [
        query(),
        ClientMessage::CollabJoin {
            id: "doc".into(),
            record_uid: record.clone(),
        },
        ClientMessage::Act {
            id: "edit".into(),
            action: Action::EditRecordText {
                target: record.clone(),
                head: Some("Forged".into()),
                body: None,
            },
        },
    ] {
        let denied = session.handle(message).await;
        assert!(
            matches!(&denied[0], ServerMessage::Error { code: Some(code), .. } if code == "session_expired")
        );
    }
    let mut owner = Session::local(engine.clone(), Arc::new(LaneHub::new()), "owner");
    let result = owner
        .handle(ClientMessage::Act {
            id: "owner-edit".into(),
            action: Action::EditRecordText {
                target: record,
                head: Some("Owner edit".into()),
                body: None,
            },
        })
        .await;
    assert!(matches!(&result[0], ServerMessage::ActionOk { .. }));
}

#[tokio::test]
async fn removing_read_permission_clears_a_live_subscription() {
    let (engine, login, _) = fixture().await;
    let mut session =
        Session::authenticated(engine.clone(), Arc::new(LaneHub::new()), "remote", login);
    session.handle(query()).await;
    engine
        .act(
            Action::RevokePermission {
                role: "reader".into(),
                permission: "record:read".into(),
            },
            None,
        )
        .await
        .unwrap();
    let result = session.on_sync_event(transport::SyncEvent::Refresh).await;
    assert!(matches!(&result[0], ServerMessage::Snapshot { rows, .. } if rows.is_empty()));
}

#[tokio::test]
async fn missing_role_never_bypasses_record_permissions() {
    let (engine, login, record) = fixture().await;
    let mut connection = engine.store.pool.acquire().await.unwrap();
    let access = store::auth::person_access_on(&mut connection, login.person_uid())
        .await
        .unwrap()
        .unwrap();
    store::auth::compare_and_set_role_on(
        &mut connection,
        login.person_uid(),
        None,
        access.revision,
    )
    .await
    .unwrap();
    drop(connection);
    let mut session = Session::authenticated(
        engine.clone(),
        Arc::new(LaneHub::new()),
        "remote",
        login.clone(),
    );
    let result = session.handle(query()).await;
    assert!(matches!(&result[0], ServerMessage::Snapshot { rows, .. } if rows.is_empty()));
    assert!(
        engine
            .act(
                Action::EditRecordText {
                    target: record,
                    head: Some("Forged".into()),
                    body: None
                },
                Some(login.person_uid().into())
            )
            .await
            .is_err()
    );
}
