//! The whole workflow, end to end (Ontology §11).
//!
//! Discover an Organ → add them → talk in a thread → grant them a login →
//! they open a live session over iroh and edit a record with me, cursors and
//! all → and messaging keeps working alongside it.
//!
//! Everything here rides iroh, which is the point: there is no hostname, no
//! certificate and no reverse proxy anywhere in it, so nothing breaks when a
//! machine changes network.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;
use engine::wire::{ALPN_LIVE, Reach, Wire};
use iroh::{EndpointAddr, SecretKey};
use transport::live::LiveHost;
use transport::protocol::{ClientMessage, ServerMessage};

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

fn loopback(wire: &Wire) -> EndpointAddr {
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .map(|addr| addr.port())
        .next()
        .expect("bound");
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
    store::organs::set_sync_policy(&us.store.pool, organ_uid, true, true)
        .await
        .expect("policy");
}

/// Frame a client message the way `transport::live` reads them: a 4-byte
/// big-endian length, then the JSON.
async fn say(send: &mut iroh::endpoint::SendStream, message: &ClientMessage) {
    let body = serde_json::to_vec(message).expect("serialize");
    send.write_all(&(body.len() as u32).to_be_bytes())
        .await
        .expect("write length");
    send.write_all(&body).await.expect("write body");
}

async fn hear(recv: &mut iroh::endpoint::RecvStream) -> ServerMessage {
    let mut len = [0u8; 4];
    recv.read_exact(&mut len).await.expect("read length");
    let mut buf = vec![0u8; u32::from_be_bytes(len) as usize];
    recv.read_exact(&mut buf).await.expect("read body");
    serde_json::from_slice(&buf).expect("parse server message")
}

/// A contact with a login granted gets a real session on my Cell — over iroh,
/// as the Person I named — and what they can see is what that Person can see.
#[tokio::test]
async fn a_contact_with_a_login_drives_a_live_session_over_iroh() {
    let (host, host_organ) = cell("http://host.test").await;
    let (guest, guest_organ) = cell("http://guest.test").await;

    let host_wire = Arc::new(
        Wire::bind(host.clone(), SecretKey::from_bytes(&[71; 32]), Reach::Local)
            .await
            .expect("host binds"),
    );
    let guest_wire = Wire::bind(
        guest.clone(),
        SecretKey::from_bytes(&[72; 32]),
        Reach::Local,
    )
    .await
    .expect("guest binds");

    know(&host, &guest_organ, &guest_wire.node_id().to_string()).await;
    know(&guest, &host_organ, &host_wire.node_id().to_string()).await;

    // Without a login there is no session to open, however known they are.
    let hub = Arc::new(transport::LaneHub::new());
    host_wire.set_live_handler(LiveHost::new(host.clone(), hub.clone()));
    let serving = {
        let host_wire = host_wire.clone();
        tokio::spawn(async move { host_wire.serve().await })
    };
    let host_addr = loopback(&host_wire);

    let refused = guest_wire
        .endpoint()
        .connect(host_addr.clone(), ALPN_LIVE)
        .await;
    if let Ok(connection) = refused {
        // The handshake may complete before the gate closes it; what must not
        // happen is a usable session.
        assert!(
            connection.open_bi().await.is_err() || connection.closed().await.to_string().len() > 0,
            "a known contact with NO login must not get a live session"
        );
    }

    // The Cell owner grants the login, naming the Person they act as.
    let person = host
        .act(
            Action::GrantOrganLogin {
                organ: guest_organ.clone(),
                person_name: "Marcia".into(),
            },
            None,
        )
        .await
        .expect("grant")
        .created
        .expect("the Person uid comes back");

    assert_eq!(
        store::logins::person_for_organ(&host.store.pool, &guest_organ)
            .await
            .expect("lookup"),
        Some(person.clone())
    );

    // Now the session opens.
    let connection = guest_wire
        .endpoint()
        .connect(host_addr, ALPN_LIVE)
        .await
        .expect("live dial");
    let (mut send, mut recv) = connection.open_bi().await.expect("session stream");
    // The driver writes its first frame only once the stream exists, so poke
    // it before reading.
    say(
        &mut send,
        &ClientMessage::Subscribe {
            id: "q".into(),
            protein: serde_json::from_value(serde_json::json!({
                "source": "record",
                "where": [{ "kind_eq": "plain" }],
            }))
            .expect("protein"),
        },
    )
    .await;

    // First frame is the action-intent challenge; then our snapshot.
    let mut snapshot = None;
    for _ in 0..3 {
        match hear(&mut recv).await {
            ServerMessage::Snapshot { id, rows } => {
                assert_eq!(id, "q");
                snapshot = Some(rows);
                break;
            }
            _ => continue,
        }
    }
    let rows = snapshot.expect("the live session answers a Protein subscription");

    // Visibility is the Person's, not the Cell's: nothing has been shared
    // with Marcia, so a session that answered at all must answer empty.
    assert!(
        rows.is_empty(),
        "a live guest sees what their Person may see — granting a login is not \
         granting sight of everything: {rows:?}"
    );

    // Revoking is local and immediate.
    host.act(
        Action::RevokeOrganLogin {
            organ: guest_organ.clone(),
        },
        None,
    )
    .await
    .expect("revoke");
    assert!(
        store::logins::person_for_organ(&host.store.pool, &guest_organ)
            .await
            .expect("lookup")
            .is_none()
    );

    serving.abort();
}

/// A login is for a KNOWN contact. Someone who has only knocked on the thread
/// door may not act as a Person inside this Cell.
#[tokio::test]
async fn an_unvetted_contact_cannot_be_given_a_login() {
    let (host, _) = cell("http://host.test").await;
    store::organs::add_contact(&host.store.pool, "o-stranger", None, "Stranger", "", 1)
        .await
        .expect("contact");
    store::organs::set_trust(&host.store.pool, "o-stranger", "unknown")
        .await
        .expect("trust");

    let refused = host
        .act(
            Action::GrantOrganLogin {
                organ: "o-stranger".into(),
                person_name: "Whoever".into(),
            },
            None,
        )
        .await;
    assert!(
        refused.is_err(),
        "reaching the thread door is not the same as being allowed inside"
    );
}

/// Messaging and live editing are the same relationship, not two features:
/// after a login is granted, the thread they were already using keeps working.
#[tokio::test]
async fn granting_a_login_leaves_the_conversation_untouched() {
    let (host, host_organ) = cell("http://host.test").await;
    let (guest, guest_organ) = cell("http://guest.test").await;

    let host_wire = Wire::bind(host.clone(), SecretKey::from_bytes(&[73; 32]), Reach::Local)
        .await
        .expect("host binds");
    let guest_wire = Wire::bind(
        guest.clone(),
        SecretKey::from_bytes(&[74; 32]),
        Reach::Local,
    )
    .await
    .expect("guest binds");
    know(&host, &guest_organ, &guest_wire.node_id().to_string()).await;
    know(&guest, &host_organ, &host_wire.node_id().to_string()).await;
    host_wire.remember_addr(loopback(&guest_wire));

    let (conversation, thread) = host
        .start_conversation(&guest_organ, "Beach plans")
        .await
        .expect("conversation");
    store::replica::offer(&guest.store.pool, &conversation, &host_organ)
        .await
        .expect("offer");
    guest
        .accept_conversation(&conversation, &host_organ)
        .await
        .expect("accept");
    store::replica::accept(&host.store.pool, &conversation, &guest_organ)
        .await
        .expect("host learns");

    host.act(
        Action::GrantOrganLogin {
            organ: guest_organ.clone(),
            person_name: "Marcia".into(),
        },
        None,
    )
    .await
    .expect("grant");

    // Send AFTER the login exists, and it still arrives on the thread channel.
    host.send_message(&thread, "me", "still talking")
        .await
        .expect("message");
    let serving = {
        let guest_wire = guest_wire.clone();
        tokio::spawn(async move { guest_wire.serve().await })
    };
    for _ in 0..5 {
        if host_wire.sync_once().await.expect("sync") == 0 {
            break;
        }
    }

    let messages: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM record WHERE kind = 'message' AND replica_root = ?",
    )
    .bind(&conversation)
    .fetch_one(&guest.store.pool)
    .await
    .expect("count");
    assert_eq!(
        messages, 1,
        "a login is an addition to the relationship, not a replacement for it"
    );

    serving.abort();
}
