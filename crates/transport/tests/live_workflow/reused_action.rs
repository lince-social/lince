use std::{collections::BTreeSet, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use engine::{
    Engine,
    actions::Action,
    collab::encode_update,
    record_change::{Mutation, Request},
    wire::{ALPN_LIVE, Reach, Wire},
};
use iroh::SecretKey;
use loro::LoroDoc;
use nucleus::{RecordKind, new_uid};
use serde_json::Value;
use store::{
    auth, records,
    sqlx::{query_as, query_scalar},
};
use tokio::{sync::mpsc, task::JoinSet, time::timeout};
use transport::{ClientMessage, LaneHub, ServerMessage, live::LiveHost, live_client::drive};

use super::{cell, know, loopback};

const AUTOSAVE_ID: &str = "body-autosave";

async fn receive(incoming: &mut mpsc::Receiver<ServerMessage>) -> ServerMessage {
    timeout(Duration::from_secs(20), incoming.recv())
        .await
        .expect("The live Action did not receive a response")
        .expect("The live client disconnected")
}

async fn send_action(outgoing: &mpsc::Sender<ClientMessage>, id: &str, action: Action) {
    outgoing
        .send(ClientMessage::Act {
            id: id.into(),
            action,
        })
        .await
        .expect("queue the live Action");
}

async fn edit(outgoing: &mpsc::Sender<ClientMessage>, uid: &str, body: &str) {
    send_action(
        outgoing,
        AUTOSAVE_ID,
        Action::EditRecordText {
            target: uid.into(),
            head: None,
            body: Some(body.into()),
        },
    )
    .await;
}

async fn record_body(host: &Engine, uid: &str) -> String {
    records::get(&host.store.pool, uid)
        .await
        .expect("read the host Record")
        .expect("find the autosave target")
        .body
}

async fn permission_denied(incoming: &mut mpsc::Receiver<ServerMessage>, expected_id: &str) {
    let reply = receive(incoming).await;
    assert!(
        matches!(&reply, ServerMessage::Error { id, message, .. }
            if id == expected_id && message.to_lowercase().contains("permission")),
        "expected a permission denial for {expected_id}: {reply:?}"
    );
}

#[tokio::test]
async fn denied_live_autosave_recovers_with_the_same_reply_id_without_allowing_a_replay() {
    let (host, host_organ) = cell("http://action-host.test").await;
    let (guest, guest_organ) = cell("http://action-guest.test").await;
    let host_wire = Arc::new(
        Wire::bind(
            Arc::clone(&host),
            SecretKey::from_bytes(&[83; 32]),
            Reach::Local,
        )
        .await
        .expect("bind the host"),
    );
    let guest_wire = Wire::bind(
        Arc::clone(&guest),
        SecretKey::from_bytes(&[84; 32]),
        Reach::Local,
    )
    .await
    .expect("bind the guest");
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
        .expect("grant the guest login")
        .created
        .expect("guest Person uid");
    let pool = &host.store.pool;
    let role = auth::ensure_role(pool, "autosave-subject")
        .await
        .expect("create the test role");
    let read = auth::ensure_permission(pool, "record", "read")
        .await
        .expect("create the read permission");
    let update = auth::ensure_permission(pool, "record", "update")
        .await
        .expect("create the update permission");
    auth::grant(pool, role, read)
        .await
        .expect("grant read permission only");
    auth::set_user_role(pool, &person, role)
        .await
        .expect("assign the guest role");
    let uid = host
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Autosave target".into(),
                body: "original".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .expect("create the target Record")
        .created
        .expect("target Record uid");
    host_wire.set_live_handler(LiveHost::new(Arc::clone(&host), Arc::new(LaneHub::new())));
    let mut tasks = JoinSet::new();
    let serving = Arc::clone(&host_wire);
    tasks.spawn(async move {
        serving.serve().await;
    });
    let connection = guest_wire
        .endpoint()
        .connect(loopback(&host_wire), ALPN_LIVE)
        .await
        .expect("connect to the live host");
    let (outgoing, requests) = mpsc::channel(16);
    let (responses, mut incoming) = mpsc::channel(16);
    tasks.spawn(async move {
        drive(connection, requests, responses, || {})
            .await
            .expect("drive the guest connection");
    });
    assert!(matches!(
        receive(&mut incoming).await,
        ServerMessage::SessionAuthenticated { .. }
    ));
    edit(&outgoing, &uid, "denied draft").await;
    permission_denied(&mut incoming, AUTOSAVE_ID).await;
    assert_eq!(record_body(&host, &uid).await, "original");
    auth::grant(pool, role, update)
        .await
        .expect("enable the same autosave to recover");
    for body in ["recovered draft", "another edit"] {
        edit(&outgoing, &uid, body).await;
        assert!(
            matches!(receive(&mut incoming).await, ServerMessage::ActionOk { id, .. } if id == AUTOSAVE_ID)
        );
        assert_eq!(record_body(&host, &uid).await, body);
    }
    let (session_id, session_challenge, sequence, old_wire, action_base64, signature): (
        String, String, i64, String, String, String,
    ) = query_as("SELECT session_id, session_challenge, sequence, message_id, action_base64, signature FROM signed_action_intent WHERE actor_person_uid = ? ORDER BY sequence LIMIT 1")
        .bind(&person)
        .fetch_one(pool)
        .await
        .expect("load the original signed envelope");
    outgoing
        .send(ClientMessage::SignedAct {
            id: old_wire.clone(),
            session_id,
            session_challenge,
            sequence: u64::try_from(sequence).expect("stored sequence is nonnegative"),
            action_base64,
            signature,
        })
        .await
        .expect("send the same signed envelope again");
    assert!(
        matches!(receive(&mut incoming).await, ServerMessage::Error { id, code, .. } if id == old_wire && code.as_deref() == Some("action_intent_replay"))
    );
    assert_eq!(record_body(&host, &uid).await, "another edit");
    edit(&outgoing, &uid, "after replay rejection").await;
    assert!(
        matches!(receive(&mut incoming).await, ServerMessage::ActionOk { id, .. } if id == AUTOSAVE_ID)
    );
    let intents: Vec<(i64, String)> = query_as(
        "SELECT sequence, message_id FROM signed_action_intent WHERE actor_person_uid = ? ORDER BY sequence",
    )
    .bind(&person)
    .fetch_all(pool)
    .await
    .expect("list the signed envelopes in sequence order");
    assert_eq!(intents.len(), 4);
    let mut ids = BTreeSet::new();
    for (expected, (sequence, id)) in (1..).zip(intents) {
        assert_eq!(sequence, expected);
        assert_ne!(id, AUTOSAVE_ID);
        assert!(ids.insert(id));
    }
    assert_eq!(record_body(&host, &uid).await, "after replay rejection");
    let document = LoroDoc::new();
    let snapshot = host
        .collab_snapshot(&uid)
        .await
        .expect("read the saved document");
    let snapshot = STANDARD.decode(snapshot).expect("decode the snapshot");
    document
        .import(&snapshot)
        .expect("import the host document");
    let version = document.oplog_vv();
    let text = document.get_text("body");
    let original = text.to_string();
    text.insert(text.len_unicode(), " + retry")
        .expect("append the durable edit");
    document.commit();
    let change = new_uid("op");
    let operation = Action::ChangeRecord {
        request: Request {
            id: change.clone(),
            record_uid: uid.clone(),
            mutation: Mutation::Text {
                update_base64: encode_update(&document, &version).expect("encode the pending edit"),
            },
        },
    };
    auth::revoke(pool, role, update)
        .await
        .expect("disable the first durable edit attempt");
    send_action(&outgoing, &change, operation.clone()).await;
    permission_denied(&mut incoming, &change).await;
    assert_eq!(record_body(&host, &uid).await, original);
    auth::grant(pool, role, update)
        .await
        .expect("allow the durable edit to recover");
    let updated = format!("{original} + retry");
    let mut observed = None;
    for attempt in [operation.clone(), operation] {
        send_action(&outgoing, &change, attempt).await;
        let reply = receive(&mut incoming).await;
        let ServerMessage::ActionOk {
            id,
            data: Some(data),
            facts,
            ..
        } = reply
        else {
            panic!("The retained durable autosave did not recover: {reply:?}");
        };
        assert_eq!(id, change);
        assert_eq!(
            data.get("change_id").and_then(Value::as_str),
            Some(change.as_str())
        );
        assert_eq!(record_body(&host, &uid).await, updated);
        let count: i64 = query_scalar("SELECT COUNT(*) FROM fact WHERE record_uid = ?")
            .bind(&uid)
            .fetch_one(pool)
            .await
            .expect("count the Record's Facts");
        let current = (data, count);
        if let Some(previous) = &observed {
            assert_eq!(&current, previous);
            assert_eq!(facts, 0);
        }
        observed = Some(current);
    }
    let receipts: i64 = query_scalar(
        "SELECT COUNT(*) FROM record_change_receipt WHERE actor = ? AND change_uid = ?",
    )
    .bind(&person)
    .bind(&change)
    .fetch_one(pool)
    .await
    .expect("count durable receipts for the change");
    assert_eq!(receipts, 1);
    drop(outgoing);
    tasks.shutdown().await;
    guest_wire.endpoint().close().await;
    host_wire.endpoint().close().await;
}
