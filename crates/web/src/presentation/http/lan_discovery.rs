//! LAN discovery, LocalSend-style (Ontology §11 "Peers"): a periodic UDP
//! multicast announce on a fixed group/port so organs on one network find
//! each other without configuration. The announce carries only public data —
//! organ uid, public-key fingerprint, api port, display name — and NEVER
//! drives trust: selecting a nearby peer runs the normal introduction +
//! challenge flow, and display names are untrusted labels. Scope is LAN
//! multicast only; internet-wide discovery, NAT traversal, and relays are
//! explicitly out.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::CellApiState;

pub(crate) const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 167);
pub(crate) const MULTICAST_PORT: u16 = 54917;
/// Announce cadence (±1s jitter is applied per cycle).
const ANNOUNCE_SECS: u64 = 5;
/// A peer expires after ~3 missed announces.
pub(crate) const EXPIRY: Duration = Duration::from_secs(16);

/// The wire announce. No secrets: the fingerprint is a hash of an already
/// public key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Announce {
    pub v: u32,
    pub organ_uid: String,
    pub fp: String,
    pub port: u16,
    pub name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NearbyPeer {
    pub organ_uid: String,
    pub name: String,
    pub addr: String,
    pub port: u16,
    pub last_seen: Instant,
}

/// The in-memory nearby list: fingerprint → last verified-format announce.
#[derive(Clone, Default)]
pub(crate) struct NearbyPeers {
    inner: Arc<Mutex<HashMap<String, NearbyPeer>>>,
}

impl NearbyPeers {
    /// Record an announce heard from `source_addr`; `own_fp` filters our own
    /// echo. Malformed payloads were already dropped by `parse_announce`.
    pub fn observe(&self, announce: Announce, source_addr: String, own_fp: Option<&str>) {
        if own_fp == Some(announce.fp.as_str()) {
            return;
        }
        let mut inner = self.inner.lock().expect("nearby peers lock");
        inner.insert(
            announce.fp,
            NearbyPeer {
                organ_uid: announce.organ_uid,
                name: announce.name,
                addr: source_addr,
                port: announce.port,
                last_seen: Instant::now(),
            },
        );
    }

    /// Current peers, expired entries pruned as a side effect.
    pub fn current(&self) -> Vec<(String, NearbyPeer)> {
        let mut inner = self.inner.lock().expect("nearby peers lock");
        let now = Instant::now();
        inner.retain(|_, peer| now.duration_since(peer.last_seen) < EXPIRY);
        inner
            .iter()
            .map(|(fp, peer)| (fp.clone(), peer.clone()))
            .collect()
    }

    /// A candidate base url for a KNOWN contact seen on the LAN — an
    /// unverified hint until a signed exchange succeeds against it.
    pub fn candidate_url_for(&self, organ_uid: &str) -> Option<String> {
        self.current()
            .into_iter()
            .find(|(_, peer)| peer.organ_uid == organ_uid)
            .map(|(_, peer)| format!("http://{}:{}", peer.addr, peer.port))
    }

    #[cfg(test)]
    pub fn backdate(&self, fp: &str, age: Duration) {
        let mut inner = self.inner.lock().expect("nearby peers lock");
        if let Some(peer) = inner.get_mut(fp) {
            peer.last_seen = Instant::now() - age;
        }
    }
}

pub(crate) fn parse_announce(bytes: &[u8]) -> Option<Announce> {
    let announce: Announce = serde_json::from_slice(bytes).ok()?;
    (announce.v == 1
        && !announce.organ_uid.is_empty()
        && !announce.fp.is_empty()
        && announce.port != 0)
        .then_some(announce)
}

/// Whether discovery is enabled: `lince.discovery` `{enabled}` on the local
/// organ record, default ENABLED when absent.
async fn discovery_enabled(state: &CellApiState) -> bool {
    let Ok(Some(organ)) = store::organs::local(&state.store.pool).await else {
        return false;
    };
    match store::records::get_extension(&state.store.pool, &organ.uid, "lince.discovery").await {
        Ok(Some(fds)) => fds
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        _ => true,
    }
}

fn multicast_socket() -> std::io::Result<std::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    // Several organs on one machine must coexist on the fixed port.
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&std::net::SocketAddr::from((Ipv4Addr::UNSPECIFIED, MULTICAST_PORT)).into())?;
    socket.join_multicast_v4(&MULTICAST_GROUP, &Ipv4Addr::UNSPECIFIED)?;
    Ok(socket.into())
}

/// Spawn the announce + listen tasks. Best-effort: a sandbox without
/// multicast logs and moves on — sync works without discovery.
pub(crate) fn spawn(state: CellApiState) {
    tokio::spawn(async move {
        let socket = match multicast_socket() {
            Ok(socket) => socket,
            Err(error) => {
                tracing::info!(%error, "LAN discovery disabled: multicast socket unavailable");
                return;
            }
        };
        let socket = match tokio::net::UdpSocket::from_std(socket) {
            Ok(socket) => socket,
            Err(error) => {
                tracing::info!(%error, "LAN discovery disabled");
                return;
            }
        };
        let socket = Arc::new(socket);
        let listen_socket = socket.clone();
        let listen_state = state.clone();

        // Listener: fold announces into the nearby list.
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                let Ok((len, from)) = listen_socket.recv_from(&mut buf).await else {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                };
                let Some(announce) = parse_announce(&buf[..len]) else {
                    continue;
                };
                let own_fp = listen_state
                    .engine
                    .local_fingerprint()
                    .await
                    .ok()
                    .flatten();
                listen_state
                    .nearby
                    .observe(announce, from.ip().to_string(), own_fp.as_deref());
            }
        });

        // Announcer: every ~5s with jitter, gated by lince.discovery.
        loop {
            if discovery_enabled(&state).await {
                if let Some(announce) = build_announce(&state).await {
                    let payload = serde_json::to_vec(&announce).expect("announce serializes");
                    let target = std::net::SocketAddr::from((MULTICAST_GROUP, MULTICAST_PORT));
                    if let Err(error) = socket.send_to(&payload, target).await {
                        tracing::debug!(%error, "discovery announce failed");
                    }
                }
            }
            // ±1s jitter from uuid entropy — no rand dep.
            let jitter_ms = u64::from(uuid::Uuid::new_v4().as_bytes()[0]) * 2000 / 255;
            tokio::time::sleep(Duration::from_millis(ANNOUNCE_SECS * 1000 - 1000 + jitter_ms))
                .await;
        }
    });
}

async fn build_announce(state: &CellApiState) -> Option<Announce> {
    let organ = store::organs::local(&state.store.pool).await.ok()??;
    let fp = state.engine.local_fingerprint().await.ok()??;
    Some(Announce {
        v: 1,
        organ_uid: organ.uid,
        fp,
        port: state.listening_port,
        name: organ.head,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn announce(fp: &str, uid: &str) -> Announce {
        Announce {
            v: 1,
            organ_uid: uid.to_string(),
            fp: fp.to_string(),
            port: 6174,
            name: "Ana's Cell".to_string(),
        }
    }

    #[test]
    fn parse_rejects_malformed_and_wrong_version() {
        assert!(parse_announce(b"not json").is_none());
        assert!(parse_announce(br#"{"v":2,"organ_uid":"o","fp":"f","port":1,"name":""}"#).is_none());
        assert!(parse_announce(br#"{"v":1,"organ_uid":"","fp":"f","port":1,"name":""}"#).is_none());
        assert!(parse_announce(br#"{"v":1,"organ_uid":"o","fp":"f","port":0,"name":""}"#).is_none());
        let ok = parse_announce(br#"{"v":1,"organ_uid":"o-1","fp":"abc","port":6174,"name":"Ana"}"#)
            .expect("valid announce");
        assert_eq!(ok.organ_uid, "o-1");
    }

    #[test]
    fn nearby_list_expires_after_missed_announces() {
        let nearby = NearbyPeers::default();
        nearby.observe(announce("fp-1", "o-1"), "192.168.0.7".into(), None);
        assert_eq!(nearby.current().len(), 1);
        nearby.backdate("fp-1", EXPIRY + Duration::from_secs(1));
        assert!(nearby.current().is_empty(), "3 missed announces = gone");
    }

    #[test]
    fn own_fingerprint_is_ignored_and_known_contacts_yield_hints() {
        let nearby = NearbyPeers::default();
        nearby.observe(announce("fp-self", "o-self"), "127.0.0.1".into(), Some("fp-self"));
        assert!(nearby.current().is_empty(), "never lists ourself");

        nearby.observe(announce("fp-ana", "o-ana"), "192.168.0.7".into(), Some("fp-self"));
        assert_eq!(
            nearby.candidate_url_for("o-ana").as_deref(),
            Some("http://192.168.0.7:6174"),
            "a known contact's announce becomes an address CANDIDATE (verified before use)"
        );
        assert!(nearby.candidate_url_for("o-stranger").is_none());
    }

    #[test]
    fn a_fresh_announce_updates_the_address() {
        let nearby = NearbyPeers::default();
        nearby.observe(announce("fp-ana", "o-ana"), "192.168.0.7".into(), None);
        nearby.observe(announce("fp-ana", "o-ana"), "10.0.0.3".into(), None);
        assert_eq!(
            nearby.candidate_url_for("o-ana").as_deref(),
            Some("http://10.0.0.3:6174")
        );
    }

    /// Real multicast on loopback — two sockets on the fixed group/port.
    #[tokio::test]
    async fn multicast_announce_reaches_a_second_socket() {
        let Ok(sender) = multicast_socket() else {
            eprintln!("multicast unavailable in this sandbox; covered by unit tests above");
            return;
        };
        let receiver = multicast_socket().expect("second socket (SO_REUSEADDR)");
        let sender = tokio::net::UdpSocket::from_std(sender).expect("tokio sender");
        let receiver = tokio::net::UdpSocket::from_std(receiver).expect("tokio receiver");

        let payload = serde_json::to_vec(&announce("fp-loop", "o-loop")).unwrap();
        let target = std::net::SocketAddr::from((MULTICAST_GROUP, MULTICAST_PORT));
        sender.send_to(&payload, target).await.expect("send");

        let mut buf = [0u8; 2048];
        let received = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let (len, _) = receiver.recv_from(&mut buf).await.expect("recv");
                if let Some(a) = parse_announce(&buf[..len]) {
                    if a.fp == "fp-loop" {
                        return a;
                    }
                }
            }
        })
        .await;
        match received {
            Ok(a) => assert_eq!(a.organ_uid, "o-loop"),
            Err(_) => eprintln!("multicast loopback not delivered in sandbox; skipping"),
        }
    }
}
