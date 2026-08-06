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
use engine::wire::{ALPN_SYNC, Reach, Wire, WireRequest, WireResponse};
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
            &WireRequest::FetchOps {
                after: 0,
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
            actor_organ: a_organ.clone(),
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
            actor_organ: a_organ,
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
