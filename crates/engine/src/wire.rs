//! iroh peer transport (Ontology §11 "Transport: iroh").
//!
//! What changes versus the signed-HTTP path in `peers.rs`: how two Organs find
//! and reach each other. What does NOT change: the op log, checkpoints, Loro
//! merge, trust, visibility, and `sync_out`/`sync_in`. The JSON bodies carried
//! here are the same ones `/organ/inbox` and `/organ/ops` carry today.
//!
//! The security shape, stated once because everything below depends on it: an
//! iroh connection is MUTUALLY AUTHENTICATED at the QUIC/TLS handshake against
//! raw public keys, so `Connection::remote_id()` returns a peer identity that
//! has already proven possession of the matching private key. That single call
//! replaces `verify_signed_request` — the timestamp freshness window, the
//! replay bound, and the per-request signature all become unnecessary, because
//! there is no unauthenticated moment on the wire to defend.
//!
//! Payload signing is NOT retired by any of this. Transport auth answers "who
//! is on this socket"; op-batch signatures answer "who wrote this op", which
//! must still hold a year later, from a backup, with no connection in sight.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::error::EngineError;
use crate::sync::{Introduction, OpBatch, WireOp};

/// Re-exported so callers can name a peer without depending on iroh directly —
/// the version is pinned here, in one place, and stays that way.
pub use iroh::{EndpointAddr as PeerAddr, EndpointId as PeerId};

/// Sync between Organs that already know each other. Versioned so a protocol
/// change bumps to `/2` and both can be served during a transition — an old
/// peer gets old behaviour instead of a broken half-upgrade (Ontology §11
/// "Compatibility and revocation floor").
pub const ALPN_SYNC: &[u8] = b"lince/sync/1";

/// First contact from an Organ we hold no contact row for. Deliberately a
/// SEPARATE ALPN rather than a check inside the sync handler: a stranger then
/// cannot negotiate the sync protocol at all, so the gate holds at the TLS
/// layer and the sync handler never runs for an unknown peer even if a later
/// bug weakens its own checks.
pub const ALPN_THREAD: &[u8] = b"lince/thread/1";

/// A live session: a contact Organ driving a Protein/Action/collab session on
/// THIS Cell, as the Person its login binds it to.
///
/// Separate from sync because it is the opposite direction of trust. Sync
/// exchanges what two Cells have both agreed to replicate; a live session is
/// someone acting INSIDE this Cell, so it is served only where a login was
/// explicitly granted and every read is gated by that Person's visibility.
///
/// Riding iroh rather than HTTPS is what makes it survive changing networks:
/// there is no hostname to go stale and no certificate tied to one, and QUIC
/// migrates the path under a connection that stays open.
pub const ALPN_LIVE: &[u8] = b"lince/live/1";

/// Ceiling on one request or response frame. A peer is authenticated but not
/// therefore trusted with unbounded memory — an authenticated contact having
/// a bug is exactly as fatal as a hostile one if nothing bounds the read.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Ceiling on requests served over ONE connection before it is closed and the
/// peer must redial. Same reasoning as the frame cap: `MAX_FRAME_BYTES` bounds
/// how big a request is, this bounds how many, and without it an authenticated
/// contact can hold a connection open issuing frames forever. Authenticated is
/// not the same as trusted with unbounded resources. Redialing is cheap.
pub const MAX_FRAMES_PER_CONNECTION: usize = 4096;

/// The mDNS service name Lince advertises under. Deliberately NOT iroh's
/// default (`irohv1`): that would list every unrelated iroh application on the
/// network as a "nearby Organ". Only Lince Cells answer to this.
pub const MDNS_SERVICE_NAME: &str = "lince";

/// How long one dial attempt may take before the sync pass moves on.
pub const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// The per-Cell node key, created at first boot at mode 0600.
///
/// This is NOT the Organ identity key, and the separation is deliberate from
/// day one even on a single-Cell Organ (Ontology §11). The node key is on the
/// network constantly and lives on every device including the least trusted
/// one; if it doubled as the identity key, compromising any running Cell would
/// forge that Organ's history forever. Split, a stolen node key costs one
/// connection identity and the attacker still cannot sign a single op.
/// Separating later would invalidate every key anyone had already saved.
pub fn node_secret(path: &Path) -> Result<SecretKey, EngineError> {
    Ok(SecretKey::from_bytes(&crate::trust::load_or_create_secret(
        path,
    )?))
}

/// How this endpoint should be reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// LAN plus internet: relays, DNS and pkarr address publishing. The
    /// DEFAULT, because a Cell that is not resolvable across the internet
    /// cannot serve the case that motivates the whole design — the always-on
    /// Cell telling the phone about a change the laptop made.
    Internet,
    /// No relays and no address publishing: reachable only where a direct
    /// path already exists. Used by tests, and by anyone who wants a Cell to
    /// leak neither approximate location nor online-hours to key holders.
    Local,
}

/// A short, human-comparable fingerprint of a NodeId.
///
/// This is DISAMBIGUATION, not a security check, and the UI must not imply
/// otherwise (Ontology §11, settled 2026-08-03). It answers "which of these
/// rows is my friend" in a room full of peers. It is derived from the real
/// key, so it cannot be spoofed by choosing a display name — but under iroh
/// the address already IS the key, so nothing is being verified here that the
/// handshake will not verify anyway.
pub fn node_fingerprint(id: &EndpointId) -> String {
    let text = id.to_string();
    text.chars().take(8).collect::<String>().to_uppercase()
}

/// An Organ seen on the local network. Defined in the nucleus because Protein
/// serves this list and cannot depend on the engine that fills it.
pub use nucleus::nearby::NearbyPeer;

/// Serves live sessions, installed from above.
///
/// The session state machine lives in `transport`, which depends on this
/// crate — so the engine cannot call it directly. Rather than invert that (or
/// move the state machine down here, where the Ledger has no business knowing
/// about sockets), the wire takes a handler and hands it authenticated
/// connections.
#[async_trait::async_trait]
pub trait LiveSessions: Send + Sync {
    /// Drive one live connection to completion. `organ_uid` is the contact the
    /// HANDSHAKE proved and `person_uid` is who their login says they act as;
    /// neither is ever read out of anything the peer sent.
    async fn serve(&self, organ_uid: String, person_uid: String, connection: Connection);
}

/// The in-memory nearby list, fed by the mDNS subscription.
///
/// Note what is NOT here versus the retired multicast announce: no
/// `organ_uid`. The old announce broadcast the Organ uid to the whole LAN so
/// the receiver could tell whether it was a known contact; under iroh the
/// NodeId answers that directly through `organ_contact.node_id`, and the uid
/// is learned at pairing over an authenticated connection instead. Strictly
/// less is published, and nothing is lost.
#[derive(Clone, Default)]
pub struct Nearby {
    inner: Arc<Mutex<HashMap<String, NearbyPeer>>>,
}

impl Nearby {
    /// Sorted by NodeId: the backing map has no order, and an arbitrary one
    /// would make a rendered list jump around and defeat change detection.
    pub fn current(&self) -> Vec<NearbyPeer> {
        let inner = self.inner.lock().expect("nearby lock");
        let mut peers: Vec<NearbyPeer> = inner.values().cloned().collect();
        peers.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        peers
    }

    /// Record a peer as present. The mDNS subscription is the only caller in
    /// production; it is public so a test can stand in for a LAN without
    /// binding a real endpoint.
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

    /// Drop a peer that stopped announcing. The list IS the presence, so
    /// removal is how a departure is expressed.
    pub fn forget(&self, node_id: &str) {
        let mut inner = self.inner.lock().expect("nearby lock");
        inner.remove(node_id);
    }
}

/// One request/response exchange, carrying the same JSON the HTTP peer routes
/// carry. Internally tagged so an unrecognised `op` from a NEWER peer fails to
/// deserialize and is answered with an error, rather than being silently
/// misread as something else (Ontology §11: fail closed on the unknown).
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WireRequest {
    /// Key and name exchange. Under iroh this establishes nothing the
    /// connection did not already establish — it is no longer a challenge.
    Introduction,
    PushOps {
        batch: OpBatch,
    },
    FetchOps {
        after: i64,
        limit: i64,
    },
    /// Offer a conversation. Cannot itself ride the grant channel — there is
    /// no grant yet — so it is its own verb on the sync ALPN.
    OfferGrant {
        root: String,
        title: String,
    },
    /// Accept an offer. Only now do ops move.
    AcceptGrant {
        root: String,
    },
    /// Individual-replica ops. `root` is the CHANNEL, checked against the
    /// local grant table on arrival; it is never read out of an op.
    PushGrantOps {
        root: String,
        batch: OpBatch,
    },
    FetchGrantOps {
        root: String,
        after: i64,
        limit: i64,
    },
    /// This Organ's signed Cell roster. Served on `lince/sync/1` only — the
    /// CONTACT tier of two-tier publishing. The public tier (what a stranger
    /// resolving a published key learns) is the front-door Cell alone, so a
    /// key on a website never discloses how many devices you have, their
    /// current addresses, or which are online right now.
    FetchRoster,
    /// Revocation certificates this Organ has published about ITS OWN keys.
    /// Pulled on every sync pass, so a revoked key stops being accepted by
    /// contacts without anyone having to be told out of band.
    FetchRevocations,
    /// A new device asking to join this Organ's roster, presenting a
    /// single-use enrolment token. Served on `lince/thread/1`, because the
    /// enrolling device is by definition not yet a contact of anything.
    Enrol {
        token: String,
        cell_uid: String,
        node_id: String,
        label: String,
        operational_key: String,
    },
}

/// One published revocation, as it travels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationCert {
    pub organ_uid: String,
    pub revoked_key: String,
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
    Ops {
        from_organ: String,
        ops: Vec<WireOp>,
        head: i64,
    },
    /// A refusal the peer may act on. Never leaks whether a contact row exists
    /// beyond what the ALPN gate already revealed by accepting the connection.
    Error {
        message: String,
    },
    /// Distinct from `Error` because these are SECURITY events, not failures.
    /// An import that fails is a data problem; a peer sending a batch
    /// attributed to a third Organ is an attempt at something, and the two
    /// must be distinguishable to the client and in any future audit.
    Refused {
        code: String,
        message: String,
    },
}

/// The Cell's iroh endpoint plus the engine it serves.
#[derive(Clone)]
pub struct Wire {
    endpoint: Endpoint,
    engine: Arc<Engine>,
    nearby: Nearby,
    /// Addresses we were TOLD rather than discovered.
    ///
    /// Two real uses: a QR scanned in person can carry the NodeId *and* the
    /// current addresses, so an in-person exchange needs no discovery
    /// mechanism at all — which is what makes pairing work on guest wifi,
    /// hotels and corporate networks where mDNS is blocked. And a Cell
    /// configured with a static peer address needs somewhere to put it.
    known_addrs: iroh::address_lookup::MemoryLookup,
    /// Installed from above; `None` means this Cell serves no live sessions,
    /// which is the correct behaviour for a headless or sync-only Cell.
    live: Arc<Mutex<Option<Arc<dyn LiveSessions>>>>,
}

impl Wire {
    /// Bind an endpoint on `secret`, serving both ALPNs.
    ///
    /// Discovery is an Endpoint BUILDER option fixed at construction, so
    /// changing `reach` means rebinding rather than mutating — the caller is
    /// expected to restart the endpoint the way the File Sync live supervisor
    /// restarts watchers on a config Fact, not to demand a reboot.
    pub async fn bind(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
    ) -> Result<Wire, EngineError> {
        Wire::bind_as(engine, secret, reach, None).await
    }

    /// Bind and advertise `display_name` on the LAN.
    ///
    /// The name travels in iroh `UserData` (245 bytes max, truncated rather
    /// than rejected — a long name must not be what stops a Cell binding). It
    /// preserves what the retired multicast announce gave the nearby list: a
    /// human label to recognise, explicitly untrusted on arrival.
    pub async fn bind_as(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        display_name: Option<&str>,
    ) -> Result<Wire, EngineError> {
        Self::bind_with_discovery(engine, secret, reach, display_name, true).await
    }

    /// Bind with an explicit LAN-presence choice.
    ///
    /// `local_discovery` controls only passive mDNS advertising/listening. It
    /// does not disable the endpoint or prevent known contacts from dialing a
    /// saved NodeId/address. Kept separate from [`Reach`] because internet
    /// publication and being visible in the room are independent disclosures.
    pub async fn bind_with_discovery(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        display_name: Option<&str>,
        local_discovery: bool,
    ) -> Result<Wire, EngineError> {
        let alpns = vec![ALPN_SYNC.to_vec(), ALPN_THREAD.to_vec(), ALPN_LIVE.to_vec()];
        let endpoint = match reach {
            Reach::Internet => Endpoint::builder(presets::N0),
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
            nearby: Nearby::default(),
            known_addrs,
            live: Arc::new(Mutex::new(None)),
        };
        if local_discovery {
            wire.spawn_mdns();
        }
        *wire.engine.nearby.lock().expect("nearby handle") = Some(wire.nearby.clone());
        Ok(wire)
    }

    /// Install the live-session handler. Without one this Cell answers
    /// `lince/live/1` by closing, which is correct for a headless or
    /// sync-only Cell rather than an error.
    pub fn set_live_handler(&self, handler: Arc<dyn LiveSessions>) {
        *self.live.lock().expect("live handler") = Some(handler);
    }

    /// Open a live session against a contact: dial them on `lince/live/1` and
    /// hand back the connection for a driver to stream frames over.
    ///
    /// This is the roaming half of the design. The connection is authenticated
    /// by key rather than by address, so when this machine changes network
    /// there is no hostname to go stale and no certificate bound to one —
    /// QUIC migrates the path and the session continues.
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

    /// Record where a peer can be reached, bypassing discovery.
    pub fn remember_addr(&self, addr: PeerAddr) {
        self.known_addrs.add_endpoint_info(addr);
    }

    /// This Cell's pairing invite: NodeId, the Organ root key, a label, and
    /// the addresses we are currently reachable at.
    ///
    /// The addresses are what make an in-person scan work with no discovery
    /// at all, which is the case that matters on guest wifi and in hotels.
    /// They are a hint that expires, not identity — the NodeId is identity,
    /// and a stale address simply fails to dial.
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

    /// Pair from an invite: remember its addresses, dial, fetch the
    /// Introduction, adopt the contact under a name the LOCAL user chose.
    ///
    /// `name` is never taken from the invite. A self-declared label is a claim
    /// by whoever made the code, and letting it become the contact's name is
    /// how "Eduardo's laptop" ends up on a stranger's row.
    pub async fn pair_with(
        &self,
        invite: &crate::pairing::PairingInvite,
        name: &str,
    ) -> Result<String, EngineError> {
        let node_id: EndpointId = invite
            .node_id
            .parse()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;

        // Teach the endpoint where to find them BEFORE dialing — this is the
        // whole point of carrying addresses in the code.
        let mut addr = EndpointAddr::new(node_id);
        for text in &invite.addrs {
            if let Ok(socket) = text.parse::<std::net::SocketAddr>() {
                addr = addr.with_ip_addr(socket);
            }
        }
        if !invite.addrs.is_empty() {
            self.remember_addr(addr.clone());
        }

        let response = self
            .request(addr, ALPN_THREAD, &WireRequest::Introduction)
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

        let pool = &self.engine.store.pool;
        // The name the local user typed wins over anything they claimed.
        if !name.trim().is_empty() {
            store::records::set_text(pool, &intro.organ_uid, Some(name.trim()), None).await?;
        }
        store::organs::set_node_id(pool, &intro.organ_uid, Some(&invite.node_id)).await?;
        store::organs::set_trust(pool, &intro.organ_uid, "known").await?;
        Ok(intro.organ_uid)
    }

    /// Close this endpoint. Used when rebinding for a discovery change.
    pub async fn shutdown(&self) {
        self.endpoint.close().await;
    }

    /// Attach mDNS and keep the nearby list fed from its event stream.
    ///
    /// mDNS is multicast and therefore LAN-only — the same reach the retired
    /// `lan_discovery.rs` had. What it adds is that iroh's DNS/pkarr lookups
    /// cover the case multicast never could, so this is no longer the only
    /// way a peer is ever found.
    fn spawn_mdns(&self) {
        use n0_future::StreamExt as _;

        let mdns = match iroh_mdns_address_lookup::MdnsAddressLookup::builder()
            .service_name(MDNS_SERVICE_NAME)
            .build(self.endpoint.id())
        {
            Ok(mdns) => mdns,
            // A machine with neither IPv4 nor IPv6 multicast is not an error
            // worth failing a bind over — the Cell still works over DNS/pkarr
            // and direct addresses, it just has no LAN nearby list.
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

    /// Organs currently visible on the local network.
    pub fn nearby(&self) -> &Nearby {
        &self.nearby
    }

    /// Whether an Organ we hold no contact row for may open the thread door.
    ///
    /// DEFAULT CLOSED (Ontology §11): publishing a NodeId then advertises
    /// reachability to people who already know you and grants nothing to
    /// anyone else. Turning it on is what opens the invite door — and the
    /// discovery UI has to SAY so, because a nearby list that silently refuses
    /// everyone reads as broken.
    pub async fn accept_unknown(&self) -> bool {
        let Ok(Some(organ)) = store::organs::local(&self.engine.store.pool).await else {
            return false;
        };
        match store::records::get_extension(&self.engine.store.pool, &organ.uid, "lince.discovery")
            .await
        {
            Ok(Some(fields)) => fields
                .get("accept_unknown")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            _ => false,
        }
    }

    /// This Cell's NodeId — the one string a contact saves, and the thing a QR
    /// encodes.
    pub fn node_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Accept connections until the endpoint closes. Each connection is served
    /// on its own task so one slow peer cannot stall the others.
    pub async fn serve(&self) {
        while let Some(incoming) = self.endpoint.accept().await {
            let wire = self.clone();
            tokio::spawn(async move {
                let connection = match incoming.await {
                    Ok(connection) => connection,
                    // A failed handshake is normal background noise (a probe, a
                    // half-open NAT path). Nothing was authenticated, so there
                    // is nothing to report to a user.
                    Err(error) => {
                        tracing::debug!(%error, "iroh handshake failed");
                        return;
                    }
                };
                if let Err(error) = wire.serve_connection(connection).await {
                    tracing::debug!(%error, "iroh connection ended");
                }
            });
        }
    }

    /// Gate by contact state, then serve request frames until the peer hangs
    /// up. The gate is the whole of the accept-side authorization decision.
    async fn serve_connection(&self, connection: Connection) -> Result<(), EngineError> {
        let peer = connection.remote_id();
        let alpn = connection.alpn().to_vec();
        let contact =
            store::organs::contact_by_node_id(&self.engine.store.pool, &peer.to_string()).await?;

        // `blocked` is terminal everywhere (Ontology §2): close without
        // answering. Checked BEFORE the ALPN split so a blocked Organ cannot
        // reach the thread door either.
        if contact.as_ref().is_some_and(|c| c.trust == "blocked") {
            connection.close(0u32.into(), b"blocked");
            return Ok(());
        }

        // `known` is what opens sync — NOT merely having a contact row. A row
        // with `trust='unknown'` is someone added but not yet vetted, and the
        // policy is explicit that they get the thread door and nothing else.
        let known = contact.as_ref().is_some_and(|c| c.trust == "known");

        match (alpn.as_slice(), known) {
            (ALPN_SYNC, true) => {}
            (ALPN_SYNC, false) => {
                // Not known: never reaches the sync protocol. They may knock on
                // `lince/thread/1` instead — that is the invite door.
                //
                // One reason for both "no row" and "row, not yet known", so the
                // refusal does not tell a prober which of the two they are.
                connection.close(0u32.into(), b"unknown organ");
                return Ok(());
            }
            // A live session is someone acting INSIDE this Cell, so being
            // known is necessary and not sufficient: a login must have been
            // granted, and it decides which Person they act as. No login = the
            // same closed door a stranger gets.
            (ALPN_LIVE, true) => {
                let organ = contact
                    .as_ref()
                    .map(|c| c.record_uid.clone())
                    .unwrap_or_default();
                let person =
                    store::logins::person_for_organ(&self.engine.store.pool, &organ).await?;
                let (Some(person), Some(handler)) =
                    (person, self.live.lock().expect("live handler").clone())
                else {
                    connection.close(0u32.into(), b"no live session for this organ");
                    return Ok(());
                };
                // Handed off whole: a live session is long-lived and streams
                // its own frames, so none of the request/response loop below —
                // including `MAX_FRAMES_PER_CONNECTION`, which would hang up
                // mid-sentence on someone typing — applies to it.
                handler.serve(organ, person, connection).await;
                return Ok(());
            }
            (ALPN_LIVE, false) => {
                connection.close(0u32.into(), b"unknown organ");
                return Ok(());
            }
            (ALPN_THREAD, true) => {}
            (ALPN_THREAD, false) => {
                // The invite door. Threads themselves land with the Threads
                // stage; what is served here today is `Introduction` only, so
                // an unknown peer can be PAIRED with — which is the whole
                // point of a nearby list — without gaining any sync reach.
                if !self.accept_unknown().await {
                    connection.close(0u32.into(), b"not accepting unknown organs");
                    return Ok(());
                }
            }
            _ => {
                connection.close(0u32.into(), b"unsupported alpn");
                return Ok(());
            }
        }

        let from_organ = contact
            .map(|contact| contact.record_uid)
            .unwrap_or_default();

        for _ in 0..MAX_FRAMES_PER_CONNECTION {
            let (mut send, mut recv) = match connection.accept_bi().await {
                Ok(streams) => streams,
                // Clean hang-up, or the peer went away. Either way we are done.
                Err(_) => return Ok(()),
            };
            let raw = recv
                .read_to_end(MAX_FRAME_BYTES)
                .await
                .map_err(|error| EngineError::Consequence(format!("peer frame: {error}")))?;
            let response = match serde_json::from_slice::<WireRequest>(&raw) {
                // On the thread door an unknown peer gets `Introduction` and
                // nothing else — the gate above let them in to be PAIRED with,
                // not to sync.
                // On the thread door an unknown peer gets Introduction (to be
                // paired with) and Enrol (a new device of YOUR OWN identity,
                // which proves itself with a single-use token) — nothing else.
                Ok(request)
                    if !known
                        && !matches!(
                            request,
                            WireRequest::Introduction | WireRequest::Enrol { .. }
                        ) =>
                {
                    WireResponse::Refused {
                        code: "not_known".into(),
                        message: "only introduction is served to an unknown Organ".into(),
                    }
                }
                Ok(request) => self.handle(&from_organ, request).await,
                // Fail closed on the unknown: a request shape this build does
                // not recognise is refused, never guessed at.
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
        // Frame budget spent. Closing is not a punishment — redialing is cheap,
        // and a bounded connection cannot become an unbounded one.
        connection.close(0u32.into(), b"frame budget spent");
        Ok(())
    }

    /// Serve one request. `authenticated` is the contact uid iroh proved on
    /// this connection — never a value read out of the request body.
    async fn handle(&self, authenticated: &str, request: WireRequest) -> WireResponse {
        match request {
            WireRequest::Introduction => match self.engine.introduction().await {
                Ok(intro) => WireResponse::Introduction { intro },
                Err(error) => WireResponse::Error {
                    message: error.to_string(),
                },
            },
            WireRequest::PushOps { batch } => {
                // The batch must belong to the Organ the HANDSHAKE proved, not
                // to whoever the body claims. This is the same check the HTTP
                // path made against `verify_signed_request`'s return, and it
                // is what stops an authenticated contact from writing ops
                // attributed to a third Organ.
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
            WireRequest::Enrol {
                token,
                cell_uid,
                node_id,
                label,
                operational_key,
            } => {
                // Enrolling requires the ROOT, which is the point: adding a
                // device to your identity is a deliberate, occasional act, and
                // if the root has been moved offline this correctly fails
                // until it is brought back.
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
                            front_door: false,
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
            WireRequest::OfferGrant { root, title } => {
                // Recorded as `offered`, never `accepted`: the local user
                // decides whether to keep a copy, and until they do nothing
                // syncs. An Organ cannot push Records into someone's store by
                // announcing them.
                match store::replica::offer(&self.engine.store.pool, &root, authenticated).await {
                    Ok(()) => {
                        // The grant row above is the mechanism; the invite is
                        // the surface a person answers. Written separately
                        // because an offer must be showable before it has been
                        // decided — and refused when this Organ already has one
                        // pending, which is the whole anti-spam rule.
                        //
                        // A blocked Organ never gets here: `serve_connection`
                        // closes on `blocked` before the ALPN split, so an
                        // invite from one is dropped before it is written.
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
                        // The same answer either way. Telling a sender that
                        // their offer was dropped would tell them whether the
                        // last one was declined or merely unanswered, which is
                        // not theirs to know.
                        WireResponse::Applied { applied: 0 }
                    }
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::AcceptGrant { root } => {
                // Only an offer WE made can be accepted. Without this check a
                // peer could accept a root they invented and open a channel
                // into any uid they could guess.
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
            WireRequest::FetchGrantOps { root, after, limit } => {
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
                match store::sync_ops::after_in_root(&self.engine.store.pool, &root, after, limit)
                    .await
                {
                    Ok(rows) => {
                        let head = rows.last().map(|row| row.seq).unwrap_or(after);
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
            WireRequest::FetchOps { after, limit } => {
                let limit = limit.clamp(1, 2000);
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(organ) => organ.map(|organ| organ.uid).unwrap_or_default(),
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match self.engine.ops_after(after, limit).await {
                    Ok((ops, head)) => WireResponse::Ops {
                        from_organ: local,
                        ops,
                        head,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
        }
    }

    /// One full sync pass over iroh: drain the outbox to every reachable
    /// contact, then pull each one's catch-up feed.
    ///
    /// This is what makes the transport work above LIVE — until this runs in
    /// the background loop, the iroh path is tested but unused and every byte
    /// still moves over the old signed-HTTP routes.
    ///
    /// Errors per contact are swallowed on purpose: a peer with a closed
    /// laptop is the normal case, not a failure. The outbox is durable, so
    /// undelivered ops simply stay queued and flush on the next pass — which
    /// IS the offline send queue, with no separate mechanism.
    pub async fn sync_once(&self) -> Result<usize, EngineError> {
        // Before anything is pushed: a contact added from a code is still
        // filed under a uid this Cell invented, and sync cannot work until the
        // real one is known. Failures here are normal (the peer is offline) —
        // the row simply stays pending for the next pass.
        if let Err(error) = self.reconcile_pending().await {
            tracing::debug!(%error, "pending introductions not reconciled this pass");
        }
        let pushed = self.push_outbox().await?;
        let pulled = self.pull_catch_up().await?;
        Ok(pushed + pulled)
    }

    /// Finish what adding by code started: dial each pending contact, take
    /// their Introduction, and re-file the row under the uid they declare.
    ///
    /// This runs on `lince/thread/1`, not the sync ALPN. The pending contact
    /// holds no row for US yet, so their sync door is shut to this Cell by
    /// design — the thread door is the one that serves an Introduction to a
    /// stranger, and it is the same door `pair_with` knocks on.
    ///
    /// Returns how many rows were reconciled.
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
                // Offline is the ordinary case, not a failure worth reporting:
                // the row stays pending and the next pass tries again.
                Ok(Err(_)) | Err(_) => continue,
            };
            let WireResponse::Introduction { intro } = response else {
                continue;
            };

            // The key TOFU'd from the code is the whole value of having added
            // by code at all. If the Organ now answering presents a different
            // root key, this is not the peer the code was for — refuse, keep
            // the pending row, and let a human look at it. Silently adopting
            // the new key would discard the only thing that was verified.
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

            // Nothing about the placeholder was ever logged or sent anywhere,
            // so retiring it is a local deletion and not a rewritten history.
            let name = contact.head.clone();
            store::organs::forget_contact(pool, &contact.record_uid).await?;
            self.engine
                .adopt_introduction(&intro, contact.proximity)
                .await?;
            // The name the local user typed still wins over anything claimed.
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

    async fn dial(&self, contact: &store::organs::Contact) -> Option<Connection> {
        // `organ_contact.node_id` is the authoritative target; roster Cells are
        // ADDITIONAL candidates, raced in order. A missing or expired roster
        // therefore cannot make a known contact unreachable.
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
        for candidate in candidates {
            let Ok(id) = candidate.parse::<EndpointId>() else {
                continue;
            };
            // Bounded: an offline peer is the NORMAL case, not an error, and
            // without a cap iroh keeps trying relays and holepunches while the
            // whole sync pass — every other contact included — waits behind
            // one closed laptop.
            match tokio::time::timeout(
                DIAL_TIMEOUT,
                self.endpoint.connect(EndpointAddr::new(id), ALPN_SYNC),
            )
            .await
            {
                Ok(Ok(connection)) => return Some(connection),
                Ok(Err(_)) | Err(_) => continue,
            }
        }
        None
    }

    async fn push_outbox(&self) -> Result<usize, EngineError> {
        // One connection per contact for the whole drain, reused across the
        // batches a single pass produces.
        let connections: std::sync::Arc<tokio::sync::Mutex<HashMap<String, Option<Connection>>>> =
            Default::default();
        let wire = self.clone();
        self.engine
            .drain_outbox(move |contact, root, batch| {
                let wire = wire.clone();
                let connections = connections.clone();
                async move {
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
                        return Err(format!("{} unreachable", contact.record_uid));
                    };
                    let request = match root {
                        Some(root) => WireRequest::PushGrantOps { root, batch },
                        None => WireRequest::PushOps { batch },
                    };
                    match wire.exchange(&connection, &request).await {
                        Ok(WireResponse::Applied { .. }) => Ok(()),
                        Ok(other) => Err(format!("{other:?}")),
                        Err(error) => Err(error.to_string()),
                    }
                }
            })
            .await
    }

    async fn pull_catch_up(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let mut pulled = 0usize;
        for contact in store::organs::contacts(pool).await? {
            if contact.trust == "blocked" || !contact.sync_in {
                continue;
            }
            let Some(connection) = self.dial(&contact).await else {
                continue;
            };
            // The general feed first.
            let request = WireRequest::FetchOps {
                after: contact.last_synced_seq,
                limit: 500,
            };
            if let Ok(WireResponse::Ops { ops, head, .. }) =
                self.exchange(&connection, &request).await
            {
                let batch = OpBatch {
                    from_organ: contact.record_uid.clone(),
                    ops,
                };
                if !batch.ops.is_empty() && self.engine.import_op_batch(&batch).await.is_ok() {
                    // Advance the checkpoint only after a successful import,
                    // so a failed apply is retried rather than skipped.
                    store::organs::set_last_synced_seq(pool, &contact.record_uid, head).await?;
                    pulled += 1;
                }
            }
            // Then every conversation they have granted US.
            for root in store::replica::roots_for_contact(pool, &contact.record_uid).await? {
                let request = WireRequest::FetchGrantOps {
                    root: root.clone(),
                    after: 0,
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
            // Revocations BEFORE the roster: a revoked key must stop chaining
            // before we evaluate anything it might have signed, or a roster
            // signed by a dead key could be accepted in the same pass that
            // learns it is dead.
            if let Ok(WireResponse::Revocations { certs }) = self
                .exchange(&connection, &WireRequest::FetchRevocations)
                .await
            {
                for cert in certs {
                    // Only about THEIR OWN keys. A contact does not get to
                    // revoke a third Organ's key by telling us to.
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
            // And refresh their roster, which is how a device they enrolled
            // becomes reachable without re-pairing.
            if let Ok(WireResponse::Roster {
                roster: Some(roster),
            }) = self.exchange(&connection, &WireRequest::FetchRoster).await
            {
                if roster.roster.organ_uid == contact.record_uid {
                    match self.engine.adopt_roster(&roster).await {
                        Ok(crate::roster::RosterOutcome::Refused) => {
                            // The alarm. A roster that does not chain is a
                            // possible takeover, never a silent update.
                            tracing::warn!(
                                organ = %contact.record_uid,
                                "REFUSED a roster that does not chain from a held key"
                            );
                        }
                        Ok(_) => {}
                        Err(error) => tracing::debug!(%error, "roster refresh failed"),
                    }
                }
            }
            connection.close(0u32.into(), b"done");
        }
        Ok(pulled)
    }

    /// Dial `addr` and run one request/response exchange.
    ///
    /// No response signature is checked, and none is needed: the handshake
    /// already proved the answering endpoint holds the private half of the
    /// NodeId we dialed. Reaching that NodeId reaches that keypair or nothing,
    /// so there is no wire left to substitute on.
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
