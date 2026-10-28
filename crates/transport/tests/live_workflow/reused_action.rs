use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use store::sqlx::Row;
use tokio::sync::mpsc;

struct Tasks(Vec<tokio::task::JoinHandle<()>>);

impl Drop for Tasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

async fn receive(incoming: &mut mpsc::Receiver<ServerMessage>) -> ServerMessage {
    tokio::time::timeout(std::time::Duration::from_secs(20), incoming.recv())
        .await
        .expect("The live Action did not receive a response")
        .expect("The live client disconnected")
}

async fn edit(outgoing: &mpsc::Sender<ClientMessage>, uid: &str, body: &str) {
    outgoing
        .send(ClientMessage::Act {
            id: "body-autosave".into(),
            action: Action::EditRecordText {
                target: uid.into(),
                head: None,
                body: Some(body.into()),
            },
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn denied_live_autosave_recovers_with_the_same_reply_id_without_allowing_a_replay() {
    let (host, host_organ) = cell("http://action-host.test").await;
    let (guest, guest_organ) = cell("http://action-guest.test").await;
    let host_wire = Arc::new(
        Wire::bind(host.clone(), SecretKey::from_bytes(&[83; 32]), Reach::Local)
            .await
            .unwrap(),
    );
    let guest_wire = Wire::bind(
        guest.clone(),
        SecretKey::from_bytes(&[84; 32]),
        Reach::Local,
    )
    .await
    .unwrap();
    know(&host, &guest_organ, &guest_wire.node_id().to_string()).await;
    know(&guest, &host_organ, &host_wire.node_id().to_string()).await;
    let person = host
        .act(
            Action::GrantOrganLogin {
                organ: guest_organ,
                person_name: "Autosave subject".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let role = store::auth::ensure_role(&host.store.pool, "autosave-subject")
        .await
        .unwrap();
    let read = store::auth::ensure_permission(&host.store.pool, "record", "read")
        .await
        .unwrap();
    let update = store::auth::ensure_permission(&host.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&host.store.pool, role, read)
        .await
        .unwrap();
    store::auth::set_user_role(&host.store.pool, &person, role)
        .await
        .unwrap();
    let uid = host
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Autosave target".into(),
                body: "original".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host_wire.set_live_handler(LiveHost::new(
        host.clone(),
        Arc::new(transport::LaneHub::new()),
    ));
    let mut tasks = Tasks(Vec::new());
    let serving = host_wire.clone();
    tasks.0.push(tokio::spawn(async move {
        serving.serve().await;
    }));
    let connection = guest_wire
        .endpoint()
        .connect(loopback(&host_wire), ALPN_LIVE)
        .await
        .unwrap();
    let (outgoing, requests) = mpsc::channel(16);
    let (responses, mut incoming) = mpsc::channel(16);
    tasks.0.push(tokio::spawn(async move {
        transport::live_client::drive(connection, requests, responses, || {})
            .await
            .unwrap();
    }));
    assert!(matches!(
        receive(&mut incoming).await,
        ServerMessage::SessionAuthenticated { .. }
    ));
    edit(&outgoing, &uid, "denied draft").await;
    assert!(
        matches!(receive(&mut incoming).await, ServerMessage::Error { id, message, .. } if id == "body-autosave" && message.to_lowercase().contains("permission"))
    );
    assert_eq!(
        store::records::get(&host.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .body,
        "original"
    );
    store::auth::grant(&host.store.pool, role, update)
        .await
        .unwrap();
    for body in ["recovered draft", "another edit"] {
        edit(&outgoing, &uid, body).await;
        assert!(
            matches!(receive(&mut incoming).await, ServerMessage::ActionOk { id, .. } if id == "body-autosave")
        );
        assert_eq!(
            store::records::get(&host.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .body,
            body
        );
    }
    let first = store::sqlx::query("SELECT session_id, session_challenge, sequence, message_id, action_base64, signature FROM signed_action_intent WHERE actor_person_uid = ? ORDER BY sequence LIMIT 1")
        .bind(&person).fetch_one(&host.store.pool).await.unwrap();
    let old_wire: String = first.get("message_id");
    outgoing
        .send(ClientMessage::SignedAct {
            id: old_wire.clone(),
            session_id: first.get("session_id"),
            session_challenge: first.get("session_challenge"),
            sequence: first.get::<i64, _>("sequence") as u64,
            action_base64: first.get("action_base64"),
            signature: first.get("signature"),
        })
        .await
        .unwrap();
    assert!(
        matches!(receive(&mut incoming).await, ServerMessage::Error { id, code, .. } if id == old_wire && code.as_deref() == Some("action_intent_replay"))
    );
    assert_eq!(
        store::records::get(&host.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .body,
        "another edit"
    );
    edit(&outgoing, &uid, "after replay rejection").await;
    assert!(
        matches!(receive(&mut incoming).await, ServerMessage::ActionOk { id, .. } if id == "body-autosave")
    );
    let intents = store::sqlx::query("SELECT sequence, message_id FROM signed_action_intent WHERE actor_person_uid = ? ORDER BY sequence").bind(&person).fetch_all(&host.store.pool).await.unwrap();
    assert_eq!(intents.len(), 4);
    let mut ids = std::collections::BTreeSet::new();
    for (index, intent) in intents.iter().enumerate() {
        assert_eq!(intent.get::<i64, _>("sequence"), index as i64 + 1);
        let id: String = intent.get("message_id");
        assert_ne!(id, "body-autosave");
        assert!(ids.insert(id));
    }
    assert_eq!(
        store::records::get(&host.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .body,
        "after replay rejection"
    );
    let document = loro::LoroDoc::new();
    document
        .import(
            &STANDARD
                .decode(host.collab_snapshot(&uid).await.unwrap())
                .unwrap(),
        )
        .unwrap();
    let version = document.oplog_vv();
    let text = document.get_text("body");
    let original = text.to_string();
    text.insert(original.chars().count(), " + retry").unwrap();
    document.commit();
    let change = nucleus::new_uid("op");
    let operation = Action::ChangeRecord {
        request: engine::record_change::Request {
            id: change.clone(),
            record_uid: uid.clone(),
            mutation: engine::record_change::Mutation::Text {
                update_base64: engine::collab::encode_update(&document, &version).unwrap(),
            },
        },
    };
    store::auth::revoke(&host.store.pool, role, update)
        .await
        .unwrap();
    outgoing
        .send(ClientMessage::Act {
            id: change.clone(),
            action: operation.clone(),
        })
        .await
        .unwrap();
    assert!(
        matches!(receive(&mut incoming).await, ServerMessage::Error { id, message, .. } if id == change && message.to_lowercase().contains("permission"))
    );
    assert_eq!(
        store::records::get(&host.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .body,
        original
    );
    store::auth::grant(&host.store.pool, role, update)
        .await
        .unwrap();
    let mut receipt = None;
    let mut fact_count = None;
    for _ in 0..2 {
        outgoing
            .send(ClientMessage::Act {
                id: change.clone(),
                action: operation.clone(),
            })
            .await
            .unwrap();
        let ServerMessage::ActionOk {
            id,
            data: Some(data),
            facts,
            ..
        } = receive(&mut incoming).await
        else {
            panic!("The retained durable autosave did not recover");
        };
        assert_eq!(id, change);
        assert_eq!(data["change_id"], change);
        if let Some(previous) = &receipt {
            assert_eq!(&data, previous);
            assert_eq!(facts, 0);
        }
        receipt = Some(data);
        assert_eq!(
            store::records::get(&host.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .body,
            format!("{original} + retry")
        );
        let count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE record_uid = ?")
                .bind(&uid)
                .fetch_one(&host.store.pool)
                .await
                .unwrap();
        if let Some(previous) = fact_count {
            assert_eq!(count, previous);
        }
        fact_count = Some(count);
    }
    let receipts: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM record_change_receipt WHERE actor = ? AND change_uid = ?",
    )
    .bind(&person)
    .bind(&change)
    .fetch_one(&host.store.pool)
    .await
    .unwrap();
    assert_eq!(receipts, 1);
    drop(outgoing);
    drop(tasks);
    guest_wire.endpoint().close().await;
    host_wire.endpoint().close().await;
}
