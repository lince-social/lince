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
    EndpointAddr::new(wire.node_id()).with_ip_addr(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
    ))
}

/// Register `them` as a contact of `us`, reachable at `node_id`.
async fn know(us: &Engine, organ_uid: &str, node_id: &str) {
    store::organs::add_contact(&us.store.pool, organ_uid, None, "peer", "", 0)
        .await
        .expect("contact");
    store::organs::set_node_id(&us.store.pool, organ_uid, Some(node_id))
        .await
        .expect("node id");
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

/// Having a contact ROW is not having trust. `add_contact` writes `known`, but
/// a row can be demoted (or arrive un-vetted), and `unknown` means the thread
/// door only — never the sync protocol.
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
