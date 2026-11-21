// non-lean-yet

//! Socket-level coverage of private Wire behavior.

use std::{collections::HashMap, sync::Arc, time::Duration};

use iroh::{
    Endpoint, EndpointAddr, SecretKey,
    endpoint::{Connection, presets},
};
use tokio::sync::mpsc;

use super::*;
use crate::sync::OpBatch;

struct HoldingHandler(mpsc::UnboundedSender<(String, Option<String>, Connection)>);

#[async_trait::async_trait]
impl LiveSessions for HoldingHandler {
    async fn serve(&self, organ: String, person: Option<String>, connection: Connection) {
        self.0.send((organ, person, connection.clone())).unwrap();
        connection.closed().await;
    }
}

fn config() -> PrivateWireConfig {
    let mut config = PrivateWireConfig::new("127.0.0.1:0".parse().unwrap(), true);
    config.max_connections = 3;
    config.max_connections_per_peer = 1;
    config.session_timeout = Duration::from_secs(20);
    config
}

async fn wire(config: PrivateWireConfig) -> Wire {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).unwrap();
    Wire::bind_private(
        Arc::new(Engine::open_memory().await.unwrap()),
        SecretKey::from_bytes(&seed),
        config,
    )
    .await
    .unwrap()
}

async fn client() -> Endpoint {
    Endpoint::builder(presets::Minimal)
        .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
        .net_report_config(iroh::endpoint::NetReportConfig::minimal())
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap()
}

fn addr(wire: &Wire) -> EndpointAddr {
    EndpointAddr::new(wire.node_id()).with_ip_addr(wire.endpoint.bound_sockets()[0])
}

fn start(wire: &Wire) -> tokio::task::JoinHandle<()> {
    let wire = wire.clone();
    tokio::spawn(async move { wire.serve().await })
}

fn handler(wire: &Wire) -> mpsc::UnboundedReceiver<(String, Option<String>, Connection)> {
    let (tx, rx) = mpsc::unbounded_channel();
    wire.set_live_handler(Arc::new(HoldingHandler(tx)));
    rx
}

async fn person(wire: &Wire, head: &str) -> String {
    store::records::create(
        &wire.engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

#[tokio::test]
async fn social_direct_deadline_includes_a_stalled_live_response() {
    let sender = wire(config()).await;
    let peer = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN_LIVE.to_vec()])
        .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
        .net_report_config(iroh::endpoint::NetReportConfig::minimal())
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap();
    let address = EndpointAddr::new(peer.id()).with_ip_addr(peer.bound_sockets()[0]);
    let peer_task = peer.clone();
    let (held, observed) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let connection = peer_task.accept().await.unwrap().await.unwrap();
        let (_send, mut recv) = connection.accept_bi().await.unwrap();
        recv.read_to_end(MAX_FRAME_BYTES).await.unwrap();
        held.send(()).unwrap();
        connection.closed().await;
    });
    let live = sender.endpoint.connect(address, ALPN_LIVE).await.unwrap();
    let uid = contact(&sender, &peer).await;
    let recipient = store::organs::contact(&sender.engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let connections = tokio::sync::Mutex::new(HashMap::new());
    let started = tokio::time::Instant::now();
    let result = sender
        .try_direct_conversation(
            &recipient,
            &WireRequest::Introduction,
            Some(live.clone()),
            &connections,
        )
        .await;
    observed.await.unwrap();
    assert!(result.is_err());
    assert!(started.elapsed() >= Duration::from_secs(3));
    assert!(started.elapsed() < Duration::from_secs(5));
    live.close(0u32.into(), b"test complete");
    server.abort();
    peer.close().await;
    sender.endpoint.close().await;
}

async fn contact(wire: &Wire, client: &Endpoint) -> String {
    let uid = nucleus::new_uid("r");
    store::organs::add_contact(&wire.engine.store.pool, &uid, None, "Remote Organ", "", 1)
        .await
        .unwrap();
    store::organs::set_node_id(
        &wire.engine.store.pool,
        &uid,
        Some(&client.id().to_string()),
    )
    .await
    .unwrap();
    uid
}

async fn grant(wire: &Wire, organ: &str, person: &str) {
    store::logins::grant(&wire.engine.store.pool, organ, person)
        .await
        .unwrap();
}

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .unwrap()
}

async fn released(wire: &Wire, slots: usize) {
    within(async {
        loop {
            if wire.connection_slots.available_permits() == slots {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
}

async fn refused(
    wire: &Wire,
    client: &Endpoint,
    received: &mut mpsc::UnboundedReceiver<(String, Option<String>, Connection)>,
) {
    if let Ok(connection) = within(client.connect(addr(wire), ALPN_LIVE)).await {
        within(connection.closed()).await;
    }
    released(wire, wire.private.unwrap().max_connections).await;
    assert!(received.try_recv().is_err());
}

async fn stop(wire: &Wire, task: tokio::task::JoinHandle<()>, clients: &[Endpoint]) {
    for client in clients {
        client.close().await;
    }
    wire.endpoint.close().await;
    within(task).await.unwrap();
    released(wire, wire.private.unwrap().max_connections).await;
    assert!(wire.open_per_peer.lock().unwrap().is_empty());
}

#[tokio::test]
async fn private_wire_explicit_login_setting_has_no_discovery_side_effects() {
    for enabled in [false, true] {
        let mut options = config();
        options.accept_credentials = enabled;
        let wire = Arc::new(wire(options).await);
        let organ = store::organs::ensure_local(&wire.engine.store.pool, "")
            .await
            .unwrap();
        store::records::set_extension(
            &wire.engine.store.pool,
            &organ.uid,
            "lince.discovery",
            &serde_json::json!({"accept_logins": !enabled, "accept_unknown": true}),
        )
        .await
        .unwrap();
        assert_eq!(wire.accept_logins().await, enabled);
        assert!(!wire.accept_unknown().await);
        assert_eq!(wire.reach(), Reach::Local);
        assert_eq!(wire.endpoint.bound_sockets().len(), 1);
        assert!(wire.endpoint.bound_sockets()[0].ip().is_loopback());
        assert_eq!(
            wire.pairing_invite().await.unwrap().addrs,
            wire.endpoint
                .bound_sockets()
                .into_iter()
                .map(|addr| addr.to_string())
                .collect::<Vec<_>>()
        );
        assert!(wire.engine.nearby.lock().unwrap().is_none());
        assert!(wire.nearby.current().is_empty());
        assert!(wire.endpoint.addr().relay_urls().next().is_none());
        wire.serve_enrolment();
        assert!(wire.engine.enroller.lock().unwrap().is_none());
        wire.endpoint.close().await;
    }
}

#[tokio::test]
async fn private_wire_granted_unknown_contact_uses_actual_peer_without_credentials() {
    let mut options = config();
    options.accept_credentials = false;
    let server = wire(options).await;
    let granted_client = client().await;
    let other_client = client().await;
    let organ = contact(&server, &granted_client).await;
    let granted_person = person(&server, "Granted Person").await;
    grant(&server, &organ, &granted_person).await;
    assert!(
        !store::auth::has_credential(&server.engine.store.pool, &granted_person)
            .await
            .unwrap()
    );
    assert!(
        !server
            .roster_names_node(&granted_client.id().to_string())
            .await
    );
    store::sqlx::query(
        "UPDATE organ_contact
            SET scope_fields = zeroblob(?), share_protein = zeroblob(?)
          WHERE record_uid = ?",
    )
    .bind(2 * 1024 * 1024_i64)
    .bind(2 * 1024 * 1024_i64)
    .bind(&organ)
    .execute(&server.engine.store.pool)
    .await
    .unwrap();
    let mut received = handler(&server);
    let task = start(&server);

    refused(&server, &other_client, &mut received).await;
    let connection = within(granted_client.connect(addr(&server), ALPN_LIVE))
        .await
        .unwrap();
    let (admitted_organ, admitted_person, accepted) = within(received.recv()).await.unwrap();
    assert_eq!(admitted_organ, organ);
    assert_eq!(admitted_person.as_deref(), Some(granted_person.as_str()));
    assert_eq!(accepted.remote_id(), granted_client.id());
    connection.close(0u32.into(), b"done");
    stop(&server, task, &[granted_client, other_client]).await;
}

#[tokio::test]
async fn private_wire_ungranted_peer_reaches_handler_only_for_credentials() {
    for accept_credentials in [false, true] {
        let mut options = config();
        options.accept_credentials = accept_credentials;
        let server = wire(options).await;
        let client = client().await;
        let organ = contact(&server, &client).await;
        let mut received = handler(&server);
        let task = start(&server);

        if accept_credentials {
            let connection = within(client.connect(addr(&server), ALPN_LIVE))
                .await
                .unwrap();
            let (admitted_organ, admitted_person, accepted) =
                within(received.recv()).await.unwrap();
            assert_eq!(admitted_organ, organ);
            assert!(admitted_person.is_none());
            assert_eq!(accepted.remote_id(), client.id());
            connection.close(0u32.into(), b"done");
        } else {
            refused(&server, &client, &mut received).await;
        }
        stop(&server, task, &[client]).await;
    }
}

#[tokio::test]
async fn private_wire_blocked_and_corrupt_peer_bindings_refuse_credentials() {
    for state in ["blocked", "blob-node"] {
        let server = wire(config()).await;
        let client = client().await;
        let organ = contact(&server, &client).await;
        let granted_person = person(&server, "Bound Person").await;
        grant(&server, &organ, &granted_person).await;
        match state {
            "blocked" => {
                store::organs::set_trust(&server.engine.store.pool, &organ, "blocked")
                    .await
                    .unwrap();
            }
            "blob-node" => {
                store::sqlx::query(
                    "UPDATE organ_contact SET node_id = CAST(node_id AS BLOB)
                      WHERE record_uid = ?",
                )
                .bind(&organ)
                .execute(&server.engine.store.pool)
                .await
                .unwrap();
            }
            _ => unreachable!(),
        }
        let mut received = handler(&server);
        let task = start(&server);
        refused(&server, &client, &mut received).await;
        stop(&server, task, &[client]).await;
    }
}

#[tokio::test]
async fn private_wire_invalid_granted_people_never_fall_back_to_credentials() {
    for state in ["deleted", "disabled", "wrong-kind", "corrupt", "missing"] {
        let server = wire(config()).await;
        let client = client().await;
        let organ = contact(&server, &client).await;
        let granted_person = person(&server, "Invalid Person").await;
        grant(&server, &organ, &granted_person).await;
        match state {
            "deleted" => {
                assert!(
                    store::records::mark_deleted(&server.engine.store.pool, &granted_person)
                        .await
                        .unwrap()
                );
            }
            "disabled" => {
                store::people::deactivate(
                    &server.engine.store.pool,
                    &granted_person,
                    "2026-09-07",
                    None,
                )
                .await
                .unwrap();
            }
            "wrong-kind" => {
                store::sqlx::query("UPDATE record SET kind = 'plain' WHERE uid = ?")
                    .bind(&granted_person)
                    .execute(&server.engine.store.pool)
                    .await
                    .unwrap();
            }
            "corrupt" => {
                store::records::set_extension_raw(
                    &server.engine.store.pool,
                    &granted_person,
                    store::people::NAMESPACE,
                    &serde_json::json!({"standing": "invalid"}),
                )
                .await
                .unwrap();
            }
            "missing" => {
                let mut connection = server.engine.store.pool.acquire().await.unwrap();
                store::sqlx::query("PRAGMA foreign_keys = OFF")
                    .execute(&mut *connection)
                    .await
                    .unwrap();
                store::sqlx::query("DELETE FROM record WHERE uid = ?")
                    .bind(&granted_person)
                    .execute(&mut *connection)
                    .await
                    .unwrap();
                store::sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(&mut *connection)
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let mut received = handler(&server);
        let task = start(&server);
        refused(&server, &client, &mut received).await;
        stop(&server, task, &[client]).await;
    }
}

#[tokio::test]
async fn private_wire_only_live_negotiates_over_real_quic() {
    let server = wire(config()).await;
    let mut received = handler(&server);
    let task = start(&server);
    let client = client().await;
    for alpn in [ALPN_SYNC, ALPN_THREAD, ALPN_HELLO, ALPN_MAILBOX] {
        assert!(within(client.connect(addr(&server), alpn)).await.is_err());
    }
    let connection = within(client.connect(addr(&server), ALPN_LIVE))
        .await
        .unwrap();
    let (organ, person, accepted) = within(received.recv()).await.unwrap();
    assert_eq!(organ, format!("node:{}", client.id()));
    assert!(person.is_none());
    assert_eq!(accepted.alpn(), ALPN_LIVE);
    connection.close(0u32.into(), b"done");
    stop(&server, task, &[client]).await;
}

#[tokio::test]
async fn private_wire_awaits_handler_without_raw_replication_fallthrough() {
    let server = wire(config()).await;
    let mut received = handler(&server);
    let task = start(&server);
    let client = client().await;
    let connection = within(client.connect(addr(&server), ALPN_LIVE))
        .await
        .unwrap();
    let (_, _, accepted) = within(received.recv()).await.unwrap();
    assert_eq!(server.connection_slots.available_permits(), 2);
    for request in [
        WireRequest::PushOps {
            batch: OpBatch {
                from_organ: "untrusted".into(),
                ops: vec![],
            },
        },
        WireRequest::PushGrantOps {
            root: "untrusted".into(),
            batch: OpBatch {
                from_organ: "untrusted".into(),
                ops: vec![],
            },
        },
    ] {
        let raw = serde_json::to_vec(&request).unwrap();
        let (mut send, mut recv) = connection.open_bi().await.unwrap();
        send.write_all(&raw).await.unwrap();
        send.finish().unwrap();
        let (mut response, mut input) = within(accepted.accept_bi()).await.unwrap();
        assert_eq!(
            within(input.read_to_end(MAX_FRAME_BYTES)).await.unwrap(),
            raw
        );
        response
            .write_all(b"private handler owns this stream")
            .await
            .unwrap();
        response.finish().unwrap();
        assert_eq!(
            within(recv.read_to_end(128)).await.unwrap(),
            b"private handler owns this stream"
        );
    }
    assert!(
        server
            .open_per_peer
            .lock()
            .unwrap()
            .contains_key(&client.id().to_string())
    );
    connection.close(0u32.into(), b"done");
    stop(&server, task, &[client]).await;
}

#[tokio::test]
async fn private_wire_missing_handler_and_disabled_login_release_budget() {
    for accept_credentials in [true, false] {
        let mut options = config();
        options.accept_credentials = accept_credentials;
        let server = wire(options).await;
        let mut received = handler(&server);
        if accept_credentials {
            *server.live.lock().unwrap() = None;
        }
        let task = start(&server);
        let client = client().await;
        if let Ok(connection) = within(client.connect(addr(&server), ALPN_LIVE)).await {
            within(connection.closed()).await;
        }
        released(&server, 3).await;
        assert!(received.try_recv().is_err());
        stop(&server, task, &[client]).await;
    }
}

#[tokio::test]
async fn private_wire_global_and_per_peer_limits_release_on_disconnect() {
    let mut options = config();
    options.max_connections = 2;
    let server = wire(options).await;
    let mut received = handler(&server);
    let task = start(&server);
    let first = client().await;
    let second = client().await;
    let third = client().await;
    let first_connection = within(first.connect(addr(&server), ALPN_LIVE))
        .await
        .unwrap();
    within(received.recv()).await.unwrap();
    if let Ok(excess) = within(first.connect(addr(&server), ALPN_LIVE)).await {
        within(excess.closed()).await;
    }
    released(&server, 1).await;
    assert!(received.try_recv().is_err());
    let second_connection = within(second.connect(addr(&server), ALPN_LIVE))
        .await
        .unwrap();
    within(received.recv()).await.unwrap();
    assert_eq!(server.connection_slots.available_permits(), 0);
    if let Ok(excess) = within(third.connect(addr(&server), ALPN_LIVE)).await {
        within(excess.closed()).await;
    }
    assert!(received.try_recv().is_err());
    first_connection.close(0u32.into(), b"done");
    released(&server, 1).await;
    let replacement = within(third.connect(addr(&server), ALPN_LIVE))
        .await
        .unwrap();
    within(received.recv()).await.unwrap();
    second_connection.close(0u32.into(), b"done");
    replacement.close(0u32.into(), b"done");
    stop(&server, task, &[first, second, third]).await;
}

#[tokio::test]
async fn private_wire_session_deadline_and_server_teardown_release_budget() {
    let mut options = config();
    options.session_timeout = Duration::from_millis(100);
    let server = wire(options).await;
    let mut received = handler(&server);
    let task = start(&server);
    let client = client().await;
    let connection = within(client.connect(addr(&server), ALPN_LIVE))
        .await
        .unwrap();
    within(received.recv()).await.unwrap();
    within(connection.closed()).await;
    released(&server, 3).await;
    let connection = within(client.connect(addr(&server), ALPN_LIVE))
        .await
        .unwrap();
    within(received.recv()).await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    within(connection.closed()).await;
    released(&server, 3).await;
    assert!(server.open_per_peer.lock().unwrap().is_empty());
    client.close().await;
    server.endpoint.close().await;
}

#[tokio::test]
async fn private_wire_rejects_invalid_limits_and_reports_exact_bind_address() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let occupied = held.local_addr().unwrap();
    let result = Wire::bind_private(
        engine.clone(),
        SecretKey::from_bytes(&[71; 32]),
        PrivateWireConfig::new(occupied, true),
    )
    .await;
    let error = result.err().unwrap().to_string();
    assert!(error.contains(&occupied.to_string()), "{error}");
    for address in ["224.0.0.1:1000", "255.255.255.255:1000", "[ff02::1]:1000"] {
        assert!(
            Wire::bind_private(
                engine.clone(),
                SecretKey::from_bytes(&[72; 32]),
                PrivateWireConfig::new(address.parse().unwrap(), true)
            )
            .await
            .is_err()
        );
    }
    let mut invalid = config();
    invalid.max_connections = 0;
    assert!(invalid.validate().is_err());
    invalid = config();
    invalid.max_connections_per_peer = 0;
    assert!(invalid.validate().is_err());
    invalid = config();
    invalid.handshake_timeout = Duration::ZERO;
    assert!(invalid.validate().is_err());
    invalid = config();
    invalid.session_timeout = Duration::ZERO;
    assert!(invalid.validate().is_err());
}

#[tokio::test]
async fn private_wire_wrong_pinned_identity_never_reaches_handler() {
    let server = wire(config()).await;
    let mut received = handler(&server);
    let task = start(&server);
    let client = client().await;
    let wrong = EndpointAddr::new(SecretKey::from_bytes(&[99; 32]).public())
        .with_ip_addr(server.endpoint.bound_sockets()[0]);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), client.connect(wrong, ALPN_LIVE))
            .await
            .map_or(true, |result| result.is_err())
    );
    assert!(received.try_recv().is_err());
    stop(&server, task, &[client]).await;
}

async fn pending_handshake(cancel_server: bool) {
    let mut options = config();
    options.max_connections = 1;
    options.handshake_timeout = Duration::from_millis(500);
    let server = wire(options).await;
    let mut received = handler(&server);
    let task = start(&server);
    let stalled = client().await;
    let healthy = client().await;
    let proxy = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    let server_addr = server.endpoint.bound_sockets()[0];
    let proxy_task = tokio::spawn(async move {
        let mut bytes = [0u8; 65536];
        loop {
            let (len, from) = proxy.recv_from(&mut bytes).await.unwrap();
            if from != server_addr {
                proxy.send_to(&bytes[..len], server_addr).await.unwrap();
            }
        }
    });
    let stalled_endpoint = stalled.clone();
    let destination = EndpointAddr::new(server.node_id()).with_ip_addr(proxy_addr);
    let dial = tokio::spawn(async move { stalled_endpoint.connect(destination, ALPN_LIVE).await });
    released(&server, 0).await;
    assert!(server.open_per_peer.lock().unwrap().is_empty());
    assert!(received.try_recv().is_err());
    if cancel_server {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        released(&server, 1).await;
        dial.abort();
        proxy_task.abort();
        let _ = dial.await;
        let _ = proxy_task.await;
        stalled.close().await;
        healthy.close().await;
        server.endpoint.close().await;
    } else {
        if let Ok(connection) = within(healthy.connect(addr(&server), ALPN_LIVE)).await {
            within(connection.closed()).await;
        }
        assert!(received.try_recv().is_err());
        released(&server, 1).await;
        dial.abort();
        proxy_task.abort();
        let _ = dial.await;
        let _ = proxy_task.await;
        let connection = within(healthy.connect(addr(&server), ALPN_LIVE))
            .await
            .unwrap();
        within(received.recv()).await.unwrap();
        connection.close(0u32.into(), b"done");
        stop(&server, task, &[stalled, healthy]).await;
    }
}

#[tokio::test]
async fn private_wire_pending_handshake_counts_toward_global_limit_and_times_out() {
    pending_handshake(false).await;
}

#[tokio::test]
async fn private_wire_pending_handshake_is_released_when_server_is_cancelled() {
    pending_handshake(true).await;
}
