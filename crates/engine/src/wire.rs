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
use crate::sync::{Introduction, OpBatch, WireOp};

/// Re-exported so callers can name a peer without depending on iroh directly —
/// the version is pinned here, in one place, and stays that way.
pub use iroh::{EndpointAddr as PeerAddr, EndpointId as PeerId};

/// Sync between Organs that already know each other.
///
/// **Bumped to `/2` on 2026-08-11.** Versioned so a protocol change hard-cuts
/// older peers, which is the intended behaviour rather than a regression to
/// soften — Lince keeps no compatibility with older builds (`AGENTS.md`), so
/// the two versions are never served side by side.
///
/// The epoch was not optional by this point. `WireOp` renamed `actor_organ` to
/// `actor_cell` and added a non-defaulted `organ_uid`, so an older peer's batch
/// no longer deserializes at all; `CellEntry` gained a signed capability set,
/// so every roster signed before it is refused; and `FetchOps` became
/// `FetchOpsSince`. The wire was already incompatible — the bump only makes it
/// say so at the TLS layer instead of failing confusingly one frame later.
pub const ALPN_SYNC: &[u8] = b"lince/sync/2";

/// First contact from an Organ we hold no contact row for. Deliberately a
/// SEPARATE ALPN rather than a check inside the sync handler: a stranger then
/// cannot negotiate the sync protocol at all, so the gate holds at the TLS
/// layer and the sync handler never runs for an unknown peer even if a later
/// bug weakens its own checks.
/// Bumped with the sync ALPN: `Introduction` and `Enrol` both carry a
/// `CellEntry`, whose signed shape changed.
pub const ALPN_THREAD: &[u8] = b"lince/thread/2";

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
pub const ALPN_LIVE: &[u8] = b"lince/live/2";

/// **NEVER BUMP THIS.** The one ALPN that must survive every epoch cut
/// (Ontology §11, decision 1).
///
/// An epoch hard-cuts older peers at the TLS layer, which is the intended
/// behaviour — but it means a Cell one epoch behind is INVISIBLE to its own
/// Organ rather than merely out of date. Your phone stops syncing and nothing
/// says why: exactly the mystery the Organ/Cell split must not introduce, and
/// an app store review can hold a device stale for a week.
///
/// Diagnosing that requires speaking across an epoch boundary, and the only
/// thing that can is a protocol that never changes. So this one carries a
/// single integer — the epoch its speaker supports — and nothing else. It has
/// no verbs, no state, and no reason to ever change shape. Adding a field to
/// it later would defeat the whole purpose, because the peer that needs to
/// answer is by definition running an older build.
///
/// Answered only to a Cell of our own roster or a known contact: "which
/// version do you run" is a fingerprint, and a stranger has no business
/// collecting it.
pub const ALPN_HELLO: &[u8] = b"lince/hello/1";

/// The epoch this build speaks, reported over [`ALPN_HELLO`]. Bumped with the
/// ALPNs above, together, as one numbered release train.
pub const WIRE_EPOCH: u32 = 2;

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

/// How many connections one peer may have open at once (Ontology §11, C4).
///
/// A cap from the FIRST deploy rather than after the first incident, because
/// an always-on Cell is a bandwidth donation with no natural ceiling and an
/// unbounded one takes down the operator's other services before it takes
/// down Lince. Conservative on purpose: a Cell syncing normally opens one
/// connection per pass and reuses it, so anything near this bound is either a
/// bug or an attempt.
///
/// Per PEER, not global — one noisy contact must not be able to lock everyone
/// else out, which a global cap would let it do.
pub const MAX_CONNECTIONS_PER_PEER: usize = 8;

/// The mDNS service name Lince advertises under. Deliberately NOT iroh's
/// default (`irohv1`): that would list every unrelated iroh application on the
/// network as a "nearby Organ". Only Lince Cells answer to this.
pub const MDNS_SERVICE_NAME: &str = "lince";

/// How long one dial attempt may take before the sync pass moves on.
pub const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// The head start each candidate gets over the next one in the dial race.
///
/// Small enough that a dead first choice costs almost nothing — the whole
/// point of racing — and large enough that a live LAN peer, which answers in
/// single-digit milliseconds, reliably wins before the next dial even starts.
/// With the previous sequential dialing that head start was the full
/// `DIAL_TIMEOUT`, which is what made one shut laptop expensive.
pub const DIAL_STAGGER: std::time::Duration = std::time::Duration::from_millis(150);

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
    /// Relays and address publishing, but NO DIRECT ADDRESSES: every path runs
    /// through a relay, so a peer resolving this Cell sees the relay's IP and
    /// never ours. The DEFAULT for a reachable Cell (Ontology §11, C4).
    ///
    /// It costs latency and someone else's bandwidth, and it buys the thing a
    /// fresh install on a café network actually needs: being findable without
    /// telling every key holder where you physically are. Turning direct
    /// connections on is one setting, once, for an Organ that has decided to
    /// be reachable.
    Relay,
    /// Relays, publishing, AND direct addresses — holepunching to a direct
    /// path when one can be found. Faster, and it discloses this machine's
    /// addresses to anyone holding the published key.
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
    /// Drive one live connection to completion.
    ///
    /// `organ_uid` is the contact the HANDSHAKE proved (or `node:<id>` when
    /// there is no contact row). `granted_person` is who their `organ_login`
    /// says they act as, and `None` means they have no such binding — so the
    /// session must make them prove a username and password before it serves
    /// anything. Neither value is ever read out of something the peer sent.
    async fn serve(
        &self,
        organ_uid: String,
        granted_person: Option<String>,
        connection: Connection,
    );
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
    /// Pairing, in the direction that was missing: the dialer hands over its
    /// OWN Introduction and gets ours back.
    ///
    /// `Introduction` alone is one-directional — the dialer learns who we are
    /// and adopts us, while we learn nothing durable and keep no row. That was
    /// enough while pairing only had to populate the dialer's contact list,
    /// and silently not enough for everything that reads the ACCEPTING side's
    /// contacts: `lince/sync/1` and `lince/live/1` both gate on
    /// `contact_by_node_id`, so a Cell paired this way stayed a stranger to
    /// the Cell it had just paired with, and live mode closed on it.
    ///
    /// Binding here creates an `unknown`-trust row and nothing more. Being
    /// dialable is not a relationship: promoting to `known` (and granting a
    /// login) stays a deliberate, separate act by the accepting side.
    Introduce {
        intro: Introduction,
    },
    PushOps {
        batch: OpBatch,
    },
    /// Catch-up by VERSION VECTOR: "here is what I already hold of YOUR ops,
    /// keyed by the Cell that wrote each — send me the rest."
    ///
    /// Replaces a cursor into the SERVER's local seq, which was never a thing
    /// the client could reason about: it meant nothing once the server pruned,
    /// and nothing at all for ops that reached the client by another path. A
    /// vector is stated in stamps both sides already share.
    ///
    /// The vector covers ONE Organ — the one being asked. Sending everything
    /// we hold would disclose which Cells of OTHER Organs we sync with, and
    /// those third parties never agreed to be named.
    FetchOpsSince {
        vector: Vec<store::sync_ops::VectorEntry>,
        limit: i64,
    },
    /// Read a Record that a message in `root` REFERENCES (Ontology §11
    /// "Individual replica and threads", C6).
    ///
    /// Mentioning a Record in a thread points at it; it does not copy it. This
    /// is the read that pointer resolves to, and it is a live read against the
    /// owner's Cell every time — which is what makes revocation real here and
    /// nowhere else in the design: stopping the share means the next read
    /// fails, because there was never a copy.
    ///
    /// It is NOT a general "give me that uid" verb, and must never become one.
    /// Three things authorise it, checked in this order at serve time: the
    /// asker holds an accepted grant on `root`; a message INSIDE that root
    /// actually references `record`; and the ordinary §12 gate for that
    /// contact then decides what, if anything, comes back.
    FetchReference {
        root: String,
        record: String,
    },
    /// Offer a conversation. Cannot itself ride the grant channel — there is
    /// no grant yet — so it is its own verb on the sync ALPN.
    OfferGrant {
        root: String,
        title: String,
        /// The durable Organ identity offered by the Cell on this
        /// authenticated Iroh connection. Receiving it creates an `unknown`
        /// peer binding, never a known contact.
        intro: Introduction,
    },
    /// Accept an offer. Only now do ops move.
    AcceptGrant {
        root: String,
    },
    /// Decline an offer so the sender can retire its offered grant and may
    /// offer another conversation later.
    DeclineGrant {
        root: String,
    },
    /// Individual-replica ops. `root` is the CHANNEL, checked against the
    /// local grant table on arrival; it is never read out of an op.
    PushGrantOps {
        root: String,
        batch: OpBatch,
    },
    /// Catch-up inside one conversation, by version vector — the grant-channel
    /// twin of `FetchOpsSince`.
    ///
    /// It used to be a cursor pinned at `after: 0`, so every pass re-fetched a
    /// conversation's entire history and re-imported it. Idempotent, therefore
    /// invisible, and O(history) per conversation per pass forever — and it
    /// left the grant channel with no retention floor at all, because nothing
    /// was ever confirmed received.
    ///
    /// Unlike the general feed this vector legitimately names the OTHER
    /// party's Cells: both sides write inside a conversation, and they are
    /// already in it.
    FetchGrantOpsSince {
        root: String,
        vector: Vec<store::sync_ops::VectorEntry>,
        limit: i64,
    },
    /// This Organ's signed Cell roster. Served on `lince/sync/1` only — the
    /// CONTACT tier of two-tier publishing. The public tier (what a stranger
    /// resolving a published key learns) is the front-door Cell alone, so a
    /// key on a website never discloses how many devices you have, their
    /// current addresses, or which are online right now.
    FetchRoster,
    /// A Transfer exchange, carrying the signed JSON that used to be an HTTP
    /// POST to `contact.base_url`. Transfer was the last subsystem speaking
    /// HTTP peer-to-peer, and a URL matches nothing the transport does: pairing
    /// parses a NodeId, the outbox dials a NodeId, and the accept gate
    /// authorises by `contact_by_node_id`. A hostname was a thing to show,
    /// never a thing to dial.
    TransferPost {
        verb: TransferVerb,
        body: serde_json::Value,
    },
    /// Key successions this Organ has signed about ITS OWN keys — "this old
    /// root endorses this new one". Pulled on every sync pass, so an Organ can
    /// rotate its identity key without every contact re-pairing.
    ///
    /// Without this verb the chain rule only ever says no: `adopt_roster`
    /// refuses a roster signed by a key that does not chain, which is correct,
    /// and nothing would ever deliver the endorsement that makes it chain.
    FetchSuccessions,
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
    /// The peer's version vector for OUR Organ's ops — what they hold of what
    /// we wrote (Ontology §11, C2b, the cross-Organ audit).
    ///
    /// `FetchOpsSince` already answers "what am I missing"; this answers the
    /// question catch-up never asks, which is "what are THEY missing, and do
    /// we actually agree". Comparing the two vectors says which side lacks
    /// what without moving a single op, because a vector is a summary and the
    /// difference falls out of it.
    ///
    /// A separate verb rather than a field on the ops response, because an
    /// audit is a question a PERSON asks occasionally, not something every
    /// sync pass should pay for.
    FetchVector { organ_uid: String },
    /// What a front door is holding for its owner (Ontology §11, C3).
    ///
    /// Served ONLY to a sibling Cell of the same Organ. A front door cannot
    /// decide about a stranger — it holds no `CAP_REPRESENT` — so this is how
    /// a Cell that can decide comes and looks.
    FetchDoorRequests { limit: i64 },
    /// Release requests a deciding Cell has taken.
    ///
    /// Separate from the fetch, so a Cell that dies between reading and
    /// deciding finds them still waiting rather than silently dropped.
    ReleaseDoorRequests { uids: Vec<String> },
}

/// Which Transfer exchange a `TransferPost` is. A closed enum rather than the
/// HTTP path string it replaces: a path is a free-form selector, and this is
/// the accept side of a peer connection. An unrecognised verb fails to
/// deserialize and is refused, which is the whole fail-closed rule.
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

/// Serves Transfer exchanges, installed from above — same seam as
/// `LiveSessions`, and for the same reason: the delivery logic lives in `web`,
/// which depends on this crate.
///
/// The body is opaque here on purpose. Every Transfer payload is already
/// signed and verified at the application layer (`sign_organ_request` /
/// `verify_wire`), so the transport is a pipe and must not acquire an opinion
/// about what it carries — moving that verification down here would give one
/// subsystem two places to decide the same thing.
#[async_trait::async_trait]
pub trait TransferPeer: Send + Sync {
    async fn handle(
        &self,
        verb: TransferVerb,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

/// One published revocation, as it travels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationCert {
    pub organ_uid: String,
    pub revoked_key: String,
    pub signature: String,
}

/// One published succession, as it travels. `created_at` is part of the signed
/// payload, not a delivery timestamp — it cannot be regenerated on arrival.
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
    /// The reply body of a `TransferPost`, opaque for the same reason the
    /// request body is.
    Transfer {
        body: serde_json::Value,
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
    /// Introductions a front door is holding, in the order they arrived.
    DoorRequests {
        requests: Vec<HeldIntroduction>,
    },
    /// A version vector, in answer to `FetchVector`.
    /// One referenced Record, as the §12 gate leaves it — narrowed by the
    /// asker's scope, so a withheld column is ABSENT from the object rather
    /// than present and blank.
    Reference {
        row: serde_json::Value,
    },
    Vector {
        vector: Vec<store::sync_ops::VectorEntry>,
    },
}

/// Whether one contact's log agrees with ours, from comparing two version
/// vectors and moving no ops at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditAgreement {
    pub contact_organ: String,
    /// Ops of ours they do not hold. NOT automatically a fault: the outbox may
    /// simply not have drained yet. It is a fault when it stays high across
    /// passes, which is why this is shown to a person rather than acted on.
    pub they_lack: usize,
    /// Cells of ours they have never seen an op from. Usually means a device
    /// you enrolled has not reached them — the case that used to be invisible.
    pub unknown_cells: usize,
}

/// A Cell of this Organ that answered the stable hello ALPN with a different
/// epoch than ours — a device that needs updating, not a device that is off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleSibling {
    pub cell_uid: String,
    pub node_id: String,
    /// The roster's label for it, so a person is told "your phone" rather than
    /// a NodeId.
    pub label: String,
    pub their_epoch: u32,
    pub our_epoch: u32,
}

/// A stranger's knock, held by a front door until a Cell that can decide sees
/// it (Ontology §11, C3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeldIntroduction {
    /// The door's own id for it, quoted back to release it.
    pub uid: String,
    /// The NodeId that knocked, authenticated by QUIC at the door. This is the
    /// only part that is identity; everything in `intro` is a claim.
    pub node_id: String,
    pub intro: Introduction,
    pub received_at: String,
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
    /// Open connections per peer, for `MAX_CONNECTIONS_PER_PEER`. Bounded
    /// concurrency is the cheapest half of a bandwidth cap and the half that
    /// can be enforced without accounting for bytes.
    open_per_peer: Arc<Mutex<HashMap<String, usize>>>,
    /// Cells of this Organ found on a different wire epoch, from the last
    /// sync pass. Transient and in memory, like `nearby`.
    /// What this endpoint was bound for. Kept because publishing anything
    /// PUBLICLY — the directory record above all — must follow the same
    /// decision the endpoint was built with, not a config value read again
    /// later and possibly disagreeing with it.
    reach: Reach,
    /// Installed from above; `None` means this Cell serves no live sessions,
    /// which is the correct behaviour for a headless or sync-only Cell.
    live: Arc<Mutex<Option<Arc<dyn LiveSessions>>>>,
    /// Installed from above; `None` means this Cell delivers no Transfers,
    /// and a peer asking is refused rather than left hanging.
    transfer: Arc<Mutex<Option<Arc<dyn TransferPeer>>>>,
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
        let alpns = vec![
            ALPN_SYNC.to_vec(),
            ALPN_THREAD.to_vec(),
            ALPN_LIVE.to_vec(),
            // Stable across epochs, so a Cell one release behind can still be
            // TOLD that it is one release behind.
            ALPN_HELLO.to_vec(),
        ];
        let endpoint = match reach {
            Reach::Internet => Endpoint::builder(presets::N0),
            // Relays and discovery, with every IP transport removed — which is
            // what makes this relay-ONLY rather than relay-preferred: there is
            // no direct path to publish, so none is published.
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
            transfer: Arc::new(Mutex::new(None)),
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

    /// Install THIS wire as the transport the Engine enrols through, so
    /// "join an Organ" is reachable as an Action from the running app rather
    /// than only from a test.
    ///
    /// Takes the `Arc` the caller already holds and keeps only a weak
    /// reference, because `Wire` owns an `Arc<Engine>` and a strong handle the
    /// other way would be a cycle.
    pub fn serve_enrolment(self: &Arc<Self>) {
        // Unsizing `Arc<Wire>` to `Arc<dyn Enroller>` keeps the SAME
        // allocation, so this temporary dropping at the end of the call
        // changes nothing: the weak handle stays upgradable for exactly as
        // long as the caller holds its own `Arc<Wire>`, and dies with it.
        let as_trait: Arc<dyn crate::enrolment::CellTransport> = self.clone();
        self.engine.set_enroller(Arc::downgrade(&as_trait));
    }

    /// Install the Transfer delivery handler. Without one, `TransferPost` is
    /// refused with a reason rather than silently accepted and dropped.
    pub fn set_transfer_handler(&self, handler: Arc<dyn TransferPeer>) {
        *self.transfer.lock().expect("transfer handler") = Some(handler);
    }

    /// Send one Transfer exchange to a contact and hand back their reply.
    ///
    /// Rides `lince/sync/1`, so the accept gate is the one that already
    /// exists: `known` contacts only. That is a real tightening over the HTTP
    /// path, which would deliver to any non-blocked contact — and it is the
    /// right gate for value transfer, where "someone whose row exists but whom
    /// nobody vetted" is exactly who should not be receiving envelopes.
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

    /// Open a live connection to a bare NodeId, with no contact row anywhere.
    ///
    /// This is what makes a FRESH Lince able to reach an Organ. `open_live`
    /// resolves its address out of `organ_contact`, which a Cell installed a
    /// minute ago does not have and cannot get — pairing needs the far side to
    /// open its discovery door, and a login must not require that. The public
    /// value the user pastes carries the NodeId and the addresses, which is
    /// everything needed to dial; who they are is then settled by the login.
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

    /// Join an existing Organ as a new Cell, from a code shown by a Cell that
    /// already belongs to it (Ontology §11, cluster C3).
    ///
    /// Served on the THREAD door, because a device being enrolled is by
    /// definition not yet a member and cannot reach the sync door. Nothing
    /// local changes until the roster comes back naming this Cell — see
    /// `enrolment.rs` for why that order is the security property.
    pub async fn enrol(&self, invite: &EnrolmentInvite) -> Result<SignedRoster, EngineError> {
        // Asked BEFORE dialing: an enrolment token is single-use, and spending
        // one to learn that this Cell was never eligible would burn it.
        self.engine.may_enrol().await?;
        let cell = store::cells::local(&self.engine.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Cell Record".into()))?;
        let node_id: EndpointId = invite
            .node_id
            .parse()
            .map_err(|_| EngineError::Consequence("the code carries an unreadable node id".into()))?;
        let mut addr = EndpointAddr::new(node_id);
        for text in &invite.addrs {
            if let Ok(socket) = text.parse::<SocketAddr>() {
                addr = addr.with_ip_addr(socket);
            }
        }
        if !invite.addrs.is_empty() {
            // The addresses in the code are what make this work on guest wifi
            // and in hotels, where no discovery reaches.
            self.remember_addr(addr.clone());
        }
        let operational = self.engine.operational_key_for(&invite.organ_uid).await?;
        let operational_key = operational.public_key_b64();
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
                },
            )
            // A closed connection is the SHAPE a spent code takes, not a
            // network fault: the thread door opens only while an enrolment is
            // outstanding, so a code that was already used or has expired
            // finds the door shut and the peer hangs up before any verb is
            // read. "Connection lost" is true and useless; this is what
            // actually happened, in the order of likelihood.
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
            WireResponse::Roster { roster: Some(signed) } => signed,
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

        // Introduce ourselves rather than only asking. Pairing is mutual or it
        // is not pairing: without this the far side keeps no row for us, and
        // every door it guards by contact — sync and live mode both — stays
        // shut on a pair that looked like it succeeded.
        let ours = self.engine.introduction().await?;
        let response = self
            .request(addr, ALPN_THREAD, &WireRequest::Introduce { intro: ours })
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

    /// Bind an Organ introduction to the NodeId proven by the current Iroh
    /// connection without promoting it to `known`.
    ///
    /// This is the identity tier used by first-contact conversations: enough
    /// information to verify durable ops and route replies, but no access to
    /// the general Organ feed. An existing binding may not be replaced by a
    /// different NodeId or Organ uid.
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
        // `add_contact` defaults to unknown. Keep this explicit at the trust
        // boundary: accepting a conversation is not adding a friend.
        store::organs::set_trust(pool, &organ_uid, "unknown").await?;
        Ok(organ_uid)
    }

    /// Offer a new individually replicated conversation to a discovered
    /// NodeId. No known-contact relationship is required on either side.
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

    /// Read a Record referenced from a conversation, LIVE from its owner
    /// (Ontology §11, C6).
    ///
    /// There is no cache and there must not be one. A reference resolves live
    /// or it resolves to nothing — offline means UNREACHABLE, not
    /// stale-but-readable — and that is exactly what buys the property the
    /// rest of the design cannot offer: revocation that actually revokes,
    /// because the reader never held a copy to keep. A cache here would trade
    /// that away for a convenience nobody asked for.
    ///
    /// Refusal and unreachability are DIFFERENT and both are returned as
    /// errors the surface must distinguish: "they are offline" is temporary
    /// and worth retrying, "no longer shared" is an answer.
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
        // The THREAD door: a conversation may exist with someone still
        // `unknown`, and a reference posted in one has to be readable or the
        // pointer is decoration.
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

    /// Answer a pending conversation invite and notify its sender before the
    /// local notification is cleared. Both operations are idempotent.
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
        match self.discovery_config(&organ.uid).await {
            Some(fields) => fields
                .get("accept_unknown")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Whether this Cell lets someone log in over `lince/live/1` with a
    /// username and password rather than a granted device binding.
    ///
    /// DEFAULT ON, and that is not the same shape of decision as
    /// `accept_unknown`. Opening the pairing door hands a stranger an
    /// Introduction for merely knocking; this hands them nothing at all unless
    /// they already know a username and password on this Cell — the same bar
    /// the HTTP login sets, checked against the same `person_credential` rows.
    /// A Cell with auth switched off has no credentials, so every attempt
    /// fails: the honest answer there is not a policy but an empty table.
    ///
    /// Read per connection, so switching it off takes effect immediately.
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

    /// This Cell's NodeId — the one string a contact saves, and the thing a QR
    /// encodes.
    pub fn node_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// What this endpoint was bound for. `Local` means nothing about this Cell
    /// may be published to the internet — no addresses, and no directory
    /// record either.
    pub fn reach(&self) -> Reach {
        self.reach
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
                // The cap. Counted here rather than inside
                // `serve_connection` so a refused connection costs nothing
                // beyond the handshake that already happened.
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

    /// Take a connection slot for `peer`, or refuse. Refusing is not a
    /// punishment — redialing is cheap and a bounded queue is what keeps one
    /// peer from being able to exhaust the others.
    fn admit(&self, peer: &str) -> bool {
        let mut open = self.open_per_peer.lock().expect("open per peer");
        let count = open.entry(peer.to_string()).or_insert(0);
        if *count >= MAX_CONNECTIONS_PER_PEER {
            return false;
        }
        *count += 1;
        true
    }

    /// Give the slot back, and forget the peer entirely when it holds none —
    /// otherwise this map grows by one entry per peer ever seen, which on a
    /// relay is a slow leak with no upper bound.
    fn release(&self, peer: &str) {
        let mut open = self.open_per_peer.lock().expect("open per peer");
        if let Some(count) = open.get_mut(peer) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                open.remove(peer);
            }
        }
    }

    /// This Cell's discovery settings: the CELL Record first, the Organ
    /// Record second. Per-device settings on a shared, syncing Record were
    /// always wrong; what made it urgent is that a relay Cell may not write,
    /// so it could not have configured itself.
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

    /// Our own Organ uid, if `node_id` is another CELL OF IT that may write.
    ///
    /// This is how a second device is recognised at all. It reads the signed
    /// roster rather than the contact table, because a sibling shares our
    /// Organ uid and `organ_contact` is keyed by exactly that — there is no
    /// row that could describe one without colliding with ourselves.
    ///
    /// Deliberately NOT a trust decision of its own: the roster is signed by
    /// the root, and being in it with `CAP_WRITE` is the decision. Nothing
    /// here is checked against a contact row, because a sibling was never
    /// meant to have one.
    ///
    /// **Roster EXPIRY is deliberately not consulted, and that is a decision
    /// rather than an omission.** Expiry is how a removed device falls out of
    /// a CONTACT's view on its own, and it works there because that contact
    /// has other ways to hear from the Organ. Between two devices of one Organ
    /// it would deadlock: the only path to a fresher roster runs through a
    /// sibling holding the root, so refusing stale rosters inbound would cut
    /// the exact connection that repairs them — a laptop offline for a month
    /// could never converge with its own phone again. Revoking a device
    /// therefore propagates at the speed of roster DISTRIBUTION, not at the
    /// speed of expiry, and the honest statement is that a revoked Cell keeps
    /// sibling access until the Cells that remain learn the new roster.
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
                    // Not ourselves. A Cell dialing its own NodeId is not a
                    // thing that happens, but treating it as a sibling would
                    // make the guard read as if it did.
                    && member.cell_uid != ours
                    && member.may(crate::roster::CAP_WRITE)
            })
            .map(|_| organ.uid)
    }

    /// Whether THIS Cell may speak for its Organ — pair, accept grants, answer
    /// for it (`CAP_REPRESENT`).
    ///
    /// An Organ with NO roster yet answers `true`: a single Cell that has not
    /// published anything is the whole Organ, and a first boot that could not
    /// pair would be a Cell unable to do the only thing it can do. The
    /// capability set only starts constraining once there is a roster to
    /// constrain it with.
    async fn may_represent(&self) -> bool {
        let pool = &self.engine.store.pool;
        let (Ok(Some(organ)), Ok(Some(cell))) =
            (store::organs::local(pool).await, store::cells::local(pool).await)
        else {
            return false;
        };
        // Mid-enrolment the local Organ is already the joined one and its
        // roster is not stored yet, so "no roster" would read as "I am the
        // whole Organ". Refuse for the duration instead.
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

    /// Hold a stranger's Introduction for a Cell that can decide about it.
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

        // A SIBLING — another Cell of this same Organ — resolved here rather
        // than after, because the ALPN arms below close the connection and
        // `from_organ` is computed further down still. `organ_contact` is keyed
        // by the contact's ORGAN uid and a sibling's is OUR OWN, so the contact
        // table cannot represent one and never will: the ROSTER is the sibling
        // list, and it is already signed by the root.
        //
        // `CAP_WRITE` is the bar, not mere membership. A relay Cell holds no
        // capabilities by design, and "the front door holds no signing
        // material" stops being a structural fact the moment a listed Cell can
        // push ops for having been listed.
        let sibling = self.sibling_organ(&peer.to_string()).await;

        // `known` is what opens sync — NOT merely having a contact row. A row
        // with `trust='unknown'` is someone added but not yet vetted, and the
        // policy is explicit that they get the thread door and nothing else.
        let known = sibling.is_some() || contact.as_ref().is_some_and(|c| c.trust == "known");
        let identified = sibling.is_some() || contact.is_some();

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
            // A live session is someone acting INSIDE this Cell. There are two
            // ways in, and they answer different questions.
            //
            // A granted contact needs no password: the handshake already proved
            // which Organ is on the connection, and `organ_login` says which
            // Person that Organ acts as. That binding is per-device — it is
            // tied to their Cell's key.
            //
            // Everyone else may present a USERNAME AND PASSWORD. That is the
            // path that is not bound to a device: a Lince installed fresh this
            // minute, holding no keys anyone has ever seen, can point at this
            // Organ and log in — which is the only way "from anywhere" can
            // actually mean anywhere. The bar is exactly the bar the HTTP login
            // sets, because it is the same credential.
            //
            // The connection is accepted here and stays ANONYMOUS. Nothing is
            // served until the login succeeds; the session layer holds that
            // gate, because it is the layer that owns the frames.
            // "Which epoch do you speak", and nothing else. Answered only to a
            // Cell of our own Organ or a known contact — a version string is a
            // fingerprint, and a stranger has no business collecting one.
            (ALPN_HELLO, _) => {
                if !identified {
                    connection.close(0u32.into(), b"not known");
                    return Ok(());
                }
                if let Ok(mut send) = connection.open_uni().await {
                    let body = serde_json::json!({ "epoch": WIRE_EPOCH }).to_string();
                    let _ = send.write_all(body.as_bytes()).await;
                    let _ = send.finish();
                    // Let the reader drain before the connection goes.
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
                if granted.is_none() && !self.accept_logins().await {
                    connection.close(0u32.into(), b"this Cell does not accept live logins");
                    return Ok(());
                }
                let Some(handler) = self.live.lock().expect("live handler").clone() else {
                    connection.close(0u32.into(), b"no live session for this organ");
                    return Ok(());
                };
                // Identify the session by the NodeId when there is no contact
                // row: a credential login does not require one, and the
                // connection id still has to be unique per peer.
                let session_organ = if organ.is_empty() {
                    format!("node:{peer}")
                } else {
                    organ
                };
                // Handed off whole: a live session is long-lived and streams
                // its own frames, so none of the request/response loop below —
                // including `MAX_FRAMES_PER_CONNECTION`, which would hang up
                // mid-sentence on someone typing — applies to it.
                handler.serve(session_organ, granted, connection).await;
                return Ok(());
            }
            (ALPN_THREAD, true) => {}
            (ALPN_THREAD, false) if !identified => {
                // The invite door. Threads themselves land with the Threads
                // stage. A completely new NodeId needs the explicit discovery
                // opt-in before it may introduce itself or offer a root — OR
                // an enrolment must be open right now.
                //
                // The enrolment window is its own door policy (Ontology §11,
                // C3). A device being enrolled is not yet a contact of
                // anything, so it arrives here as a stranger; making the owner
                // also switch on "accept unknown Organs" to add their own
                // phone would conflate two unrelated decisions and leave that
                // door open long afterwards. This one opens only while an
                // issued token is unused and unexpired — minutes — and the
                // verb-level gate below still admits nothing but `Enrol`
                // unless the peer is known.
                let enrolling = store::roster::enrolment_is_open(&self.engine.store.pool)
                    .await
                    .unwrap_or(false);
                if !enrolling && !self.accept_unknown().await {
                    connection.close(0u32.into(), b"not accepting unknown organs");
                    return Ok(());
                }
            }
            // Already identified but not known: only the request-level grant
            // verbs below are admitted. This is the individual-replica tier,
            // and remains usable if the user later closes first contact.
            (ALPN_THREAD, false) => {}
            _ => {
                connection.close(0u32.into(), b"unsupported alpn");
                return Ok(());
            }
        }

        // A sibling's ops carry OUR OWN Organ uid, which is exactly what
        // `Engine::inadmissible` requires of a batch: `op.organ_uid` must
        // equal `from_organ`. The roster check in that same gate then confirms
        // the authoring Cell is a member — so a sibling batch is verified by
        // the machinery that already exists, with no exemption.
        let from_organ = sibling
            .or_else(|| contact.map(|contact| contact.record_uid))
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
                            WireRequest::Introduction
                                | WireRequest::Introduce { .. }
                                | WireRequest::Enrol { .. }
                                | WireRequest::OfferGrant { .. }
                                | WireRequest::AcceptGrant { .. }
                                | WireRequest::DeclineGrant { .. }
                                | WireRequest::PushGrantOps { .. }
                                | WireRequest::FetchGrantOpsSince { .. }
                                // A conversation with someone not yet known is
                                // the individual-replica tier working as
                                // designed, and a reference posted in one has
                                // to be readable or the pointer is decoration.
                                // It admits nothing extra: the verb re-checks
                                // the accepted grant itself, and everything it
                                // can reach had to be mentioned in that same
                                // conversation first.
                                | WireRequest::FetchReference { .. }
                        ) =>
                {
                    WireResponse::Refused {
                        code: "not_known".into(),
                        message: "only introduction is served to an unknown Organ".into(),
                    }
                }
                Ok(request) => {
                    // A first conversation is deliberately possible before
                    // either side is known. For the first offer only, bind
                    // the Introduction to the NodeId authenticated by Iroh
                    // and keep its trust tier `unknown`. Every later grant
                    // request resolves through that binding.
                    // THE FRONT DOOR (Ontology §11, C3). A Cell without
                    // `CAP_REPRESENT` may not speak for the Organ, and binding
                    // a stranger as a contact is speaking for it — so it holds
                    // the request instead of deciding, and a Cell that CAN
                    // decide collects it over `FetchDoorRequests`. The VPS
                    // holds no identity-signing material; this is what that
                    // means in practice rather than as a promise.
                    // `OfferGrant` is held for the same reason `Introduce` is:
                    // accepting a conversation from a stranger BINDS them as a
                    // peer, which is speaking for the Organ. A door with no
                    // `CAP_REPRESENT` may not, so it holds the introduction
                    // that came with the offer and the owner's Cell decides.
                    // The offer itself is not preserved — the stranger has to
                    // ask again once they are known, which is honest: the door
                    // never promised to carry a conversation, only not to lose
                    // the knock.
                    if let WireRequest::Introduce { intro }
                    | WireRequest::OfferGrant { intro, .. } = &request
                    {
                        if from_organ.is_empty() && !self.may_represent().await {
                            let response = match self.hold_at_the_door(&peer.to_string(), intro).await
                            {
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
                    self.handle(&authenticated, request).await
                }
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
            // Both answer with ours. `Introduce` additionally bound THEIRS
            // above, in `serve_connection`, where the NodeId iroh proved is
            // still in hand — the intro in the body is a claim, and it is that
            // pairing of claim with proven NodeId that makes the row worth
            // keeping.
            WireRequest::Introduction | WireRequest::Introduce { .. } => {
                match self.engine.introduction().await {
                    Ok(intro) => WireResponse::Introduction { intro },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
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
                            // An enrolled device is an ordinary personal Cell.
                            // Narrowing one (a relay, a phone you no longer
                            // fully trust) is a later, deliberate edit of the
                            // roster, not something the enrolling device gets
                            // to ask for — it does not choose its own grant.
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
                                // Wake every open board. Only on `Some`: a
                                // suppressed duplicate changed nothing, and
                                // waking on it would push an identical list.
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
            WireRequest::DeclineGrant { root } => {
                match store::replica::state(&self.engine.store.pool, &root, authenticated).await {
                    Ok(Some(_)) => {
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
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(organ) => organ.map(|organ| organ.uid).unwrap_or_default(),
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                // The retention floor, DERIVED rather than reported. The
                // vector says which of our ops this peer already holds, so
                // the floor is the last seq before the first one they lack —
                // not the count they hold. A gap in the middle means
                // everything after it is unconfirmed too, however much of the
                // tail they happen to have, and pruning past a gap deletes
                // ops the peer can no longer ask for.
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
                // The head is taken BEFORE narrowing, deliberately. It is a
                // position in our log, not a count of what was sent — if
                // narrowing moved it backwards, a contact whose scope excludes
                // the newest ops would ask for the same range forever.
                let head = rows.last().map(|row| row.seq).unwrap_or_default();
                // Per-contact narrowing, applied at SERVE time and nowhere
                // else (Ontology §12, C5). The visibility gate decided WHICH
                // Records; this decides which columns of them.
                let scope = store::organs::contact(&self.engine.store.pool, authenticated)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|contact| contact.scope_fields);
                let rows = store::sync_ops::narrow_ops_to_scope(rows, scope.as_deref());
                // Per-record hiding: WHICH Records, where the scope above said
                // which columns of them. Both paths out of the general feed
                // apply both filters — the push path is in `drain_outbox` and
                // the two must be changed together.
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
                                // Fail CLOSED. An unreadable hide list is the
                                // one place in this path where guessing wide
                                // sends a Record somebody named as withheld.
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
                // Three checks, in this order, and the order is the point: a
                // refusal must never depend on something the asker was not
                // already entitled to know. "You hold no grant here" tells
                // them nothing they did not already know about their own
                // grants; asking about the Record first would answer "does
                // this uid exist on your Cell" to anyone who guessed one.
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
                    // Deliberately the same wording as the withheld case
                    // below. "Nothing in that conversation points at it" and
                    // "it is no longer shared with you" are different facts,
                    // and telling them apart would let a grantee probe which
                    // uids exist on our Cell one guess at a time.
                    Ok(false) => return reference_gone(),
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                // The ORDINARY §12 gate, reached THROUGH the reference rather
                // than around it. A reference must never be a way to serve
                // something we are otherwise withholding, so this is the same
                // hide list and the same per-contact scope the feed uses —
                // not a second, more permissive path to the same Record.
                let contact = store::organs::contact(&self.engine.store.pool, authenticated)
                    .await
                    .ok()
                    .flatten();
                match store::visibility::hidden_from_organ(
                    &self.engine.store.pool,
                    authenticated,
                )
                .await
                {
                    Ok(hidden) if hidden.contains(&record) => return reference_gone(),
                    Ok(_) => {}
                    // Fail CLOSED: an unreadable hide list must not serve a
                    // Record somebody named as withheld.
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                }
                // One selector language, doing both jobs at once: the same
                // `fields` that narrows the op feed narrows the row here, so a
                // withheld column comes back ABSENT rather than blank.
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
                            // Recorded only for a read that actually SERVED
                            // something. A refusal is not a read, and logging
                            // one would turn this into a record of who
                            // attempted what — which is a different, nastier
                            // table, and one nobody consented to.
                            //
                            // A failure here must not fail the read: the
                            // receipt is a courtesy to the owner, and dropping
                            // the answer because we could not write our own
                            // note would punish the reader for our problem.
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
                        // Deleted since it was mentioned. Same answer again:
                        // the reference resolves live or it resolves to
                        // nothing, and "nothing" is one answer, not three.
                        None => reference_gone(),
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchVector { organ_uid } => {
                // Only about the ASKER's own Organ, or ours. Anything else
                // would be answering "how active are that third party's
                // devices" to someone who was never told they exist — the same
                // third-party rule the sent vector already follows.
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
                match store::sync_ops::version_vector_for_organ(
                    &self.engine.store.pool,
                    &organ_uid,
                )
                .await
                {
                    Ok(vector) => WireResponse::Vector { vector },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchDoorRequests { limit } => {
                // SIBLINGS ONLY. `authenticated` is our own Organ uid exactly
                // when `sibling_organ` matched this peer against our signed
                // roster — a contact's is their uid, never ours. Held
                // Introductions name strangers who knocked, so serving them to
                // anyone else would hand out a list of people trying to reach
                // this Organ.
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
                                // A row that will not parse is a row written by
                                // a build that is gone. Skipped rather than
                                // failing the whole fetch, which would wedge
                                // the queue on one bad entry.
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
        }
    }

    /// Whether `authenticated` is this Organ itself — which, coming off a
    /// connection, can only mean a sibling Cell. `serve_connection` sets it
    /// from `sibling_organ`, and a contact's uid is never our own.
    async fn is_sibling(&self, authenticated: &str) -> bool {
        !authenticated.is_empty()
            && store::organs::local(&self.engine.store.pool)
                .await
                .ok()
                .flatten()
                .is_some_and(|organ| organ.uid == authenticated)
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
        let pulled = self.pull_catch_up().await? + self.pull_siblings().await?;
        // The idle moment, after every peer has been served: fold long tails
        // into snapshots, then drop what every contact already has.
        //
        // ORDER MATTERS and is the sequencing rule decision 2 exists to make
        // safe. Compaction is what puts a `snapshot` op above a record's crdt
        // tail, and a crdt op is prunable only once such a snapshot exists.
        // Pruning first would simply find less to do; the reverse of this pair
        // — pruning crdt ops with no snapshot above them — is what would lose
        // text with no recovery path, and the predicate refuses to.
        //
        // Both are best-effort. A failed maintenance pass must never fail a
        // sync pass, and neither loses data by being skipped.
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

    /// Open a connection to a contact, over whichever of their Cells answers
    /// first. `None` means every candidate failed and the directory had
    /// nothing newer — the ordinary state of a contact who is simply offline.
    pub async fn dial(&self, contact: &store::organs::Contact) -> Option<Connection> {
        // `organ_contact.node_id` is the authoritative target; roster Cells are
        // ADDITIONAL candidates. A missing or expired roster therefore cannot
        // make a known contact unreachable.
        //
        // All of them are RACED (Ontology §11, "Dial policy"), with the
        // preference order below as a tiebreak only. No leader election:
        // leaders exist for consensus, and an op log with CRDTs converges
        // without one.
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
        // Last resort: ask the directory where they are NOW (Ontology §11,
        // C3). This is what makes the saved key self-sufficient — every Cell
        // in every roster we hold can be gone, and the root key adopted at
        // pairing still resolves to a front door. Tried only after everything
        // known has failed, because it costs a network round trip and buys
        // nothing in the ordinary case of a contact whose laptop is simply on.
        //
        // Only on an `Internet` endpoint: a Cell bound `Local` publishes
        // nothing to the world and must not quietly start querying it either.
        if self.reach == Reach::Local {
            return None;
        }
        let resolved = match self.engine.resolve_public_record(&contact.record_uid).await {
            Ok(Some(record)) => record,
            Ok(None) => return None,
            // A refusal here is the interesting case — a record signed by
            // their key that names a DIFFERENT Organ — and it must not be
            // swallowed into "unreachable".
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

    /// Sort dial candidates best-first: a Cell we can see on the LAN right
    /// now, then everything else in the order it was collected.
    ///
    /// This is a TIEBREAK, not a policy — the race below starts every
    /// candidate, so a preferred Cell that is powered off costs a stagger
    /// interval rather than a whole dial timeout. Preferring the LAN is about
    /// latency: it is the one difference between candidates this Cell can
    /// actually observe. The Ontology also wants the always-on Cell preferred
    /// "for bulk", which is not implemented — nothing at dial time knows how
    /// much is about to move, and guessing would make small syncs pay a relay
    /// hop.
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

    /// RACE every candidate, first answer wins.
    ///
    /// Sequential dialing made an offline Cell cost the FULL dial timeout
    /// before the next was tried, so a contact whose laptop was listed first
    /// and shut delayed every later candidate by that much — per contact, per
    /// pass. Racing removes that entirely: the reachable Cell answers while
    /// the dead one is still timing out, and dropping the set cancels the
    /// losers.
    ///
    /// Staggered rather than simultaneous, so preference still means
    /// something. A better candidate gets a head start measured in
    /// milliseconds; if it answers it wins, and if it is off the others are
    /// already in flight. This is the happy-eyeballs shape, for the same
    /// reason it exists there.
    async fn try_candidates(
        &self,
        contact: &store::organs::Contact,
        candidates: &[String],
    ) -> Option<Connection> {
        use n0_future::StreamExt as _;

        // Unknown peers may use only the thread door, where every individual-
        // replica request is checked against an explicit accepted grant. Known
        // contacts use the general sync door.
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
                // Bounded: an offline peer is the NORMAL case, not an error,
                // and without a cap iroh keeps trying relays and holepunches
                // while the whole sync pass — every other contact included —
                // waits behind one closed laptop.
                tokio::time::timeout(DIAL_TIMEOUT, endpoint.connect(EndpointAddr::new(id), alpn))
                    .await
            });
        }
        while let Some(finished) = racing.next().await {
            if let Ok(Ok(connection)) = finished {
                // Dropping the set here cancels every other dial in flight.
                return Some(connection);
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

    /// Converge with the other Cells of THIS Organ (Ontology §11, C3).
    ///
    /// PULL ONLY, and that is a decision rather than an oversight. The outbox
    /// is keyed by `contact_organ` and a sibling's is our own, so a reactive
    /// push would need a second queue keyed by Cell. What pull-only costs is
    /// immediacy between two devices that are both awake: a change typed on
    /// the phone reaches an open laptop within the catch-up interval rather
    /// than at once. What it does not cost is the case the design is actually
    /// for — "edited on the phone all day, walk in the door, laptop
    /// converges" — which is catch-up by definition.
    ///
    /// No per-sibling cursor exists or is wanted. The version vector is
    /// derived from our own log every pass, so a sibling that pruned, one
    /// enrolled last week, and a half-applied previous batch are all the same
    /// question: what am I missing.
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
            // EVERY other Cell is dialed, not only the writers. A front door
            // holds `relay_capabilities()` — no write at all — and it is
            // precisely the Cell holding a queue of strangers waiting for a
            // person. Skipping it for having no capabilities would mean
            // nobody ever emptied the door, which is the one job it has.
            // Whether its OPS are taken is a separate question, asked below.
            let Ok(id) = member.node_id.parse::<EndpointId>() else {
                continue;
            };
            // The SYNC door: a sibling is `known` on the other side by the
            // same roster, so this is the general feed and not the thread
            // door's grant tier.
            let Ok(Ok(connection)) = tokio::time::timeout(
                DIAL_TIMEOUT,
                self.endpoint.connect(EndpointAddr::new(id), ALPN_SYNC),
            )
            .await
            else {
                // A sibling that is off is the ordinary case, not an error —
                // but a sibling one EPOCH behind looks identical from here,
                // and that one is a device someone needs to update rather than
                // a device someone turned off. Ask over the one ALPN that
                // survives an epoch cut.
                self.note_if_stale(member, id).await;
                continue;
            };
            // Empty the sibling's front door first, if it is one. A held
            // request is somebody waiting on a person, so it should not sit
            // behind a large op fetch.
            if let Err(error) = self.collect_door_requests(&connection).await {
                tracing::debug!(%error, cell = %member.label, "door not collected this pass");
            }
            // Ops, on the other hand, are taken only from a Cell the root said
            // may write. A relay that cannot write has no ops of its own to
            // give, and asking one for them would be asking it to relay a feed
            // it was never trusted to author.
            if !member.may(crate::roster::CAP_WRITE) {
                continue;
            }
            let vector = store::sync_ops::version_vector_for_organ(pool, &organ.uid).await?;
            let request = WireRequest::FetchOpsSince { vector, limit: 500 };
            if let Ok(WireResponse::Ops { ops, .. }) = self.exchange(&connection, &request).await {
                let batch = OpBatch {
                    // Our own Organ, because that is whose ops these are —
                    // written on another device of it. `inadmissible` checks
                    // exactly this and then checks the authoring Cell against
                    // the roster, so nothing is exempted here.
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

    /// Ask a peer which wire epoch it speaks, over the ALPN that never
    /// changes. `None` means it did not answer at all.
    pub async fn hello(&self, addr: EndpointAddr) -> Option<u32> {
        let connection = tokio::time::timeout(
            DIAL_TIMEOUT,
            self.endpoint.connect(addr, ALPN_HELLO),
        )
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

    /// Ask a sibling we could not reach which epoch it speaks, and record it
    /// if the answer is "an older one" (Ontology §11, decision 1).
    ///
    /// An epoch hard-cuts old peers at the TLS layer, so from the dialing side
    /// a stale Cell and a powered-off Cell are the same silence. They are not
    /// the same problem: one needs updating and the other needs nothing. This
    /// is the difference, and it is why `ALPN_HELLO` may never be bumped.
    async fn note_if_stale(&self, member: &crate::roster::CellEntry, id: EndpointId) {
        let Ok(Ok(connection)) = tokio::time::timeout(
            DIAL_TIMEOUT,
            self.endpoint.connect(EndpointAddr::new(id), ALPN_HELLO),
        )
        .await
        else {
            // No answer on the stable ALPN either: genuinely unreachable.
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
            // Same epoch but the sync door did not open: that is a real
            // failure and not a version problem, so do not mislabel it.
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

    /// Cells of this Organ found running a different wire epoch on the last
    /// pass. In memory and transient, like the nearby list and for the same
    /// reason: it is an observation about right now, not a fact about anyone.
    pub fn stale_siblings(&self) -> Vec<StaleSibling> {
        self.engine.stale_siblings.lock().expect("stale siblings").clone()
    }

    /// Ask a contact what they hold of OUR ops, and say whether the two logs
    /// agree (Ontology §11, C2b — the cross-Organ audit).
    ///
    /// `audit_read_model` compares this Cell against its own log, which
    /// catches local divergence and nothing else. This is the other half: a
    /// peer's log can be behind, or pruned, or wrong, and catch-up never
    /// notices because catch-up only ever asks what IT is missing.
    ///
    /// It REPORTS rather than repairs, and that is the point of the box. A
    /// disagreement between two Organs is not obviously anyone's bug — a peer
    /// legitimately prunes, and a Cell legitimately holds ops it has not sent
    /// yet — so quietly re-sending on the next pass would hide the one case
    /// worth seeing: two logs that do not converge no matter how many passes
    /// run. That is a person's decision, so a person is told.
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
        // What they hold of OUR ops.
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
        // How many of OUR ops they lack: everything past their per-Cell
        // maximum. Counted from our own log, so no ops move to find out.
        let they_lack =
            store::sync_ops::ops_missing_from_vector(pool, &local.uid, &theirs, i64::MAX)
                .await?
                .len();
        // And the reverse question, answered from the same two summaries: a
        // Cell of ours they have never heard of at all is the shape that says
        // "you enrolled a device and this contact never learned about it".
        let unknown_cells = ours
            .iter()
            .filter(|entry| !theirs.iter().any(|held| held.actor_cell == entry.actor_cell))
            .count();
        Ok(Some(AuditAgreement {
            contact_organ: contact_organ.to_string(),
            they_lack,
            unknown_cells,
        }))
    }

    /// Take what a sibling front door is holding and decide about it here
    /// (Ontology §11, "Front-door mechanics").
    ///
    /// The door held these because it could not decide: no `CAP_REPRESENT`, so
    /// binding a stranger as a contact is not something it may do. This Cell
    /// can, so it binds each one the ordinary way — as `unknown`, pending a
    /// person, exactly as if the stranger had knocked here directly. Nothing
    /// is auto-accepted; what the front door buys is that the knock is not
    /// lost while the owner's phone was off.
    async fn collect_door_requests(&self, connection: &Connection) -> Result<(), EngineError> {
        if !self.may_represent().await {
            // Two front doors talking to each other: neither may decide, and
            // moving a request between them would only lose track of it.
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
            // The SAME binding a direct knock gets, with the same refusals —
            // the door's copy of the NodeId is what iroh authenticated at the
            // door, and everything in `intro` is still a claim.
            match self.bind_unknown_peer(&held.node_id, &held.intro).await {
                Ok(organ_uid) => {
                    tracing::info!(
                        organ = %organ_uid,
                        "took a request from the front door; it is waiting for a decision"
                    );
                    taken.push(held.uid);
                }
                Err(error) => {
                    // A refused binding is DONE WITH, not retried forever: the
                    // stranger claimed something that conflicts with what we
                    // hold, and re-fetching it every pass would make the door
                    // permanently full.
                    //
                    // But it is a SECURITY event, not a failure — the refusals
                    // `bind_unknown_peer` makes are "this NodeId is already
                    // bound to another Organ" and its mirror, which is someone
                    // claiming to be a contact you already hold. Dropping that
                    // into a log line only would make it invisible, so it goes
                    // to the quarantine ring C0 built for exactly this class.
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
            // Only known contacts receive the general Organ feed. An unknown
            // peer can still synchronize roots both parties explicitly
            // accepted below.
            if contact.trust == "known" && contact.sync_in {
                // What we already hold of THIS contact's ops. No checkpoint to
                // keep in step and none to get wrong: the answer is derived
                // from our own log every pass, so a peer that pruned, a Cell
                // they enrolled last week, and a partially-applied previous
                // batch all resolve to the same question — "what am I
                // missing" — instead of three separate failure modes.
                let vector = store::sync_ops::version_vector_for_organ(
                    pool,
                    &contact.record_uid,
                )
                .await?;
                let request = WireRequest::FetchOpsSince {
                    vector,
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
                        // Kept as a DIAGNOSTIC only — "how far had we got" for
                        // a human reading the contact row. Correctness no
                        // longer depends on it, which is the point: it is the
                        // peer's local seq, and that number stops meaning
                        // anything the moment they prune.
                        store::organs::set_last_synced_seq(pool, &contact.record_uid, head).await?;
                        pulled += 1;
                    }
                }
            }
            // Then every conversation they have granted US.
            for root in store::replica::roots_for_contact(pool, &contact.record_uid).await? {
                // What we already hold INSIDE this conversation. Replaces a
                // cursor pinned at zero, which re-fetched the whole
                // conversation every pass.
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
            // Then successions: AFTER revocations, so a dead key cannot endorse
            // a live one in the same pass that learns it is dead, and BEFORE
            // the roster, so a roster signed by a freshly-endorsed key already
            // chains by the time it is evaluated. Getting this order wrong
            // does not fail loudly — it just refuses a legitimate rotation
            // until the next pass, or accepts one it should have refused.
            if let Ok(WireResponse::Successions { certs }) = self
                .exchange(&connection, &WireRequest::FetchSuccessions)
                .await
            {
                for cert in certs {
                    // Only about THEIR OWN keys, exactly as with revocations: a
                    // contact does not get to rotate a third Organ's identity
                    // by telling us it happened.
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
                        // A succession that does not chain is the silent-takeover
                        // attempt the rule exists to catch. Refusing is the
                        // designed behaviour, but it must be visible.
                        Ok(false) => tracing::warn!(
                            organ = %cert.organ_uid,
                            old = %cert.old_key,
                            "REFUSED a key succession that does not chain from a key we hold"
                        ),
                        Err(error) => tracing::warn!(%error, "succession could not be evaluated"),
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
}

/// The one answer a reference read gives when it cannot serve the Record —
/// withheld, retracted, never referenced, or deleted since.
///
/// ONE answer on purpose. These are different facts, and distinguishing them
/// would turn a conversation into a way to probe which uids exist on someone
/// else's Cell, one guess at a time. It is also the right thing to SAY: from
/// the reader's side all four mean the same thing, and it is a permission
/// answer rather than an error that reads like a bug.
fn reference_gone() -> WireResponse {
    WireResponse::Refused {
        code: "not_shared".into(),
        message: "that is no longer shared with you".into(),
    }
}
