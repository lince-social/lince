//! Individual replica (Ontology §11 "Threads"): per-record-per-contact sync,
//! and the three enforcement points that must agree — outbox enqueue,
//! feed-serve, and the import gate.
//!
//! The most important test here is a NEGATIVE one: a contact with full
//! `sync_out`/`sync_in` and no grant must receive NOTHING from a conversation.
//! Every other test is a happy path, and a happy path cannot catch a leak.

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

/// A full-trust contact: `sync_out` and `sync_in` both on. Precisely the peer
/// that must still see nothing without a grant.
async fn full_contact(us: &Engine, organ_uid: &str, node_id: &str) {
    store::organs::add_contact(&us.store.pool, organ_uid, None, "peer", "", 0)
        .await
        .expect("contact");
    store::organs::set_node_id(&us.store.pool, organ_uid, Some(node_id))
        .await
        .expect("node id");
    // Explicit: recording a contact does not trust them, and `known` is what
    // opens the sync ALPN. A helper called "full contact" has to say so.
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

    // A second thread needs no new grant — that is the whole reason to nest.
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

/// THE load-bearing test. A contact with full sync and no grant must receive
/// nothing from a conversation — not through the outbox, and not through a
/// catch-up fetch either.
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

    // A ordinary record: this one SHOULD travel, so the test proves the feed
    // works and is not silently empty for some unrelated reason.
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

    // A conversation with a THIRD party, never granted to B.
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

    // B pulls A's whole general feed, exactly as the sync runner would.
    let a_serving = tokio::spawn(async move { a_wire.serve().await });
    let response = b_wire
        .request(
            a_addr,
            ALPN_SYNC,
            &WireRequest::FetchOpsSince {
                // An empty vector is "I have nothing of yours", which is what a
                // from-zero pull IS — no special case, no cursor.
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

    // The public record is in the feed; nothing from the conversation is.
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

    // And importing that feed must not materialise the message either.
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

/// The happy path: offer, accept, then ops flow — and only then.
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

    // Before acceptance, pushing must be refused.
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

    // B is offered the conversation and accepts it.
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

    // The message landed, and landed INSIDE the root on B's side too — the
    // root came from the channel, so B's copy is scoped the same way.
    let messages: Vec<String> = store::replica::records_in_root(&b.store.pool, &conversation)
        .await
        .expect("records in root");
    assert!(
        !messages.is_empty(),
        "B must hold the conversation it accepted"
    );

    serving.abort();
}

/// A grantee must not be able to re-scope one of our general-feed Records by
/// naming it through their grant channel.
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

    // B has an ordinary Record of its own, on the general feed.
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

    // B grants A a conversation.
    let conversation = "r-conv-test";
    store::replica::offer(&b.store.pool, conversation, &a_organ)
        .await
        .expect("offer");
    store::replica::accept(&b.store.pool, conversation, &a_organ)
        .await
        .expect("accept");

    // A pushes, through that grant, an op aimed at B's unrelated Record.
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

/// Revoking stops reach without touching their copy.
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

/// An Assertion may not join a private Record to a general-feed one — its op
/// takes the subject's root, so the link would carry the private uid onto the
/// general feed.
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

/// The beach case: you meet, exchange keys, they go home and close the laptop.
/// You send anyway. The message must arrive when they next come online, or the
/// whole exchange was pointless.
///
/// No announce protocol is involved, and that is the point of this test —
/// sending to an unreachable peer leaves the ops QUEUED, and the ordinary sync
/// pass delivers them once a connection succeeds. If this ever stops being
/// true, offline sending is silently broken in the one case it exists for.
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
    // What B's `AcceptGrant` does to A when it arrives. Enqueue reads the
    // grant state on the SENDER, so without this A queues nothing and the
    // test would pass or fail for the wrong reason.
    store::replica::accept(&a.store.pool, &conversation, &b_organ)
        .await
        .expect("a learns of the acceptance");

    // B's laptop is shut: nobody is serving. The send still succeeds locally.
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

    // They come home and open it.
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

/// Message order must not depend on whose clock is right.
///
/// `created_at` is local wall time, so a peer whose clock is slow would have
/// its messages sort into the past forever. The op log already carries an HLC
/// so ordering never rests on a clock; `record.created_hlc` is that stamp
/// denormalized onto the row where a read can reach it.
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
    // Sync to quiescence rather than once: a pass drains a bounded slice of
    // the outbox, and the runner loops. Ordering is the subject here, so the
    // test must not depend on how much fits in one pass.
    for _ in 0..5 {
        if a_wire.sync_once().await.expect("deliver") == 0 {
            break;
        }
    }

    // B's clock is WRONG — badly, and in the direction that would reverse the
    // conversation if `created_at` decided the order.
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

/// The one question a contact panel has to answer before offering: is there
/// already a conversation with this person? A grant is neither a link nor a
/// Fact, so `include.conversations` is the only way to ask — and it must count
/// an OFFER, not just an accepted one. Re-offering a conversation they have
/// not answered yet is exactly the mistake, and it mints a second one beside
/// the pending first.
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

    // And it belongs to THEM. The grant is keyed by the contact's organ uid;
    // reading it the other way round would give every contact the same list.
    let others = after
        .iter()
        .find(|row| row["uid"] == serde_json::json!(stranger))
        .expect("the other row");
    assert_eq!(others["conversations"].as_array().map(Vec::len), Some(0));

    // A root that is not a conversation must not read as one.
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

/// Per-record hiding on the PULL path (Ontology §12, C5).
///
/// The push path has its own test in `organ_sync.rs`, and the two exist
/// separately on purpose: the field scope shipped covering pull alone and the
/// gap survived a green suite, because every test that could have caught it
/// drove the other path. A filter on the general feed needs one test per way
/// out of it.
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

/// A live reference resolves THROUGH the §12 gate, never around it
/// (Ontology §11, C6).
///
/// The load-bearing test is the negative one, as it was for grants: mentioning
/// a Record in a thread must not become a way to serve something the ordinary
/// feed is withholding, and a grantee must not be able to name any uid they
/// like and get an answer.
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

    // A conversation A shares with B, and a Record A mentions in it.
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

    // The reference resolves, live, and carries the Record.
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
            assert_eq!(row["head"], "The plan", "the pointer resolves to the record");
        }
        other => panic!("expected the referenced record, got {other:?}"),
    }

    // A uid that was NEVER mentioned is refused, however good the grant is.
    // Without this the conversation is a read oracle for the whole Cell.
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

    // Hiding the mentioned Record from B closes the reference too — the gate
    // is the SAME gate, not a second more permissive path to the same row.
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

/// The per-contact scope narrows a reference read exactly as it narrows the
/// feed — one selector language, doing both jobs — and a withheld column comes
/// back ABSENT rather than present and blank.
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
    store::organs::set_contact_scope(&a.store.pool, &b_organ, Some(&["head".to_string(), "body".to_string()]))
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

/// The boundary rule was written as "same root or refuse", which refused three
/// cases when only two are leaks — and the third is how a reference is
/// expressed at all. This pins all three, because the fix is a narrowing of a
/// safety rule and the next person needs to see exactly how far it went.
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

    // ALLOWED: private subject, general-feed object. The op takes the
    // subject's root, so it reaches only that conversation's grant holders.
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

    // REFUSED: general-feed subject, private object — the op would ride the
    // general feed carrying a private uid.
    assert!(
        link(public.clone(), thread.clone()).await.is_err(),
        "a private uid must not reach the general feed"
    );

    // REFUSED: two different conversations — joining them widens both.
    assert!(
        link(thread, other_thread).await.is_err(),
        "two conversations must not be joined by a link"
    );
    let _ = other_conversation;
}

/// Consent from BOTH parties: the sharer offers, the receiver accepts, and
/// only then does anything land. An offer on its own must move no ops in
/// either direction — otherwise announcing a conversation is a way to push
/// Records into somebody's store.
///
/// The gate exists on both sides and this checks both, because either one
/// alone would look like it worked: the outbox only fans out to `accepted`
/// grants, and `import_grant_batch` refuses a channel without one.
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

    // Outbound: an offered-but-unaccepted grant is not a delivery target.
    assert!(
        store::sync_ops::outbox_due(&a.store.pool)
            .await
            .expect("outbox")
            .iter()
            .all(|row| row.uid != message),
        "nothing may be queued for a conversation nobody has accepted"
    );

    // Inbound: and if it arrived anyway, the receiver refuses the channel.
    // The root is taken from the CHANNEL, never the payload — and B has
    // agreed to nothing, so the channel itself is refused before any op is
    // looked at.
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

/// Reading a reference is a read receipt to its owner (Ontology §11, C6).
///
/// The read is a live request against their Cell, so it is observable whether
/// or not anyone records it. Recording it is what lets BOTH sides be told:
/// the reader before they read, the owner afterwards.
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
    assert_eq!(reads[0].0, b_organ, "and it names the Organ, never the Cell");
    assert_eq!(reads[0].1, 2, "counted, so an open tab is not a surveillance log");

    // A REFUSAL is not a read. Recording one would turn this into a record of
    // who attempted what, which is a different and nastier table.
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

/// "Send a copy" is a SEPARATE and irreversible act, not a variant of
/// referencing (Ontology §11, C6).
///
/// A reference is a pointer that can be taken back; a copy lands in the other
/// party's store and cannot be recalled. The mechanics have to make that real:
/// a new uid, inside the conversation, with the original untouched.
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

    // Editing the ORIGINAL afterwards must not reach the copy. This is the
    // difference from a reference stated as a test: a reference would have
    // shown the new text, a copy shows what was sent.
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

/// A Record already inside another conversation cannot be copied across.
/// Going through a copy would be the same cross-root widening `assert`
/// refuses, wearing a different verb.
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

/// Deleting a conversation ends it HERE and reaches nobody (Ontology §11, C6).
///
/// Both halves are needed and the test checks both: revoking alone leaves it
/// in the list, removing alone leaves their ops welcome so it repopulates on
/// the next sync. And no tombstone may be emitted — a tombstone is a synced op
/// kind, so it would delete THEIR copy too.
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
            // Named by a THREAD, not the root: a person deleting a
            // conversation may well have a thread selected, and deleting only
            // that would leave the conversation half-present and still syncing.
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

    // What remains is exactly one thing: they may knock again. One pending at
    // a time, which is what makes it a knock rather than a channel.
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
