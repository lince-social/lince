//! The identity floor (Ontology §11 "Key compromise: the root/operational
//! split, and honest recovery").
//!
//! The problem, stated without flinching: whoever holds an Organ's identity
//! private key can sign a roster adding their own device, sign ops as the
//! owner, and BE that Organ to everyone holding the public key. There is no
//! authority to report that to, and the attacker can announce "I was
//! compromised, here is my new key" exactly as well as the victim can. No
//! design removes that; designs only change who has to be fooled.
//!
//! Three moves, in order of leverage, and this module implements the first
//! two:
//!
//! 1. **Make the catastrophic case rare.** The ROOT key signs only two things
//!    — the Cell roster and key successions — and is meant to live offline.
//!    Each Cell holds an OPERATIONAL key for everything routine. Compromising
//!    a device is then compromising one revocable credential, not the
//!    identity. This is the standard shape (TLS roots and intermediates, SSH
//!    CAs, PGP primary keys with subkeys, Signal identity keys with prekeys).
//! 2. **Make substitution VISIBLE.** A contact accepts a new root key only if
//!    it CHAINS from one they already hold. Anything else is a blocking
//!    warning that needs a human, never a silent update — the cheap
//!    approximation of key transparency, and what turns a silent takeover
//!    into an alarm.
//! 3. Recovery stays deliberately simple: publish the pre-signed revocation
//!    certificate, then re-pair through QR in person or an already-trusted
//!    chat. M-of-N social recovery was considered and rejected — every
//!    recovery path is also an attack path.
//!
//! **There is no trust-on-first-use here.** TOFU happens exactly once, at
//! pairing, through the Introduction that already carries an Organ's keys. By
//! the time any roster arrives we already hold a key for that Organ, so every
//! roster must chain. Without that rule a stranger who guessed a contact uid
//! could present a self-signed roster and become them.

use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::error::EngineError;
use crate::trust::Signer;

/// The key id under which an Organ's ROOT public key is published and
/// exchanged in an Introduction. Contacts adopt it at pairing; every later
/// roster is checked against it.
pub const ROOT_KEY_ID: &str = "ed25519:root:v1";

/// The key id an individual Cell's OPERATIONAL key is published under
/// (Ontology §11, decision 4; the C1 box closed 2026-08-11).
///
/// One row per Cell, all under the Organ's `actor_uid`, because everything
/// that resolves a key — `keys_of(organ)`, a transfer envelope's
/// `origin_key_id`, the Introduction exchange — asks by Organ and must keep
/// working. Putting the Cell in the KEY ID rather than in `actor_uid` gives
/// each device its own row without moving that question.
///
/// It was `ed25519:organ:v1` for every Cell, which made the identity of a
/// second device impossible to represent: `identity_key` is keyed
/// `(actor_uid, key_id)` and `require_published_key` refuses to overwrite a
/// published key, so two Cells of one Organ collided on one row and the second
/// could not bind its own key at all. Enrolment did not trip it — only the
/// root travels there, under a different id — but sibling sync trips it
/// immediately.
pub fn cell_key_id(cell_uid: &str) -> String {
    format!("ed25519:cell:{cell_uid}:v1")
}

/// Whether a key id names a ROOT key — the only kind that may speak for the
/// identity itself.
pub fn is_root_key_id(key_id: &str) -> bool {
    key_id == ROOT_KEY_ID
}

/// How long a published roster stays valid. Self-limiting credentials beat
/// remembering to revoke: a Cell that stops syncing fresh rosters loses
/// authority on its own.
pub const ROSTER_VALIDITY_DAYS: i64 = 30;

/// Domain separation, same discipline as `peers::PEER_SIGNING_DOMAIN`: a
/// signature made for one purpose must never verify as another.
const ROSTER_DOMAIN: &str = "lince/roster/1\n";
const SUCCESSION_DOMAIN: &str = "lince/succession/1\n";
const REVOCATION_DOMAIN: &str = "lince/revocation/1\n";

/// Field and record separators for the signing payload. Chosen because they
/// cannot occur in a uid, a base64 key or an RFC3339 timestamp — and any
/// label containing one is REJECTED at build time rather than escaped, so
/// two different rosters can never produce the same bytes.
const FIELD_SEP: char = '\u{1f}';
const RECORD_SEP: char = '\u{1e}';

/// How long an enrolment token stays valid. Short because it grants
/// membership in the identity, and the flow it serves is "hold the new device
/// up to this screen", which takes a minute.
pub const ENROLMENT_TOKEN_TTL_MINUTES: i64 = 10;

/// One member Cell: a device, its connection identity, and the operational key
/// the root certifies by listing it here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellEntry {
    pub cell_uid: String,
    pub node_id: String,
    pub label: String,
    pub operational_key: String,
    /// This Cell's current X25519 SEALING key, for mail left with a carrier
    /// that must not read it (Ontology C4, `crate::seal`).
    ///
    /// Here rather than anywhere else because the roster is already the signed,
    /// versioned, expiring statement of who an Organ's Cells are, and it is
    /// refreshed at the top of every catch-up — so a rotation reaches senders
    /// through a channel that exists, with a root signature over it, and
    /// nothing new has to be published or trusted.
    ///
    /// Optional because a Cell that has never generated one is a normal state
    /// (it simply cannot be mailed, and `seal` refuses rather than falling
    /// back to plaintext), and because the sealing key is deliberately NOT the
    /// identity key: one key doing both jobs forecloses rotation and ties
    /// uncollected mail to identity-key replacement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealing_key: Option<crate::seal::SealingKey>,
    /// Whether this Cell belongs to the PUBLIC tier — the resolution of "I
    /// want an add-me-in-Lince key without exposing my devices".
    ///
    /// The identity key is safe to publish anywhere; on its own it is an
    /// identifier. The exposure is in what it RESOLVES TO. So only front-door
    /// Cells (the always-on one) publish addresses to the DHT, and a stranger
    /// holding the key learns that one machine exists and nothing else.
    /// Personal Cells never appear in any public record — contacts get the
    /// full roster over an already-authenticated connection and dial them
    /// directly, which is what the contact tier is for.
    ///
    /// Enforced in practice by `lince.discovery.internet`: a personal Cell
    /// turns it off and publishes no addresses at all.
    #[serde(default)]
    pub front_door: bool,
    /// What this Cell may do IN THE ORGAN'S NAME (Ontology §11, decision 4).
    ///
    /// A Cell is a permission subject, not merely a member. Without this a
    /// device is either wholly you or not you at all, which makes two things
    /// unbuildable: narrowing a stolen phone instead of reissuing an identity,
    /// and the promise that a front door holds no signing material — that one
    /// stays an assertion rather than a structural fact.
    ///
    /// Signed by the root along with the membership, so a Cell cannot widen
    /// its own grant. Empty = nothing, deliberately: an unknown or missing
    /// capability set must degrade to no authority, never to full authority.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Write into the Organ's data at all — log ops, sync outward. A relay Cell
/// has this and nothing else, which is what turns "the front door holds no
/// signing material" from a promise into a structural fact.
pub const CAP_WRITE: &str = "write";
/// Run Karma rules in the Organ's name. Separable because a Cell that carries
/// traffic should not also be firing rules with outward consequences.
pub const CAP_KARMA: &str = "karma";
/// Speak for the Organ to contacts: pair, accept grants, answer for it.
pub const CAP_REPRESENT: &str = "represent";

/// What an ordinary personal device gets. Named rather than inlined because
/// "what a normal Cell may do" is a policy that will move.
pub fn full_capabilities() -> Vec<String> {
    vec![
        CAP_WRITE.to_string(),
        CAP_KARMA.to_string(),
        CAP_REPRESENT.to_string(),
    ]
}

/// A carrier and nothing more: no writes, no rules, no speaking for anyone.
pub fn relay_capabilities() -> Vec<String> {
    Vec::new()
}

impl CellEntry {
    pub fn may(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|held| held == capability)
    }
}

/// One place this Organ's mail may be left when its own Cells cannot be
/// reached (Ontology C4).
///
/// The RECIPIENT publishes these, never the sender. A sender-chosen mailbox
/// lets anyone drop data on any mutual contact, at a box the recipient may
/// never think to look in; published here, the only boxes anyone may use are
/// the ones the recipient named and therefore watches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickupPoint {
    /// The carrier's Organ. What the recipient consented to, in the terms they
    /// consented in: "this friend of mine holds my mail".
    pub organ_uid: String,
    /// The carrier Cell to dial. A snapshot, and knowingly so: this is the
    /// mirror of the failure the carrier side fixed by registering ROOT KEYS
    /// instead of addresses, but a sender holds only the RECIPIENT's roster —
    /// it has no way to look the carrier's own roster up. The staleness is
    /// bounded by the roster's own validity, and a carrier that changes node
    /// id costs the recipient one re-publish.
    pub node_id: String,
    /// What the recipient calls this box. Their own note, for their own panel.
    pub label: String,
}

/// The published identity: one Organ, one root key, its member Cells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roster {
    pub organ_uid: String,
    pub root_key: String,
    pub version: i64,
    pub not_after: String,
    pub cells: Vec<CellEntry>,
    /// Where mail may be left for this Organ. Organ-level rather than
    /// per-Cell: a sealed bundle is addressed to the Organ and any of its
    /// Cells opens it, so which device collects is nobody else's business.
    ///
    /// In the roster for the same reason the sealing keys are — it is the
    /// signed, expiring, already-refreshed statement of how to reach this
    /// Organ — and inside the signature for the same reason too: an unsigned
    /// pickup point is a redirection that costs an attacker nothing.
    #[serde(default)]
    pub pickup: Vec<PickupPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedRoster {
    pub roster: Roster,
    pub signature: String,
}

/// What happened when a roster arrived. Not a bool, because "refused" and
/// "expired" call for very different responses from the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosterOutcome {
    /// Verified, newer, stored.
    Accepted,
    /// Verified but not newer than what we hold. Normal and quiet.
    NotNewer,
    /// Past `not_after`. The previously-held roster survives UNCHANGED and
    /// stays usable for dialing — an expired roster must not silently drop a
    /// contact you talk to daily to unreachable. What it must not do is
    /// introduce new Cells, so it is refused as an update and surfaced as a
    /// staleness warning instead.
    Expired,
    /// Signed by a key that does not chain from one we already hold, or by a
    /// revoked key. This is the alarm: a loud, blocking refusal that keeps the
    /// old roster intact and needs a human decision.
    Refused,
}

/// The exact bytes a root key signs for a roster. Built explicitly rather than
/// by serializing a struct: field order from `serde` is not a signing
/// guarantee, and a reordered struct would silently change what a signature
/// means.
/// Whether a roster is internally consistent: signed by the root key it
/// names, and not expired.
///
/// This is HALF of `adopt_roster`'s check and must never be mistaken for all
/// of it — it says nothing about whether that root key is one we have any
/// reason to trust, which is what `key_chains` decides. Split out because a
/// mailbox needs exactly this half: it verifies a presented roster's integrity
/// itself, then asks whether the key chains from the one registration
/// recorded, rather than from anything this Cell happens to have paired with.
pub fn roster_signature_is_valid(signed: &SignedRoster) -> bool {
    let Ok(payload) = roster_signing_payload(&signed.roster) else {
        return false;
    };
    if !verify_with(&signed.roster.root_key, &payload, &signed.signature) {
        return false;
    }
    DateTime::parse_from_rfc3339(&signed.roster.not_after)
        .map(|when| when.with_timezone(&Utc) > Utc::now())
        .unwrap_or(false)
}

pub fn roster_signing_payload(roster: &Roster) -> Result<Vec<u8>, EngineError> {
    fn push(out: &mut String, value: &str) -> Result<(), EngineError> {
        if value.contains(FIELD_SEP) || value.contains(RECORD_SEP) {
            return Err(EngineError::Consequence(
                "roster field contains a reserved separator".into(),
            ));
        }
        out.push_str(value);
        out.push(FIELD_SEP);
        Ok(())
    }
    let mut out = String::from(ROSTER_DOMAIN);
    push(&mut out, &roster.organ_uid)?;
    push(&mut out, &roster.root_key)?;
    push(&mut out, &roster.version.to_string())?;
    push(&mut out, &roster.not_after)?;
    for cell in &roster.cells {
        push(&mut out, &cell.cell_uid)?;
        push(&mut out, &cell.node_id)?;
        push(&mut out, &cell.label)?;
        push(&mut out, &cell.operational_key)?;
        push(&mut out, if cell.front_door { "1" } else { "0" })?;
        // Inside the signature for the same reason the capabilities are.
        // Mail is sealed to whatever the roster says, so a sealing key the
        // root did not endorse is a redirection: substitute your own and every
        // bundle a contact sends is sealed to you, while the roster still
        // verifies and still names the right devices. The expiry is signed
        // too — otherwise it could be pushed out to keep a retired key
        // collecting mail long after its private half was meant to be gone.
        if let Some(key) = &cell.sealing_key {
            push(&mut out, &key.key_id)?;
            push(&mut out, &key.public)?;
            push(&mut out, &key.not_after)?;
        }
        // Inside the signature, or a Cell could widen its own grant in transit
        // and the root's endorsement would still verify.
        for capability in &cell.capabilities {
            push(&mut out, capability)?;
        }
        out.push(RECORD_SEP);
    }
    // Inside the signature, or naming a pickup point would be free: substitute
    // a box you run for one the recipient chose and every contact who falls
    // back leaves their mail with you. It stays sealed — the sealing keys are
    // signed too — but you learn who writes to them, how often and how much,
    // which is the whole metadata cost this design makes a person consent to.
    for point in &roster.pickup {
        push(&mut out, &point.organ_uid)?;
        push(&mut out, &point.node_id)?;
        push(&mut out, &point.label)?;
        out.push(RECORD_SEP);
    }
    Ok(out.into_bytes())
}

/// Whether a roster naming `cells` under `root_key` would differ from the one
/// we already hold — that is, whether publishing is worth a new version.
///
/// Re-signing on every boot would burn through versions and, worse, train
/// contacts to accept a stream of rosters they have no reason to inspect. But
/// the comparison has to be of the WHOLE MEMBER SET.
///
/// **The bug this function exists to end (found 2026-08-09, fixed here).** The
/// caller used to ask "is this Cell present with a node id and capabilities",
/// which is satisfied by THIS Cell being present no matter who else was
/// removed. So revoking a DIFFERENT Cell re-signed nothing, nothing was
/// published, and the revoked device stayed a member of the published identity
/// until the roster expired 30 days later. A revocation that never publishes
/// is not a revocation.
pub fn needs_publishing(held: Option<&SignedRoster>, root_key: &str, cells: &[CellEntry]) -> bool {
    let Some(held) = held else {
        return true;
    };
    if held.roster.root_key != root_key {
        return true;
    }
    // An entry with NO capability set is not "unchanged" either. Capabilities
    // arrived after some rosters were signed and an absent set grants nothing,
    // so leaving one in place would quietly strip a Cell of the right to write
    // in its own Organ — and nothing would ever re-sign to fix it.
    if held
        .roster
        .cells
        .iter()
        .any(|cell| cell.capabilities.is_empty())
    {
        return true;
    }
    // Order is not meaning: a roster is a set, and re-signing because two
    // members swapped positions would burn a version for nothing.
    let mut held_cells = held.roster.cells.clone();
    let mut next_cells = cells.to_vec();
    held_cells.sort_by(|left, right| left.cell_uid.cmp(&right.cell_uid));
    next_cells.sort_by(|left, right| left.cell_uid.cmp(&right.cell_uid));
    held_cells != next_cells
}

pub fn succession_signing_payload(
    organ_uid: &str,
    old_key: &str,
    new_key: &str,
    created_at: &str,
) -> Vec<u8> {
    format!("{SUCCESSION_DOMAIN}{organ_uid}\n{old_key}\n{new_key}\n{created_at}").into_bytes()
}

pub fn revocation_signing_payload(organ_uid: &str, revoked_key: &str) -> Vec<u8> {
    format!("{REVOCATION_DOMAIN}{organ_uid}\n{revoked_key}").into_bytes()
}

fn verify_with(public_key_b64: &str, payload: &[u8], signature_b64: &str) -> bool {
    let Ok(key_bytes) = B64.decode(public_key_b64) else {
        return false;
    };
    let Ok(key_bytes) = <[u8; 32]>::try_from(key_bytes.as_slice()) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&key_bytes) else {
        return false;
    };
    let Ok(sig_bytes) = B64.decode(signature_b64) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&sig_bytes) else {
        return false;
    };
    key.verify(payload, &signature).is_ok()
}

impl Engine {
    /// Publish the root public key so it travels in this Organ's Introduction.
    /// That is the ONLY moment a contact trusts it on faith; everything after
    /// must chain from it.
    pub async fn publish_root_key(&self, root: &Signer) -> Result<(), EngineError> {
        crate::trust::adopt_key(
            &self.store,
            &root.actor_uid,
            ROOT_KEY_ID,
            &root.public_key_b64(),
        )
        .await
    }

    /// This Cell's OPERATIONAL public key — the transport organ signer, which
    /// the root certifies simply by listing it in the roster.
    pub async fn local_organ_public_key(&self) -> Result<Option<String>, EngineError> {
        Ok(self
            .organ_signer
            .lock()
            .await
            .as_ref()
            .map(|signer| signer.public_key_b64()))
    }

    /// Sign and store this Organ's roster. `version` advances by one, so a
    /// replayed older roster can never re-add a device that was removed.
    ///
    /// Starts life as a roster of ONE — the format has to be final before
    /// anyone saves the key, but the multi-device UX can arrive later without
    /// stranding a contact who paired today.
    pub async fn publish_roster(
        &self,
        root: &Signer,
        cells: Vec<CellEntry>,
    ) -> Result<SignedRoster, EngineError> {
        let organ_uid = root.actor_uid.clone();
        let previous = store::roster::get(&self.store.pool, &organ_uid).await?;
        // The pickup points are carried forward rather than passed in, because
        // every caller of this function is assembling a MEMBER LIST and none of
        // them knows anything about mailboxes. Passing them in would mean the
        // boot path — which republishes whenever a device or a sealing key
        // changed — silently unpublishing every pickup point the owner chose.
        // Changing them goes through `set_pickup_points`.
        let pickup = self
            .roster_of(&organ_uid)
            .await?
            .map(|signed| signed.roster.pickup)
            .unwrap_or_default();
        let roster = Roster {
            organ_uid: organ_uid.clone(),
            root_key: root.public_key_b64(),
            version: previous.map(|row| row.version).unwrap_or(0) + 1,
            not_after: (Utc::now() + Duration::days(ROSTER_VALIDITY_DAYS)).to_rfc3339(),
            cells,
            pickup,
        };
        let payload = roster_signing_payload(&roster)?;
        let signed = SignedRoster {
            signature: root.sign_bytes(&payload),
            roster,
        };
        self.store_roster(&signed).await?;
        self.mirror_roster(&signed).await?;
        Ok(signed)
    }

    /// Re-sign our own roster with a sibling Cell's rotated sealing key, on
    /// that Cell's request.
    ///
    /// `from_node` is the node id the transport proved for the caller, and it
    /// must be the one the roster already records for `cell_uid`: a Cell may
    /// move its OWN mail key and nobody else's. Everything but the key is
    /// copied from the held entry — label, node id, operational key,
    /// capabilities, front-door status — so this verb cannot widen anything.
    ///
    /// Honored automatically rather than queued for the owner to approve, and
    /// that is deliberate. A device asking to rotate already holds the private
    /// half of the key it is replacing, so refusing changes nothing an
    /// attacker could not already do; the remedy for a stolen device is
    /// revocation, which removes it from this list entirely. Requiring a
    /// person to press something would instead leave a device unable to
    /// receive mail for as long as its owner happened to be away.
    pub async fn republish_sealing_key(
        &self,
        from_node: &str,
        cell_uid: &str,
        sealing_key: crate::seal::SealingKey,
    ) -> Result<(), EngineError> {
        let Some(root) = self.root_signer().await? else {
            // The ordinary answer from every Cell that is not the one holding
            // the root, which is most of them.
            return Err(EngineError::Consequence(
                "this Cell does not hold the root key, so it cannot publish a roster".into(),
            ));
        };
        let organ_uid = root.actor_uid.clone();
        let Some(held) = self.roster_of(&organ_uid).await? else {
            return Err(EngineError::Consequence(
                "there is no roster to change".into(),
            ));
        };
        let mut cells = held.roster.cells.clone();
        let Some(entry) = cells.iter_mut().find(|cell| cell.cell_uid == cell_uid) else {
            return Err(EngineError::Consequence(
                "that Cell is not a member of this roster".into(),
            ));
        };
        if entry.node_id != from_node {
            return Err(EngineError::Consequence(
                "a Cell may only publish its own mail key".into(),
            ));
        }
        if entry.sealing_key.as_ref() == Some(&sealing_key) {
            // Already published. The asking side re-asks on every pass until
            // it sees its key in the roster, so this is the common case and
            // not a failure — burning a roster version on it would train
            // contacts to accept a stream of rosters that change nothing.
            return Ok(());
        }
        entry.sealing_key = Some(sealing_key);
        self.publish_roster(&root, cells).await?;
        Ok(())
    }

    async fn store_roster(&self, signed: &SignedRoster) -> Result<(), EngineError> {
        // Whenever a roster for OUR OWN Organ lands — published here or signed
        // on another device and adopted — flatten this Cell's capabilities out
        // of it so the database can enforce them. Both directions matter: a
        // relay Cell never publishes a roster, it only ever adopts one, and it
        // is precisely the Cell the enforcement is for.
        if let (Some(local_organ), Some(local_cell)) = (
            store::organs::local(&self.store.pool).await?,
            store::cells::local(&self.store.pool).await?,
        ) {
            if local_organ.uid == signed.roster.organ_uid {
                let capabilities = signed
                    .roster
                    .cells
                    .iter()
                    .find(|cell| cell.cell_uid == local_cell.uid)
                    // Not named at all means revoked, and an absent capability
                    // set grants nothing — the same rule everywhere else.
                    .map(|cell| cell.capabilities.clone())
                    .unwrap_or_default();
                store::roster::project_local_capabilities(&self.store.pool, &capabilities).await?;
            }
        }
        store::roster::put(
            &self.store.pool,
            &store::roster::StoredRoster {
                organ_uid: signed.roster.organ_uid.clone(),
                root_key: signed.roster.root_key.clone(),
                version: signed.roster.version,
                not_after: signed.roster.not_after.clone(),
                payload: serde_json::to_string(&signed.roster)
                    .map_err(|error| EngineError::Consequence(error.to_string()))?,
                signature: signed.signature.clone(),
            },
        )
        .await?;
        Ok(())
    }

    pub async fn roster_of(&self, organ_uid: &str) -> Result<Option<SignedRoster>, EngineError> {
        let Some(stored) = store::roster::get(&self.store.pool, organ_uid).await? else {
            return Ok(None);
        };
        let roster: Roster = serde_json::from_str(&stored.payload)
            .map_err(|error| EngineError::Consequence(error.to_string()))?;
        Ok(Some(SignedRoster {
            roster,
            signature: stored.signature,
        }))
    }

    /// Whether `key` may speak for `organ_uid`: either it is a key we already
    /// hold, or a succession chain we hold leads to it from one.
    ///
    /// A revoked key never qualifies, however well it chains — that is the
    /// point of the revocation certificate.
    pub async fn key_chains(&self, organ_uid: &str, key: &str) -> Result<bool, EngineError> {
        if store::roster::is_revoked(&self.store.pool, organ_uid, key).await? {
            return Ok(false);
        }
        // ROOT keys only. An operational key is a device's key for signing
        // traffic; it must never be able to validate a roster, or a stolen
        // phone could sign itself a roster adding more devices and the whole
        // two-key split would be decoration. Before per-Cell key ids there was
        // exactly one operational key per Organ and this set quietly contained
        // it — the questions "may this key speak for the identity" and "may
        // this key sign traffic in its name" had never been separated.
        let held: HashSet<String> = crate::trust::keys_of(&self.store, organ_uid)
            .await?
            .into_iter()
            .filter(|(key_id, _)| is_root_key_id(key_id))
            .map(|(_, public_key)| public_key)
            .collect();
        if held.contains(key) {
            return Ok(true);
        }
        // Walk forward from every key we hold along stored successions.
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for (old_key, new_key) in store::roster::successions(&self.store.pool, organ_uid).await? {
            edges.entry(old_key).or_default().push(new_key);
        }
        let mut seen: HashSet<String> = held.clone();
        let mut frontier: Vec<String> = held.into_iter().collect();
        while let Some(current) = frontier.pop() {
            let Some(next) = edges.get(&current) else {
                continue;
            };
            for candidate in next {
                if store::roster::is_revoked(&self.store.pool, organ_uid, candidate).await? {
                    continue;
                }
                if candidate == key {
                    return Ok(true);
                }
                if seen.insert(candidate.clone()) {
                    frontier.push(candidate.clone());
                }
            }
        }
        Ok(false)
    }

    /// Verify and store a roster that arrived from a peer.
    ///
    /// Never trusts on first use: by the time a roster arrives, pairing has
    /// already adopted this Organ's root key through the Introduction. A
    /// roster signed by anything that does not chain to a held key is REFUSED
    /// and the previously-held roster is left exactly as it was.
    pub async fn adopt_roster(&self, signed: &SignedRoster) -> Result<RosterOutcome, EngineError> {
        let organ_uid = &signed.roster.organ_uid;
        let payload = roster_signing_payload(&signed.roster)?;
        if !verify_with(&signed.roster.root_key, &payload, &signed.signature) {
            return Ok(RosterOutcome::Refused);
        }
        if !self.key_chains(organ_uid, &signed.roster.root_key).await? {
            return Ok(RosterOutcome::Refused);
        }
        let fresh = DateTime::parse_from_rfc3339(&signed.roster.not_after)
            .map(|when| when.with_timezone(&Utc) > Utc::now())
            .unwrap_or(false);
        if !fresh {
            return Ok(RosterOutcome::Expired);
        }
        let previous = store::roster::get(&self.store.pool, organ_uid).await?;
        if previous
            .as_ref()
            .is_some_and(|row| row.version >= signed.roster.version)
        {
            return Ok(RosterOutcome::NotNewer);
        }
        self.store_roster(signed).await?;
        Ok(RosterOutcome::Accepted)
    }

    /// Whether the roster we hold for an Organ has passed its expiry. Callers
    /// keep dialing its Cells — last-known-good beats unreachable — but should
    /// say so, because a roster that stopped refreshing is how a removed
    /// device is supposed to fall out on its own.
    pub async fn roster_is_stale(&self, organ_uid: &str) -> Result<bool, EngineError> {
        let Some(stored) = store::roster::get(&self.store.pool, organ_uid).await? else {
            return Ok(false);
        };
        Ok(DateTime::parse_from_rfc3339(&stored.not_after)
            .map(|when| when.with_timezone(&Utc) <= Utc::now())
            .unwrap_or(true))
    }

    /// Whether a Cell may do something in its Organ's name.
    ///
    /// **Effective permission = the Organ's permissions ∩ the Cell's
    /// capabilities**, and this is the intersection's second half — the first
    /// is whatever already decides what that Organ may do. One rule, evaluated
    /// in one place, degrading safely at every unknown: no roster, no entry,
    /// or no capability set all answer `false`. A device we cannot describe is
    /// a device we do not extend authority to.
    pub async fn cell_may(
        &self,
        organ_uid: &str,
        cell_uid: &str,
        capability: &str,
    ) -> Result<bool, EngineError> {
        Ok(self
            .roster_of(organ_uid)
            .await?
            .and_then(|signed| {
                signed
                    .roster
                    .cells
                    .into_iter()
                    .find(|cell| cell.cell_uid == cell_uid)
            })
            .is_some_and(|cell| cell.may(capability)))
    }

    /// Additional dial candidates for a contact, from their roster.
    ///
    /// DELIBERATELY additive: `organ_contact.node_id` stays the authoritative
    /// target. Deriving the dial target from the roster would make every
    /// existing contact unreachable the moment a roster is missing or expired,
    /// and would couple the tested accept gate to this new path. One change at
    /// a time.
    pub async fn roster_node_ids(&self, organ_uid: &str) -> Result<Vec<String>, EngineError> {
        Ok(self
            .roster_of(organ_uid)
            .await?
            .map(|signed| {
                signed
                    .roster
                    .cells
                    .into_iter()
                    .map(|cell| cell.node_id)
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Mirror the roster onto the local Organ Record as `lince.roster`, for
    /// surfaces to read through Protein.
    ///
    /// A read-only projection, never the source of truth: the signed blob in
    /// `organ_roster` is. It carries no secret — the contact tier gets the
    /// full roster anyway — so it is safe on a table that syncs. An enrolment
    /// TOKEN must never go here for exactly the opposite reason.
    async fn mirror_roster(&self, signed: &SignedRoster) -> Result<(), EngineError> {
        let cells: Vec<serde_json::Value> = signed
            .roster
            .cells
            .iter()
            .map(|cell| {
                serde_json::json!({
                    "cell_uid": cell.cell_uid,
                    "node_id": cell.node_id,
                    "label": cell.label,
                    "front_door": cell.front_door,
                    "capabilities": cell.capabilities,
                    // Carried so the device list can show whether this device
                    // can be sent sealed mail at all. It is a PUBLIC key with
                    // an expiry on it — the same thing every contact reads out
                    // of the signed roster — so mirroring it discloses nothing
                    // the contact tier does not already have.
                    "sealing_key": cell.sealing_key,
                })
            })
            .collect();
        // RAW: this mirror is display state each Cell derives from a signed
        // blob it already holds, so it never needed to travel — and as a
        // logged write it locked a relay Cell out of publishing its own
        // roster, since a relay has no write capability.
        store::records::set_extension_raw(
            &self.store.pool,
            &signed.roster.organ_uid,
            "lince.roster",
            &serde_json::json!({
                "version": signed.roster.version,
                "not_after": signed.roster.not_after,
                "root_key": signed.roster.root_key,
                "cells": cells,
                // Mirrored for the same reason and with the same reasoning: a
                // pickup point is published to every contact by definition,
                // since a sender who could not read it could not use it.
                "pickup": signed.roster.pickup,
            }),
        )
        .await?;
        Ok(())
    }

    /// Remember where this Cell keeps its root key, so enrolment and
    /// revocation can load it on demand instead of holding it in memory.
    pub fn set_root_key_path(&self, path: std::path::PathBuf) {
        *self.root_key_path.lock().expect("root key path") = Some(path);
    }

    /// Remember where this Cell keeps its sealing keyring.
    pub fn set_sealing_keyring_path(&self, path: std::path::PathBuf) {
        *self.sealing_keyring_path.lock().expect("sealing keyring") = Some(path);
    }

    /// This Cell's sealing keyring, rotated and pruned as a side effect.
    ///
    /// Rotation happens HERE, on read, rather than on a timer: a Cell that was
    /// switched off across its own rotation point must rotate when it comes
    /// back, and a timer in a process that was not running fires never. Every
    /// caller either publishes the current key or opens mail with the retained
    /// ones, so both are exactly the moments the keyring should be fresh.
    pub async fn sealing_keyring(&self) -> Result<Option<crate::seal::Keyring>, EngineError> {
        let path = self.sealing_keyring_path.lock().expect("sealing keyring").clone();
        let (Some(path), Some(cell)) = (path, store::cells::local(&self.store.pool).await?) else {
            return Ok(None);
        };
        Ok(Some(crate::seal::load_keyring(&path, &cell.uid)?))
    }

    /// The sealing key to publish in this Cell's roster entry, if it has one.
    pub async fn published_sealing_key(
        &self,
    ) -> Result<Option<crate::seal::SealingKey>, EngineError> {
        Ok(self
            .sealing_keyring()
            .await?
            .and_then(|keyring| keyring.current()))
    }

    /// Whether the roster names the mail key this Cell is actually using.
    ///
    /// A read, with no rotation and no dialing: the panel asks it, and the
    /// sync pass is what fixes it. `true` when there is nothing to compare —
    /// a Cell with no keyring and no roster is not broken, it simply has no
    /// mail key, and `seal` already refuses honestly for that case.
    pub async fn own_sealing_key_is_published(&self) -> Result<bool, EngineError> {
        let (Some(organ), Some(cell)) = (
            store::organs::local(&self.store.pool).await?,
            store::cells::local(&self.store.pool).await?,
        ) else {
            return Ok(true);
        };
        let Some(current) = self
            .sealing_keyring()
            .await?
            .and_then(|keyring| keyring.current())
        else {
            return Ok(true);
        };
        Ok(self
            .roster_of(&organ.uid)
            .await?
            .and_then(|held| {
                held.roster
                    .cells
                    .iter()
                    .find(|entry| entry.cell_uid == cell.uid)
                    .and_then(|entry| entry.sealing_key.clone())
            })
            .as_ref()
            == Some(&current))
    }

    /// Load the root key IF it is on this Cell. `None` is the healthy state
    /// once it has been moved offline.
    pub async fn root_signer(&self) -> Result<Option<Signer>, EngineError> {
        let path = self.root_key_path.lock().expect("root key path").clone();
        let (Some(path), Some(organ)) = (path, store::organs::local(&self.store.pool).await?)
        else {
            return Ok(None);
        };
        if !root_key_present(&path) {
            return Ok(None);
        }
        Ok(Some(Signer::load_or_create(
            &path,
            &organ.uid,
            ROOT_KEY_ID,
        )?))
    }

    /// Every revocation certificate this Organ has published about its own
    /// keys — what contacts pull so a dead key stops being accepted.
    pub async fn published_revocations(
        &self,
        organ_uid: &str,
    ) -> Result<Vec<(String, String)>, EngineError> {
        Ok(store::roster::revocations_of(&self.store.pool, organ_uid).await?)
    }

    /// Issue a single-use, short-lived enrolment token. Returns the PLAINTEXT,
    /// which exists only in this return value and on the screen that shows it
    /// — the database holds a hash.
    ///
    /// The caller must hold the root, because redeeming this adds a Cell to
    /// the roster and only the root can sign that.
    pub async fn issue_enrolment_token(&self) -> Result<String, EngineError> {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let expires_at = (Utc::now() + Duration::minutes(ENROLMENT_TOKEN_TTL_MINUTES)).to_rfc3339();
        store::roster::put_enrolment_token(&self.store.pool, &hash_token(&token), &expires_at)
            .await?;
        Ok(token)
    }

    /// Redeem a token and enrol the presenting Cell.
    ///
    /// The token is claimed FIRST: a claim that fails means expired, unknown,
    /// or already used, and none of those may proceed to touch the roster.
    pub async fn redeem_enrolment(
        &self,
        root: &Signer,
        token: &str,
        cell: CellEntry,
    ) -> Result<SignedRoster, EngineError> {
        if !store::roster::redeem_enrolment_token(&self.store.pool, &hash_token(token)).await? {
            return Err(EngineError::Consequence(
                "enrolment token is unknown, expired, or already used".into(),
            ));
        }
        self.enrol_cell(root, cell).await
    }

    /// Add a Cell to this Organ's roster. Requires the root — enrolling a
    /// device is one of only two acts the root exists for, and it SHOULD feel
    /// deliberate.
    pub async fn enrol_cell(
        &self,
        root: &Signer,
        cell: CellEntry,
    ) -> Result<SignedRoster, EngineError> {
        let mut cells = self
            .roster_of(&root.actor_uid)
            .await?
            .map(|signed| signed.roster.cells)
            .unwrap_or_default();
        cells.retain(|existing| existing.cell_uid != cell.cell_uid);
        cells.push(cell);
        self.publish_roster(root, cells).await
    }

    /// Publish where this Organ's mail may be left (Ontology C4).
    ///
    /// Needs the root for the same reason enrolment does, and it is the same
    /// class of act: a pickup point decides who gets to hold your unread mail,
    /// so a compromised ordinary device must not be able to add one. It also
    /// cannot be a per-Cell decision — the roster is one document and every
    /// Cell of the Organ collects from the same boxes.
    ///
    /// Publishing ONE is allowed. Two is the advice, not the rule: refusing the
    /// first would make the second unreachable, and the panel says plainly
    /// what a single box costs.
    pub async fn set_pickup_points(
        &self,
        root: &Signer,
        pickup: Vec<PickupPoint>,
    ) -> Result<SignedRoster, EngineError> {
        let cells = self
            .roster_of(&root.actor_uid)
            .await?
            .map(|signed| signed.roster.cells)
            .unwrap_or_default();
        let organ_uid = root.actor_uid.clone();
        let previous = store::roster::get(&self.store.pool, &organ_uid).await?;
        let roster = Roster {
            organ_uid: organ_uid.clone(),
            root_key: root.public_key_b64(),
            version: previous.map(|row| row.version).unwrap_or(0) + 1,
            not_after: (Utc::now() + Duration::days(ROSTER_VALIDITY_DAYS)).to_rfc3339(),
            cells,
            pickup,
        };
        let payload = roster_signing_payload(&roster)?;
        let signed = SignedRoster {
            signature: root.sign_bytes(&payload),
            roster,
        };
        self.store_roster(&signed).await?;
        self.mirror_roster(&signed).await?;
        Ok(signed)
    }

    /// Where OUR mail may be left, as this Cell last published it.
    pub async fn own_pickup_points(&self) -> Result<Vec<PickupPoint>, EngineError> {
        let Some(local) = store::organs::local(&self.store.pool).await? else {
            return Ok(Vec::new());
        };
        Ok(self
            .roster_of(&local.uid)
            .await?
            .map(|signed| signed.roster.pickup)
            .unwrap_or_default())
    }

    /// This Cell's own node id, as its Organ's roster publishes it.
    ///
    /// Read from the published roster rather than from the endpoint, because
    /// what goes into a code somebody else will dial has to be what this Organ
    /// has told the world — an address only this process knows is one nobody
    /// can resolve.
    pub async fn own_node_id(&self) -> Result<Option<String>, EngineError> {
        let Some(local) = store::organs::local(&self.store.pool).await? else {
            return Ok(None);
        };
        let Some(cell) = store::cells::local(&self.store.pool).await? else {
            return Ok(None);
        };
        Ok(self.roster_of(&local.uid).await?.and_then(|signed| {
            signed
                .roster
                .cells
                .into_iter()
                .find(|entry| entry.cell_uid == cell.uid)
                .map(|entry| entry.node_id)
        }))
    }

    /// Remove a Cell. This IS revocation: the roster is the membership list,
    /// and the version bump is what stops the old roster being replayed to
    /// re-add a stolen device.
    ///
    /// Under root-offline this always needs the drawer, which is the correct
    /// cost and the point of the split. Two things keep it from being painful:
    /// roster entries EXPIRE, so a stolen Cell loses authority on its own even
    /// if the owner never reaches the root; and the pre-signed revocation
    /// certificate is stored with the root, so one trip yields both acts.
    pub async fn revoke_cell(
        &self,
        root: &Signer,
        cell_uid: &str,
    ) -> Result<SignedRoster, EngineError> {
        let mut cells = self
            .roster_of(&root.actor_uid)
            .await?
            .map(|signed| signed.roster.cells)
            .unwrap_or_default();
        cells.retain(|existing| existing.cell_uid != cell_uid);
        self.publish_roster(root, cells).await
    }

    /// Sign "this old root endorses this new root". Rotation without every
    /// contact re-pairing.
    pub async fn sign_succession(
        &self,
        old_root: &Signer,
        new_key_b64: &str,
    ) -> Result<(), EngineError> {
        let organ_uid = old_root.actor_uid.clone();
        let old_key = old_root.public_key_b64();
        let created_at = Utc::now().to_rfc3339();
        let payload = succession_signing_payload(&organ_uid, &old_key, new_key_b64, &created_at);
        let signature = old_root.sign_bytes(&payload);
        store::roster::record_succession(
            &self.store.pool,
            &organ_uid,
            &old_key,
            new_key_b64,
            &signature,
            &created_at,
        )
        .await?;
        Ok(())
    }

    /// Accept a succession from a peer. Only chains: the OLD key must already
    /// be one we can trust, or this is precisely the silent-takeover attempt
    /// the chain rule exists to make loud.
    pub async fn adopt_succession(
        &self,
        organ_uid: &str,
        old_key: &str,
        new_key: &str,
        created_at: &str,
        signature: &str,
    ) -> Result<bool, EngineError> {
        let payload = succession_signing_payload(organ_uid, old_key, new_key, created_at);
        if !verify_with(old_key, &payload, signature) {
            return Ok(false);
        }
        if !self.key_chains(organ_uid, old_key).await? {
            return Ok(false);
        }
        store::roster::record_succession(
            &self.store.pool,
            organ_uid,
            old_key,
            new_key,
            signature,
            created_at,
        )
        .await?;
        Ok(true)
    }

    /// Every succession this Organ has signed about its own keys, for serving
    /// to contacts. Without this the chain rule is a one-way street: rotating
    /// produces a roster every contact correctly REFUSES, and nothing carries
    /// the endorsement that would let them accept it.
    pub async fn published_successions(
        &self,
        organ_uid: &str,
    ) -> Result<Vec<store::roster::SuccessionRow>, EngineError> {
        Ok(store::roster::published_successions(&self.store.pool, organ_uid).await?)
    }

    /// Generate the pre-signed revocation certificate for a root key.
    ///
    /// Made at key CREATION and meant to be stored offline beside the root, so
    /// the drawer trip that revokes a stolen device also yields this. It does
    /// not prove a replacement key is genuine — it kills the old one, which is
    /// damage limitation that works even when identity cannot yet be
    /// re-established.
    pub fn revocation_certificate(&self, root: &Signer) -> (String, String) {
        let key = root.public_key_b64();
        let payload = revocation_signing_payload(&root.actor_uid, &key);
        (key, root.sign_bytes(&payload))
    }

    /// Consume a revocation certificate. Verified against the key it revokes —
    /// a key can always authorise its own death, which is why this needs no
    /// chain check.
    pub async fn adopt_revocation(
        &self,
        organ_uid: &str,
        revoked_key: &str,
        signature: &str,
    ) -> Result<bool, EngineError> {
        let payload = revocation_signing_payload(organ_uid, revoked_key);
        if !verify_with(revoked_key, &payload, signature) {
            return Ok(false);
        }
        store::roster::record_revocation(&self.store.pool, organ_uid, revoked_key, signature)
            .await?;
        Ok(true)
    }
}

pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    B64.encode(hasher.finalize())
}

/// Whether the root key file is on this Cell.
///
/// `false` is not an error state — it is the DESIRED one once the owner has
/// moved the root to offline media. The Cell keeps syncing, serving and
/// talking; what it cannot do is enrol or revoke a device, or sign a
/// succession, which is exactly the point of the split.
pub fn root_key_present(path: &std::path::Path) -> bool {
    path.exists()
}

/// Copy the root key to `destination` (removable media, a token, a drive kept
/// in a drawer) at mode 0600.
///
/// Refuses to overwrite: an existing file at the destination might be someone
/// else's root, and silently replacing it would destroy an identity.
pub fn export_root_key(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), EngineError> {
    let secret = std::fs::read(source)?;
    if secret.len() != 32 {
        return Err(EngineError::Consequence(
            "root key file is not 32 bytes".into(),
        ));
    }
    if destination.exists() {
        return Err(EngineError::Consequence(
            "a file already exists at the destination; refusing to overwrite a key".into(),
        ));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    use std::io::Write as _;
    let mut file = options.open(destination)?;
    file.write_all(&secret)?;
    file.sync_all()?;
    Ok(())
}

/// Delete the local root key, having VERIFIED byte-for-byte that `copy_at`
/// holds the same key.
///
/// The verification is the whole point. "Detach" without it is
/// "irrecoverably destroy your identity because you thought you had a backup",
/// and that mistake is unrecoverable by construction — there is no authority
/// to appeal to.
pub fn detach_root_key(
    path: &std::path::Path,
    copy_at: &std::path::Path,
) -> Result<(), EngineError> {
    let local = std::fs::read(path)?;
    let copy = std::fs::read(copy_at).map_err(|error| {
        EngineError::Consequence(format!("cannot read the exported copy: {error}"))
    })?;
    if local != copy {
        return Err(EngineError::Consequence(
            "the exported copy does not match the local root key; refusing to detach".into(),
        ));
    }
    std::fs::remove_file(path)?;
    Ok(())
}
