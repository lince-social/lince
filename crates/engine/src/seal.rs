//! Sealing a batch for a carrier that must not read it (Ontology C4, the
//! blind mailbox).
//!
//! # What this is for
//!
//! Every other sync path in Lince hands bytes to the Cell that will apply
//! them, over an authenticated QUIC connection. A mailbox is the one path
//! where a THIRD PARTY holds the bytes, at rest, for as long as the recipient
//! is away. That is the only place where encrypting payloads earns its cost,
//! and the scope stops exactly there: this seals what TRAVELS through a
//! carrier. The local store stays plaintext at rest.
//!
//! # The construction, stated so it can be audited without reading the code
//!
//! One [`SealedBundle`] carries one [`OpBatch`](crate::sync::OpBatch), serialized
//! as JSON, for one recipient Organ.
//!
//! * **Unit.** The BATCH is sealed, never the individual op. Taken from
//!   Keyhive's published reasoning: per-op sealing destroys compression, and
//!   it leaks the shape of everything — op count, field boundaries and timing
//!   are a usable picture of activity even when every value is opaque.
//! * **AEAD.** ChaCha20-Poly1305. A fresh 32-byte content key and a fresh
//!   12-byte nonce per bundle, always; no key or nonce is ever reused across
//!   bundles.
//! * **KEM.** X25519. One ephemeral keypair per bundle, one Diffie-Hellman
//!   per recipient Cell. The ephemeral private key is dropped as soon as the
//!   wraps are built, which is what makes the bundle unopenable by its own
//!   sender afterwards.
//! * **KDF.** HKDF-SHA256 over the DH output.
//!   `salt = ephemeral_pub || recipient_pub`, and
//!   `info = "lince.seal.v1 wrap" || key_id`, so every recipient of the same
//!   bundle derives a different wrapping key and no wrap can be replayed
//!   against a different key of the same Cell.
//! * **AAD.** The content AEAD is bound to `v || from_organ || to_organ`, so a
//!   bundle cannot be re-labelled for a different recipient and still open.
//!   Each wrap is bound to its own `key_id`.
//! * **Sender authentication.** An ed25519 signature by the SENDING CELL's
//!   operational key over the whole transcript (see [`transcript`]).
//!
//! # Why the signature is not optional
//!
//! `crate::sync::inadmissible` says it plainly: there is no signature on a
//! `WireOp`, and what stands in for one is the connection — an op arrives over
//! an authenticated stream from a known Organ, and everything it claims about
//! itself must agree with who is on the other end. That comment also names the
//! condition under which it stops working: *"with relay on, `from_organ` is a
//! carrier and this check has to become a signature."* A mailbox IS relaying.
//! A bundle collected from one has no connection to anchor it, and an
//! ephemeral-X25519 seal on its own proves only that the sender knew a public
//! sealing key — which everyone holding the recipient's roster does. Without
//! the signature `from_organ` would be attacker-chosen, and every check that
//! compares against it (including the Cell-ownership guard) would be
//! comparing against a value the attacker picked.
//!
//! So the anchor moves rather than disappearing: over a connection it is the
//! connection, out of a mailbox it is this signature, and
//! [`OpenedBundle::from_cell`] is what the import path may believe.
//!
//! # Sealing keys are PER CELL
//!
//! Published in the signed roster beside `operational_key`
//! (see [`crate::roster::CellEntry`]). Per-Organ was the obvious reading of
//! "a per-Organ sealing key", and it forces a mechanism nothing else in the
//! design needs: one private key shared by every Cell of an Organ has to reach
//! each new device at enrolment and be re-distributed on every rotation.
//! Per-Cell deletes that problem outright — no sealing private key ever
//! leaves the Cell that generated it — and costs one extra wrap per Cell,
//! about eighty bytes. The content key is wrapped to EVERY current Cell of the
//! recipient, so mail is collectable from whichever device comes online first.

use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

/// The construction version. Bumped whenever any of the primitives, the KDF
/// inputs or the transcript change. There is no negotiation and no fallback
/// branch: an unknown version refuses (`AGENTS.md`, fail closed).
pub const SEAL_VERSION: u8 = 1;

const WRAP_INFO: &[u8] = b"lince.seal.v1 wrap";
const CONTENT_AAD: &[u8] = b"lince.seal.v1 content";
const TRANSCRIPT_TAG: &[u8] = b"lince.seal.v1 transcript";

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Everything that can go wrong opening or building a bundle.
///
/// Deliberately coarse on the failure side: `Undecipherable` covers a wrong
/// key, a truncated bundle and a tampered one alike, because distinguishing
/// them tells an attacker which part of a forgery was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealError {
    /// A version this build does not implement.
    UnknownVersion(u8),
    /// No recipients were supplied, or a recipient's published key is not a
    /// 32-byte X25519 point.
    BadRecipients(String),
    /// The bundle is not addressed to any key this Cell holds.
    NotForUs,
    /// The signature is absent, malformed, or does not verify against the key
    /// the caller supplied for the sending Cell.
    Unauthenticated,
    /// Decryption failed. No further detail, on purpose.
    Undecipherable,
    /// The plaintext opened but is not a batch.
    Malformed(String),
}

impl std::fmt::Display for SealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SealError::UnknownVersion(v) => write!(f, "unknown seal version {v}"),
            SealError::BadRecipients(why) => write!(f, "bad recipients: {why}"),
            SealError::NotForUs => write!(f, "bundle is not addressed to a key we hold"),
            SealError::Unauthenticated => write!(f, "bundle signature did not verify"),
            SealError::Undecipherable => write!(f, "bundle could not be opened"),
            SealError::Malformed(why) => write!(f, "opened bundle was not a batch: {why}"),
        }
    }
}

impl std::error::Error for SealError {}

/// A published sealing key: the identifier a wrap names, and the point.
///
/// `key_id` is `x25519:cell:<cell_uid>:<generation>`, mirroring the shape
/// `roster::cell_key_id` already uses for operational keys. The generation is
/// what makes rotation expressible — two keys of the same Cell are two
/// entries, not one entry that changed underneath a sender.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealingKey {
    pub key_id: String,
    /// Base64 X25519 public key, 32 bytes.
    pub public: String,
    /// When the private half is deleted.
    ///
    /// Carried per key rather than inherited from the roster's own
    /// `not_after`, because the failure it prevents is silent: a sender
    /// holding a stale roster would otherwise seal to a key whose private
    /// half is already gone, and produce a bundle nobody can ever open. With
    /// an expiry on the key the sender refuses to seal and stays in its retry
    /// window instead.
    pub not_after: String,
}

/// One recipient Cell's copy of the content key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedTo {
    pub key_id: String,
    pub nonce: String,
    pub wrapped: String,
}

/// A batch sealed for one recipient Organ.
///
/// The metadata that is NOT sealed is exactly what a carrier needs to hold and
/// hand back the bundle: who it is for, who left it, and which Cell signed it.
/// That is the metadata cost the design states out loud — a mailbox learns who
/// writes to you, when, and how much, while reading nothing — and it is why
/// the recipient, never the sender, chooses the mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedBundle {
    pub v: u8,
    pub to_organ: String,
    pub from_organ: String,
    pub from_cell: String,
    /// Base64 X25519 ephemeral public key for this bundle.
    pub ephemeral: String,
    pub nonce: String,
    pub ciphertext: String,
    pub recipients: Vec<SealedTo>,
    pub signature: String,
}

/// What actually gets encrypted: a batch, and WHICH CHANNEL it belongs to.
///
/// A batch alone was not enough. Ops flow on two channels — the general Organ
/// feed and one per individually-replicated root — and the receiving side
/// refuses a conversation op that arrives on the general feed, by design
/// (`import_ops`, "the general feed may not touch an individually-replicated
/// Record"). Over a connection the channel is the request verb; out of a
/// mailbox there is no verb, so the channel has to travel.
///
/// It travels INSIDE the ciphertext, not beside it, because which conversation
/// someone is writing to is exactly the metadata a carrier is not entitled to.
/// And it costs nothing to carry it in the payload: the root is authorized on
/// arrival against our OWN accepted-grant table, so naming a root buys a
/// sender nothing they were not already granted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MailedBatch {
    /// The individually-replicated root this batch belongs to, or `None` for
    /// the general feed.
    #[serde(default)]
    pub root: Option<String>,
    pub batch: crate::sync::OpBatch,
}

/// A bundle that opened AND authenticated.
///
/// The two facts travel together because neither is usable alone: bytes that
/// decrypted but were signed by nobody are attacker-chosen, and a signature
/// over bytes that did not decrypt says nothing about what they were.
#[derive(Debug, Clone)]
pub struct OpenedBundle {
    /// The Cell whose operational key signed this bundle. THIS is what the
    /// import path may believe about the sender — not `SealedBundle::from_cell`,
    /// which is an unverified label until the signature check has run.
    pub from_cell: String,
    pub from_organ: String,
    pub batch: crate::sync::OpBatch,
    /// The channel the sender sealed this on. `None` is the general feed.
    pub root: Option<String>,
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).expect("system entropy unavailable");
    out
}

fn decode_point(value: &str) -> Result<[u8; 32], SealError> {
    let raw = b64()
        .decode(value)
        .map_err(|_| SealError::BadRecipients(format!("{value} is not base64")))?;
    <[u8; 32]>::try_from(raw.as_slice())
        .map_err(|_| SealError::BadRecipients(format!("{value} is not 32 bytes")))
}

/// The bytes both sides sign and verify.
///
/// Length-prefixed field by field rather than concatenated, so no two
/// different bundles can produce the same transcript by moving a boundary —
/// the classic way a signature over "everything joined together" turns out to
/// cover something other than what was read.
///
/// Public because a forgery test has to be able to re-sign: several of the
/// attacks worth checking are ones the SENDER can mount, and against those a
/// signature check proves nothing, so the test must get past it to reach the
/// property actually being tested.
pub fn transcript(bundle: &SealedBundle) -> Vec<u8> {
    let mut out = Vec::new();
    let mut push = |part: &[u8]| {
        out.extend_from_slice(&(part.len() as u64).to_be_bytes());
        out.extend_from_slice(part);
    };
    push(TRANSCRIPT_TAG);
    push(&[bundle.v]);
    push(bundle.to_organ.as_bytes());
    push(bundle.from_organ.as_bytes());
    push(bundle.from_cell.as_bytes());
    push(bundle.ephemeral.as_bytes());
    push(bundle.nonce.as_bytes());
    push(bundle.ciphertext.as_bytes());
    for recipient in &bundle.recipients {
        push(recipient.key_id.as_bytes());
        push(recipient.nonce.as_bytes());
        push(recipient.wrapped.as_bytes());
    }
    out
}

fn content_aad(v: u8, from_organ: &str, to_organ: &str) -> Vec<u8> {
    let mut aad = CONTENT_AAD.to_vec();
    aad.push(v);
    aad.extend_from_slice(from_organ.as_bytes());
    aad.push(0);
    aad.extend_from_slice(to_organ.as_bytes());
    aad
}

/// Derive the wrapping key for one recipient from the DH output.
fn wrap_key(shared: &[u8; 32], ephemeral: &[u8; 32], recipient: &[u8; 32], key_id: &str) -> Key {
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(ephemeral);
    salt.extend_from_slice(recipient);
    let mut info = WRAP_INFO.to_vec();
    info.extend_from_slice(key_id.as_bytes());
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut out = [0u8; 32];
    hk.expand(&info, &mut out)
        .expect("32 bytes is a valid HKDF-SHA256 length");
    *Key::from_slice(&out)
}

/// Seal a batch for `to_organ`, readable by every Cell in `recipients`.
///
/// `signing` is the sending Cell's OPERATIONAL key — the same key its roster
/// entry publishes, so the recipient can verify with what it already holds and
/// no new key type has to be distributed to make sealing authentic.
pub fn seal(
    mail: &MailedBatch,
    from_cell: &str,
    to_organ: &str,
    recipients: &[SealingKey],
    signing: &SigningKey,
) -> Result<SealedBundle, SealError> {
    if recipients.is_empty() {
        return Err(SealError::BadRecipients(
            "an Organ with no published sealing key cannot be mailed".into(),
        ));
    }
    let batch = &mail.batch;
    let plaintext = serde_json::to_vec(mail)
        .map_err(|why| SealError::Malformed(format!("batch would not serialize: {why}")))?;

    let content_key = random_bytes::<32>();
    let content_nonce = random_bytes::<12>();
    let aead = ChaCha20Poly1305::new(Key::from_slice(&content_key));
    let aad = content_aad(SEAL_VERSION, &batch.from_organ, to_organ);
    let ciphertext = aead
        .encrypt(
            Nonce::from_slice(&content_nonce),
            Payload {
                msg: &plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| SealError::Undecipherable)?;

    let ephemeral_secret = StaticSecret::from(random_bytes::<32>());
    let ephemeral_public = PublicKey::from(&ephemeral_secret);

    let mut wrapped_for = Vec::with_capacity(recipients.len());
    for recipient in recipients {
        let point = decode_point(&recipient.public)?;
        let shared = ephemeral_secret.diffie_hellman(&PublicKey::from(point));
        let key = wrap_key(
            shared.as_bytes(),
            ephemeral_public.as_bytes(),
            &point,
            &recipient.key_id,
        );
        let nonce = random_bytes::<12>();
        let wrapped = ChaCha20Poly1305::new(&key)
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &content_key,
                    aad: recipient.key_id.as_bytes(),
                },
            )
            .map_err(|_| SealError::Undecipherable)?;
        wrapped_for.push(SealedTo {
            key_id: recipient.key_id.clone(),
            nonce: b64().encode(nonce),
            wrapped: b64().encode(wrapped),
        });
    }
    // The ephemeral private key dies here. Nothing retains it, which is what
    // makes the bundle unopenable by its own sender the moment this returns.
    drop(ephemeral_secret);

    let mut bundle = SealedBundle {
        v: SEAL_VERSION,
        to_organ: to_organ.to_string(),
        from_organ: batch.from_organ.clone(),
        from_cell: from_cell.to_string(),
        ephemeral: b64().encode(ephemeral_public.as_bytes()),
        nonce: b64().encode(content_nonce),
        ciphertext: b64().encode(&ciphertext),
        recipients: wrapped_for,
        signature: String::new(),
    };
    bundle.signature = b64().encode(signing.sign(&transcript(&bundle)).to_bytes());
    Ok(bundle)
}

/// Open a bundle with one of this Cell's sealing private keys, verifying the
/// sender first.
///
/// `sender_key` is the ed25519 operational key of `bundle.from_cell`, looked
/// up in the SIGNED ROSTER of `bundle.from_organ` by the caller. The signature
/// is checked BEFORE any decryption is attempted, so a bundle from nobody
/// never reaches the AEAD at all.
///
/// `ours` is `(key_id, x25519 private)` for every sealing key this Cell still
/// retains, current and within-window old ones alike — retaining them for the
/// window plus grace is what stops a rotation from stranding mail that was
/// already sealed and not yet collected.
/// A published operational key, as `open` wants it. `None` for anything that
/// is not 32 base64 bytes — an unusable key is not an authenticated sender.
pub fn verifying_key(published: &str) -> Option<VerifyingKey> {
    let raw = b64().decode(published).ok()?;
    VerifyingKey::from_bytes(&<[u8; 32]>::try_from(raw.as_slice()).ok()?).ok()
}

pub fn open(
    bundle: &SealedBundle,
    sender_key: &VerifyingKey,
    ours: &[(String, [u8; 32])],
) -> Result<OpenedBundle, SealError> {
    if bundle.v != SEAL_VERSION {
        return Err(SealError::UnknownVersion(bundle.v));
    }
    let signature = b64()
        .decode(&bundle.signature)
        .ok()
        .and_then(|raw| <[u8; 64]>::try_from(raw.as_slice()).ok())
        .map(|bytes| Signature::from_bytes(&bytes))
        .ok_or(SealError::Unauthenticated)?;
    sender_key
        .verify(&transcript(bundle), &signature)
        .map_err(|_| SealError::Unauthenticated)?;

    let ephemeral = decode_point(&bundle.ephemeral)?;
    let (wrap, secret) = bundle
        .recipients
        .iter()
        .find_map(|wrap| {
            ours.iter()
                .find(|(key_id, _)| *key_id == wrap.key_id)
                .map(|(_, secret)| (wrap, secret))
        })
        .ok_or(SealError::NotForUs)?;

    let secret = StaticSecret::from(*secret);
    let shared = secret.diffie_hellman(&PublicKey::from(ephemeral));
    let key = wrap_key(
        shared.as_bytes(),
        &ephemeral,
        PublicKey::from(&secret).as_bytes(),
        &wrap.key_id,
    );
    let nonce = b64()
        .decode(&wrap.nonce)
        .map_err(|_| SealError::Undecipherable)?;
    let wrapped = b64()
        .decode(&wrap.wrapped)
        .map_err(|_| SealError::Undecipherable)?;
    let content_key = ChaCha20Poly1305::new(&key)
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &wrapped,
                aad: wrap.key_id.as_bytes(),
            },
        )
        .map_err(|_| SealError::Undecipherable)?;
    let content_key =
        <[u8; 32]>::try_from(content_key.as_slice()).map_err(|_| SealError::Undecipherable)?;

    let content_nonce = b64()
        .decode(&bundle.nonce)
        .map_err(|_| SealError::Undecipherable)?;
    let ciphertext = b64()
        .decode(&bundle.ciphertext)
        .map_err(|_| SealError::Undecipherable)?;
    let aad = content_aad(bundle.v, &bundle.from_organ, &bundle.to_organ);
    let plaintext = ChaCha20Poly1305::new(Key::from_slice(&content_key))
        .decrypt(
            Nonce::from_slice(&content_nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| SealError::Undecipherable)?;

    let mail: MailedBatch = serde_json::from_slice(&plaintext)
        .map_err(|why| SealError::Malformed(format!("{why}")))?;
    let MailedBatch { root, batch } = mail;
    // The sealed batch must agree with the label the carrier routed on.
    // Disagreement is not a decryption failure — it is a sender that signed
    // one thing and addressed another — but it refuses just the same.
    if batch.from_organ != bundle.from_organ {
        return Err(SealError::Malformed(
            "sealed batch names a different Organ than the bundle".into(),
        ));
    }
    Ok(OpenedBundle {
        from_cell: bundle.from_cell.clone(),
        from_organ: bundle.from_organ.clone(),
        batch,
        root,
    })
}

/// Generate a sealing keypair, returning `(private, published)`.
///
/// The private half never leaves the Cell that called this; the published half
/// goes into that Cell's roster entry, which the root signs.
pub fn generate(cell_uid: &str, generation: u32, not_after: &str) -> ([u8; 32], SealingKey) {
    let secret = StaticSecret::from(random_bytes::<32>());
    let public = PublicKey::from(&secret);
    (
        secret.to_bytes(),
        SealingKey {
            key_id: format!("x25519:cell:{cell_uid}:{generation}"),
            public: b64().encode(public.as_bytes()),
            not_after: not_after.to_string(),
        },
    )
}

/// How long a mailbox holds an uncollected bundle, and — the same number —
/// how long past its expiry a sealing private key must survive.
///
/// They are ONE number by design, not by coincidence. A bundle sealed at the
/// last moment a key was still publishable can be collected up to this long
/// afterwards, so a private key deleted any earlier would strand mail that
/// was accepted in good faith. Change one and the other has to move with it.
pub const RETENTION_DAYS: i64 = 30;

/// How long a key stays publishable before the next generation takes over.
///
/// Half its lifetime, so the published key always has at least
/// `ROTATE_AFTER_DAYS` of validity left. A sender that seals against a cached
/// roster is therefore never racing an expiry it cannot see, and the sender's
/// own refusal on an expired key stays a real error rather than an everyday
/// occurrence nobody looks at.
pub const ROTATE_AFTER_DAYS: i64 = 30;

/// Slack past the point where nothing openable can remain, for clocks that
/// disagree and for a Cell that was switched off across its own rotation.
pub const GRACE_DAYS: i64 = 3;

/// One retained sealing key: what was published, and the private half.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyringEntry {
    pub key_id: String,
    /// Base64 X25519 private key. This file is the reason the keyring is
    /// written 0600 and never leaves the Cell.
    pub secret: String,
    pub public: String,
    /// After this, senders must stop sealing to it.
    pub not_after: String,
    /// After this, the private half is deleted and anything still sealed to
    /// this key is unopenable by everyone, including us. That is the forward
    /// secrecy a separate rotated key was chosen for.
    pub delete_after: String,
}

/// This Cell's sealing keys: the current one and the retired-but-retained.
///
/// Rotation is a property of a KEYRING, not of a key, which is why this type
/// exists rather than a single file holding 32 bytes like the identity keys
/// next to it. It is also why a rotation is visible in the roster: the
/// published entry changes, so `roster::needs_publishing` re-signs on its own
/// without rotation needing to know anything about publishing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keyring {
    pub entries: Vec<KeyringEntry>,
    /// The highest generation ever issued, kept separately from the entries.
    ///
    /// It has to outlive them. Derived from the entries instead, the counter
    /// restarts at 1 once the last old key is pruned, and a fresh key is then
    /// published under an identifier some sender may still hold in a cached
    /// roster — pointing at a DIFFERENT point. Nothing opens either way, but
    /// the failure changes from "no such key of mine" to "could not decrypt",
    /// which is the confusing kind. Monotonic here, forever.
    #[serde(default)]
    pub generation: u32,
}

impl Keyring {
    /// The key to publish: the newest one still within its sealing window.
    pub fn current(&self) -> Option<SealingKey> {
        let moment = now();
        self.entries
            .iter()
            .filter(|entry| entry.not_after > moment)
            .max_by(|a, b| a.not_after.cmp(&b.not_after))
            .map(|entry| SealingKey {
                key_id: entry.key_id.clone(),
                public: entry.public.clone(),
                not_after: entry.not_after.clone(),
            })
    }

    /// Every private key still retained, for [`open`].
    ///
    /// Includes retired ones on purpose: a bundle is opened with the key it
    /// was sealed to, which is whatever the sender's roster said at the time.
    pub fn open_keys(&self) -> Vec<(String, [u8; 32])> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let raw = b64().decode(&entry.secret).ok()?;
                let secret = <[u8; 32]>::try_from(raw.as_slice()).ok()?;
                Some((entry.key_id.clone(), secret))
            })
            .collect()
    }

    /// Delete every key nothing can still be sealed to and nothing uncollected
    /// can still need. Returns how many were dropped.
    pub fn prune(&mut self) -> usize {
        let before = self.entries.len();
        let moment = now();
        self.entries.retain(|entry| entry.delete_after > moment);
        before - self.entries.len()
    }

    /// Rotate if there is no publishable key, or the current one is past its
    /// rotation point. Returns whether anything changed.
    ///
    /// Idempotent and cheap to call on every boot, which is how it is meant to
    /// run: there is no timer, because a Cell that was off for a year must
    /// rotate when it comes back, not a year later.
    pub fn ensure_current(&mut self, cell_uid: &str) -> bool {
        let dropped = self.prune();
        if let Some(current) = self.current() {
            // Still inside its window and not yet due for replacement.
            if current.not_after > days_from_now(ROTATE_AFTER_DAYS) {
                return dropped > 0;
            }
        }
        // Recovered from the entries as well, so a keyring written before the
        // counter existed still moves forward rather than starting over.
        let seen = self
            .entries
            .iter()
            .filter_map(|entry| entry.key_id.rsplit(':').next()?.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        let generation = self.generation.max(seen) + 1;
        self.generation = generation;
        let not_after = days_from_now(ROTATE_AFTER_DAYS * 2);
        let (secret, published) = generate(cell_uid, generation, &not_after);
        self.entries.push(KeyringEntry {
            key_id: published.key_id,
            secret: b64().encode(secret),
            public: published.public,
            not_after,
            delete_after: days_from_now(ROTATE_AFTER_DAYS * 2 + RETENTION_DAYS + GRACE_DAYS),
        });
        true
    }
}

/// Compared as strings throughout: RFC3339 in UTC sorts lexicographically,
/// which is the same reason the rest of the codebase stores stamps this way.
fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn days_from_now(days: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::days(days))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Load this Cell's keyring, rotating and pruning, and write it back if
/// anything changed.
///
/// Written 0600 like every other key file. There is no recovery path if it is
/// lost: uncollected mail becomes unopenable, which is the correct failure —
/// the alternative is a copy of it somewhere, and a sealing key with a backup
/// has no forward secrecy.
pub fn load_keyring(path: &std::path::Path, cell_uid: &str) -> std::io::Result<Keyring> {
    let mut keyring: Keyring = match std::fs::read(path) {
        Ok(raw) => serde_json::from_slice(&raw).unwrap_or_default(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Keyring::default(),
        Err(error) => return Err(error),
    };
    if keyring.ensure_current(cell_uid) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_vec_pretty(&keyring)?;
        let temporary = path.with_extension("tmp");
        {
            use std::io::Write as _;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(&body)?;
            file.sync_all()?;
        }
        // Replaced atomically: a keyring truncated by a crash mid-write would
        // lose retained keys, and losing those silently strands mail.
        std::fs::rename(&temporary, path)?;
    }
    Ok(keyring)
}
