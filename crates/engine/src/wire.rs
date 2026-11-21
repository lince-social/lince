// non-lean-yet

//! Peer networking and the public Wire API.

mod ailuros;
mod cell_transport;
mod discovery;
mod dispatch;
mod endpoint;
mod exchange;
mod groups;
mod inbound;
mod location;
mod mail_delivery;
mod mailbox;
mod peers;
mod presence;
#[cfg(test)]
mod private_wire;
mod protocol;
mod record_moves;
mod relays;
mod replication;
mod siblings;
mod social;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use iroh::{Endpoint, EndpointAddr, EndpointId, endpoint::Connection};

use crate::{Engine, error::EngineError};

pub use endpoint::{
    DEFAULT_PEER_PORT, PrivateWireConfig, Reach, configured_peer_port, node_secret,
};
pub use iroh::{EndpointAddr as PeerAddr, EndpointId as PeerId};
pub use mailbox::{CarrierProbe, MailLeft};
pub use protocol::{
    AuditAgreement, ExpiredMail, HeldIntroduction, MailboxBundle, RevocationCert, StaleSibling,
    SuccessionCert, TransferPeer, TransferVerb, WireRequest, WireResponse,
};
pub use relays::configured_relays;

pub const ALPN_SYNC: &[u8] = b"lince/sync/2";

pub const ALPN_THREAD: &[u8] = b"lince/thread/2";

pub const ALPN_LIVE: &[u8] = b"lince/live/2";

pub const ALPN_LOCATION: &[u8] = b"lince/location/1";

pub const ALPN_HELLO: &[u8] = b"lince/hello/1";

pub const ALPN_MAILBOX: &[u8] = b"lince/mailbox/1";
pub const ALPN_SOCIAL: &[u8] = b"lince/social/1";
pub const ALPN_AILUROS: &[u8] = ailuros_protocol::ALPN;

pub const WIRE_EPOCH: u32 = 2;

pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

pub const MAX_FRAMES_PER_CONNECTION: usize = 4096;

pub const MAX_CONNECTIONS_PER_PEER: usize = 8;

pub const MDNS_SERVICE_NAME: &str = "lince";

pub const DIAL_TIMEOUT: Duration = Duration::from_secs(6);

pub const DIAL_STAGGER: Duration = Duration::from_millis(150);

pub fn node_fingerprint(id: &EndpointId) -> String {
    let text = id.to_string();
    text.chars().take(8).collect::<String>().to_uppercase()
}

pub use nucleus::nearby::NearbyPeer;

#[async_trait::async_trait]
pub trait LiveSessions: Send + Sync {
    async fn serve(
        &self,
        organ_uid: String,
        granted_person: Option<String>,
        connection: Connection,
    );
}

#[derive(Clone, Default)]
pub struct Nearby {
    inner: Arc<Mutex<HashMap<String, NearbyPeer>>>,
}

impl Nearby {
    pub fn current(&self) -> Vec<NearbyPeer> {
        let inner = self.inner.lock().expect("nearby lock");
        let mut peers: Vec<NearbyPeer> = inner.values().cloned().collect();
        peers.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        peers
    }

    pub fn observe(&self, node_id: String, fingerprint: String, name: String) {
        let mut inner = self.inner.lock().expect("nearby lock");
        inner.insert(
            node_id.clone(),
            NearbyPeer {
                node_id,
                fingerprint,
                name,
            },
        );
    }

    pub fn forget(&self, node_id: &str) {
        let mut inner = self.inner.lock().expect("nearby lock");
        inner.remove(node_id);
    }
}

#[derive(Clone)]
pub struct Wire {
    endpoint: Endpoint,
    pub(crate) engine: Arc<Engine>,
    nearby: Nearby,
    known_addrs: iroh::address_lookup::MemoryLookup,
    open_per_peer: Arc<Mutex<HashMap<String, usize>>>,
    presence_connections: Arc<tokio::sync::Mutex<HashMap<String, Connection>>>,
    reach: Reach,
    peer_port: u16,
    relays: Vec<iroh::RelayUrl>,
    local_discovery: discovery::LocalDiscovery,
    live: Arc<Mutex<Option<Arc<dyn LiveSessions>>>>,
    live_connections: Arc<Mutex<HashMap<String, Connection>>>,
    transfer: Arc<Mutex<Option<Arc<dyn TransferPeer>>>>,
    private: Option<PrivateWireConfig>,
    connection_slots: Arc<tokio::sync::Semaphore>,
    sync_lock: Arc<tokio::sync::Mutex<()>>,
    pub(crate) blob_slots: Arc<tokio::sync::Semaphore>,
    ailuros_auth_slots: Arc<tokio::sync::Semaphore>,
}
