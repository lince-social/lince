use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};

use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::error::EngineError;
use crate::pairing::EnrolmentInvite;
use crate::roster::SignedRoster;
use crate::sync::{Delivery, Introduction, OpBatch, WireOp};

pub use iroh::{EndpointAddr as PeerAddr, EndpointId as PeerId};

pub const ALPN_SYNC: &[u8] = b"lince/sync/2";

pub const ALPN_THREAD: &[u8] = b"lince/thread/2";

pub const ALPN_LIVE: &[u8] = b"lince/live/2";

pub const ALPN_HELLO: &[u8] = b"lince/hello/1";

pub const ALPN_MAILBOX: &[u8] = b"lince/mailbox/1";

fn not_your_mailbox() -> WireResponse {
    WireResponse::Refused {
        code: "mailbox_not_yours".into(),
        message: "this mailbox holds nothing you may collect".into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxBundle {
    pub uid: String,
    pub from_organ: String,
    pub from_cell: String,
    pub body: String,
    pub received_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpiredMail {
    pub uid: String,
    pub to_organ: String,
    pub bytes: i64,
    pub received_at: String,
    pub expired_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailLeft {
    Left { carrier: String, uid: String },
    NoRoster,
    NoPickupPoints,
    NoneAccepted { refusals: Vec<(String, String)> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarrierProbe {
    Carrying(store::mailbox::Waiting),
    Refused,
    Unreachable,
}

pub const WIRE_EPOCH: u32 = 2;

pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

pub const MAX_FRAMES_PER_CONNECTION: usize = 4096;

pub const MAX_CONNECTIONS_PER_PEER: usize = 8;

pub const MDNS_SERVICE_NAME: &str = "lince";

pub const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

pub const DIAL_STAGGER: std::time::Duration = std::time::Duration::from_millis(150);

pub fn node_secret(path: &Path) -> Result<SecretKey, EngineError> {
    Ok(SecretKey::from_bytes(&crate::trust::load_or_create_secret(
        path,
    )?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Relay,
    Internet,
    Local,
}

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WireRequest {
    Introduction,
    Introduce {
        intro: Introduction,
    },
    PushOps {
        batch: OpBatch,
    },
    MailboxDeposit {
        body: String,
    },
    MailboxWaiting {
        organ_uid: String,
        roster: SignedRoster,
    },
    MailboxCollect {
        organ_uid: String,
        roster: SignedRoster,
        limit: i64,
    },
    MailboxCollected {
        organ_uid: String,
        roster: SignedRoster,
        uids: Vec<String>,
    },
    MailboxAskToCarry {
        roster: SignedRoster,
    },
    MailboxRedeemInvite {
        token: String,
        roster: SignedRoster,
    },
    PublishSealingKey {
        cell_uid: String,
        sealing_key: crate::seal::SealingKey,
    },
    MailboxExpiries,
    MailboxExpiriesHeard {
        uids: Vec<String>,
    },
    FetchOpsSince {
        vector: Vec<store::sync_ops::VectorEntry>,
        limit: i64,
    },
    FetchReference {
        root: String,
        record: String,
    },
    OfferGrant {
        root: String,
        title: String,
        intro: Introduction,
    },
    AcceptGrant {
        root: String,
    },
    DeclineGrant {
        root: String,
    },
    PushGrantOps {
        root: String,
        batch: OpBatch,
    },
    FetchGrantOpsSince {
        root: String,
        vector: Vec<store::sync_ops::VectorEntry>,
        limit: i64,
    },
    FetchRoster,
    TransferPost {
        verb: TransferVerb,
        body: serde_json::Value,
    },
    FetchSuccessions,
    FetchRevocations,
    Enrol {
        token: String,
        cell_uid: String,
        node_id: String,
        label: String,
        operational_key: String,
        #[serde(default)]
        sealing_key: Option<crate::seal::SealingKey>,
    },
    FetchVector {
        organ_uid: String,
    },
    FetchDoorRequests {
        limit: i64,
    },
    ReleaseDoorRequests {
        uids: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferVerb {
    Envelope,
    Pull,
    Receipt,
    Command,
    PolicyEvent,
    ApplicationAttestation,
}

#[async_trait::async_trait]
pub trait TransferPeer: Send + Sync {
    async fn handle(
        &self,
        verb: TransferVerb,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationCert {
    pub organ_uid: String,
    pub revoked_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessionCert {
    pub organ_uid: String,
    pub old_key: String,
    pub new_key: String,
    pub created_at: String,
    pub signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "ok", rename_all = "snake_case")]
pub enum WireResponse {
    Introduction {
        intro: Introduction,
    },
    Applied {
        applied: usize,
    },
    Roster {
        roster: Option<crate::roster::SignedRoster>,
    },
    Revocations {
        certs: Vec<RevocationCert>,
    },
    Successions {
        certs: Vec<SuccessionCert>,
    },
    Transfer {
        body: serde_json::Value,
    },
    Ops {
        from_organ: String,
        ops: Vec<WireOp>,
        head: i64,
    },
    MailboxAccepted {
        uid: String,
    },
    MailboxBundles {
        bundles: Vec<MailboxBundle>,
    },
    MailboxWaiting {
        bundles: i64,
        bytes: i64,
        oldest_expires_at: Option<String>,
    },
    MailboxAsked,
    MailboxExpired {
        expired: Vec<ExpiredMail>,
    },
    MailboxCarrying {
        label: String,
        quota_bytes: i64,
    },
    Error {
        message: String,
    },
    Refused {
        code: String,
        message: String,
    },
    DoorRequests {
        requests: Vec<HeldIntroduction>,
    },
    Reference {
        row: serde_json::Value,
    },
    Vector {
        vector: Vec<store::sync_ops::VectorEntry>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditAgreement {
    pub contact_organ: String,
    pub they_lack: usize,
    pub unknown_cells: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleSibling {
    pub cell_uid: String,
    pub node_id: String,
    pub label: String,
    pub their_epoch: u32,
    pub our_epoch: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeldIntroduction {
    pub uid: String,
    pub node_id: String,
    pub intro: Introduction,
    pub received_at: String,
}

#[derive(Clone)]
pub struct Wire {
    endpoint: Endpoint,
    engine: Arc<Engine>,
    nearby: Nearby,
    known_addrs: iroh::address_lookup::MemoryLookup,
    open_per_peer: Arc<Mutex<HashMap<String, usize>>>,
    reach: Reach,
    live: Arc<Mutex<Option<Arc<dyn LiveSessions>>>>,
    live_connections: Arc<Mutex<HashMap<String, Connection>>>,
    transfer: Arc<Mutex<Option<Arc<dyn TransferPeer>>>>,
}

impl Wire {
    pub async fn bind(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
    ) -> Result<Wire, EngineError> {
        Wire::bind_as(engine, secret, reach, None).await
    }

    pub async fn bind_as(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        display_name: Option<&str>,
    ) -> Result<Wire, EngineError> {
        Self::bind_with_discovery(engine, secret, reach, display_name, true).await
    }

    pub async fn bind_with_discovery(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        display_name: Option<&str>,
        local_discovery: bool,
    ) -> Result<Wire, EngineError> {
        let alpns = vec![
            ALPN_SYNC.to_vec(),
            ALPN_THREAD.to_vec(),
            ALPN_LIVE.to_vec(),
            ALPN_HELLO.to_vec(),
            ALPN_MAILBOX.to_vec(),
        ];
        let endpoint = match reach {
            Reach::Internet => Endpoint::builder(presets::N0),
            Reach::Relay => Endpoint::builder(presets::N0).clear_ip_transports(),
            Reach::Local => Endpoint::builder(presets::Minimal),
        }
        .secret_key(secret)
        .alpns(alpns)
        .bind()
        .await
        .map_err(|error| EngineError::Consequence(format!("iroh bind failed: {error}")))?;

        if let Some(name) = display_name {
            let clipped: String = name
                .chars()
                .take(iroh::address_lookup::UserData::MAX_LENGTH / 4)
                .collect();
            match iroh::address_lookup::UserData::try_from(clipped) {
                Ok(data) => endpoint.set_user_data_for_address_lookup(Some(data)),
                Err(error) => tracing::warn!(%error, "display name rejected for discovery"),
            }
        }

        let known_addrs = iroh::address_lookup::MemoryLookup::new();
        if let Ok(services) = endpoint.address_lookup() {
            services.add(known_addrs.clone());
        }
        let wire = Wire {
            endpoint,
            engine,
            reach,
            open_per_peer: Arc::new(Mutex::new(HashMap::new())),
            nearby: Nearby::default(),
            known_addrs,
            live: Arc::new(Mutex::new(None)),
            live_connections: Arc::new(Mutex::new(HashMap::new())),
            transfer: Arc::new(Mutex::new(None)),
        };
        if local_discovery {
            wire.spawn_mdns();
        }
        *wire.engine.nearby.lock().expect("nearby handle") = Some(wire.nearby.clone());
        Ok(wire)
    }

    pub fn set_live_handler(&self, handler: Arc<dyn LiveSessions>) {
        *self.live.lock().expect("live handler") = Some(handler);
    }

    pub fn remember_live_connection(&self, contact_organ: &str, connection: &Connection) {
        self.live_connections
            .lock()
            .expect("live connections")
            .insert(contact_organ.to_string(), connection.clone());
    }

    pub fn serve_enrolment(self: &Arc<Self>) {
        let as_trait: Arc<dyn crate::enrolment::CellTransport> = self.clone();
        self.engine.set_enroller(Arc::downgrade(&as_trait));
    }

    pub fn set_transfer_handler(&self, handler: Arc<dyn TransferPeer>) {
        *self.transfer.lock().expect("transfer handler") = Some(handler);
    }

    pub async fn transfer_post(
        &self,
        contact_organ: &str,
        verb: TransferVerb,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, EngineError> {
        let contact = store::organs::contact(&self.engine.store.pool, contact_organ)
            .await?
            .ok_or_else(|| {
                EngineError::Consequence(format!("{contact_organ} is not introduced"))
            })?;
        if contact.trust != "known" {
            return Err(EngineError::Consequence(format!(
                "{contact_organ} is not a known contact, so nothing is delivered to it"
            )));
        }
        let node_id: iroh::EndpointId = contact
            .node_id
            .as_deref()
            .unwrap_or_default()
            .parse()
            .map_err(|_| {
                EngineError::Consequence(format!("{contact_organ} has no dialable NodeId"))
            })?;
        match self
            .request(
                EndpointAddr::new(node_id),
                ALPN_SYNC,
                &WireRequest::TransferPost { verb, body },
            )
            .await?
        {
            WireResponse::Transfer { body } => Ok(body),
            WireResponse::Error { message } | WireResponse::Refused { message, .. } => {
                Err(EngineError::Consequence(message))
            }
            _ => Err(EngineError::Consequence(
                "peer answered a Transfer exchange with something else".into(),
            )),
        }
    }

    pub async fn open_live(&self, contact_organ: &str) -> Result<Connection, EngineError> {
        let contact = store::organs::contact(&self.engine.store.pool, contact_organ)
            .await?
            .ok_or_else(|| EngineError::Consequence("not a contact".into()))?;
        let node_id = contact
            .node_id
            .as_deref()
            .ok_or_else(|| EngineError::Consequence("no address for this contact".into()))?
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;
        tokio::time::timeout(
            DIAL_TIMEOUT,
            self.endpoint.connect(EndpointAddr::new(node_id), ALPN_LIVE),
        )
        .await
        .map_err(|_| EngineError::Consequence("they did not answer".into()))?
        .map_err(|error| EngineError::Consequence(format!("live dial failed: {error}")))
    }

    pub async fn open_live_at(
        &self,
        invite: &crate::pairing::PairingInvite,
    ) -> Result<Connection, EngineError> {
        let node_id: EndpointId = invite
            .node_id
            .parse()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;
        let mut addr = EndpointAddr::new(node_id);
        for text in &invite.addrs {
            if let Ok(socket) = text.parse::<std::net::SocketAddr>() {
                addr = addr.with_ip_addr(socket);
            }
        }
        if !invite.addrs.is_empty() {
            self.remember_addr(addr.clone());
        }
        tokio::time::timeout(DIAL_TIMEOUT, self.endpoint.connect(addr, ALPN_LIVE))
            .await
            .map_err(|_| EngineError::Consequence("they did not answer".into()))?
            .map_err(|error| EngineError::Consequence(format!("live dial failed: {error}")))
    }

    pub async fn enrol(&self, invite: &EnrolmentInvite) -> Result<SignedRoster, EngineError> {
        self.engine.may_enrol().await?;
        let cell = store::cells::local(&self.engine.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Cell Record".into()))?;
        let node_id: EndpointId = invite.node_id.parse().map_err(|_| {
            EngineError::Consequence("the code carries an unreadable node id".into())
        })?;
        let mut addr = EndpointAddr::new(node_id);
        for text in &invite.addrs {
            if let Ok(socket) = text.parse::<SocketAddr>() {
                addr = addr.with_ip_addr(socket);
            }
        }
        if !invite.addrs.is_empty() {
            self.remember_addr(addr.clone());
        }
        let operational = self.engine.operational_key_for(&invite.organ_uid).await?;
        let operational_key = operational.public_key_b64();
        let sealing_key = self.engine.published_sealing_key().await?;
        let response = self
            .request(
                addr,
                ALPN_THREAD,
                &WireRequest::Enrol {
                    token: invite.token.clone(),
                    cell_uid: cell.uid.clone(),
                    node_id: self.node_id().to_string(),
                    label: cell.label.clone(),
                    operational_key,
                    sealing_key,
                },
            )
            .await
            .map_err(|error| {
                EngineError::Consequence(format!(
                    "the other Cell did not accept the connection. An enrolment code works \
                     once and expires after {} minutes — ask that Cell for a new one. \
                     ({error})",
                    crate::roster::ENROLMENT_TOKEN_TTL_MINUTES
                ))
            })?;
        let signed = match response {
            WireResponse::Roster {
                roster: Some(signed),
            } => signed,
            WireResponse::Roster { roster: None } => {
                return Err(EngineError::Consequence(
                    "the other Cell accepted the code but returned no roster".into(),
                ));
            }
            WireResponse::Refused { code, message } => {
                return Err(EngineError::Consequence(format!("{code}: {message}")));
            }
            other => {
                return Err(EngineError::Consequence(format!(
                    "unexpected answer to enrolment: {other:?}"
                )));
            }
        };
        self.engine.join_organ(invite, &signed, operational).await?;
        Ok(signed)
    }

    pub fn remember_addr(&self, addr: PeerAddr) {
        self.known_addrs.add_endpoint_info(addr);
    }

    pub async fn pairing_invite(&self) -> Result<crate::pairing::PairingInvite, EngineError> {
        let organ = store::organs::local(&self.engine.store.pool).await?;
        let root_key = match &organ {
            Some(organ) => crate::trust::keys_of(&self.engine.store, &organ.uid)
                .await?
                .into_iter()
                .find(|(key_id, _)| key_id == crate::roster::ROOT_KEY_ID)
                .map(|(_, public_key)| public_key),
            None => None,
        };
        Ok(crate::pairing::PairingInvite {
            node_id: self.node_id().to_string(),
            root_key,
            label: organ.map(|organ| organ.head),
            addrs: self
                .endpoint
                .addr()
                .ip_addrs()
                .map(|addr| addr.to_string())
                .collect(),
        })
    }

    pub async fn pair_with(
        &self,
        invite: &crate::pairing::PairingInvite,
        name: &str,
    ) -> Result<String, EngineError> {
        let node_id: EndpointId = invite
            .node_id
            .parse()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;

        let mut addr = EndpointAddr::new(node_id);
        for text in &invite.addrs {
            if let Ok(socket) = text.parse::<std::net::SocketAddr>() {
                addr = addr.with_ip_addr(socket);
            }
        }
        if !invite.addrs.is_empty() {
            self.remember_addr(addr.clone());
        }

        let ours = self.engine.introduction().await?;
        let response = self
            .request(
                addr.clone(),
                ALPN_THREAD,
                &WireRequest::Introduce { intro: ours },
            )
            .await?;
        let intro = match response {
            WireResponse::Introduction { intro } => intro,
            other => {
                return Err(EngineError::Consequence(format!(
                    "pairing refused: {other:?}"
                )));
            }
        };
        self.engine.adopt_introduction(&intro, 1).await?;

        if let Ok(WireResponse::Roster {
            roster: Some(roster),
        }) = self
            .request(addr, ALPN_THREAD, &WireRequest::FetchRoster)
            .await
        {
            self.adopt_fetched_roster(&roster, &intro.organ_uid).await;
        }

        let pool = &self.engine.store.pool;
        if !name.trim().is_empty() {
            store::records::set_text(pool, &intro.organ_uid, Some(name.trim()), None).await?;
        }
        store::organs::set_node_id(pool, &intro.organ_uid, Some(&invite.node_id)).await?;
        store::organs::set_trust(pool, &intro.organ_uid, "known").await?;
        Ok(intro.organ_uid)
    }

    async fn bind_unknown_peer(
        &self,
        node_id: &str,
        intro: &Introduction,
    ) -> Result<String, EngineError> {
        let pool = &self.engine.store.pool;
        if store::organs::local(pool)
            .await?
            .is_some_and(|local| local.uid == intro.organ_uid)
        {
            return Err(EngineError::Forbidden(
                "a remote Cell claimed this Cell's Organ uid".into(),
            ));
        }
        if let Some(by_node) = store::organs::contact_by_node_id(pool, node_id).await? {
            if by_node.record_uid != intro.organ_uid {
                return Err(EngineError::Forbidden(
                    "this NodeId is already bound to another Organ".into(),
                ));
            }
            return Ok(by_node.record_uid);
        }
        if let Some(by_uid) = store::organs::contact(pool, &intro.organ_uid).await? {
            if by_uid
                .node_id
                .as_deref()
                .is_some_and(|held| held != node_id)
            {
                return Err(EngineError::Forbidden(
                    "this Organ is already bound to another NodeId".into(),
                ));
            }
            if by_uid.trust == "blocked" {
                return Err(EngineError::Forbidden("this Organ is blocked".into()));
            }
            store::organs::set_node_id(pool, &intro.organ_uid, Some(node_id)).await?;
            return Ok(intro.organ_uid.clone());
        }

        let organ_uid = self.engine.adopt_introduction(intro, 1).await?;
        store::organs::set_node_id(pool, &organ_uid, Some(node_id)).await?;
        store::organs::set_trust(pool, &organ_uid, "unknown").await?;
        Ok(organ_uid)
    }

    pub async fn offer_conversation_to_node(
        &self,
        node_id: &str,
        title: &str,
    ) -> Result<(String, String), EngineError> {
        let id = node_id
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("malformed NodeId".into()))?;
        let remote = self
            .request(
                EndpointAddr::new(id),
                ALPN_THREAD,
                &WireRequest::Introduction,
            )
            .await?;
        let WireResponse::Introduction {
            intro: remote_intro,
        } = remote
        else {
            return Err(EngineError::Consequence(
                "the nearby Cell refused to introduce itself".into(),
            ));
        };
        let contact = self.bind_unknown_peer(node_id, &remote_intro).await?;
        let (conversation, thread) = self.engine.start_conversation(&contact, title).await?;
        let intro = self.engine.introduction().await?;
        match self
            .request(
                EndpointAddr::new(id),
                ALPN_THREAD,
                &WireRequest::OfferGrant {
                    root: conversation.clone(),
                    title: title.to_string(),
                    intro,
                },
            )
            .await?
        {
            WireResponse::Applied { .. } => Ok((conversation, thread)),
            WireResponse::Refused { message, .. } | WireResponse::Error { message } => {
                Err(EngineError::Consequence(message))
            }
            other => Err(EngineError::Consequence(format!(
                "unexpected conversation-offer response: {other:?}"
            ))),
        }
    }

    pub async fn fetch_reference(
        &self,
        owner_organ: &str,
        root: &str,
        record: &str,
    ) -> Result<serde_json::Value, EngineError> {
        let contact = store::organs::contact(&self.engine.store.pool, owner_organ)
            .await?
            .ok_or_else(|| EngineError::Consequence("not a contact".into()))?;
        let node_id = contact
            .node_id
            .as_deref()
            .ok_or_else(|| EngineError::Consequence("no address for this contact".into()))?
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;
        let request = WireRequest::FetchReference {
            root: root.to_string(),
            record: record.to_string(),
        };
        match self
            .request(EndpointAddr::new(node_id), ALPN_THREAD, &request)
            .await?
        {
            WireResponse::Reference { row } => Ok(row),
            WireResponse::Refused { message, .. } | WireResponse::Error { message } => {
                Err(EngineError::Consequence(message))
            }
            other => Err(EngineError::Consequence(format!(
                "unexpected reference response: {other:?}"
            ))),
        }
    }

    pub async fn answer_conversation_invite(
        &self,
        invite_uid: &str,
        accept: bool,
    ) -> Result<String, EngineError> {
        let invite = store::invites::get(&self.engine.store.pool, invite_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("no such invite".into()))?;
        let contact = store::organs::contact(&self.engine.store.pool, &invite.from_organ)
            .await?
            .ok_or_else(|| {
                EngineError::Consequence("invite sender has no identity binding".into())
            })?;
        let node_id = contact
            .node_id
            .as_deref()
            .ok_or_else(|| EngineError::Consequence("invite sender has no NodeId".into()))?
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("invite sender has a malformed NodeId".into()))?;
        let request = if accept {
            WireRequest::AcceptGrant {
                root: invite.root.clone(),
            }
        } else {
            WireRequest::DeclineGrant {
                root: invite.root.clone(),
            }
        };
        match self
            .request(EndpointAddr::new(node_id), ALPN_THREAD, &request)
            .await?
        {
            WireResponse::Applied { .. } => {}
            WireResponse::Refused { message, .. } | WireResponse::Error { message } => {
                return Err(EngineError::Consequence(message));
            }
            other => {
                return Err(EngineError::Consequence(format!(
                    "unexpected invite response: {other:?}"
                )));
            }
        }
        if accept {
            self.engine.accept_invite(invite_uid).await?;
        } else {
            self.engine.decline_invite(invite_uid).await?;
        }
        Ok(invite.root)
    }

    pub async fn shutdown(&self) {
        self.endpoint.close().await;
    }

    fn spawn_mdns(&self) {
        use n0_future::StreamExt as _;

        let mdns = match iroh_mdns_address_lookup::MdnsAddressLookup::builder()
            .service_name(MDNS_SERVICE_NAME)
            .build(self.endpoint.id())
        {
            Ok(mdns) => mdns,
            Err(error) => {
                tracing::warn!(%error, "mDNS unavailable; nearby list will stay empty");
                return;
            }
        };
        if let Ok(services) = self.endpoint.address_lookup() {
            services.add(mdns.clone());
        }

        let nearby = self.nearby.clone();
        tokio::spawn(async move {
            let mut events = mdns.subscribe().await;
            while let Some(event) = events.next().await {
                match event {
                    iroh_mdns_address_lookup::DiscoveryEvent::Discovered {
                        endpoint_info, ..
                    } => {
                        let id = endpoint_info.endpoint_id;
                        nearby.observe(
                            id.to_string(),
                            node_fingerprint(&id),
                            endpoint_info
                                .data
                                .user_data()
                                .map(|data| data.to_string())
                                .unwrap_or_default(),
                        );
                    }
                    iroh_mdns_address_lookup::DiscoveryEvent::Expired { endpoint_id } => {
                        nearby.forget(&endpoint_id.to_string());
                    }
                    _ => {}
                }
            }
        });
    }

    pub fn nearby(&self) -> &Nearby {
        &self.nearby
    }

    pub async fn accept_unknown(&self) -> bool {
        let Ok(Some(organ)) = store::organs::local(&self.engine.store.pool).await else {
            return false;
        };
        match self.discovery_config(&organ.uid).await {
            Some(fields) => fields
                .get("accept_unknown")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            _ => false,
        }
    }

    pub async fn accept_logins(&self) -> bool {
        let Ok(Some(organ)) = store::organs::local(&self.engine.store.pool).await else {
            return false;
        };
        match self.discovery_config(&organ.uid).await {
            Some(fields) => fields
                .get("accept_logins")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            _ => true,
        }
    }

    pub fn node_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn reach(&self) -> Reach {
        self.reach
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub async fn serve(&self) {
        while let Some(incoming) = self.endpoint.accept().await {
            let wire = self.clone();
            tokio::spawn(async move {
                let connection = match incoming.await {
                    Ok(connection) => connection,
                    Err(error) => {
                        tracing::debug!(%error, "iroh handshake failed");
                        return;
                    }
                };
                let peer = connection.remote_id().to_string();
                if !wire.admit(&peer) {
                    tracing::warn!(
                        %peer,
                        limit = MAX_CONNECTIONS_PER_PEER,
                        "refusing a connection: this peer already holds the maximum"
                    );
                    connection.close(0u32.into(), b"too many connections");
                    return;
                }
                if let Err(error) = wire.serve_connection(connection).await {
                    tracing::debug!(%error, "iroh connection ended");
                }
                wire.release(&peer);
            });
        }
    }

    fn admit(&self, peer: &str) -> bool {
        let mut open = self.open_per_peer.lock().expect("open per peer");
        let count = open.entry(peer.to_string()).or_insert(0);
        if *count >= MAX_CONNECTIONS_PER_PEER {
            return false;
        }
        *count += 1;
        true
    }

    fn release(&self, peer: &str) {
        let mut open = self.open_per_peer.lock().expect("open per peer");
        if let Some(count) = open.get_mut(peer) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                open.remove(peer);
            }
        }
    }

    async fn discovery_config(&self, organ_uid: &str) -> Option<serde_json::Value> {
        if let Ok(Some(fields)) =
            store::cells::config(&self.engine.store.pool, "lince.discovery").await
        {
            return Some(fields);
        }
        store::records::get_extension(&self.engine.store.pool, organ_uid, "lince.discovery")
            .await
            .ok()
            .flatten()
    }

    pub async fn roster_names_node(&self, node_id: &str) -> bool {
        let Ok(Some(organ)) = store::organs::local(&self.engine.store.pool).await else {
            return false;
        };
        let Ok(Some(signed)) = self.engine.roster_of(&organ.uid).await else {
            return false;
        };
        signed
            .roster
            .cells
            .iter()
            .any(|member| member.node_id == node_id)
    }

    pub async fn sibling_organ(&self, node_id: &str) -> Option<String> {
        let organ = store::organs::local(&self.engine.store.pool).await.ok()??;
        let signed = self.engine.roster_of(&organ.uid).await.ok()??;
        let ours = store::cells::local(&self.engine.store.pool)
            .await
            .ok()??
            .uid;
        signed
            .roster
            .cells
            .iter()
            .find(|member| {
                member.node_id == node_id
                    && member.cell_uid != ours
                    && member.may(crate::roster::CAP_WRITE)
            })
            .map(|_| organ.uid)
    }

    async fn may_represent(&self) -> bool {
        let pool = &self.engine.store.pool;
        let (Ok(Some(organ)), Ok(Some(cell))) = (
            store::organs::local(pool).await,
            store::cells::local(pool).await,
        ) else {
            return false;
        };
        if self.engine.identity_in_flux() {
            return false;
        }
        match self.engine.roster_of(&organ.uid).await {
            Ok(None) => true,
            Ok(Some(_)) => self
                .engine
                .cell_may(&organ.uid, &cell.uid, crate::roster::CAP_REPRESENT)
                .await
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    async fn hold_at_the_door(
        &self,
        node_id: &str,
        intro: &Introduction,
    ) -> Result<(), EngineError> {
        let body = serde_json::to_string(intro)
            .map_err(|error| EngineError::Consequence(error.to_string()))?;
        store::door::hold(&self.engine.store.pool, node_id, &intro.organ_uid, &body).await?;
        tracing::info!(
            %node_id,
            organ = %intro.organ_uid,
            "a stranger knocked at the front door; held for the owner"
        );
        Ok(())
    }

    async fn serve_connection(&self, connection: Connection) -> Result<(), EngineError> {
        let peer = connection.remote_id();
        let alpn = connection.alpn().to_vec();
        let contact =
            store::organs::contact_by_node_id(&self.engine.store.pool, &peer.to_string()).await?;

        if contact.as_ref().is_some_and(|c| c.trust == "blocked") {
            connection.close(0u32.into(), b"blocked");
            return Ok(());
        }

        let sibling = self.sibling_organ(&peer.to_string()).await;

        let known = sibling.is_some() || contact.as_ref().is_some_and(|c| c.trust == "known");
        let listed = self.roster_names_node(&peer.to_string()).await;
        let identified = sibling.is_some() || contact.is_some() || listed;

        match (alpn.as_slice(), known) {
            (ALPN_SYNC, true) => {}
            (ALPN_SYNC, false) => {
                connection.close(0u32.into(), b"unknown organ");
                return Ok(());
            }
            (ALPN_MAILBOX, _) => {}
            (ALPN_HELLO, _) => {
                if !identified {
                    connection.close(0u32.into(), b"not known");
                    return Ok(());
                }
                if let Ok(mut send) = connection.open_uni().await {
                    let body = serde_json::json!({ "epoch": WIRE_EPOCH }).to_string();
                    let _ = send.write_all(body.as_bytes()).await;
                    let _ = send.finish();
                    connection.closed().await;
                }
                return Ok(());
            }
            (ALPN_LIVE, known) => {
                let organ = contact
                    .as_ref()
                    .map(|c| c.record_uid.clone())
                    .unwrap_or_default();
                let granted = if known {
                    store::logins::person_for_organ(&self.engine.store.pool, &organ).await?
                } else {
                    None
                };
                let granted = match granted {
                    Some(person)
                        if store::people::is_active(&self.engine.store.pool, &person).await? =>
                    {
                        Some(person)
                    }
                    _ => None,
                };
                if granted.is_none() && !self.accept_logins().await {
                    connection.close(0u32.into(), b"this Cell does not accept live logins");
                    return Ok(());
                }
                let Some(handler) = self.live.lock().expect("live handler").clone() else {
                    connection.close(0u32.into(), b"no live session for this organ");
                    return Ok(());
                };
                let session_organ = if organ.is_empty() {
                    format!("node:{peer}")
                } else {
                    organ
                };
                let live_connection = connection.clone();
                tokio::spawn(async move {
                    handler
                        .serve(session_organ, granted, live_connection.clone())
                        .await;
                    live_connection.close(0u32.into(), b"live session ended");
                });
            }
            (ALPN_THREAD, true) => {}
            (ALPN_THREAD, false) if !identified => {
                let enrolling = store::roster::enrolment_is_open(&self.engine.store.pool)
                    .await
                    .unwrap_or(false);
                if !enrolling && !self.accept_unknown().await {
                    connection.close(0u32.into(), b"not accepting unknown organs");
                    return Ok(());
                }
            }
            (ALPN_THREAD, false) => {}
            _ => {
                connection.close(0u32.into(), b"unsupported alpn");
                return Ok(());
            }
        }

        let from_organ = sibling
            .or_else(|| contact.map(|contact| contact.record_uid))
            .unwrap_or_default();

        for _ in 0..MAX_FRAMES_PER_CONNECTION {
            let (mut send, mut recv) = match connection.accept_bi().await {
                Ok(streams) => streams,
                Err(_) => return Ok(()),
            };
            let raw = recv
                .read_to_end(MAX_FRAME_BYTES)
                .await
                .map_err(|error| EngineError::Consequence(format!("peer frame: {error}")))?;
            let response = match serde_json::from_slice::<WireRequest>(&raw) {
                Ok(request)
                    if alpn.as_slice() == ALPN_LIVE
                        && !matches!(
                            request,
                            WireRequest::PushOps { .. } | WireRequest::PushGrantOps { .. }
                        ) =>
                {
                    WireResponse::Refused {
                        code: "wrong_door".into(),
                        message: "that verb is not served on the live connection".into(),
                    }
                }
                Ok(request)
                    if (alpn.as_slice() == ALPN_MAILBOX)
                        != matches!(
                            request,
                            WireRequest::MailboxDeposit { .. }
                                | WireRequest::MailboxWaiting { .. }
                                | WireRequest::MailboxCollect { .. }
                                | WireRequest::MailboxCollected { .. }
                                | WireRequest::MailboxAskToCarry { .. }
                                | WireRequest::MailboxRedeemInvite { .. }
                                | WireRequest::MailboxExpiries
                                | WireRequest::MailboxExpiriesHeard { .. }
                        ) =>
                {
                    WireResponse::Refused {
                        code: "wrong_door".into(),
                        message: "that verb is not served on this door".into(),
                    }
                }
                Ok(request)
                    if !known
                        && alpn.as_slice() != ALPN_MAILBOX
                        && !matches!(
                            request,
                            WireRequest::Introduction
                                | WireRequest::Introduce { .. }
                                | WireRequest::Enrol { .. }
                                | WireRequest::OfferGrant { .. }
                                | WireRequest::AcceptGrant { .. }
                                | WireRequest::DeclineGrant { .. }
                                | WireRequest::PushGrantOps { .. }
                                | WireRequest::FetchGrantOpsSince { .. }
                                | WireRequest::FetchReference { .. }
                                | WireRequest::PublishSealingKey { .. }
                        ) =>
                {
                    WireResponse::Refused {
                        code: "not_known".into(),
                        message: "only introduction is served to an unknown Organ".into(),
                    }
                }
                Ok(request) => {
                    if let WireRequest::Introduce { intro }
                    | WireRequest::OfferGrant { intro, .. } = &request
                    {
                        if from_organ.is_empty() && !self.may_represent().await {
                            let response =
                                match self.hold_at_the_door(&peer.to_string(), intro).await {
                                    Ok(()) => WireResponse::Refused {
                                        code: "held_for_owner".into(),
                                        message: "this is a front door and cannot accept for its \
                                              owner. Your request is waiting for one of their \
                                              devices to see it."
                                            .into(),
                                    },
                                    Err(error) => WireResponse::Error {
                                        message: error.to_string(),
                                    },
                                };
                            let bytes = serde_json::to_vec(&response)
                                .map_err(|error| EngineError::Consequence(error.to_string()))?;
                            send.write_all(&bytes).await.map_err(|error| {
                                EngineError::Consequence(format!("peer write: {error}"))
                            })?;
                            send.finish().map_err(|error| {
                                EngineError::Consequence(format!("peer finish: {error}"))
                            })?;
                            continue;
                        }
                    }
                    let authenticated = match &request {
                        WireRequest::OfferGrant { intro, .. }
                        | WireRequest::Introduce { intro }
                            if from_organ.is_empty() =>
                        {
                            match self.bind_unknown_peer(&peer.to_string(), intro).await {
                                Ok(organ_uid) => organ_uid,
                                Err(error) => {
                                    let response = WireResponse::Refused {
                                        code: "identity_binding_refused".into(),
                                        message: error.to_string(),
                                    };
                                    let bytes = serde_json::to_vec(&response).map_err(|error| {
                                        EngineError::Consequence(error.to_string())
                                    })?;
                                    send.write_all(&bytes).await.map_err(|error| {
                                        EngineError::Consequence(format!("peer write: {error}"))
                                    })?;
                                    send.finish().map_err(|error| {
                                        EngineError::Consequence(format!("peer finish: {error}"))
                                    })?;
                                    continue;
                                }
                            }
                        }
                        _ => from_organ.clone(),
                    };
                    self.handle(&authenticated, &peer.to_string(), request)
                        .await
                }
                Err(error) => WireResponse::Error {
                    message: format!("unreadable request: {error}"),
                },
            };
            let bytes = serde_json::to_vec(&response)
                .map_err(|error| EngineError::Consequence(error.to_string()))?;
            send.write_all(&bytes)
                .await
                .map_err(|error| EngineError::Consequence(format!("peer write: {error}")))?;
            send.finish()
                .map_err(|error| EngineError::Consequence(format!("peer finish: {error}")))?;
        }
        connection.close(0u32.into(), b"frame budget spent");
        Ok(())
    }

    async fn handle(&self, authenticated: &str, peer: &str, request: WireRequest) -> WireResponse {
        match request {
            WireRequest::Introduction | WireRequest::Introduce { .. } => {
                match self.engine.introduction().await {
                    Ok(intro) => WireResponse::Introduction { intro },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::PushOps { batch } => {
                if batch.from_organ != authenticated {
                    tracing::warn!(
                        claimed = %batch.from_organ,
                        proven = %authenticated,
                        "peer pushed a batch attributed to another Organ"
                    );
                    return WireResponse::Refused {
                        code: "batch_peer_mismatch".into(),
                        message: "the batch does not belong to the Organ on this connection".into(),
                    };
                }
                match self.engine.import_op_batch(&batch).await {
                    Ok(applied) => WireResponse::Applied { applied },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchRoster => {
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(Some(organ)) => organ.uid,
                    Ok(None) => {
                        return WireResponse::Roster { roster: None };
                    }
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match self.engine.roster_of(&local).await {
                    Ok(roster) => WireResponse::Roster { roster },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchRevocations => {
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(Some(organ)) => organ.uid,
                    _ => return WireResponse::Revocations { certs: Vec::new() },
                };
                match self.engine.published_revocations(&local).await {
                    Ok(rows) => WireResponse::Revocations {
                        certs: rows
                            .into_iter()
                            .map(|(revoked_key, signature)| RevocationCert {
                                organ_uid: local.clone(),
                                revoked_key,
                                signature,
                            })
                            .collect(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::TransferPost { verb, body } => {
                let handler = self.transfer.lock().expect("transfer handler").clone();
                let Some(handler) = handler else {
                    return WireResponse::Error {
                        message: "this Cell does not deliver Transfers".into(),
                    };
                };
                match handler.handle(verb, body).await {
                    Ok(body) => WireResponse::Transfer { body },
                    Err(message) => WireResponse::Error { message },
                }
            }
            WireRequest::FetchSuccessions => {
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(Some(organ)) => organ.uid,
                    _ => return WireResponse::Successions { certs: Vec::new() },
                };
                match self.engine.published_successions(&local).await {
                    Ok(rows) => WireResponse::Successions {
                        certs: rows
                            .into_iter()
                            .map(|row| SuccessionCert {
                                organ_uid: local.clone(),
                                old_key: row.old_key,
                                new_key: row.new_key,
                                created_at: row.created_at,
                                signature: row.signature,
                            })
                            .collect(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::Enrol {
                token,
                cell_uid,
                node_id,
                label,
                operational_key,
                sealing_key,
            } => {
                let root = match self.engine.root_signer().await {
                    Ok(Some(root)) => root,
                    Ok(None) => {
                        return WireResponse::Refused {
                            code: "root_key_absent".into(),
                            message: "this Cell cannot enrol devices without its root key".into(),
                        };
                    }
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match self
                    .engine
                    .redeem_enrolment(
                        &root,
                        &token,
                        crate::roster::CellEntry {
                            cell_uid,
                            node_id,
                            label,
                            operational_key,
                            sealing_key,
                            front_door: false,
                            capabilities: crate::roster::full_capabilities(),
                        },
                    )
                    .await
                {
                    Ok(roster) => {
                        tracing::info!(version = roster.roster.version, "device enrolled");
                        WireResponse::Roster {
                            roster: Some(roster),
                        }
                    }
                    Err(error) => WireResponse::Refused {
                        code: "enrolment_denied".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::OfferGrant {
                root,
                title,
                intro: _,
            } => {
                if store::offers::standing_refusal(
                    &self.engine.store.pool,
                    store::offers::OfferKind::ThreadInvite,
                    &root,
                    authenticated,
                )
                .await
                .unwrap_or(false)
                {
                    tracing::info!(
                        from = %authenticated,
                        "offer absorbed: this conversation was declined and the refusal still stands"
                    );
                    return WireResponse::Applied { applied: 0 };
                }
                match store::replica::offer(&self.engine.store.pool, &root, authenticated).await {
                    Ok(()) => {
                        match store::invites::put(
                            &self.engine.store.pool,
                            authenticated,
                            &root,
                            &title,
                        )
                        .await
                        {
                            Ok(Some(_)) => {
                                tracing::info!(%root, from = %authenticated, %title, "conversation offered");
                                self.engine.notify_notifications_changed();
                            }
                            Ok(None) => {
                                tracing::info!(
                                    from = %authenticated,
                                    "offer ignored: this Organ already has an invite pending"
                                );
                            }
                            Err(error) => {
                                return WireResponse::Error {
                                    message: error.to_string(),
                                };
                            }
                        }
                        WireResponse::Applied { applied: 0 }
                    }
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::AcceptGrant { root } => {
                match store::replica::state(&self.engine.store.pool, &root, authenticated).await {
                    Ok(Some(_)) => {
                        match store::replica::accept(&self.engine.store.pool, &root, authenticated)
                            .await
                        {
                            Ok(()) => WireResponse::Applied { applied: 0 },
                            Err(error) => WireResponse::Error {
                                message: error.to_string(),
                            },
                        }
                    }
                    Ok(None) => WireResponse::Refused {
                        code: "no_such_offer".into(),
                        message: "no grant was offered to you on that root".into(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::DeclineGrant { root } => {
                match store::replica::state(&self.engine.store.pool, &root, authenticated).await {
                    Ok(Some(_)) => {
                        let _ = store::offers::refuse(
                            &self.engine.store.pool,
                            store::offers::OfferKind::ReplicaGrant,
                            &root,
                            authenticated,
                        )
                        .await;
                        match store::replica::revoke(&self.engine.store.pool, &root, authenticated)
                            .await
                        {
                            Ok(()) => WireResponse::Applied { applied: 0 },
                            Err(error) => WireResponse::Error {
                                message: error.to_string(),
                            },
                        }
                    }
                    Ok(None) => WireResponse::Applied { applied: 0 },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::PushGrantOps { root, batch } => {
                if batch.from_organ != authenticated {
                    tracing::warn!(
                        claimed = %batch.from_organ,
                        proven = %authenticated,
                        "peer pushed a grant batch attributed to another Organ"
                    );
                    return WireResponse::Refused {
                        code: "batch_peer_mismatch".into(),
                        message: "the batch does not belong to the Organ on this connection".into(),
                    };
                }
                match self.engine.import_grant_batch(&root, &batch).await {
                    Ok(applied) => WireResponse::Applied { applied },
                    Err(error) => WireResponse::Refused {
                        code: "grant_denied".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchGrantOpsSince {
                root,
                vector,
                limit,
            } => {
                match store::replica::is_accepted(&self.engine.store.pool, &root, authenticated)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        return WireResponse::Refused {
                            code: "grant_denied".into(),
                            message: "no accepted grant on that root".into(),
                        };
                    }
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                let limit = limit.clamp(1, 2000);
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(organ) => organ.map(|organ| organ.uid).unwrap_or_default(),
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match store::sync_ops::ops_missing_from_vector_in_root(
                    &self.engine.store.pool,
                    &root,
                    &vector,
                    limit,
                )
                .await
                {
                    Ok(rows) => {
                        let head = rows.last().map(|row| row.seq).unwrap_or_default();
                        match self.engine.hydrate_ops(rows).await {
                            Ok(ops) => WireResponse::Ops {
                                from_organ: local,
                                ops,
                                head,
                            },
                            Err(error) => WireResponse::Error {
                                message: error.to_string(),
                            },
                        }
                    }
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchOpsSince { vector, limit } => {
                let limit = limit.clamp(1, 2000);
                if vector.is_empty() {
                    match store::contact_rate::backing_off(
                        &self.engine.store.pool,
                        authenticated,
                        store::contact_rate::RateKind::FullLogServe,
                    )
                    .await
                    {
                        Ok(Some(message)) => {
                            return WireResponse::Refused {
                                code: "rate_limited".into(),
                                message,
                            };
                        }
                        Ok(None) => {
                            if let Err(error) = store::contact_rate::spend(
                                &self.engine.store.pool,
                                authenticated,
                                store::contact_rate::RateKind::FullLogServe,
                            )
                            .await
                            {
                                tracing::warn!(%error, "could not record a full-log serve");
                            }
                        }
                        Err(error) => tracing::warn!(%error, "could not read the serve allowance"),
                    }
                }
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(organ) => organ.map(|organ| organ.uid).unwrap_or_default(),
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match store::sync_ops::seq_covered_by_vector(
                    &self.engine.store.pool,
                    &local,
                    &vector,
                )
                .await
                {
                    Ok(covered) => {
                        if let Err(error) = store::organs::advance_peer_acked_seq(
                            &self.engine.store.pool,
                            authenticated,
                            covered,
                        )
                        .await
                        {
                            tracing::warn!(%error, "could not record peer retention floor");
                        }
                    }
                    Err(error) => tracing::warn!(%error, "could not derive retention floor"),
                }
                let rows = match store::sync_ops::ops_missing_from_vector(
                    &self.engine.store.pool,
                    &local,
                    &vector,
                    limit,
                )
                .await
                {
                    Ok(rows) => rows,
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                let head = rows.last().map(|row| row.seq).unwrap_or_default();
                let scope = store::organs::contact(&self.engine.store.pool, authenticated)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|contact| contact.scope_fields);
                let links =
                    store::sync_ops::resolve_link_scope(&self.engine.store.pool, scope.as_deref())
                        .await
                        .unwrap_or_default();
                let rows = store::sync_ops::narrow_ops_to_scope(rows, scope.as_deref(), &links);
                let rows = match store::visibility::hidden_from_organ(
                    &self.engine.store.pool,
                    authenticated,
                )
                .await
                {
                    Ok(hidden) if !hidden.is_empty() => {
                        let mut kept = Vec::with_capacity(rows.len());
                        for row in rows {
                            match store::visibility::op_hidden_from(
                                &self.engine.store.pool,
                                &hidden,
                                &row.tbl,
                                &row.uid,
                            )
                            .await
                            {
                                Ok(false) => kept.push(row),
                                Ok(true) => {}
                                Err(error) => {
                                    return WireResponse::Error {
                                        message: error.to_string(),
                                    };
                                }
                            }
                        }
                        kept
                    }
                    Ok(_) => rows,
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match self.engine.hydrate_ops(rows).await {
                    Ok(ops) => WireResponse::Ops {
                        from_organ: local,
                        ops,
                        head,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchReference { root, record } => {
                match store::replica::is_accepted(&self.engine.store.pool, &root, authenticated)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        return WireResponse::Refused {
                            code: "no_grant".into(),
                            message: "no accepted grant on that conversation".into(),
                        };
                    }
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                match store::replica::root_references_record(
                    &self.engine.store.pool,
                    &root,
                    &record,
                )
                .await
                {
                    Ok(true) => {}
                    Ok(false) => return reference_gone(),
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                let contact = store::organs::contact(&self.engine.store.pool, authenticated)
                    .await
                    .ok()
                    .flatten();
                match store::visibility::hidden_from_organ(&self.engine.store.pool, authenticated)
                    .await
                {
                    Ok(hidden) if hidden.contains(&record) => return reference_gone(),
                    Ok(_) => {}
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                let protein = protein::Protein {
                    source: protein::Source::Record,
                    filter: vec![protein::Predicate::UidEq(record.clone())],
                    fields: contact.and_then(|contact| contact.scope_fields),
                    include: Default::default(),
                    aggregate: None,
                    order: Vec::new(),
                    limit: Some(1),
                };
                match protein::execute(&self.engine.store, &protein).await {
                    Ok(rows) => match rows.into_iter().next() {
                        Some(row) => {
                            if let Err(error) = store::replica::note_reference_read(
                                &self.engine.store.pool,
                                authenticated,
                                &record,
                                &root,
                            )
                            .await
                            {
                                tracing::warn!(%error, "could not record a reference read");
                            }
                            WireResponse::Reference { row }
                        }
                        None => reference_gone(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchVector { organ_uid } => {
                let local = store::organs::local(&self.engine.store.pool)
                    .await
                    .ok()
                    .flatten()
                    .map(|organ| organ.uid)
                    .unwrap_or_default();
                if organ_uid != authenticated && organ_uid != local {
                    return WireResponse::Refused {
                        code: "not_your_vector".into(),
                        message: "a vector is served only for your Organ or ours".into(),
                    };
                }
                match store::sync_ops::version_vector_for_organ(&self.engine.store.pool, &organ_uid)
                    .await
                {
                    Ok(vector) => WireResponse::Vector { vector },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchDoorRequests { limit } => {
                if !self.is_sibling(authenticated).await {
                    return WireResponse::Refused {
                        code: "not_a_cell_of_this_organ".into(),
                        message: "only this Organ's own Cells may read the door".into(),
                    };
                }
                match store::door::held(&self.engine.store.pool, limit.clamp(1, 500)).await {
                    Ok(rows) => WireResponse::DoorRequests {
                        requests: rows
                            .into_iter()
                            .filter_map(|row| {
                                serde_json::from_str::<Introduction>(&row.intro)
                                    .ok()
                                    .map(|intro| HeldIntroduction {
                                        uid: row.uid,
                                        node_id: row.node_id,
                                        intro,
                                        received_at: row.received_at,
                                    })
                            })
                            .collect(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::ReleaseDoorRequests { uids } => {
                if !self.is_sibling(authenticated).await {
                    return WireResponse::Refused {
                        code: "not_a_cell_of_this_organ".into(),
                        message: "only this Organ's own Cells may clear the door".into(),
                    };
                }
                match store::door::release(&self.engine.store.pool, &uids).await {
                    Ok(()) => WireResponse::Applied {
                        applied: uids.len(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MailboxDeposit { body } => {
                match self.engine.accept_bundle(&body, peer).await {
                    Ok(uid) => WireResponse::MailboxAccepted { uid },
                    Err(refusal) => WireResponse::Refused {
                        code: refusal.code().into(),
                        message: refusal.to_string(),
                    },
                }
            }
            WireRequest::MailboxWaiting { organ_uid, roster } => {
                match self.engine.may_collect(&organ_uid, peer, &roster).await {
                    Ok(true) => {
                        match store::mailbox::waiting(&self.engine.store.pool, &organ_uid).await {
                            Ok(waiting) => WireResponse::MailboxWaiting {
                                bundles: waiting.bundles,
                                bytes: waiting.bytes,
                                oldest_expires_at: waiting.oldest_expires_at,
                            },
                            Err(error) => WireResponse::Error {
                                message: error.to_string(),
                            },
                        }
                    }
                    Ok(false) => not_your_mailbox(),
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MailboxCollect {
                organ_uid,
                roster,
                limit,
            } => match self.engine.may_collect(&organ_uid, peer, &roster).await {
                Ok(true) => {
                    match self
                        .engine
                        .bundles_for(&organ_uid, limit.clamp(1, 256))
                        .await
                    {
                        Ok(held) => WireResponse::MailboxBundles {
                            bundles: held
                                .into_iter()
                                .map(|bundle| MailboxBundle {
                                    uid: bundle.uid,
                                    from_organ: bundle.from_organ,
                                    from_cell: bundle.from_cell,
                                    body: bundle.body,
                                    received_at: bundle.received_at,
                                    expires_at: bundle.expires_at,
                                })
                                .collect(),
                        },
                        Err(error) => WireResponse::Error {
                            message: error.to_string(),
                        },
                    }
                }
                Ok(false) => not_your_mailbox(),
                Err(error) => WireResponse::Error {
                    message: error.to_string(),
                },
            },
            WireRequest::MailboxCollected {
                organ_uid,
                roster,
                uids,
            } => match self.engine.may_collect(&organ_uid, peer, &roster).await {
                Ok(true) => match self.engine.confirm_collected(&organ_uid, &uids).await {
                    Ok(dropped) => WireResponse::Applied {
                        applied: dropped as usize,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                },
                Ok(false) => not_your_mailbox(),
                Err(error) => WireResponse::Error {
                    message: error.to_string(),
                },
            },
            WireRequest::MailboxAskToCarry { roster } => {
                match self.engine.note_carry_request(&roster).await {
                    Ok(()) => WireResponse::MailboxAsked,
                    Err(error) => WireResponse::Refused {
                        code: "mailbox_ask_refused".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MailboxRedeemInvite { token, roster } => {
                match self.engine.redeem_mailbox_invite(&token, &roster).await {
                    Ok(registration) => WireResponse::MailboxCarrying {
                        label: registration.label,
                        quota_bytes: registration.quota_bytes,
                    },
                    Err(error) => WireResponse::Refused {
                        code: "mailbox_invite_refused".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::PublishSealingKey {
                cell_uid,
                sealing_key,
            } => {
                match self
                    .engine
                    .republish_sealing_key(peer, &cell_uid, sealing_key)
                    .await
                {
                    Ok(()) => WireResponse::Applied { applied: 1 },
                    Err(error) => WireResponse::Refused {
                        code: "sealing_key_refused".into(),
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::MailboxExpiries => match self.engine.expiries_for(peer).await {
                Ok(notices) => WireResponse::MailboxExpired {
                    expired: notices
                        .into_iter()
                        .map(|notice| ExpiredMail {
                            uid: notice.uid,
                            to_organ: notice.to_organ,
                            bytes: notice.bytes,
                            received_at: notice.received_at,
                            expired_at: notice.expires_at,
                        })
                        .collect(),
                },
                Err(error) => WireResponse::Error {
                    message: error.to_string(),
                },
            },
            WireRequest::MailboxExpiriesHeard { uids } => {
                match self.engine.forget_expiries(peer, &uids).await {
                    Ok(dropped) => WireResponse::Applied {
                        applied: dropped as usize,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
        }
    }

    async fn is_sibling(&self, authenticated: &str) -> bool {
        !authenticated.is_empty()
            && store::organs::local(&self.engine.store.pool)
                .await
                .ok()
                .flatten()
                .is_some_and(|organ| organ.uid == authenticated)
    }

    pub async fn sync_once(&self) -> Result<usize, EngineError> {
        if let Err(error) = self.reconcile_pending().await {
            tracing::debug!(%error, "pending introductions not reconciled this pass");
        }
        let pushed = self.push_outbox().await?;
        let mut pulled = self.pull_catch_up().await? + self.pull_siblings().await?;
        match self.collect_own_mail().await {
            Ok(count) => pulled += count,
            Err(error) => tracing::debug!(%error, "mail not collected this pass"),
        }
        if let Err(error) = self.publish_own_sealing_key().await {
            tracing::debug!(%error, "this Cell's mail key was not published this pass");
        }
        if let Err(error) = self.hear_about_expired_mail().await {
            tracing::debug!(%error, "carriers not asked about expired mail this pass");
        }
        if let Err(error) = self.engine.sweep_mailbox().await {
            tracing::debug!(%error, "the mailbox was not swept this pass");
        }
        if let Err(error) = self.engine.compact_stale_docs().await {
            tracing::debug!(%error, "collab compaction skipped this pass");
        }
        match self.engine.prune_op_log(false).await {
            Ok(report) if report.removed > 0 => {
                tracing::debug!(
                    removed = report.removed,
                    retained = report.retained,
                    floor = report.floor,
                    "pruned superseded ops"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(%error, "prune skipped this pass"),
        }
        Ok(pushed + pulled)
    }

    pub async fn reconcile_pending(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let mut reconciled = 0;
        for contact in store::organs::pending_introductions(pool).await? {
            let Some(node_id) = contact.node_id.clone() else {
                continue;
            };
            let Ok(id) = node_id.parse::<EndpointId>() else {
                continue;
            };
            let response = match tokio::time::timeout(
                DIAL_TIMEOUT,
                self.request(
                    EndpointAddr::new(id),
                    ALPN_THREAD,
                    &WireRequest::Introduction,
                ),
            )
            .await
            {
                Ok(Ok(response)) => response,
                Ok(Err(_)) | Err(_) => continue,
            };
            let WireResponse::Introduction { intro } = response else {
                continue;
            };

            let pasted_root = crate::trust::keys_of(&self.engine.store, &contact.record_uid)
                .await?
                .into_iter()
                .find(|(key_id, _)| key_id == crate::roster::ROOT_KEY_ID)
                .map(|(_, key)| key);
            let offered_root = intro
                .keys
                .iter()
                .find(|(key_id, _)| key_id == crate::roster::ROOT_KEY_ID)
                .map(|(_, key)| key.clone());
            if let (Some(pasted), Some(offered)) = (&pasted_root, &offered_root) {
                if pasted != offered {
                    tracing::warn!(
                        %node_id,
                        "the Organ at this address presents a different root key than the \
                         code did; leaving the contact pending"
                    );
                    continue;
                }
            }

            let name = contact.head.clone();
            store::organs::forget_contact(pool, &contact.record_uid).await?;
            self.engine
                .adopt_introduction(&intro, contact.proximity)
                .await?;
            if !name.trim().is_empty() {
                store::records::set_text(pool, &intro.organ_uid, Some(name.trim()), None).await?;
            }
            if let Some(root) = pasted_root {
                crate::trust::adopt_key(
                    &self.engine.store,
                    &intro.organ_uid,
                    crate::roster::ROOT_KEY_ID,
                    &root,
                )
                .await?;
            }
            store::organs::set_node_id(pool, &intro.organ_uid, Some(&node_id)).await?;
            store::organs::set_trust(pool, &intro.organ_uid, "known").await?;
            store::organs::set_pending_introduction(pool, &intro.organ_uid, false).await?;
            tracing::info!(
                uid = %intro.organ_uid,
                "a contact added by code is now filed under the uid they declare"
            );
            reconciled += 1;
        }
        Ok(reconciled)
    }

    pub async fn dial(&self, contact: &store::organs::Contact) -> Option<Connection> {
        let connection = self.dial_candidates(contact).await;
        let pool = &self.engine.store.pool;
        let noted = match connection {
            Some(_) => store::organs::mark_reachable(pool, &contact.record_uid).await,
            None => store::organs::mark_unreachable(pool, &contact.record_uid).await,
        };
        if let Err(error) = noted {
            tracing::debug!(%error, contact = %contact.record_uid, "reachability not recorded");
        }
        connection
    }

    async fn dial_candidates(&self, contact: &store::organs::Contact) -> Option<Connection> {
        let mut candidates: Vec<String> = Vec::new();
        if let Some(node_id) = &contact.node_id {
            candidates.push(node_id.clone());
        }
        if let Ok(from_roster) = self.engine.roster_node_ids(&contact.record_uid).await {
            for node_id in from_roster {
                if !candidates.contains(&node_id) {
                    candidates.push(node_id);
                }
            }
        }
        candidates = self.in_preference_order(candidates);
        if let Some(connection) = self.try_candidates(contact, &candidates).await {
            return Some(connection);
        }
        if self.reach == Reach::Local {
            return None;
        }
        let resolved = match self.engine.resolve_public_record(&contact.record_uid).await {
            Ok(Some(record)) => record,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(
                    contact = %contact.record_uid,
                    %error,
                    "the directory record for this contact was refused"
                );
                return None;
            }
        };
        let fresh: Vec<String> = resolved
            .node_ids
            .into_iter()
            .filter(|node_id| !candidates.contains(node_id))
            .collect();
        if fresh.is_empty() {
            return None;
        }
        tracing::info!(
            contact = %contact.record_uid,
            doors = fresh.len(),
            "no known Cell answered; dialing the front door from the directory"
        );
        self.try_candidates(contact, &fresh).await
    }

    fn in_preference_order(&self, candidates: Vec<String>) -> Vec<String> {
        let on_the_lan: Vec<String> = self
            .nearby
            .current()
            .into_iter()
            .map(|peer| peer.node_id)
            .collect();
        let mut ordered = candidates;
        ordered.sort_by_key(|node_id| !on_the_lan.contains(node_id));
        ordered
    }

    async fn try_candidates(
        &self,
        contact: &store::organs::Contact,
        candidates: &[String],
    ) -> Option<Connection> {
        use n0_future::StreamExt as _;

        let alpn = if contact.trust == "known" {
            ALPN_SYNC
        } else {
            ALPN_THREAD
        };
        let mut racing = n0_future::FuturesUnordered::new();
        for (position, candidate) in candidates.iter().enumerate() {
            let Ok(id) = candidate.parse::<EndpointId>() else {
                continue;
            };
            let endpoint = self.endpoint.clone();
            racing.push(async move {
                tokio::time::sleep(DIAL_STAGGER * position as u32).await;
                tokio::time::timeout(DIAL_TIMEOUT, endpoint.connect(EndpointAddr::new(id), alpn))
                    .await
            });
        }
        while let Some(finished) = racing.next().await {
            if let Ok(Ok(connection)) = finished {
                return Some(connection);
            }
        }
        None
    }

    pub async fn push_outbox(&self) -> Result<usize, EngineError> {
        let connections: std::sync::Arc<tokio::sync::Mutex<HashMap<String, Option<Connection>>>> =
            Default::default();
        let wire = self.clone();
        self.engine
            .drain_outbox(move |contact, root, batch| {
                let wire = wire.clone();
                let connections = connections.clone();
                async move {
                    if contact.delivery() == store::organs::Delivery::Mailbox {
                        return wire.mail_now(&contact, root.as_deref(), &batch).await;
                    }
                    let request = match root {
                        Some(ref root) => WireRequest::PushGrantOps {
                            root: root.clone(),
                            batch: batch.clone(),
                        },
                        None => WireRequest::PushOps {
                            batch: batch.clone(),
                        },
                    };
                    let live = wire
                        .live_connections
                        .lock()
                        .expect("live connections")
                        .get(&contact.record_uid)
                        .cloned();
                    if let Some(live) = live {
                        match wire.exchange(&live, &request).await {
                            Ok(WireResponse::Applied { .. }) => return Delivery::Sent,
                            Ok(other) => return Delivery::Failed(format!("{other:?}")),
                            Err(error) => tracing::debug!(
                                contact = %contact.record_uid,
                                %error,
                                "live delta path unavailable; falling back to sync"
                            ),
                        }
                    }
                    let mut open = connections.lock().await;
                    let entry = match open.get(&contact.record_uid) {
                        Some(existing) => existing.clone(),
                        None => {
                            let dialed = wire.dial(&contact).await;
                            open.insert(contact.record_uid.clone(), dialed.clone());
                            dialed
                        }
                    };
                    let Some(connection) = entry else {
                        drop(open);
                        if contact.delivery() == store::organs::Delivery::Direct {
                            return Delivery::Failed(format!("{} unreachable", contact.record_uid));
                        }
                        return wire.mail_if_due(&contact, root.as_deref(), &batch).await;
                    };
                    match wire.exchange(&connection, &request).await {
                        Ok(WireResponse::Applied { .. }) => Delivery::Sent,
                        Ok(other) => Delivery::Failed(format!("{other:?}")),
                        Err(error) => Delivery::Failed(error.to_string()),
                    }
                }
            })
            .await
    }

    pub const MAIL_AFTER: chrono::Duration = chrono::Duration::minutes(10);

    pub const NOTICE_LINGER_DAYS: i64 = 15;

    async fn mail_if_due(
        &self,
        contact: &store::organs::Contact,
        root: Option<&str>,
        batch: &OpBatch,
    ) -> Delivery {
        let unreachable = format!("{} unreachable", contact.record_uid);
        let now = chrono::Utc::now();
        let since = match store::organs::contact(&self.engine.store.pool, &contact.record_uid).await
        {
            Ok(Some(fresh)) => fresh,
            _ => return Delivery::Failed(unreachable),
        };
        let waited = |stamp: &Option<String>| {
            stamp
                .as_deref()
                .and_then(|when| chrono::DateTime::parse_from_rfc3339(when).ok())
                .map(|when| now - when.with_timezone(&chrono::Utc))
        };
        match waited(&since.unreachable_since) {
            Some(elapsed) if elapsed >= Self::MAIL_AFTER => {}
            _ => return Delivery::Failed(unreachable),
        }
        if let Some(elapsed) = waited(&since.mailed_at) {
            if elapsed < Self::MAIL_AFTER {
                return Delivery::Failed(unreachable);
            }
        }
        self.mail_now(contact, root, batch).await
    }

    async fn mail_now(
        &self,
        contact: &store::organs::Contact,
        root: Option<&str>,
        batch: &OpBatch,
    ) -> Delivery {
        let unreachable = format!("{} unreachable", contact.record_uid);
        match self.leave_mail(&contact.record_uid, root, batch).await {
            Ok(MailLeft::Left { carrier, uid }) => {
                tracing::info!(
                    contact = %contact.record_uid, %carrier, %uid,
                    "batch left as mail"
                );
                Delivery::Mailed
            }
            Ok(_) | Err(_) => Delivery::Failed(unreachable),
        }
    }

    async fn pull_siblings(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let Some(organ) = store::organs::local(pool).await? else {
            return Ok(0);
        };
        let Some(signed) = self.engine.roster_of(&organ.uid).await? else {
            return Ok(0);
        };
        let Some(ours) = store::cells::local(pool).await? else {
            return Ok(0);
        };
        let mut pulled = 0usize;
        for member in &signed.roster.cells {
            if member.cell_uid == ours.uid {
                continue;
            }
            let Ok(id) = member.node_id.parse::<EndpointId>() else {
                continue;
            };
            let Ok(Ok(connection)) = tokio::time::timeout(
                DIAL_TIMEOUT,
                self.endpoint.connect(EndpointAddr::new(id), ALPN_SYNC),
            )
            .await
            else {
                self.note_if_stale(member, id).await;
                continue;
            };
            if let Err(error) = self.collect_door_requests(&connection).await {
                tracing::debug!(%error, cell = %member.label, "door not collected this pass");
            }
            if !member.may(crate::roster::CAP_WRITE) {
                continue;
            }
            let vector = store::sync_ops::version_vector_for_organ(pool, &organ.uid).await?;
            let request = WireRequest::FetchOpsSince { vector, limit: 500 };
            if let Ok(WireResponse::Ops { ops, .. }) = self.exchange(&connection, &request).await {
                let batch = OpBatch {
                    from_organ: organ.uid.clone(),
                    ops,
                };
                if !batch.ops.is_empty() && self.engine.import_op_batch(&batch).await.is_ok() {
                    pulled += 1;
                }
            }
        }
        Ok(pulled)
    }

    pub async fn hello(&self, addr: EndpointAddr) -> Option<u32> {
        let connection =
            tokio::time::timeout(DIAL_TIMEOUT, self.endpoint.connect(addr, ALPN_HELLO))
                .await
                .ok()?
                .ok()?;
        let mut recv = tokio::time::timeout(DIAL_TIMEOUT, connection.accept_uni())
            .await
            .ok()?
            .ok()?;
        let raw = tokio::time::timeout(DIAL_TIMEOUT, recv.read_to_end(4096))
            .await
            .ok()?
            .ok()?;
        serde_json::from_slice::<serde_json::Value>(&raw)
            .ok()?
            .get("epoch")
            .and_then(serde_json::Value::as_u64)
            .map(|epoch| epoch as u32)
    }

    async fn note_if_stale(&self, member: &crate::roster::CellEntry, id: EndpointId) {
        let Ok(Ok(connection)) = tokio::time::timeout(
            DIAL_TIMEOUT,
            self.endpoint.connect(EndpointAddr::new(id), ALPN_HELLO),
        )
        .await
        else {
            return;
        };
        let Ok(Ok(mut recv)) = tokio::time::timeout(DIAL_TIMEOUT, connection.accept_uni()).await
        else {
            return;
        };
        let Ok(Ok(raw)) = tokio::time::timeout(DIAL_TIMEOUT, recv.read_to_end(4096)).await else {
            return;
        };
        let epoch = serde_json::from_slice::<serde_json::Value>(&raw)
            .ok()
            .and_then(|body| body.get("epoch").and_then(serde_json::Value::as_u64));
        let Some(epoch) = epoch else { return };
        if epoch as u32 == WIRE_EPOCH {
            return;
        }
        tracing::warn!(
            cell = %member.label,
            node_id = %member.node_id,
            theirs = epoch,
            ours = WIRE_EPOCH,
            "this device of your Organ speaks a different wire epoch and cannot \
             sync until it is updated"
        );
        let mut stale = self.engine.stale_siblings.lock().expect("stale siblings");
        stale.retain(|other: &StaleSibling| other.node_id != member.node_id);
        stale.push(StaleSibling {
            cell_uid: member.cell_uid.clone(),
            node_id: member.node_id.clone(),
            label: member.label.clone(),
            their_epoch: epoch as u32,
            our_epoch: WIRE_EPOCH,
        });
    }

    pub fn stale_siblings(&self) -> Vec<StaleSibling> {
        self.engine
            .stale_siblings
            .lock()
            .expect("stale siblings")
            .clone()
    }

    pub async fn audit_against(
        &self,
        contact_organ: &str,
    ) -> Result<Option<AuditAgreement>, EngineError> {
        let pool = &self.engine.store.pool;
        let Some(contact) = store::organs::contact(pool, contact_organ).await? else {
            return Ok(None);
        };
        let Some(connection) = self.dial(&contact).await else {
            return Ok(None);
        };
        let Some(local) = store::organs::local(pool).await? else {
            return Ok(None);
        };
        let theirs = match self
            .exchange(
                &connection,
                &WireRequest::FetchVector {
                    organ_uid: local.uid.clone(),
                },
            )
            .await?
        {
            WireResponse::Vector { vector } => vector,
            WireResponse::Refused { code, message } => {
                return Err(EngineError::Consequence(format!("{code}: {message}")));
            }
            other => {
                return Err(EngineError::Consequence(format!(
                    "unexpected answer to an audit: {other:?}"
                )));
            }
        };
        let ours = store::sync_ops::version_vector_for_organ(pool, &local.uid).await?;
        let they_lack =
            store::sync_ops::ops_missing_from_vector(pool, &local.uid, &theirs, i64::MAX)
                .await?
                .len();
        let unknown_cells = ours
            .iter()
            .filter(|entry| {
                !theirs
                    .iter()
                    .any(|held| held.actor_cell == entry.actor_cell)
            })
            .count();
        Ok(Some(AuditAgreement {
            contact_organ: contact_organ.to_string(),
            they_lack,
            unknown_cells,
        }))
    }

    async fn collect_door_requests(&self, connection: &Connection) -> Result<(), EngineError> {
        if !self.may_represent().await {
            return Ok(());
        }
        let response = self
            .exchange(connection, &WireRequest::FetchDoorRequests { limit: 50 })
            .await?;
        let WireResponse::DoorRequests { requests } = response else {
            return Ok(());
        };
        let mut taken: Vec<String> = Vec::new();
        for held in requests {
            match self.bind_unknown_peer(&held.node_id, &held.intro).await {
                Ok(organ_uid) => {
                    tracing::info!(
                        organ = %organ_uid,
                        "took a request from the front door; it is waiting for a decision"
                    );
                    taken.push(held.uid);
                }
                Err(error) => {
                    if let Err(error) = store::organs::quarantine(
                        &self.engine.store.pool,
                        &held.intro.organ_uid,
                        "front door: introduction refused",
                        &serde_json::to_string(&held.intro).unwrap_or_default(),
                    )
                    .await
                    {
                        tracing::warn!(%error, "could not quarantine a refused door request");
                    }
                    tracing::warn!(%error, "a held request could not be bound; quarantined");
                    taken.push(held.uid);
                }
            }
        }
        if !taken.is_empty() {
            self.exchange(
                connection,
                &WireRequest::ReleaseDoorRequests { uids: taken },
            )
            .await?;
        }
        Ok(())
    }

    async fn refresh_roster(&self, connection: &Connection, organ_uid: &str) {
        if let Ok(WireResponse::Roster {
            roster: Some(roster),
        }) = self.exchange(connection, &WireRequest::FetchRoster).await
        {
            self.adopt_fetched_roster(&roster, organ_uid).await;
        }
    }

    async fn adopt_fetched_roster(&self, roster: &crate::roster::SignedRoster, organ_uid: &str) {
        if roster.roster.organ_uid != organ_uid {
            return;
        }
        match self.engine.adopt_roster(roster).await {
            Ok(crate::roster::RosterOutcome::Refused) => {
                tracing::warn!(
                    organ = %organ_uid,
                    "REFUSED a roster that does not chain from a held key"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(%error, "roster refresh failed"),
        }
    }

    async fn pull_catch_up(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let mut pulled = 0usize;
        for contact in store::organs::contacts(pool).await? {
            if contact.trust == "blocked" {
                continue;
            }
            let Some(connection) = self.dial(&contact).await else {
                continue;
            };
            self.refresh_roster(&connection, &contact.record_uid).await;
            if contact.trust == "known" && contact.sync_in {
                let vector =
                    store::sync_ops::version_vector_for_organ(pool, &contact.record_uid).await?;
                let request = WireRequest::FetchOpsSince { vector, limit: 500 };
                if let Ok(WireResponse::Ops { ops, head, .. }) =
                    self.exchange(&connection, &request).await
                {
                    let batch = OpBatch {
                        from_organ: contact.record_uid.clone(),
                        ops,
                    };
                    if !batch.ops.is_empty() && self.engine.import_op_batch(&batch).await.is_ok() {
                        store::organs::set_last_synced_seq(pool, &contact.record_uid, head).await?;
                        pulled += 1;
                    }
                }
            }
            for root in store::replica::roots_for_contact(pool, &contact.record_uid).await? {
                let vector = store::sync_ops::version_vector_for_root(pool, &root).await?;
                let request = WireRequest::FetchGrantOpsSince {
                    root: root.clone(),
                    vector,
                    limit: 500,
                };
                if let Ok(WireResponse::Ops { ops, .. }) =
                    self.exchange(&connection, &request).await
                {
                    let batch = OpBatch {
                        from_organ: contact.record_uid.clone(),
                        ops,
                    };
                    if !batch.ops.is_empty()
                        && self.engine.import_grant_batch(&root, &batch).await.is_ok()
                    {
                        pulled += 1;
                    }
                }
            }
            if contact.trust != "known" {
                continue;
            }
            if let Ok(WireResponse::Revocations { certs }) = self
                .exchange(&connection, &WireRequest::FetchRevocations)
                .await
            {
                for cert in certs {
                    if cert.organ_uid != contact.record_uid {
                        continue;
                    }
                    match self
                        .engine
                        .adopt_revocation(&cert.organ_uid, &cert.revoked_key, &cert.signature)
                        .await
                    {
                        Ok(true) => tracing::warn!(
                            organ = %cert.organ_uid,
                            "adopted a key revocation certificate"
                        ),
                        _ => {}
                    }
                }
            }
            if let Ok(WireResponse::Successions { certs }) = self
                .exchange(&connection, &WireRequest::FetchSuccessions)
                .await
            {
                for cert in certs {
                    if cert.organ_uid != contact.record_uid {
                        continue;
                    }
                    match self
                        .engine
                        .adopt_succession(
                            &cert.organ_uid,
                            &cert.old_key,
                            &cert.new_key,
                            &cert.created_at,
                            &cert.signature,
                        )
                        .await
                    {
                        Ok(true) => tracing::warn!(
                            organ = %cert.organ_uid,
                            "adopted a key succession: this Organ rotated its identity key"
                        ),
                        Ok(false) => tracing::warn!(
                            organ = %cert.organ_uid,
                            old = %cert.old_key,
                            "REFUSED a key succession that does not chain from a key we hold"
                        ),
                        Err(error) => tracing::warn!(%error, "succession could not be evaluated"),
                    }
                }
            }
            connection.close(0u32.into(), b"done");
        }
        Ok(pulled)
    }

    pub async fn deposit_bundle(
        &self,
        addr: impl Into<EndpointAddr>,
        body: &str,
    ) -> Result<Result<String, String>, EngineError> {
        match self
            .request(
                addr,
                ALPN_MAILBOX,
                &WireRequest::MailboxDeposit {
                    body: body.to_string(),
                },
            )
            .await?
        {
            WireResponse::MailboxAccepted { uid } => Ok(Ok(uid)),
            WireResponse::Refused { code, .. } => Ok(Err(code)),
            WireResponse::Error { message } => Err(EngineError::Consequence(message)),
            _ => Err(EngineError::Consequence(
                "carrier answered a deposit with something else".into(),
            )),
        }
    }

    pub async fn collect_mail(
        &self,
        addr: impl Into<EndpointAddr>,
        organ_uid: &str,
        roster: &SignedRoster,
        limit: i64,
    ) -> Result<Vec<MailboxBundle>, EngineError> {
        let addr = addr.into();
        let response = self
            .request(
                addr.clone(),
                ALPN_MAILBOX,
                &WireRequest::MailboxCollect {
                    organ_uid: organ_uid.to_string(),
                    roster: roster.clone(),
                    limit,
                },
            )
            .await?;
        let bundles = match response {
            WireResponse::MailboxBundles { bundles } => bundles,
            WireResponse::Refused { message, .. } => {
                return Err(EngineError::Consequence(message));
            }
            WireResponse::Error { message } => return Err(EngineError::Consequence(message)),
            _ => {
                return Err(EngineError::Consequence(
                    "carrier answered a collection with something else".into(),
                ));
            }
        };
        if !bundles.is_empty() {
            let uids = bundles.iter().map(|b| b.uid.clone()).collect::<Vec<_>>();
            let _ = self
                .request(
                    addr,
                    ALPN_MAILBOX,
                    &WireRequest::MailboxCollected {
                        organ_uid: organ_uid.to_string(),
                        roster: roster.clone(),
                        uids,
                    },
                )
                .await;
        }
        Ok(bundles)
    }

    pub async fn leave_mail(
        &self,
        to_organ: &str,
        root: Option<&str>,
        batch: &OpBatch,
    ) -> Result<MailLeft, EngineError> {
        let Some(their_roster) = self.engine.roster_of(to_organ).await? else {
            return Ok(MailLeft::NoRoster);
        };
        if their_roster.roster.pickup.is_empty() {
            return Ok(MailLeft::NoPickupPoints);
        }
        let bundle = self.engine.seal_batch_for(to_organ, root, batch).await?;
        let body =
            serde_json::to_string(&bundle).map_err(|e| EngineError::Consequence(e.to_string()))?;
        let mut refusals = Vec::new();
        for point in &their_roster.roster.pickup {
            let Ok(id) = point.node_id.parse::<EndpointId>() else {
                refusals.push((point.organ_uid.clone(), "unusable_node_id".to_string()));
                continue;
            };
            match self.deposit_bundle(EndpointAddr::new(id), &body).await {
                Ok(Ok(uid)) => {
                    store::mail_left::record(
                        &self.engine.store.pool,
                        &uid,
                        &point.organ_uid,
                        &point.node_id,
                        to_organ,
                    )
                    .await?;
                    return Ok(MailLeft::Left {
                        carrier: point.organ_uid.clone(),
                        uid,
                    });
                }
                Ok(Err(code)) => refusals.push((point.organ_uid.clone(), code)),
                Err(_) => refusals.push((point.organ_uid.clone(), "unreachable".to_string())),
            }
        }
        Ok(MailLeft::NoneAccepted { refusals })
    }

    pub async fn collect_own_mail(&self) -> Result<usize, EngineError> {
        let points = self.engine.own_pickup_points().await?;
        if points.is_empty() {
            return Ok(0);
        }
        let Some(local) = store::organs::local(&self.engine.store.pool).await? else {
            return Ok(0);
        };
        let Some(ours) = self.engine.roster_of(&local.uid).await? else {
            return Ok(0);
        };
        let mut imported = 0usize;
        for point in points {
            let Ok(id) = point.node_id.parse::<EndpointId>() else {
                continue;
            };
            let Ok(bundles) = self
                .collect_mail(EndpointAddr::new(id), &local.uid, &ours, 50)
                .await
            else {
                continue;
            };
            for held in bundles {
                let Ok(bundle) = serde_json::from_str::<crate::seal::SealedBundle>(&held.body)
                else {
                    tracing::warn!(carrier = %point.organ_uid, "a collected bundle would not parse");
                    continue;
                };
                match self.engine.open_mailed(&bundle).await {
                    Ok(opened) => match self.engine.import_mailed_batch(&opened).await {
                        Ok(count) => imported += count,
                        Err(error) => tracing::warn!(%error, "a mailed batch was refused"),
                    },
                    Err(error) => tracing::warn!(%error, "a collected bundle could not be opened"),
                }
            }
        }
        Ok(imported)
    }

    pub async fn publish_own_sealing_key(&self) -> Result<bool, EngineError> {
        let pool = &self.engine.store.pool;
        let (Some(local_organ), Some(local_cell)) = (
            store::organs::local(pool).await?,
            store::cells::local(pool).await?,
        ) else {
            return Ok(false);
        };
        let Some(current) = self.engine.published_sealing_key().await? else {
            return Ok(false);
        };
        let Some(held) = self.engine.roster_of(&local_organ.uid).await? else {
            return Ok(false);
        };
        let published = held
            .roster
            .cells
            .iter()
            .find(|cell| cell.cell_uid == local_cell.uid)
            .and_then(|cell| cell.sealing_key.clone());
        if published.as_ref() == Some(&current) {
            return Ok(true);
        }
        let ours = self.node_id().to_string();
        if self
            .engine
            .republish_sealing_key(&ours, &local_cell.uid, current.clone())
            .await
            .is_ok()
        {
            return Ok(true);
        }
        for sibling in &held.roster.cells {
            if sibling.cell_uid == local_cell.uid {
                continue;
            }
            let Ok(id) = sibling.node_id.parse::<EndpointId>() else {
                continue;
            };
            let asked = self
                .request(
                    EndpointAddr::new(id),
                    ALPN_THREAD,
                    &WireRequest::PublishSealingKey {
                        cell_uid: local_cell.uid.clone(),
                        sealing_key: current.clone(),
                    },
                )
                .await;
            if matches!(asked, Ok(WireResponse::Applied { .. })) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn hear_about_expired_mail(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let _ = store::mail_left::prune(
            pool,
            crate::seal::RETENTION_DAYS + crate::seal::GRACE_DAYS + Self::NOTICE_LINGER_DAYS,
        )
        .await;
        let carriers = store::mail_left::carriers_to_ask(pool).await?;
        let mut believed = 0usize;
        for node in carriers {
            let Ok(id) = node.parse::<EndpointId>() else {
                continue;
            };
            let Ok(expired) = self.ask_expiries(EndpointAddr::new(id)).await else {
                continue;
            };
            if expired.is_empty() {
                continue;
            }
            let reports: Vec<(String, String)> = expired
                .iter()
                .map(|one| (one.uid.clone(), one.expired_at.clone()))
                .collect();
            match self.engine.note_expired_mail(&node, &reports).await {
                Ok(ours) => believed += ours.len(),
                Err(error) => tracing::warn!(%error, "an expiry report could not be recorded"),
            }
            let uids: Vec<String> = expired.into_iter().map(|one| one.uid).collect();
            let _ = self
                .request(
                    EndpointAddr::new(id),
                    ALPN_MAILBOX,
                    &WireRequest::MailboxExpiriesHeard { uids },
                )
                .await;
        }
        Ok(believed)
    }

    async fn ask_expiries(
        &self,
        addr: impl Into<EndpointAddr>,
    ) -> Result<Vec<ExpiredMail>, EngineError> {
        match self
            .request(addr, ALPN_MAILBOX, &WireRequest::MailboxExpiries)
            .await?
        {
            WireResponse::MailboxExpired { expired } => Ok(expired),
            WireResponse::Refused { .. } => Ok(Vec::new()),
            WireResponse::Error { message } => Err(EngineError::Consequence(message)),
            _ => Err(EngineError::Consequence(
                "carrier answered an expiry question with something else".into(),
            )),
        }
    }

    pub async fn carrier_probe(&self, node_id: &str) -> CarrierProbe {
        let Ok(id) = node_id.parse::<EndpointId>() else {
            return CarrierProbe::Unreachable;
        };
        let pool = &self.engine.store.pool;
        let Ok(Some(local)) = store::organs::local(pool).await else {
            return CarrierProbe::Unreachable;
        };
        let Ok(Some(ours)) = self.engine.roster_of(&local.uid).await else {
            return CarrierProbe::Unreachable;
        };
        match self
            .request(
                EndpointAddr::new(id),
                ALPN_MAILBOX,
                &WireRequest::MailboxWaiting {
                    organ_uid: local.uid.clone(),
                    roster: ours,
                },
            )
            .await
        {
            Ok(WireResponse::MailboxWaiting {
                bundles,
                bytes,
                oldest_expires_at,
            }) => CarrierProbe::Carrying(store::mailbox::Waiting {
                bundles,
                bytes,
                oldest_expires_at,
            }),
            Ok(WireResponse::Refused { .. }) => CarrierProbe::Refused,
            _ => CarrierProbe::Unreachable,
        }
    }

    pub async fn ask_to_be_carried(&self, node_id: &str) -> Result<(), EngineError> {
        let id = node_id
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("that is not a usable device address".into()))?;
        let pool = &self.engine.store.pool;
        let local = store::organs::local(pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Organ".into()))?;
        let ours = self.engine.roster_of(&local.uid).await?.ok_or_else(|| {
            EngineError::Consequence(
                "this Organ has published no device list, so a carrier would have no way to \
                 check who is collecting later."
                    .into(),
            )
        })?;
        match self
            .request(
                EndpointAddr::new(id),
                ALPN_MAILBOX,
                &WireRequest::MailboxAskToCarry { roster: ours },
            )
            .await?
        {
            WireResponse::MailboxAsked => Ok(()),
            WireResponse::Refused { message, .. } => Err(EngineError::Consequence(message)),
            other => Err(EngineError::Consequence(format!("{other:?}"))),
        }
    }

    pub async fn redeem_mailbox_invite(
        &self,
        node_id: &str,
        token: &str,
    ) -> Result<(String, i64), EngineError> {
        let id = node_id
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("that is not a usable device address".into()))?;
        let pool = &self.engine.store.pool;
        let local = store::organs::local(pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Organ".into()))?;
        let ours = self.engine.roster_of(&local.uid).await?.ok_or_else(|| {
            EngineError::Consequence(
                "this Organ has published no device list, so there is nothing to register.".into(),
            )
        })?;
        match self
            .request(
                EndpointAddr::new(id),
                ALPN_MAILBOX,
                &WireRequest::MailboxRedeemInvite {
                    token: token.to_string(),
                    roster: ours,
                },
            )
            .await?
        {
            WireResponse::MailboxCarrying { label, quota_bytes } => Ok((label, quota_bytes)),
            WireResponse::Refused { message, .. } => Err(EngineError::Consequence(message)),
            other => Err(EngineError::Consequence(format!("{other:?}"))),
        }
    }

    pub async fn mail_waiting(
        &self,
        addr: impl Into<EndpointAddr>,
        organ_uid: &str,
        roster: &SignedRoster,
    ) -> Result<store::mailbox::Waiting, EngineError> {
        match self
            .request(
                addr,
                ALPN_MAILBOX,
                &WireRequest::MailboxWaiting {
                    organ_uid: organ_uid.to_string(),
                    roster: roster.clone(),
                },
            )
            .await?
        {
            WireResponse::MailboxWaiting {
                bundles,
                bytes,
                oldest_expires_at,
            } => Ok(store::mailbox::Waiting {
                bundles,
                bytes,
                oldest_expires_at,
            }),
            WireResponse::Refused { message, .. } => Err(EngineError::Consequence(message)),
            WireResponse::Error { message } => Err(EngineError::Consequence(message)),
            _ => Err(EngineError::Consequence(
                "carrier answered with something else".into(),
            )),
        }
    }

    pub async fn request(
        &self,
        addr: impl Into<EndpointAddr>,
        alpn: &[u8],
        request: &WireRequest,
    ) -> Result<WireResponse, EngineError> {
        let connection =
            self.endpoint.connect(addr, alpn).await.map_err(|error| {
                EngineError::Consequence(format!("iroh connect failed: {error}"))
            })?;
        let response = self.exchange(&connection, request).await;
        connection.close(0u32.into(), b"done");
        response
    }

    async fn exchange(
        &self,
        connection: &Connection,
        request: &WireRequest,
    ) -> Result<WireResponse, EngineError> {
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| EngineError::Consequence(format!("peer open: {error}")))?;
        let bytes = serde_json::to_vec(request)
            .map_err(|error| EngineError::Consequence(error.to_string()))?;
        send.write_all(&bytes)
            .await
            .map_err(|error| EngineError::Consequence(format!("peer write: {error}")))?;
        send.finish()
            .map_err(|error| EngineError::Consequence(format!("peer finish: {error}")))?;
        let raw = recv
            .read_to_end(MAX_FRAME_BYTES)
            .await
            .map_err(|error| EngineError::Consequence(format!("peer read: {error}")))?;
        serde_json::from_slice(&raw)
            .map_err(|error| EngineError::Consequence(format!("unreadable response: {error}")))
    }
}

#[async_trait::async_trait]
impl crate::enrolment::CellTransport for Wire {
    async fn enrol(&self, invite: &EnrolmentInvite) -> Result<SignedRoster, EngineError> {
        Wire::enrol(self, invite).await
    }

    async fn audit_against(
        &self,
        contact_organ: &str,
    ) -> Result<Option<AuditAgreement>, EngineError> {
        Wire::audit_against(self, contact_organ).await
    }

    async fn carrier_probe(&self, node_id: &str) -> CarrierProbe {
        Wire::carrier_probe(self, node_id).await
    }

    async fn collect_mail_now(&self) -> Result<usize, EngineError> {
        Wire::collect_own_mail(self).await
    }

    async fn sync_now(&self) -> Result<usize, EngineError> {
        Wire::sync_once(self).await
    }

    async fn ask_to_be_carried(&self, node_id: &str) -> Result<(), EngineError> {
        Wire::ask_to_be_carried(self, node_id).await
    }

    async fn redeem_mailbox_invite(
        &self,
        node_id: &str,
        token: &str,
    ) -> Result<(String, i64), EngineError> {
        Wire::redeem_mailbox_invite(self, node_id, token).await
    }
}

fn reference_gone() -> WireResponse {
    WireResponse::Refused {
        code: "not_shared".into(),
        message: "that is no longer shared with you".into(),
    }
}
