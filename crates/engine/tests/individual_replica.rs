use std::sync::Arc;

use engine::Engine;
use engine::sync::OpBatch;
use engine::trust::Signer;
use engine::wire::{ALPN_SYNC, ALPN_THREAD, Reach, Wire, WireRequest, WireResponse};
use iroh::{EndpointAddr, SecretKey};
use nucleus::RecordKind;
use store::records::NewRecord;

async fn cell(base_url: &str) -> (Arc<Engine>, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (Arc::new(e), organ)
}

fn secret(seed: u8) -> SecretKey {
    SecretKey::from_bytes(&[seed; 32])
}

fn loopback(wire: &Wire) -> EndpointAddr {
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .map(|addr| addr.port())
        .next()
        .expect("bound");
    EndpointAddr::new(wire.node_id()).with_ip_addr((std::net::Ipv4Addr::LOCALHOST, port).into())
}

async fn full_contact(us: &Engine, organ_uid: &str, node_id: &str) {
    store::organs::add_contact(&us.store.pool, organ_uid, None, "peer", "", 0)
        .await
        .expect("contact");
    store::organs::set_node_id(&us.store.pool, organ_uid, Some(node_id))
        .await
        .expect("node id");
    store::organs::set_trust(&us.store.pool, organ_uid, "known")
        .await
        .expect("trust");
    store::organs::set_sync_policy(&us.store.pool, organ_uid, true, true)
        .await
        .expect("sync policy");
}

#[tokio::test]
async fn conversation_records_are_born_inside_their_root() {
    let (a, _) = cell("http://a.test").await;
    let (conversation, thread) = a
        .start_conversation("o-friend", "Beach plans")
        .await
        .expect("conversation");
    let message = a
        .send_message(&thread, "me", "what did we do last thursday?")
        .await
        .expect("message");

    for uid in [&conversation, &thread, &message] {
        assert_eq!(
            store::replica::root_of(&a.store.pool, uid)
                .await
                .expect("root"),
            Some(conversation.clone()),
            "every level must be inside the conversation root"
        );
    }

    let second = a
        .open_thread(&conversation, "Another topic")
        .await
        .expect("second thread");
    assert_eq!(
        store::replica::root_of(&a.store.pool, &second)
            .await
            .expect("root"),
        Some(conversation)
    );
}

#[tokio::test]
async fn a_full_sync_contact_without_a_grant_receives_nothing() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(30), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(31), Reach::Local)
        .await
        .expect("b binds");

    full_contact(&a, &b_organ, &b_wire.node_id().to_string()).await;
    full_contact(&b, &a_organ, &a_wire.node_id().to_string()).await;

    store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("public-note"),
            kind: RecordKind::Plain,
            head: "Public",
            body: "ordinary feed",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    let (_conversation, thread) = a
        .start_conversation("o-someone-else", "Private")
        .await
        .expect("conversation");
    let secret_message = a
        .send_message(&thread, "me", "this must never reach B")
        .await
        .expect("message");

    let a_addr = loopback(&a_wire);
    let _ = loopback(&b_wire);
    let b_serving = {
        let b_wire = b_wire.clone();
        tokio::spawn(async move { b_wire.serve().await })
    };
    let _ = &b_serving;

    let a_serving = tokio::spawn(async move { a_wire.serve().await });
    let response = b_wire
        .request(
            a_addr,
            ALPN_SYNC,
            &WireRequest::FetchOpsSince {
                vector: Vec::new(),
                limit: 2000,
            },
        )
        .await
        .expect("fetch");
    let ops = match response {
        WireResponse::Ops { ops, .. } => ops,
        other => panic!("expected Ops, got {other:?}"),
    };

    assert!(
        ops.iter()
            .any(|op| op.value.as_deref() == Some("\"Public\"")),
        "the ordinary feed must still work, or this test proves nothing"
    );
    for op in &ops {
        let root = store::replica::root_of(&a.store.pool, &op.uid)
            .await
            .expect("root");
        assert!(
            root.is_none(),
            "an individually-replicated record leaked onto the general feed: {op:?}"
        );
    }

    b.import_op_batch(&OpBatch {
        from_organ: a_organ,
        ops,
    })
    .await
    .expect("import");
    assert!(
        store::records::get(&b.store.pool, &secret_message)
            .await
            .expect("get")
            .is_none(),
        "B must hold no copy of a conversation it was never granted"
    );

    a_serving.abort();
    b_serving.abort();
}

#[tokio::test]
async fn a_granted_conversation_syncs_to_its_contact() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(32), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(33), Reach::Local)
        .await
        .expect("b binds");

    full_contact(&a, &b_organ, &b_wire.node_id().to_string()).await;
    full_contact(&b, &a_organ, &a_wire.node_id().to_string()).await;

    let (conversation, thread) = a
        .start_conversation(&b_organ, "Beach plans")
        .await
        .expect("conversation");
    a.send_message(&thread, "me", "meet at six")
        .await
        .expect("message");

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

    let (ops, _) = a.ops_after(0, 2000).await.expect("ops");
    let _ = ops;
    let root_ops = store::sync_ops::after_in_root(&a.store.pool, &conversation, 0, 2000)
        .await
        .expect("root ops");
    assert!(
        !root_ops.is_empty(),
        "the conversation must have logged ops"
    );
    let batch = OpBatch {
        from_organ: a_organ.clone(),
        ops: a.hydrate_ops(root_ops).await.expect("hydrate"),
    };

    let refused = a_wire
        .request(
            b_addr.clone(),
            ALPN_SYNC,
            &WireRequest::PushGrantOps {
                root: conversation.clone(),
                batch: batch.clone(),
            },
        )
        .await
        .expect("call completes");
    assert!(
        matches!(refused, WireResponse::Refused { .. }),
        "no grant yet, so nothing may be pushed: {refused:?}"
    );

    store::replica::offer(&b.store.pool, &conversation, &a_organ)
        .await
        .expect("offer");
    b.accept_conversation(&conversation, &a_organ)
        .await
        .expect("accept");

    let applied = a_wire
        .request(
            b_addr,
            ALPN_SYNC,
            &WireRequest::PushGrantOps {
                root: conversation.clone(),
                batch,
            },
        )
        .await
        .expect("push");
    match applied {
        WireResponse::Applied { applied } => assert!(applied > 0, "nothing applied"),
        other => panic!("expected Applied, got {other:?}"),
    }

    let messages: Vec<String> = store::replica::records_in_root(&b.store.pool, &conversation)
        .await
        .expect("records in root");
    assert!(
        !messages.is_empty(),
        "B must hold the conversation it accepted"
    );

    serving.abort();
}

#[tokio::test]
async fn a_grant_channel_cannot_touch_a_record_outside_its_root() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let b_wire = Wire::bind(b.clone(), secret(34), Reach::Local)
        .await
        .expect("b binds");
    let a_wire = Wire::bind(a.clone(), secret(35), Reach::Local)
        .await
        .expect("a binds");
    full_contact(&b, &a_organ, &a_wire.node_id().to_string()).await;

    let victim = store::records::create(
        &b.store.pool,
        NewRecord {
            slug: Some("b-own-note"),
            kind: RecordKind::Plain,
            head: "B's note",
            body: "not shared with A",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;

    let conversation = "r-conv-test";
    store::replica::offer(&b.store.pool, conversation, &a_organ)
        .await
        .expect("offer");
    store::replica::accept(&b.store.pool, conversation, &a_organ)
        .await
        .expect("accept");

    let forged = OpBatch {
        from_organ: a_organ.clone(),
        ops: vec![engine::sync::WireOp {
            tbl: "record".into(),
            uid: victim.clone(),
            field: "body".into(),
            kind: "set".into(),
            value: Some("\"overwritten\"".into()),
            hlc: nucleus::hlc::next(),
            actor_cell: a_organ.clone(),
            organ_uid: a_organ.clone(),
            fact: None,
        }],
    };

    let result = b.import_grant_batch(conversation, &forged).await;
    assert!(
        result.is_err(),
        "a grant channel must not reach a record outside its root"
    );
    let row = store::records::get(&b.store.pool, &victim)
        .await
        .expect("get")
        .expect("still there");
    assert_eq!(row.body, "not shared with A", "the record was rewritten");

    let _ = (a_wire, b_wire, b_organ);
}

#[tokio::test]
async fn revoking_a_grant_stops_further_ops() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, _b_organ) = cell("http://b.test").await;

    let conversation = "r-conv-revoke";
    store::replica::offer(&b.store.pool, conversation, &a_organ)
        .await
        .expect("offer");
    store::replica::accept(&b.store.pool, conversation, &a_organ)
        .await
        .expect("accept");
    assert!(
        store::replica::is_accepted(&b.store.pool, conversation, &a_organ)
            .await
            .expect("state")
    );

    b.revoke_conversation(conversation, &a_organ)
        .await
        .expect("revoke");

    let batch = OpBatch {
        from_organ: a_organ.clone(),
        ops: vec![engine::sync::WireOp {
            tbl: "record".into(),
            uid: "r-new-message".into(),
            field: "body".into(),
            kind: "set".into(),
            value: Some("\"still talking\"".into()),
            hlc: nucleus::hlc::next(),
            actor_cell: a_organ.clone(),
            organ_uid: a_organ,
            fact: None,
        }],
    };
    assert!(
        b.import_grant_batch(conversation, &batch).await.is_err(),
        "a revoked grant must refuse ops at the door"
    );
    let _ = a;
}

#[tokio::test]
async fn assertions_cannot_cross_a_replica_boundary() {
    let (a, _) = cell("http://a.test").await;
    let (_conversation, thread) = a
        .start_conversation("o-friend", "Private")
        .await
        .expect("conversation");
    let public = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("public"),
            kind: RecordKind::Plain,
            head: "Public",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;

    let predicate = store::concepts::ensure(&a.store.pool, "mentions")
        .await
        .expect("predicate");
    let result = store::assertions::assert(
        &a.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &public,
            predicate_uid: &predicate,
            object_uid: Some(&thread),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await;
    assert!(
        result.is_err(),
        "a general-feed record must not be linked to a private one"
    );
}

#[tokio::test]
async fn a_message_sent_while_they_were_offline_arrives_when_they_return() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(51), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(52), Reach::Local)
        .await
        .expect("b binds");

    full_contact(&a, &b_organ, &b_wire.node_id().to_string()).await;
    full_contact(&b, &a_organ, &a_wire.node_id().to_string()).await;
    a_wire.remember_addr(loopback(&b_wire));

    let (conversation, thread) = a
        .start_conversation(&b_organ, "Beach plans")
        .await
        .expect("conversation");
    store::replica::offer(&b.store.pool, &conversation, &a_organ)
        .await
        .expect("offer");
    b.accept_conversation(&conversation, &a_organ)
        .await
        .expect("accept");
    store::replica::accept(&a.store.pool, &conversation, &b_organ)
        .await
        .expect("a learns of the acceptance");

    a.send_message(&thread, "me", "meet at six")
        .await
        .expect("message");
    let delivered = a_wire.sync_once().await.expect("a pass runs even offline");
    assert_eq!(
        delivered, 0,
        "nothing could be delivered to a closed laptop"
    );
    assert!(
        store::records::get(&b.store.pool, &thread)
            .await
            .expect("get")
            .is_none(),
        "and B has nothing yet"
    );

    let serving = {
        let b_wire = b_wire.clone();
        tokio::spawn(async move { b_wire.serve().await })
    };
    a_wire.sync_once().await.expect("the next pass delivers");

    let messages: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM record WHERE kind = 'message' AND replica_root = ?",
    )
    .bind(&conversation)
    .fetch_one(&b.store.pool)
    .await
    .expect("count");
    assert_eq!(
        messages, 1,
        "the queued message arrived on the next successful connection — no \
         announce protocol needed, the retry IS the delivery"
    );

    serving.abort();
}

#[tokio::test]
async fn a_conversation_orders_by_hlc_not_by_a_machines_clock() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(61), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(62), Reach::Local)
        .await
        .expect("b binds");
    full_contact(&a, &b_organ, &b_wire.node_id().to_string()).await;
    full_contact(&b, &a_organ, &a_wire.node_id().to_string()).await;
    a_wire.remember_addr(loopback(&b_wire));

    let (conversation, thread) = a
        .start_conversation(&b_organ, "Beach plans")
        .await
        .expect("conversation");
    store::replica::offer(&b.store.pool, &conversation, &a_organ)
        .await
        .expect("offer");
    b.accept_conversation(&conversation, &a_organ)
        .await
        .expect("accept");
    store::replica::accept(&a.store.pool, &conversation, &b_organ)
        .await
        .expect("a learns of the acceptance");

    let first = a.send_message(&thread, "me", "one").await.expect("first");
    let second = a.send_message(&thread, "me", "two").await.expect("second");

    let serving = {
        let b_wire = b_wire.clone();
        tokio::spawn(async move { b_wire.serve().await })
    };
    for _ in 0..5 {
        if a_wire.sync_once().await.expect("deliver") == 0 {
            break;
        }
    }

    store::sqlx::query("UPDATE record SET created_at = ? WHERE uid = ?")
        .bind("1999-01-01T00:00:00Z")
        .bind(&second)
        .execute(&b.store.pool)
        .await
        .expect("skew the clock");

    let protein: protein::Protein = serde_json::from_value(serde_json::json!({
        "source": "record",
        "where": [{ "kind_eq": "message" }],
        "order": [{ "asc": "created_hlc" }],
    }))
    .expect("protein");
    let rows = protein::execute(&b.store, &protein).await.expect("execute");
    let order: Vec<&str> = rows.iter().map(|r| r["uid"].as_str().unwrap()).collect();

    assert_eq!(
        order,
        vec![first.as_str(), second.as_str()],
        "the conversation must read in the order it was WRITTEN, whatever the \
         receiving machine believes the time to be"
    );
    assert!(
        rows[0]["created_hlc"].is_i64(),
        "an imported record carries the origin's stamp, not a fresh local one — \
         otherwise order would just be arrival order, which is the same bug"
    );

    serving.abort();
}

#[tokio::test]
async fn a_contact_carries_the_conversations_shared_with_them() {
    use protein::{Include, Predicate, Protein, Source};

    let (a, _) = cell("http://a.test").await;
    let friend = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Organ,
            head: "Friend",
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .expect("their organ record")
    .uid;
    let stranger = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Organ,
            head: "Stranger",
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .expect("another organ record")
    .uid;

    let organs = |engine: Arc<Engine>| async move {
        protein::execute(
            &engine.store,
            &Protein {
                source: Source::Record,
                filter: vec![Predicate::KindEq("organ".to_string())],
                fields: None,
                include: Include {
                    conversations: true,
                    ..Include::default()
                },
                aggregate: None,
                order: Vec::new(),
                limit: None,
            },
        )
        .await
        .expect("organ rows")
    };

    let before = organs(a.clone()).await;
    for row in &before {
        assert_eq!(
            row["conversations"].as_array().map(Vec::len),
            Some(0),
            "nobody is talking yet"
        );
    }

    let (conversation, _) = a
        .start_conversation(&friend, "Beach plans")
        .await
        .expect("conversation");

    let after = organs(a.clone()).await;
    let theirs = after
        .iter()
        .find(|row| row["uid"] == serde_json::json!(friend))
        .expect("their row");
    let shared = theirs["conversations"].as_array().expect("array");
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0]["uid"], serde_json::json!(conversation));
    assert_eq!(shared[0]["head"], serde_json::json!("Beach plans"));
    assert_eq!(
        shared[0]["state"],
        serde_json::json!("offered"),
        "an unanswered offer still means a conversation exists"
    );

    let others = after
        .iter()
        .find(|row| row["uid"] == serde_json::json!(stranger))
        .expect("the other row");
    assert_eq!(others["conversations"].as_array().map(Vec::len), Some(0));

    let note = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Shared note",
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .expect("note")
    .uid;
    store::replica::make_own_root(&a.store.pool, &note)
        .await
        .expect("own root");
    store::replica::offer(&a.store.pool, &note, &friend)
        .await
        .expect("offer the note");
    let with_note = organs(a.clone()).await;
    let theirs = with_note
        .iter()
        .find(|row| row["uid"] == serde_json::json!(friend))
        .expect("their row");
    assert_eq!(
        theirs["conversations"].as_array().map(Vec::len),
        Some(1),
        "a granted note is not a conversation"
    );
}

#[tokio::test]
async fn a_hidden_record_is_absent_from_a_pulled_feed() {
    let (a, _a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(40), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(41), Reach::Local)
        .await
        .expect("b binds");
    full_contact(&a, &b_organ, &b_wire.node_id().to_string()).await;

    for (slug, head) in [("shared-note", "Shared"), ("hidden-note", "Hidden")] {
        store::records::create(
            &a.store.pool,
            NewRecord {
                slug: Some(slug),
                kind: RecordKind::Plain,
                head,
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await
        .expect("record");
    }
    let hidden_uid = store::records::resolve(&a.store.pool, "hidden-note")
        .await
        .expect("resolve")
        .expect("record")
        .uid;
    store::visibility::set_hidden_from_organ(&a.store.pool, &b_organ, &hidden_uid, true)
        .await
        .expect("hide");

    let a_addr = loopback(&a_wire);
    let _ = loopback(&b_wire);
    let a_serving = tokio::spawn(async move { a_wire.serve().await });
    let response = b_wire
        .request(
            a_addr,
            ALPN_SYNC,
            &WireRequest::FetchOpsSince {
                vector: Vec::new(),
                limit: 2000,
            },
        )
        .await
        .expect("fetch");
    let ops = match response {
        WireResponse::Ops { ops, .. } => ops,
        other => panic!("expected Ops, got {other:?}"),
    };

    assert!(
        ops.iter()
            .any(|op| op.value.as_deref() == Some("\"Shared\"")),
        "the ordinary feed must still work, or this test proves nothing"
    );
    assert!(
        ops.iter().all(|op| op.uid != hidden_uid),
        "not one op of a hidden Record may be served"
    );
    assert!(
        ops.iter()
            .all(|op| op.value.as_deref() != Some("\"Hidden\"")),
        "and its contents least of all"
    );

    a_serving.abort();
}

#[tokio::test]
async fn a_reference_read_goes_through_the_visibility_gate() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(50), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(51), Reach::Local)
        .await
        .expect("b binds");
    full_contact(&a, &b_organ, &b_wire.node_id().to_string()).await;
    full_contact(&b, &a_organ, &a_wire.node_id().to_string()).await;

    let (conversation, thread) = a
        .start_conversation(&b_organ, "About the plan")
        .await
        .expect("conversation");
    store::replica::accept(&a.store.pool, &conversation, &b_organ)
        .await
        .expect("accept");
    let mentioned = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("the-plan"),
            kind: RecordKind::Plain,
            head: "The plan",
            body: "meet at six",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    let unmentioned = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("private-note"),
            kind: RecordKind::Plain,
            head: "Private",
            body: "not for B",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    a.act(
        engine::actions::Action::CreateMessage {
            thread: thread.clone(),
            body: "see this".into(),
            author: None,
            state: nucleus::MessageState::Finished,
            parent: None,
            references: vec![mentioned.clone()],
        },
        None,
    )
    .await
    .expect("message with a reference");

    let a_addr = loopback(&a_wire);
    let _ = loopback(&b_wire);
    let a_serving = tokio::spawn(async move { a_wire.serve().await });

    let response = b_wire
        .request(
            a_addr.clone(),
            ALPN_THREAD,
            &WireRequest::FetchReference {
                root: conversation.clone(),
                record: mentioned.clone(),
            },
        )
        .await
        .expect("fetch");
    match response {
        WireResponse::Reference { row } => {
            assert_eq!(
                row["head"], "The plan",
                "the pointer resolves to the record"
            );
        }
        other => panic!("expected the referenced record, got {other:?}"),
    }

    let response = b_wire
        .request(
            a_addr.clone(),
            ALPN_THREAD,
            &WireRequest::FetchReference {
                root: conversation.clone(),
                record: unmentioned.clone(),
            },
        )
        .await
        .expect("fetch");
    assert!(
        matches!(&response, WireResponse::Refused { code, .. } if code == "not_shared"),
        "an unmentioned record must not be readable: {response:?}"
    );

    store::visibility::set_hidden_from_organ(&a.store.pool, &b_organ, &mentioned, true)
        .await
        .expect("hide");
    let response = b_wire
        .request(
            a_addr.clone(),
            ALPN_THREAD,
            &WireRequest::FetchReference {
                root: conversation.clone(),
                record: mentioned.clone(),
            },
        )
        .await
        .expect("fetch");
    assert!(
        matches!(&response, WireResponse::Refused { code, .. } if code == "not_shared"),
        "revocation is REAL here: the next read fails, because there was never a copy"
    );

    a_serving.abort();
}

#[tokio::test]
async fn a_reference_read_is_narrowed_by_the_contacts_scope() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(52), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(53), Reach::Local)
        .await
        .expect("b binds");
    full_contact(&a, &b_organ, &b_wire.node_id().to_string()).await;
    full_contact(&b, &a_organ, &a_wire.node_id().to_string()).await;
    store::organs::set_contact_scope(
        &a.store.pool,
        &b_organ,
        Some(&["head".to_string(), "body".to_string()]),
    )
    .await
    .expect("scope");

    let (conversation, thread) = a
        .start_conversation(&b_organ, "Narrowed")
        .await
        .expect("conversation");
    store::replica::accept(&a.store.pool, &conversation, &b_organ)
        .await
        .expect("accept");
    let mentioned = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("narrowed-record"),
            kind: RecordKind::Plain,
            head: "Visible head",
            body: "visible body",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    a.act(
        engine::actions::Action::CreateMessage {
            thread: thread.clone(),
            body: "look".into(),
            author: None,
            state: nucleus::MessageState::Finished,
            parent: None,
            references: vec![mentioned.clone()],
        },
        None,
    )
    .await
    .expect("message");

    let a_addr = loopback(&a_wire);
    let _ = loopback(&b_wire);
    let a_serving = tokio::spawn(async move { a_wire.serve().await });

    let response = b_wire
        .request(
            a_addr,
            ALPN_THREAD,
            &WireRequest::FetchReference {
                root: conversation,
                record: mentioned,
            },
        )
        .await
        .expect("fetch");
    match response {
        WireResponse::Reference { row } => {
            assert_eq!(row["head"], "Visible head");
            assert!(
                row.get("quantity").is_none(),
                "a withheld column is ABSENT, not blank — a surface drawing \
                 `null` as zero would draw a permission boundary as data: {row}"
            );
            assert!(
                row.get("uid").is_some(),
                "the identifying columns always survive, or the answer is useless"
            );
        }
        other => panic!("expected the referenced record, got {other:?}"),
    }

    a_serving.abort();
}

#[tokio::test]
async fn a_message_may_mention_an_ordinary_record_but_not_the_reverse() {
    let (a, _) = cell("http://a.test").await;
    let (conversation, thread) = a
        .start_conversation("o-friend", "Private")
        .await
        .expect("conversation");
    let (other_conversation, other_thread) = a
        .start_conversation("o-someone-else", "Also private")
        .await
        .expect("second conversation");
    let public = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("ordinary"),
            kind: RecordKind::Plain,
            head: "Ordinary",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    let predicate = store::concepts::ensure(&a.store.pool, "mentions")
        .await
        .expect("predicate");
    let link = |subject: String, object: String| {
        let pool = a.store.pool.clone();
        let predicate = predicate.clone();
        async move {
            store::assertions::assert(
                &pool,
                store::assertions::NewAssertion {
                    subject_uid: &subject,
                    predicate_uid: &predicate,
                    object_uid: Some(&object),
                    role: store::assertions::AssertionRole::Ordinary,
                    quantity: None,
                    unit_uid: None,
                    asserted_by: None,
                },
            )
            .await
        }
    };

    let allowed = link(thread.clone(), public.clone()).await;
    assert!(
        allowed.is_ok(),
        "a message must be able to mention an ordinary record: {allowed:?}"
    );
    let root = store::replica::root_of(&a.store.pool, &thread)
        .await
        .expect("root");
    assert_eq!(
        root,
        Some(conversation.clone()),
        "and mentioning must not drag the message out of its conversation"
    );
    assert_eq!(
        store::replica::root_of(&a.store.pool, &public)
            .await
            .expect("root"),
        None,
        "nor pull the mentioned record INTO it"
    );

    assert!(
        link(public.clone(), thread.clone()).await.is_err(),
        "a private uid must not reach the general feed"
    );

    assert!(
        link(thread, other_thread).await.is_err(),
        "two conversations must not be joined by a link"
    );
    let _ = other_conversation;
}

#[tokio::test]
async fn an_offer_alone_moves_nothing_until_it_is_accepted() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let (conversation, thread) = a
        .start_conversation(&b_organ, "Not yet agreed")
        .await
        .expect("conversation");
    store::organs::add_contact(&a.store.pool, &b_organ, None, "B", "", 0)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&a.store.pool, &b_organ, true, true)
        .await
        .expect("policy");
    store::replica::offer(&a.store.pool, &conversation, &b_organ)
        .await
        .expect("offer");
    let message = a
        .send_message(&thread, "me", "sent before they agreed")
        .await
        .expect("message");

    assert!(
        store::sync_ops::outbox_due(&a.store.pool)
            .await
            .expect("outbox")
            .iter()
            .all(|row| row.uid != message),
        "nothing may be queued for a conversation nobody has accepted"
    );

    let forced = b
        .import_grant_batch(
            &conversation,
            &OpBatch {
                from_organ: a_organ,
                ops: Vec::new(),
            },
        )
        .await;
    assert!(
        forced.is_err(),
        "a grant channel into a conversation B never accepted must be refused"
    );
    assert!(
        store::records::get(&b.store.pool, &message)
            .await
            .expect("get")
            .is_none(),
        "and B holds no copy of a conversation they never accepted"
    );
}

#[tokio::test]
async fn reading_a_reference_leaves_a_receipt_but_a_refusal_does_not() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(54), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(55), Reach::Local)
        .await
        .expect("b binds");
    full_contact(&a, &b_organ, &b_wire.node_id().to_string()).await;
    full_contact(&b, &a_organ, &a_wire.node_id().to_string()).await;

    let (conversation, thread) = a
        .start_conversation(&b_organ, "Receipts")
        .await
        .expect("conversation");
    store::replica::accept(&a.store.pool, &conversation, &b_organ)
        .await
        .expect("accept");
    let mentioned = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("read-me"),
            kind: RecordKind::Plain,
            head: "Read me",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    let never_mentioned = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("do-not-read-me"),
            kind: RecordKind::Plain,
            head: "Private",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    a.act(
        engine::actions::Action::CreateMessage {
            thread: thread.clone(),
            body: "here".into(),
            author: None,
            state: nucleus::MessageState::Finished,
            parent: None,
            references: vec![mentioned.clone()],
        },
        None,
    )
    .await
    .expect("message");

    let a_addr = loopback(&a_wire);
    let _ = loopback(&b_wire);
    let a_serving = tokio::spawn(async move { a_wire.serve().await });

    assert!(
        store::replica::reference_reads(&a.store.pool, &mentioned)
            .await
            .expect("reads")
            .is_empty(),
        "nothing is recorded before anyone reads"
    );

    for _ in 0..2 {
        let response = b_wire
            .request(
                a_addr.clone(),
                ALPN_THREAD,
                &WireRequest::FetchReference {
                    root: conversation.clone(),
                    record: mentioned.clone(),
                },
            )
            .await
            .expect("fetch");
        assert!(matches!(response, WireResponse::Reference { .. }));
    }

    let reads = store::replica::reference_reads(&a.store.pool, &mentioned)
        .await
        .expect("reads");
    assert_eq!(reads.len(), 1, "one row per reader, not one per read");
    assert_eq!(
        reads[0].0, b_organ,
        "and it names the Organ, never the Cell"
    );
    assert_eq!(
        reads[0].1, 2,
        "counted, so an open tab is not a surveillance log"
    );

    let refused = b_wire
        .request(
            a_addr,
            ALPN_THREAD,
            &WireRequest::FetchReference {
                root: conversation,
                record: never_mentioned.clone(),
            },
        )
        .await
        .expect("fetch");
    assert!(matches!(refused, WireResponse::Refused { .. }));
    assert!(
        store::replica::reference_reads(&a.store.pool, &never_mentioned)
            .await
            .expect("reads")
            .is_empty(),
        "a refused attempt leaves no trace"
    );

    a_serving.abort();
}

#[tokio::test]
async fn a_copy_is_a_new_record_inside_the_conversation() {
    let (a, _a_organ) = cell("http://a.test").await;
    let (conversation, thread) = a
        .start_conversation("o-friend", "Working on it")
        .await
        .expect("conversation");
    let source = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("the-draft"),
            kind: RecordKind::Plain,
            head: "The draft",
            body: "first version",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;

    let copy = a
        .act(
            engine::actions::Action::SendRecordCopy {
                thread: thread.clone(),
                record: source.clone(),
            },
            None,
        )
        .await
        .expect("copy")
        .created
        .expect("the copy's uid");

    assert_ne!(
        copy, source,
        "a copy must be its own Record — sharing the uid would make later edits \
         flow back through the grant channel, which is a shared document, not a copy"
    );
    assert_eq!(
        store::replica::root_of(&a.store.pool, &copy)
            .await
            .expect("root"),
        Some(conversation),
        "the copy lives inside the conversation, which is what makes it travel"
    );
    assert_eq!(
        store::replica::root_of(&a.store.pool, &source)
            .await
            .expect("root"),
        None,
        "and the original is untouched, still on the general feed"
    );
    let copied = store::records::get(&a.store.pool, &copy)
        .await
        .expect("get")
        .expect("the copy");
    assert_eq!(copied.head, "The draft");
    assert_eq!(copied.body, "first version");
    assert_eq!(
        copied.slug, None,
        "the slug is a local suggestion and must not collide with the original"
    );

    a.act(
        engine::actions::Action::EditRecordText {
            target: source.clone(),
            head: None,
            body: Some("second version".into()),
        },
        None,
    )
    .await
    .expect("edit");
    assert_eq!(
        store::records::get(&a.store.pool, &copy)
            .await
            .expect("get")
            .expect("the copy")
            .body,
        "first version",
        "a copy is a moment, not a window"
    );
}

#[tokio::test]
async fn a_copy_cannot_move_a_record_between_conversations() {
    let (a, _a_organ) = cell("http://a.test").await;
    let (_first, first_thread) = a
        .start_conversation("o-friend", "One")
        .await
        .expect("conversation");
    let (_second, second_thread) = a
        .start_conversation("o-someone-else", "Two")
        .await
        .expect("conversation");
    let message = a
        .send_message(&first_thread, "me", "private to the first")
        .await
        .expect("message");

    let result = a
        .act(
            engine::actions::Action::SendRecordCopy {
                thread: second_thread,
                record: message,
            },
            None,
        )
        .await;
    assert!(
        result.is_err(),
        "copying across conversations would widen both, exactly as a link would"
    );
}

#[tokio::test]
async fn deleting_a_conversation_is_local_and_emits_no_tombstone() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let (conversation, thread) = a
        .start_conversation(&b_organ, "Ending this")
        .await
        .expect("conversation");
    store::organs::add_contact(&a.store.pool, &b_organ, None, "B", "", 0)
        .await
        .expect("contact");
    store::replica::accept(&a.store.pool, &conversation, &b_organ)
        .await
        .expect("accept");
    let message = a
        .send_message(&thread, "me", "before the end")
        .await
        .expect("message");

    let tombstones_before = store::sync_ops::after(&a.store.pool, &a_organ, 0, 10_000)
        .await
        .expect("ops")
        .into_iter()
        .filter(|op| op.kind == "tombstone")
        .count();

    a.act(
        engine::actions::Action::DeleteConversation {
            conversation: thread.clone(),
        },
        None,
    )
    .await
    .expect("delete");

    for uid in [&conversation, &thread, &message] {
        assert!(
            store::records::get(&a.store.pool, uid)
                .await
                .expect("get")
                .is_none(),
            "the conversation, its threads and its messages all go"
        );
    }
    assert!(
        !store::replica::is_accepted(&a.store.pool, &conversation, &b_organ)
            .await
            .expect("grant"),
        "and their grant goes with it, or their ops would still be accepted"
    );
    assert_eq!(
        store::sync_ops::after(&a.store.pool, &a_organ, 0, 10_000)
            .await
            .expect("ops")
            .into_iter()
            .filter(|op| op.kind == "tombstone")
            .count(),
        tombstones_before,
        "NO tombstone: it is a synced op kind, so one here would delete their \
         copy too — and their copy is theirs"
    );

    let first = store::invites::put(&a.store.pool, &b_organ, "r-new-root", "Try again")
        .await
        .expect("invite");
    assert!(first.is_some(), "a new invite may arrive after a deletion");
    let second = store::invites::put(&a.store.pool, &b_organ, "r-another", "And again")
        .await
        .expect("invite");
    assert!(
        second.is_none(),
        "but only one pending at a time, or deletion buys nothing"
    );
    let _ = b;
}

#[tokio::test]
async fn a_move_inside_a_conversation_never_reaches_the_general_feed() {
    let (a, a_organ) = cell("http://a.test").await;
    full_contact(&a, "o-watcher", "node-watcher").await;

    store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("public-note"),
            kind: RecordKind::Plain,
            head: "Public",
            body: "ordinary feed",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    let (conversation, thread) = a
        .start_conversation("o-someone-else", "Private")
        .await
        .expect("conversation");
    let message = a
        .send_message(&thread, "me", "this must never reach the feed")
        .await
        .expect("message");

    store::concepts::ensure(&a.store.pool, "done")
        .await
        .expect("concept");
    a.act(
        engine::actions::Action::TransitionRecord {
            subject: message.clone(),
            retract: Vec::new(),
            assert: vec!["done".to_string()],
            quantity: None,
        },
        None,
    )
    .await
    .expect("move");

    let inside: Vec<String> = vec![conversation.clone(), thread.clone(), message.clone()];
    let assertions = store::assertions::for_subjects(&a.store.pool, &inside)
        .await
        .expect("assertions");
    assert!(
        !assertions.is_empty(),
        "a conversation is made of assertions — without any, this test proves nothing"
    );

    let feed = store::sync_ops::after(&a.store.pool, &a_organ, 0, 1000)
        .await
        .expect("feed");
    assert!(
        feed.iter().any(|op| op.tbl == "record"),
        "the ordinary record still rides the general feed"
    );
    for op in &feed {
        assert!(
            !inside.contains(&op.uid),
            "a conversation Record reached the general feed: {op:?}"
        );
        assert!(
            !assertions.iter().any(|a| a.uid == op.uid),
            "a conversation Assertion reached the general feed: {op:?}"
        );
    }

    let queued = store::sync_ops::outbox_due(&a.store.pool)
        .await
        .expect("outbox");
    for row in &queued {
        if row.contact_organ != "o-watcher" {
            continue;
        }
        assert!(
            !inside.contains(&row.uid) && !assertions.iter().any(|a| a.uid == row.uid),
            "queued to an ungranted contact: {row:?}"
        );
    }
}
