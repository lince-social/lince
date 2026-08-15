//! The PUBLIC directory record: identity key → where to reach this Organ
//! (Ontology §11, "Identity, roster, and publishing", cluster C3).
//!
//! Without this, "one key is all they save" is false. A contact learns roster
//! v2 only by reaching a Cell listed in roster v1, so adding a laptop while
//! the old Cells are off or lost strands the new one forever. Publishing under
//! the identity key itself closes that loop: identity key → current front door
//! resolves with no prior roster at all.
//!
//! # pkarr in one line
//!
//! A phone book whose lookup key is your public key. A small signed blob goes
//! into the mainline DHT (or a relay that fronts it); anyone holding the public
//! key fetches it and verifies the signature. Nothing to do with Lince Records
//! — the "record" in "resource record" is the DNS sense.
//!
//! # This publishes the PUBLIC TIER ONLY, and the byte cap is why
//!
//! The Ontology estimated "five Cells plus a version counter, expiry and
//! signature lands near 250 bytes", counting NodeIds alone. A real
//! `SignedRoster` is nothing like that: each `CellEntry` also carries a uuid,
//! a label, a 44-char operational key and a capability list, so one Cell is
//! ~520 bytes of JSON and five are ~1330 — past the 1000-byte DNS packet the
//! DHT will carry (BEP44; `SignedPacket::MAX_BYTES` is 1104 with the key,
//! signature and timestamp on top). The full roster DOES NOT FIT, and that is
//! not a limitation to work around — it is the two-tier design arriving as a
//! hard constraint:
//!
//! - public tier (here): the front-door Cells only. A stranger who finds the
//!   key on a website learns that one machine exists and nothing else.
//! - contact tier (`FetchRoster` over an authenticated connection): the full
//!   roster, so contacts dial personal devices directly.
//!
//! Untiered, anyone holding the published key would learn how many devices the
//! Organ has, each one's current address, and which are online right now —
//! which is to say whether the owner is home, travelling or asleep.
//!
//! # There is no second signature layer, deliberately
//!
//! The packet is signed by the keypair it is addressed BY, and that keypair is
//! the Organ root key. pkarr's own signature check therefore IS the root
//! signature check; a separate `SignedFrontDoor` payload would be a second
//! scheme to get wrong for no gain. What the resolver still must check is that
//! the `organ_uid` inside matches the one it expects for that key, which is
//! `verify_for`.
//!
//! # Publishing needs the root; REPUBLISHING does not
//!
//! DHT entries expire in hours, so something has to republish on a timer. The
//! signed bytes are stored (`store::roster::put_public_packet`) and re-sent
//! verbatim, so a front-door Cell holding no identity-signing material can do
//! it. Re-signing on every republish would quietly require the root online
//! forever and undo the whole key split — the thing the Ontology warns about
//! in so many words.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use pkarr::dns::rdata::{RData, TXT};
use pkarr::dns::Name;
use pkarr::{Keypair, PublicKey, SignedPacket};

use crate::error::EngineError;
use crate::roster::{ROOT_KEY_ID, SignedRoster};
use crate::trust::Signer;

/// The DNS name the Lince record lives under, inside the Organ's own zone.
/// Underscore-prefixed by DNS convention for a name that is a service entry
/// rather than a host.
const RECORD_NAME: &str = "_lince";

/// How long a resolver may treat the record as fresh. Half an hour: long
/// enough that resolution is not a DHT round trip every time, short enough
/// that a front door that moves is not stale for an afternoon.
const RECORD_TTL_SECS: u32 = 1800;

/// The format version inside the record. Bumped when the attribute set
/// changes; an unknown version is refused rather than half-read, on the same
/// fail-closed rule as the wire.
const RECORD_VERSION: &str = "1";

/// How many front doors are published. The cap exists because the packet has a
/// hard byte ceiling and an over-large one fails at publish time; four is well
/// inside it and more front doors than any Organ needs.
pub const MAX_PUBLIC_CELLS: usize = 4;

/// The public tier, decoded: who this is, which roster it came from, and the
/// front doors a stranger may dial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicRecord {
    pub organ_uid: String,
    /// The roster version this was cut from. A contact holding an older roster
    /// learns from this that a newer one exists without being handed it.
    pub roster_version: i64,
    /// Front-door NodeIds, in roster order.
    pub node_ids: Vec<String>,
}

/// The public tier of a roster: front-door Cells and nothing else.
///
/// Filtering here rather than at publish time is what makes the privacy
/// property structural — a personal Cell cannot reach the DHT by any path
/// through this module, because it never survives this function.
pub fn public_record(signed: &SignedRoster) -> PublicRecord {
    PublicRecord {
        organ_uid: signed.roster.organ_uid.clone(),
        roster_version: signed.roster.version,
        node_ids: signed
            .roster
            .cells
            .iter()
            .filter(|cell| cell.front_door)
            .map(|cell| cell.node_id.clone())
            .take(MAX_PUBLIC_CELLS)
            .collect(),
    }
}

/// Sign a public record under the Organ ROOT key, returning the bytes to store
/// and broadcast.
///
/// Takes raw secret bytes rather than a `Signer` because this is the one place
/// the root key is used by a library that wants its own key type, and handing
/// it the 32 bytes keeps `trust::Signer` from growing a pkarr-shaped hole.
pub fn encode(record: &PublicRecord, root_secret: &[u8; 32]) -> Result<Vec<u8>, EngineError> {
    if record.node_ids.is_empty() {
        // Publishing a record with no address is worse than publishing none:
        // it answers "where is this Organ" with an authoritative "nowhere",
        // and it stays that answer until it expires.
        return Err(EngineError::Consequence(
            "a public record needs at least one front-door Cell".into(),
        ));
    }
    let keypair = Keypair::from_secret_key(root_secret);
    let mut strings = vec![
        format!("v={RECORD_VERSION}"),
        format!("o={}", record.organ_uid),
        format!("r={}", record.roster_version),
    ];
    // One attribute per Cell rather than one comma-joined list: a TXT
    // character-string caps at 255 bytes, which two NodeIds already exceed,
    // and `attributes()` collapses repeated keys so `c=` twice would silently
    // lose a front door.
    for (index, node_id) in record.node_ids.iter().take(MAX_PUBLIC_CELLS).enumerate() {
        strings.push(format!("c{index}={node_id}"));
    }
    let mut txt = TXT::new();
    for string in &strings {
        txt.add_string(string)
            .map_err(|error| EngineError::Consequence(format!("public record: {error}")))?;
    }
    let name = Name::new(RECORD_NAME)
        .map_err(|error| EngineError::Consequence(format!("public record name: {error}")))?;
    let packet = SignedPacket::builder()
        .txt(name, txt, RECORD_TTL_SECS)
        .sign(&keypair)
        // The only realistic failure is `PacketTooLarge`, and it is worth
        // surfacing as itself: it means the record grew past what the DHT
        // carries, which is the constraint this whole module is shaped by.
        .map_err(|error| EngineError::Consequence(format!("public record: {error}")))?;
    Ok(packet.serialize())
}

/// Parse a resolved packet back into a public record.
///
/// Signature verification is NOT done here — `SignedPacket` only exists in a
/// verified state when it came from `Client::resolve` or `from_relay_payload`.
/// `decode_stored` is the entry point for bytes off disk, where that is not
/// true yet.
pub fn decode(packet: &SignedPacket) -> Result<PublicRecord, EngineError> {
    let mut version: Option<String> = None;
    let mut organ_uid: Option<String> = None;
    let mut roster_version: i64 = 0;
    let mut cells: Vec<(usize, String)> = Vec::new();
    for record in packet.all_resource_records() {
        let RData::TXT(txt) = &record.rdata else {
            continue;
        };
        for (key, value) in txt.attributes() {
            let Some(value) = value else { continue };
            match key.as_str() {
                "v" => version = Some(value),
                "o" => organ_uid = Some(value),
                "r" => roster_version = value.parse().unwrap_or(0),
                other => {
                    if let Some(index) = other.strip_prefix('c') {
                        if let Ok(index) = index.parse::<usize>() {
                            cells.push((index, value));
                        }
                    }
                }
            }
        }
    }
    // Fail closed on an unknown version, the same rule the wire follows: a
    // newer Organ's record is refused rather than half-read as if the fields
    // it does carry still mean what they used to.
    if version.as_deref() != Some(RECORD_VERSION) {
        return Err(EngineError::Consequence(
            "public record is not a version this build understands".into(),
        ));
    }
    let Some(organ_uid) = organ_uid.filter(|uid| !uid.is_empty()) else {
        return Err(EngineError::Consequence(
            "public record names no Organ".into(),
        ));
    };
    // `attributes()` is a HashMap, so the order Cells come back in is not the
    // order they went in; the index in the key is what restores it.
    cells.sort_by_key(|(index, _)| *index);
    Ok(PublicRecord {
        organ_uid,
        roster_version,
        node_ids: cells.into_iter().map(|(_, node_id)| node_id).collect(),
    })
}

/// Re-verify bytes that came off disk and hand back the packet, ready to be
/// republished verbatim.
///
/// `SignedPacket::deserialize` parses without checking the signature, so
/// trusting it directly would let anything that could write the database
/// choose where contacts dial. Round-tripping through `from_relay_payload`
/// re-runs the ed25519 check against the key the packet is addressed by.
pub fn decode_stored(bytes: &[u8]) -> Result<SignedPacket, EngineError> {
    let parsed = SignedPacket::deserialize(bytes)
        .map_err(|error| EngineError::Consequence(format!("stored public record: {error}")))?;
    SignedPacket::from_relay_payload(&parsed.public_key(), &parsed.to_relay_payload()).map_err(
        |error| EngineError::Consequence(format!("stored public record is not signed: {error}")),
    )
}

/// The base64 public key a packet is addressed by — the Organ ROOT key, in the
/// same encoding `roster.root_key` uses.
pub fn packet_root_key(packet: &SignedPacket) -> String {
    B64.encode(packet.public_key().as_bytes())
}

/// A root key, as base64, turned into the lookup key.
pub fn lookup_key(root_key_b64: &str) -> Result<PublicKey, EngineError> {
    let bytes = B64
        .decode(root_key_b64)
        .map_err(|_| EngineError::Consequence("root key is not base64".into()))?;
    let bytes: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| EngineError::Consequence("root key is not 32 bytes".into()))?;
    PublicKey::try_from(&bytes)
        .map_err(|error| EngineError::Consequence(format!("root key is not a key: {error}")))
}

/// Check a resolved record against what we already believe about this Organ.
///
/// The signature proves the holder of the root key wrote it. It does NOT prove
/// the record is about the Organ we were looking for — a key we saved under
/// one uid answering with another is either a mistake or a substitution, and
/// either way the addresses in it must not be dialed as that contact.
pub fn verify_for(record: &PublicRecord, expected_organ_uid: &str) -> Result<(), EngineError> {
    if record.organ_uid != expected_organ_uid {
        return Err(EngineError::Consequence(format!(
            "public record for {expected_organ_uid} names a different Organ ({})",
            record.organ_uid
        )));
    }
    Ok(())
}

/// The network half: a pkarr client pointed at relays.
///
/// RELAYS rather than a DHT client, deliberately. A DHT client opens its own
/// UDP socket and runs a bootstrap alongside iroh's, which is a second network
/// stack in-process for a feature that publishes a few hundred bytes an hour.
/// Relays are HTTP PUT/GET over the connection machinery that already exists,
/// and they give C4 the obvious extension point: an Organ running its own
/// infrastructure points this at its own relay instead of the public ones.
pub struct Directory {
    client: pkarr::Client,
}

impl Directory {
    /// Build a client against `relays`, or the public defaults when empty.
    pub fn new(relays: &[String]) -> Result<Directory, EngineError> {
        let mut builder = pkarr::Client::builder();
        // No DHT socket: see the type comment.
        builder.no_dht();
        if !relays.is_empty() {
            builder.relays(relays).map_err(|error| {
                EngineError::Consequence(format!("directory relay url: {error}"))
            })?;
        }
        let client = builder
            .build()
            .map_err(|error| EngineError::Consequence(format!("directory client: {error}")))?;
        Ok(Directory { client })
    }

    /// Broadcast an already-signed packet. Needs no key — see the module
    /// comment on why that matters.
    pub async fn publish(&self, packet: &SignedPacket) -> Result<(), EngineError> {
        self.client
            .publish(packet)
            .await
            .map(|_| ())
            .map_err(|error| EngineError::Consequence(format!("publishing the record: {error}")))
    }

    /// Look an Organ up by its root key. `None` means nothing was found, which
    /// is the ordinary answer for an Organ that publishes nothing.
    pub async fn resolve(&self, key: &PublicKey) -> Option<SignedPacket> {
        self.client
            .resolve(key, pkarr::ResolvePolicy::CacheFirst)
            .await
            .ok()
    }
}

impl crate::Engine {
    /// The relays this Cell uses, from `lince.discovery.relays` on the local
    /// Organ. Empty means the public defaults.
    /// Which pkarr relays this Cell publishes through — SELF-HOSTING, the C4
    /// box (Ontology §11 "The user's own infrastructure").
    ///
    /// Empty means the public defaults, which is the honest starting point:
    /// reachability that depends on somebody else's boxes is still
    /// reachability, and pretending otherwise would just mean a Cell nobody
    /// can find. Naming your own relay here is what makes the last piece of
    /// this Organ's infrastructure its own.
    ///
    /// Read from the CELL Record, because which relay a machine uses is a
    /// property of that machine — a laptop on a home connection and a VPS in a
    /// datacentre have no reason to agree — and because a relay Cell may not
    /// write to the Organ Record at all.
    async fn directory_relays(&self, organ_uid: &str) -> Vec<String> {
        let cell = store::cells::config(&self.store.pool, "lince.discovery")
            .await
            .ok()
            .flatten();
        let fields = match cell {
            Some(fields) => fields,
            None => {
                let Ok(Some(fields)) =
                    store::records::get_extension(&self.store.pool, organ_uid, "lince.discovery")
                        .await
                else {
                    return Vec::new();
                };
                fields
            }
        };
        fields
            .get("relays")
            .and_then(serde_json::Value::as_array)
            .map(|relays| {
                relays
                    .iter()
                    .filter_map(|relay| relay.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn directory(&self, organ_uid: &str) -> Result<&Directory, EngineError> {
        self.directory
            .get_or_try_init(|| async {
                let relays = self.directory_relays(organ_uid).await;
                Directory::new(&relays)
            })
            .await
    }

    /// Cut the public tier from this Organ's roster, sign it with the root, and
    /// store the signed bytes. Broadcasting is `republish_public_record`, and
    /// the split is the point: this needs the root, that one does not.
    pub async fn sign_public_record(&self, root: &Signer) -> Result<(), EngineError> {
        let Some(signed) = self.roster_of(&root.actor_uid).await? else {
            return Err(EngineError::Consequence(
                "no roster to cut a public record from".into(),
            ));
        };
        let record = public_record(&signed);
        // Sign only when the record would actually differ from the stored one.
        //
        // pkarr orders packets by an embedded timestamp and a publish is
        // refused if it is older than what the relay's cache holds, so minting
        // a fresh signature on every boot puts this Cell into a timestamp race
        // with the keyless Cell republishing the stored bytes — two packets
        // for one key, differing only in when they were signed. Same bytes for
        // the same content keeps the ordering trivial.
        if let Some(stored) = store::roster::public_packet(&self.store.pool, &root.actor_uid).await?
        {
            if let Ok(previous) = decode_stored(&stored).and_then(|packet| decode(&packet)) {
                if previous == record {
                    return Ok(());
                }
            }
        }
        if record.node_ids.is_empty() {
            // Not an error: an Organ whose Cells are all personal has made
            // exactly the choice the two tiers exist to offer. Clear any
            // previously published bytes so turning the front door OFF stops
            // the republish timer rather than leaving it broadcasting an
            // address that is no longer meant to be public.
            store::roster::clear_public_packet(&self.store.pool, &root.actor_uid).await?;
            return Ok(());
        }
        let bytes = encode(&record, &root.secret_bytes())?;
        store::roster::put_public_packet(&self.store.pool, &root.actor_uid, &bytes).await?;
        Ok(())
    }

    /// Broadcast the stored packet. Safe on a Cell holding no identity-signing
    /// material, which is what makes "the VPS republishes the roster" true
    /// without it becoming "the VPS signs the roster".
    ///
    /// `Ok(false)` means there was nothing to publish.
    pub async fn republish_public_record(&self, organ_uid: &str) -> Result<bool, EngineError> {
        let Some(bytes) = store::roster::public_packet(&self.store.pool, organ_uid).await? else {
            return Ok(false);
        };
        let packet = decode_stored(&bytes)?;
        self.directory(organ_uid).await?.publish(&packet).await?;
        Ok(true)
    }

    /// Resolve a contact by the root key we hold for them, returning the front
    /// doors they publish.
    ///
    /// This is the half that makes the saved key self-sufficient: with no
    /// reachable Cell from any roster we hold, the key alone still answers
    /// "where are they now".
    pub async fn resolve_public_record(
        &self,
        organ_uid: &str,
    ) -> Result<Option<PublicRecord>, EngineError> {
        let Some(root_key) = crate::trust::key_of(&self.store, organ_uid, ROOT_KEY_ID).await?
        else {
            return Ok(None);
        };
        let key = lookup_key(&root_key)?;
        // Relay configuration is OURS, never theirs. With no local Organ the
        // fallback is the public defaults, NOT the contact's uid: the client
        // is cached for the process, so reading a contact's settings even once
        // would pin this Cell to relays chosen by someone else.
        let local = store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.uid)
            .unwrap_or_default();
        let Some(packet) = self.directory(&local).await?.resolve(&key).await else {
            return Ok(None);
        };
        let record = decode(&packet)?;
        verify_for(&record, organ_uid)?;
        Ok(Some(record))
    }
}
