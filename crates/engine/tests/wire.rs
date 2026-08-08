//! iroh transport acceptance (Ontology §11 "Transport: iroh"): two Cells bind
//! endpoints, one pushes op batches to the other over ALPN `lince/sync/1`, and
//! the accept-side gate refuses an Organ it holds no contact row for.
//!
//! Everything here runs on `Reach::Local` — no relays, no DNS, no pkarr — so
//! the test exercises the protocol without touching anyone's infrastructure.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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

/// A dialable address for an endpoint bound on this machine. `bound_sockets()`
/// reports the wildcard bind, so point the port at loopback explicitly.
fn loopback(wire: &Wire) -> EndpointAddr {
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .map(|addr| addr.port())
        .next()
        .expect("endpoint is bound");
    EndpointAddr::new(wire.node_id())
        .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
}

/// Register `them` as a KNOWN contact of `us`, reachable at `node_id`.
///
/// `set_trust` is explicit because `add_contact` deliberately does not imply
/// it: recording an address is not a decision to trust, and `known` is exactly
/// what opens the sync ALPN. A helper named `know` has to do the knowing.
async fn know(us: &Engine, organ_uid: &str, node_id: &str) {
    store::organs::add_contact(&us.store.pool, organ_uid, None, "peer", "", 0)
        .await
        .expect("contact");
    store::organs::set_node_id(&us.store.pool, organ_uid, Some(node_id))
        .await
        .expect("node id");
    store::organs::set_trust(&us.store.pool, organ_uid, "known")
        .await
        .expect("trust");
}

#[tokio::test]
async fn known_peer_pushes_ops_over_iroh() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(1), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(2), Reach::Local)
        .await
        .expect("b binds");

    // Each side knows the other by NodeId — the only routing input.
    know(&a, &b_organ, &b_wire.node_id().to_string()).await;
    know(&b, &a_organ, &a_wire.node_id().to_string()).await;

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

    // A writes something, then ships its op log to B.
    store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("shared"),
            kind: RecordKind::Plain,
            head: "Shared Note",
            body: "hello over quic",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    let (ops, _head) = a.ops_after(0, 500).await.expect("ops");
    assert!(!ops.is_empty(), "creating a record must log ops");

    let response = a_wire
        .request(
            b_addr.clone(),
            ALPN_SYNC,
            &WireRequest::PushOps {
                batch: OpBatch {
                    from_organ: a_organ.clone(),
                    ops,
                },
            },
        )
        .await
        .expect("push succeeds");

    match response {
        WireResponse::Applied { applied } => assert!(applied > 0, "B applied nothing"),
        other => panic!("expected Applied, got {other:?}"),
    }

    let landed = store::records::resolve(&b.store.pool, "shared")
        .await
        .expect("resolve");
    assert!(landed.is_some(), "the record must exist on B after sync");

    // And the introduction round-trips on the same authenticated connection.
    let response = a_wire
        .request(b_addr, ALPN_SYNC, &WireRequest::Introduction)
        .await
        .expect("introduction succeeds");
    match response {
        WireResponse::Introduction { intro } => assert_eq!(intro.organ_uid, b_organ),
        other => panic!("expected Introduction, got {other:?}"),
    }

    serving.abort();
}

#[tokio::test]
async fn unknown_node_id_cannot_reach_the_sync_protocol() {
    let (b, _b_organ) = cell("http://b.test").await;
    // A stranger: B holds no contact row bound to this NodeId.
    let (stranger, stranger_organ) = cell("http://stranger.test").await;

    let b_wire = Wire::bind(b.clone(), secret(3), Reach::Local)
        .await
        .expect("b binds");
    let s_wire = Wire::bind(stranger.clone(), secret(4), Reach::Local)
        .await
        .expect("stranger binds");

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

    store::records::create(
        &stranger.store.pool,
        NewRecord {
            slug: Some("intruder"),
            kind: RecordKind::Plain,
            head: "Intruder",
            body: "should never land",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");
    let (ops, _) = stranger.ops_after(0, 500).await.expect("ops");

    // Assert the two halves SEPARATELY, so a bug that broke dialing outright
    // cannot leave this test green by producing an indistinguishable error.
    //
    // Half one: the handshake succeeds. Anyone holding a NodeId can open a
    // connection — exactly the new surface a published key creates.
    let connection = s_wire
        .endpoint()
        .connect(b_addr.clone(), ALPN_SYNC)
        .await
        .expect("the handshake itself must succeed for this test to mean anything");

    // Half two: the GATE is what refuses, with its own reason.
    let closed = connection.closed().await.to_string();
    assert!(
        closed.contains("unknown organ"),
        "expected the accept gate's refusal, got: {closed}"
    );

    let result = s_wire
        .request(
            b_addr,
            ALPN_SYNC,
            &WireRequest::PushOps {
                batch: OpBatch {
                    from_organ: stranger_organ,
                    ops,
                },
            },
        )
        .await;

    match result {
        Err(_) => {}
        Ok(WireResponse::Error { .. }) => {}
        Ok(other) => panic!("stranger must be refused, got {other:?}"),
    }

    let landed = store::records::resolve(&b.store.pool, "intruder")
        .await
        .expect("resolve");
    assert!(landed.is_none(), "an unknown Organ must not write to B");

    serving.abort();
}

/// Having a contact ROW is not having trust. `add_contact` deliberately writes
/// `unknown`, which means the grant-checked thread door only — never the
/// general sync protocol.
#[tokio::test]
async fn a_contact_who_is_not_known_cannot_reach_the_sync_protocol() {
    let (b, _b_organ) = cell("http://b.test").await;
    let (peer, peer_organ) = cell("http://peer.test").await;

    let b_wire = Wire::bind(b.clone(), secret(5), Reach::Local)
        .await
        .expect("b binds");
    let p_wire = Wire::bind(peer.clone(), secret(6), Reach::Local)
        .await
        .expect("peer binds");

    know(&b, &peer_organ, &p_wire.node_id().to_string()).await;
    store::organs::set_trust(&b.store.pool, &peer_organ, "unknown")
        .await
        .expect("demote to unknown");

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

    let connection = p_wire
        .endpoint()
        .connect(b_addr, ALPN_SYNC)
        .await
        .expect("handshake succeeds");
    let closed = connection.closed().await.to_string();
    assert!(
        closed.contains("unknown organ"),
        "a contact row with trust='unknown' must not open sync, got: {closed}"
    );

    serving.abort();
}

/// `blocked` is terminal (Ontology §2) and is checked BEFORE the ALPN split, so
/// a blocked Organ cannot reach the thread door either.
#[tokio::test]
async fn blocked_organ_is_closed_on_every_alpn() {
    let (b, _b_organ) = cell("http://b.test").await;
    let (peer, peer_organ) = cell("http://peer.test").await;

    let b_wire = Wire::bind(b.clone(), secret(7), Reach::Local)
        .await
        .expect("b binds");
    let p_wire = Wire::bind(peer.clone(), secret(8), Reach::Local)
        .await
        .expect("peer binds");

    know(&b, &peer_organ, &p_wire.node_id().to_string()).await;
    store::organs::set_trust(&b.store.pool, &peer_organ, "blocked")
        .await
        .expect("block");

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

    for alpn in [ALPN_SYNC, engine::wire::ALPN_THREAD] {
        let connection = p_wire
            .endpoint()
            .connect(b_addr.clone(), alpn)
            .await
            .expect("handshake succeeds");
        let closed = connection.closed().await.to_string();
        assert!(
            closed.contains("blocked"),
            "blocked must be refused as blocked on {}, got: {closed}",
            String::from_utf8_lossy(alpn)
        );
    }

    serving.abort();
}

/// The thread door is CLOSED by default: publishing a NodeId advertises
/// reachability to people who already know you and grants nothing to anyone
/// else. Turning it on is what opens the invite door.
#[tokio::test]
async fn thread_door_is_closed_to_unknown_organs_by_default() {
    let (b, _b_organ) = cell("http://b.test").await;
    let (stranger, _) = cell("http://stranger.test").await;

    let b_wire = Wire::bind(b.clone(), secret(9), Reach::Local)
        .await
        .expect("b binds");
    let s_wire = Wire::bind(stranger.clone(), secret(10), Reach::Local)
        .await
        .expect("stranger binds");

    assert!(!b_wire.accept_unknown().await, "default must be closed");

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

    let connection = s_wire
        .endpoint()
        .connect(b_addr, engine::wire::ALPN_THREAD)
        .await
        .expect("handshake succeeds");
    let closed = connection.closed().await.to_string();
    assert!(
        closed.contains("not accepting unknown"),
        "the thread door must be shut by default, got: {closed}"
    );

    serving.abort();
}

/// With the door open, an unknown Organ may fetch an Introduction — that is
/// what makes pairing from a nearby list possible — and NOTHING else. It must
/// not be able to push ops.
#[tokio::test]
async fn open_thread_door_serves_introduction_but_not_general_sync() {
    let (b, b_organ) = cell("http://b.test").await;
    let (stranger, stranger_organ) = cell("http://stranger.test").await;

    store::records::set_extension(
        &b.store.pool,
        &b_organ,
        "lince.discovery",
        &serde_json::json!({ "accept_unknown": true }),
    )
    .await
    .expect("open the door");

    let b_wire = Wire::bind(b.clone(), secret(11), Reach::Local)
        .await
        .expect("b binds");
    let s_wire = Wire::bind(stranger.clone(), secret(12), Reach::Local)
        .await
        .expect("stranger binds");

    assert!(b_wire.accept_unknown().await, "the door must now be open");

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

    // Introduction is served: this is what pairing needs.
    let response = s_wire
        .request(
            b_addr.clone(),
            engine::wire::ALPN_THREAD,
            &WireRequest::Introduction,
        )
        .await
        .expect("introduction is served through the open thread door");
    match response {
        WireResponse::Introduction { intro } => assert_eq!(intro.organ_uid, b_organ),
        other => panic!("expected Introduction, got {other:?}"),
    }

    // Ops are NOT: an open invite door is not sync reach.
    store::records::create(
        &stranger.store.pool,
        NewRecord {
            slug: Some("intruder"),
            kind: RecordKind::Plain,
            head: "Intruder",
            body: "should never land",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");
    let (ops, _) = stranger.ops_after(0, 500).await.expect("ops");

    let response = s_wire
        .request(
            b_addr,
            engine::wire::ALPN_THREAD,
            &WireRequest::PushOps {
                batch: OpBatch {
                    from_organ: stranger_organ,
                    ops,
                },
            },
        )
        .await
        .expect("the call completes");
    match response {
        WireResponse::Refused { code, .. } => assert_eq!(code, "not_known"),
        other => panic!("an unknown Organ must not push ops, got {other:?}"),
    }
    assert!(
        store::records::resolve(&b.store.pool, "intruder")
            .await
            .expect("resolve")
            .is_none(),
        "nothing may land from the invite door"
    );

    serving.abort();
}

/// The LIVE path: `sync_once` is what the background runner calls, so this is
/// the test that says the iroh transport is actually wired up rather than
/// merely present. Two Cells converge with no HTTP anywhere.
#[tokio::test]
async fn sync_once_converges_two_cells() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(20), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(21), Reach::Local)
        .await
        .expect("b binds");

    know(&a, &b_organ, &b_wire.node_id().to_string()).await;
    know(&b, &a_organ, &a_wire.node_id().to_string()).await;
    store::organs::set_sync_policy(&a.store.pool, &b_organ, true, true)
        .await
        .expect("policy");
    store::organs::set_sync_policy(&b.store.pool, &a_organ, true, true)
        .await
        .expect("policy");

    a_wire.remember_addr(loopback(&b_wire));
    let serving = {
        let b_wire = b_wire.clone();
        tokio::spawn(async move { b_wire.serve().await })
    };

    store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("converged"),
            kind: RecordKind::Plain,
            head: "Converged",
            body: "over quic, no http",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    a_wire.sync_once().await.expect("sync pass");

    let landed = store::records::resolve(&b.store.pool, "converged")
        .await
        .expect("resolve");
    assert!(
        landed.is_some(),
        "a background sync pass must move the record with no HTTP involved"
    );

    serving.abort();
}

/// First contact is an individual replica, not friendship: a discovered
/// NodeId can offer one conversation, the receiver accepts it, and that root
/// syncs while both contact rows remain `unknown` and the general feed stays
/// closed.
#[tokio::test]
async fn unknown_nearby_peers_can_accept_and_sync_one_conversation() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;
    store::records::set_extension(
        &b.store.pool,
        &b_organ,
        "lince.discovery",
        &serde_json::json!({ "accept_unknown": true }),
    )
    .await
    .expect("B opens first contact");

    let a_wire = Wire::bind(a.clone(), secret(61), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(62), Reach::Local)
        .await
        .expect("b binds");
    a_wire.remember_addr(loopback(&b_wire));
    b_wire.remember_addr(loopback(&a_wire));
    let a_serving = {
        let wire = a_wire.clone();
        tokio::spawn(async move { wire.serve().await })
    };
    let b_serving = {
        let wire = b_wire.clone();
        tokio::spawn(async move { wire.serve().await })
    };

    let (conversation, thread) = a_wire
        .offer_conversation_to_node(&b_wire.node_id().to_string(), "Hello")
        .await
        .expect("unknown offer reaches B");
    let pending = store::invites::pending(&b.store.pool)
        .await
        .expect("pending invites");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].root, conversation);

    for (cell, peer) in [(&*a, &b_organ), (&*b, &a_organ)] {
        assert_eq!(
            store::organs::contact(&cell.store.pool, peer)
                .await
                .expect("contact query")
                .expect("unknown identity binding")
                .trust,
            "unknown",
            "a conversation request must not add either side as known"
        );
    }

    b_wire
        .answer_conversation_invite(&pending[0].record_uid, true)
        .await
        .expect("B accepts and acknowledges A");
    b_wire.sync_once().await.expect("B pulls accepted root");
    assert!(
        store::records::get(&b.store.pool, &conversation)
            .await
            .expect("conversation query")
            .is_some(),
        "acceptance must pull the offered conversation"
    );
    let received_thread = store::records::get(&b.store.pool, &thread)
        .await
        .expect("thread query")
        .expect("the first thread rides the conversation root");
    assert!(
        received_thread.quantity.is_positive(),
        "the first thread is active, got {:?}; quantity ops: {:?}",
        received_thread.quantity,
        store::sync_ops::for_field(&b.store.pool, "record", &thread, "quantity")
            .await
            .expect("quantity ops")
    );
    let thread_of = store::concepts::resolve(&b.store.pool, "thread-of")
        .await
        .expect("thread-of query")
        .expect("thread-of arrived");
    assert_eq!(
        store::assertions::objects_from_subject(&b.store.pool, &thread, &thread_of)
            .await
            .expect("thread relation")
            .into_iter()
            .map(|record| record.uid)
            .collect::<Vec<_>>(),
        vec![conversation.clone()],
        "the synced thread must be visible to the Record/Protein projection"
    );

    let message = a
        .act(
            engine::actions::Action::CreateMessage {
                thread: thread.clone(),
                body: "hey, I'm here".into(),
                parent: None,
                references: vec![],
            },
            None,
        )
        .await
        .expect("Record Sand message action")
        .created
        .expect("message uid");
    a_wire.sync_once().await.expect("A pushes grant ops");
    let received_message = store::records::get(&b.store.pool, &message)
        .await
        .expect("message query")
        .expect("message reached B");
    assert_eq!(received_message.body, "hey, I'm here");
    assert!(
        received_message.quantity.is_positive(),
        "a synced message is active and visible"
    );
    let message_in = store::concepts::resolve(&b.store.pool, "message-in")
        .await
        .expect("message-in query")
        .expect("message-in arrived");
    assert_eq!(
        store::assertions::objects_from_subject(&b.store.pool, &message, &message_in)
            .await
            .expect("message relation")
            .into_iter()
            .map(|record| record.uid)
            .collect::<Vec<_>>(),
        vec![thread.clone()],
        "the synced message must be visible inside its Record thread"
    );

    let (declined_root, _) = a_wire
        .offer_conversation_to_node(&b_wire.node_id().to_string(), "Not now")
        .await
        .expect("a second offer reaches B after the first was answered");
    let declined_invite = store::invites::pending(&b.store.pool)
        .await
        .expect("second pending invite")
        .into_iter()
        .find(|invite| invite.root == declined_root)
        .expect("second invite is visible");
    b_wire
        .answer_conversation_invite(&declined_invite.record_uid, false)
        .await
        .expect("B declines and acknowledges A");
    assert_eq!(
        store::replica::state(&a.store.pool, &declined_root, &b_organ)
            .await
            .expect("sender grant state"),
        None,
        "declining must retire the sender's offered grant"
    );

    a_serving.abort();
    b_serving.abort();
}

/// The offline send queue, which is not a separate mechanism: a peer that is
/// down leaves ops QUEUED rather than dropped, and a later pass delivers them.
#[tokio::test]
async fn ops_for_an_unreachable_peer_stay_queued_and_flush_later() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(22), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(23), Reach::Local)
        .await
        .expect("b binds");

    know(&a, &b_organ, &b_wire.node_id().to_string()).await;
    know(&b, &a_organ, &a_wire.node_id().to_string()).await;
    store::organs::set_sync_policy(&a.store.pool, &b_organ, true, true)
        .await
        .expect("policy");

    a_wire.remember_addr(loopback(&b_wire));

    // B is NOT serving yet — the closed-laptop case.
    store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("beach"),
            kind: RecordKind::Plain,
            head: "Beach",
            body: "sent while they were away",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    let _ = a_wire.sync_once().await;
    assert!(
        !store::sync_ops::outbox_due(&a.store.pool)
            .await
            .expect("outbox")
            .is_empty(),
        "undelivered ops must stay queued, not be dropped"
    );

    // They come home.
    let serving = {
        let b_wire = b_wire.clone();
        tokio::spawn(async move { b_wire.serve().await })
    };
    a_wire.sync_once().await.expect("second pass");

    assert!(
        store::records::resolve(&b.store.pool, "beach")
            .await
            .expect("resolve")
            .is_some(),
        "the queue must flush on the next successful pass"
    );

    serving.abort();
}

#[test]
fn fingerprint_is_derived_from_the_key_and_stable() {
    let a = SecretKey::from_bytes(&[42; 32]).public();
    let b = SecretKey::from_bytes(&[43; 32]).public();
    assert_eq!(
        engine::wire::node_fingerprint(&a),
        engine::wire::node_fingerprint(&a),
        "deterministic"
    );
    assert_ne!(
        engine::wire::node_fingerprint(&a),
        engine::wire::node_fingerprint(&b),
        "different keys must be distinguishable in a nearby list"
    );
    assert_eq!(engine::wire::node_fingerprint(&a).len(), 8);
}

#[test]
fn node_key_is_stable_across_calls() {
    // The node key is created once at mode 0600 and reused forever after —
    // a Cell whose NodeId changed on restart would strand every saved contact.
    let dir = std::env::temp_dir().join(format!("lince-wire-{}", uuid::Uuid::new_v4()));
    let path = dir.join("node_key");
    let first = engine::wire::node_secret(&path).expect("first");
    let second = engine::wire::node_secret(&path).expect("second");
    assert_eq!(first.public(), second.public());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Adding a contact from a pasted code cannot learn their uid, so the row is
/// held under an invented one. This is the repair: the next sync pass dials
/// them, takes their Introduction, and re-files the row under the uid they
/// declare — without which every batch they ever push is refused as belonging
/// to a different Organ, and paste-to-add is a silent dead end.
#[tokio::test]
async fn a_contact_added_by_code_is_refiled_under_the_uid_they_declare() {
    let (a, _a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    // B answers strangers: the thread door is where an Introduction is served
    // to someone who holds no row for you, which is exactly A's situation.
    store::records::set_extension(
        &b.store.pool,
        &b_organ,
        "lince.discovery",
        &serde_json::json!({ "accept_unknown": true }),
    )
    .await
    .expect("open the door");

    let a_wire = Wire::bind(a.clone(), secret(31), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(32), Reach::Local)
        .await
        .expect("b binds");
    let b_node = b_wire.node_id().to_string();
    a_wire.remember_addr(loopback(&b_wire));
    let serving = {
        let b_wire = b_wire.clone();
        tokio::spawn(async move { b_wire.serve().await })
    };

    // What `add-known-organ` leaves behind: a row under an invented uid.
    let placeholder = format!("o-{b_node}");
    a.act(
        engine::actions::Action::AddKnownOrgan {
            invite: engine::pairing::PairingInvite {
                node_id: b_node.clone(),
                root_key: None,
                label: None,
                addrs: vec![],
            }
            .encode(),
            name: "Bea".into(),
        },
        None,
    )
    .await
    .expect("add by code");
    assert!(
        store::organs::contact(&a.store.pool, &placeholder)
            .await
            .unwrap()
            .expect("placeholder row")
            .pending_introduction,
        "a row added from a code owes an Introduction"
    );

    assert_eq!(
        a_wire.reconcile_pending().await.expect("reconcile"),
        1,
        "one row reconciled"
    );

    // The invented uid is gone and B's own uid is the contact.
    assert!(
        store::organs::contact(&a.store.pool, &placeholder)
            .await
            .unwrap()
            .is_none(),
        "the placeholder must not survive: a batch attributed to it would be \
         refused, which is the bug being fixed"
    );
    let real = store::organs::contact(&a.store.pool, &b_organ)
        .await
        .unwrap()
        .expect("B is now a contact under their own uid");
    assert!(!real.pending_introduction);
    assert_eq!(real.trust, "known");
    assert_eq!(real.node_id.as_deref(), Some(b_node.as_str()));
    // The name the LOCAL user typed survives the swap — it was never theirs
    // to declare.
    assert_eq!(real.head, "Bea");

    serving.abort();
}

/// The root key TOFU'd from the code is the one thing adding by code actually
/// verifies. If the Organ answering at that address presents a different one,
/// reconciliation must refuse rather than quietly adopt the new key.
#[tokio::test]
async fn a_root_key_that_does_not_match_the_code_leaves_the_contact_pending() {
    let (a, _a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;
    let b_root = engine::trust::Signer::generate(&b_organ, engine::roster::ROOT_KEY_ID);
    b.publish_root_key(&b_root).await.expect("b has a root key");

    store::records::set_extension(
        &b.store.pool,
        &b_organ,
        "lince.discovery",
        &serde_json::json!({ "accept_unknown": true }),
    )
    .await
    .expect("open the door");

    let a_wire = Wire::bind(a.clone(), secret(33), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(34), Reach::Local)
        .await
        .expect("b binds");
    let b_node = b_wire.node_id().to_string();
    a_wire.remember_addr(loopback(&b_wire));
    let serving = {
        let b_wire = b_wire.clone();
        tokio::spawn(async move { b_wire.serve().await })
    };

    // A code carrying somebody ELSE's root key for B's address.
    let placeholder = format!("o-{b_node}");
    a.act(
        engine::actions::Action::AddKnownOrgan {
            invite: engine::pairing::PairingInvite {
                node_id: b_node.clone(),
                root_key: Some(
                    engine::trust::Signer::generate("someone-else", "root").public_key_b64(),
                ),
                label: None,
                addrs: vec![],
            }
            .encode(),
            name: "Not Bea".into(),
        },
        None,
    )
    .await
    .expect("add by code");

    assert_eq!(
        a_wire.reconcile_pending().await.expect("reconcile"),
        0,
        "a key that does not match the code is not this peer"
    );
    assert!(
        store::organs::contact(&a.store.pool, &placeholder)
            .await
            .unwrap()
            .expect("the pending row stays")
            .pending_introduction,
        "left pending for a human to look at, never silently re-keyed"
    );
    assert!(
        store::organs::contact(&a.store.pool, &b_organ)
            .await
            .unwrap()
            .is_none(),
        "and the mismatched Organ is not adopted"
    );

    serving.abort();
}

/// The paste path with the code the Profile panel ACTUALLY renders — a real
/// `pairing_invite()`, root key and addresses and all — rather than one built
/// by hand in a test. Every other test here hand-builds a bare invite, which
/// is why this never showed up.
///
/// And the case that matters in practice: you already met them. You found each
/// other on the LAN, you have a conversation open, and then one of you pastes
/// the code to make it official. The NodeId is already bound to their real
/// contact row, and `organ_contact.node_id` is UNIQUE.
#[tokio::test]
async fn pasting_the_code_of_someone_already_met_upgrades_that_contact() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, _b_organ) = cell("http://b.test").await;
    let a_root = Signer::generate(&a_organ, engine::roster::ROOT_KEY_ID);
    a.publish_root_key(&a_root).await.expect("a has a root key");

    let a_wire = Wire::bind(a.clone(), secret(41), Reach::Local)
        .await
        .expect("a binds");
    let a_node = a_wire.node_id().to_string();

    // The exact string in A's "Your pairing code" field.
    let code = a_wire.pairing_invite().await.expect("invite").encode();

    // B already knows A the way discovery leaves them: real uid, bound NodeId.
    know(&b, &a_organ, &a_node).await;

    b.act(
        engine::actions::Action::AddKnownOrgan {
            invite: code,
            name: "Eduardo".into(),
        },
        None,
    )
    .await
    .expect("pasting the code of someone you already met must not fail");

    // One row, theirs, still under their real uid — not a second placeholder.
    let placeholder = format!("o-{a_node}");
    assert!(
        store::organs::contact(&b.store.pool, &placeholder)
            .await
            .unwrap()
            .is_none(),
        "no placeholder: the NodeId already resolves to a real contact"
    );
    let real = store::organs::contact(&b.store.pool, &a_organ)
        .await
        .unwrap()
        .expect("A is still the contact");
    assert_eq!(real.trust, "known");
    assert!(
        !real.pending_introduction,
        "they have already introduced themselves; the paste must not undo that"
    );
    assert_eq!(
        real.node_id.as_deref(),
        Some(a_node.as_str()),
        "the binding survives"
    );
}

/// `blocked` is terminal, and the paste path must not be the one door that
/// walks it back. Blocking someone and then adding their code — theirs by
/// accident, or handed over by them a second time — has to refuse.
#[tokio::test]
async fn pasting_the_code_of_a_blocked_organ_is_refused() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, _b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(43), Reach::Local)
        .await
        .expect("a binds");
    let code = a_wire.pairing_invite().await.expect("invite").encode();

    know(&b, &a_organ, &a_wire.node_id().to_string()).await;
    store::organs::set_trust(&b.store.pool, &a_organ, "blocked")
        .await
        .expect("block");

    let refused = b
        .act(
            engine::actions::Action::AddKnownOrgan {
                invite: code,
                name: "Eduardo".into(),
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "a blocked Organ must not be addable by code");
    assert_eq!(
        store::organs::contact(&b.store.pool, &a_organ)
            .await
            .unwrap()
            .expect("still there")
            .trust,
        "blocked",
        "and the block survives the attempt"
    );
}

/// Pairing is MUTUAL, or the doors it is supposed to open stay shut.
///
/// `pair_with` used to send only `Introduction`: the dialer learned who the
/// far side was and adopted it, and the far side kept nothing. That looked
/// like success on the only screen anyone was watching — the dialer's contact
/// list — while `lince/sync/1` and `lince/live/1` both gate on
/// `contact_by_node_id` on the ACCEPTING side, so the freshly paired Cell was
/// still a stranger there. This is the case that made a `--server` box
/// impossible to reach: pair from the laptop, then get closed on.
///
/// What the accepting side must NOT do is trust them. The row lands `unknown`;
/// promoting it is a separate, deliberate act.
#[tokio::test]
async fn pairing_leaves_a_contact_row_on_both_sides() {
    let (host, host_organ) = cell("http://host.test").await;
    let (guest, guest_organ) = cell("http://guest.test").await;

    store::records::set_extension(
        &host.store.pool,
        &host_organ,
        "lince.discovery",
        &serde_json::json!({ "accept_unknown": true }),
    )
    .await
    .expect("open the door");

    let host_wire = Wire::bind(host.clone(), secret(41), Reach::Local)
        .await
        .expect("host binds");
    let guest_wire = Wire::bind(guest.clone(), secret(42), Reach::Local)
        .await
        .expect("guest binds");

    let host_addr = loopback(&host_wire);
    let guest_node = guest_wire.node_id().to_string();
    let serving = tokio::spawn(async move { host_wire.serve().await });

    let invite = engine::pairing::PairingInvite {
        node_id: host_addr.id.to_string(),
        root_key: None,
        label: None,
        addrs: host_addr
            .ip_addrs()
            .map(|addr| addr.to_string())
            .collect(),
    };
    let paired = guest_wire
        .pair_with(&invite, "The Server")
        .await
        .expect("pairing succeeds");
    assert_eq!(paired, host_organ, "the guest adopted the host's Organ");

    // The half that used to be missing.
    let on_host = store::organs::contact_by_node_id(&host.store.pool, &guest_node)
        .await
        .expect("query")
        .expect("the host must remember whoever paired with it");
    assert_eq!(
        on_host.record_uid, guest_organ,
        "and remember them as the Organ they proved, not a NodeId alone"
    );
    assert_eq!(
        on_host.trust, "unknown",
        "being dialable is not a relationship: trust stays a separate decision"
    );

    serving.abort();
}
