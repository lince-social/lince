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

    know(&a, &b_organ, &b_wire.node_id().to_string()).await;
    know(&b, &a_organ, &a_wire.node_id().to_string()).await;

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

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

    let connection = s_wire
        .endpoint()
        .connect(b_addr.clone(), ALPN_SYNC)
        .await
        .expect("the handshake itself must succeed for this test to mean anything");

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
                author: None,
                state: nucleus::MessageState::Finished,
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
    let dir = std::env::temp_dir().join(format!("lince-wire-{}", uuid::Uuid::new_v4()));
    let path = dir.join("node_key");
    let first = engine::wire::node_secret(&path).expect("first");
    let second = engine::wire::node_secret(&path).expect("second");
    assert_eq!(first.public(), second.public());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_contact_added_by_code_is_refiled_under_the_uid_they_declare() {
    let (a, _a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

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
    assert_eq!(real.head, "Bea");

    serving.abort();
}

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

    let code = a_wire.pairing_invite().await.expect("invite").encode();

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
    assert!(
        refused.is_err(),
        "a blocked Organ must not be addable by code"
    );
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
        addrs: host_addr.ip_addrs().map(|addr| addr.to_string()).collect(),
    };
    let paired = guest_wire
        .pair_with(&invite, "The Server")
        .await
        .expect("pairing succeeds");
    assert_eq!(paired, host_organ, "the guest adopted the host's Organ");

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

struct EchoTransfer;

#[async_trait::async_trait]
impl engine::wire::TransferPeer for EchoTransfer {
    async fn handle(
        &self,
        verb: engine::wire::TransferVerb,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({ "saw": format!("{verb:?}"), "echo": body }))
    }
}

#[tokio::test]
async fn a_transfer_envelope_reaches_a_contact_over_iroh() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(21), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(22), Reach::Local)
        .await
        .expect("b binds");
    know(&a, &b_organ, &b_wire.node_id().to_string()).await;
    know(&b, &a_organ, &a_wire.node_id().to_string()).await;

    a_wire.remember_addr(loopback(&b_wire));
    b_wire.set_transfer_handler(Arc::new(EchoTransfer));
    let serving = tokio::spawn(async move { b_wire.serve().await });

    let reply = a_wire
        .transfer_post(
            &b_organ,
            engine::wire::TransferVerb::Envelope,
            serde_json::json!({ "envelope_uid": "e1" }),
        )
        .await
        .expect("the envelope is delivered");
    assert_eq!(reply["saw"], "Envelope");
    assert_eq!(
        reply["echo"],
        serde_json::json!({ "envelope_uid": "e1" }),
        "the signed body crosses untouched — the transport is a pipe"
    );

    serving.abort();
}

#[tokio::test]
async fn a_transfer_is_not_delivered_to_an_unvetted_contact() {
    let (a, a_organ) = cell("http://a.test").await;
    let (b, b_organ) = cell("http://b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(23), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(24), Reach::Local)
        .await
        .expect("b binds");

    store::organs::add_contact(&a.store.pool, &b_organ, None, "peer", "", 0)
        .await
        .expect("contact");
    store::organs::set_node_id(&a.store.pool, &b_organ, Some(&b_wire.node_id().to_string()))
        .await
        .expect("node id");
    know(&b, &a_organ, &a_wire.node_id().to_string()).await;

    a_wire.remember_addr(loopback(&b_wire));
    b_wire.set_transfer_handler(Arc::new(EchoTransfer));
    let serving = tokio::spawn(async move { b_wire.serve().await });

    let refused = a_wire
        .transfer_post(
            &b_organ,
            engine::wire::TransferVerb::Envelope,
            serde_json::json!({ "envelope_uid": "e1" }),
        )
        .await;
    assert!(
        refused.is_err(),
        "an unvetted contact must not receive an envelope"
    );

    serving.abort();
}

#[tokio::test]
async fn a_dead_cell_does_not_delay_the_live_one() {
    let (a, a_organ) = cell("http://race-a.test").await;
    let (b, b_organ) = cell("http://race-b.test").await;

    let a_wire = Wire::bind(a.clone(), secret(41), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(42), Reach::Local)
        .await
        .expect("b binds");
    know(&b, &a_organ, &a_wire.node_id().to_string()).await;

    let dead_port = {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("probe socket");
        socket.local_addr().expect("probe addr").port()
    };
    let dead = secret(43).public();
    a_wire.remember_addr(
        EndpointAddr::new(dead)
            .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), dead_port)),
    );
    a_wire.remember_addr(loopback(&b_wire));

    let unreachable = secret(44).public();

    know(&a, &b_organ, &dead.to_string()).await;
    let b_root = Signer::generate(&b_organ, engine::roster::ROOT_KEY_ID);
    a.publish_roster(
        &b_root,
        vec![
            engine::roster::CellEntry {
                cell_uid: "c-dead".into(),
                node_id: dead.to_string(),
                label: "the shut laptop".into(),
                operational_key: "k-dead".into(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            },
            engine::roster::CellEntry {
                cell_uid: "c-unreachable".into(),
                node_id: unreachable.to_string(),
                label: "a Cell nothing can resolve".into(),
                operational_key: "k-unreachable".into(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            },
            engine::roster::CellEntry {
                cell_uid: "c-live".into(),
                node_id: b_wire.node_id().to_string(),
                label: "the one that is on".into(),
                operational_key: "k-live".into(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            },
        ],
    )
    .await
    .expect("a holds a roster for b");

    let serving = tokio::spawn(async move { b_wire.serve().await });
    let contact = store::organs::contact(&a.store.pool, &b_organ)
        .await
        .expect("query")
        .expect("contact row");

    let started = std::time::Instant::now();
    let connection = a_wire.dial(&contact).await;
    let took = started.elapsed();

    assert!(connection.is_some(), "the live Cell must answer");
    assert!(
        took < std::time::Duration::from_secs(1),
        "the race took {took:?}: the dead candidate is being waited on. It \
         measures ~170ms — one DIAL_STAGGER plus a loopback handshake — \
         against the 6s a sequential dial would spend timing out first"
    );

    serving.abort();
}

#[tokio::test]
async fn one_peer_cannot_hold_unlimited_connections() {
    let (a, a_organ) = cell("http://capped-a.test").await;
    let (b, b_organ) = cell("http://capped-b.test").await;
    let a_wire = Wire::bind(a.clone(), secret(111), Reach::Local)
        .await
        .expect("a binds");
    let b_wire = Wire::bind(b.clone(), secret(112), Reach::Local)
        .await
        .expect("b binds");
    know(&a, &b_organ, &b_wire.node_id().to_string()).await;
    know(&b, &a_organ, &a_wire.node_id().to_string()).await;

    let b_addr = loopback(&b_wire);
    let serving = tokio::spawn(async move { b_wire.serve().await });

    let mut held = Vec::new();
    for _ in 0..engine::wire::MAX_CONNECTIONS_PER_PEER {
        match tokio::time::timeout(
            engine::wire::DIAL_TIMEOUT,
            a_wire.endpoint().connect(b_addr.clone(), ALPN_SYNC),
        )
        .await
        {
            Ok(Ok(connection)) => held.push(connection),
            _ => break,
        }
    }
    assert_eq!(
        held.len(),
        engine::wire::MAX_CONNECTIONS_PER_PEER,
        "the cap must admit everything up to it"
    );
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let extra = tokio::time::timeout(
        engine::wire::DIAL_TIMEOUT,
        a_wire.endpoint().connect(b_addr, ALPN_SYNC),
    )
    .await;
    if let Ok(Ok(connection)) = extra {
        let closed =
            tokio::time::timeout(std::time::Duration::from_secs(5), connection.closed()).await;
        assert!(
            closed.is_ok(),
            "a connection past the cap must be closed rather than served"
        );
    }

    drop(held);
    serving.abort();
}
