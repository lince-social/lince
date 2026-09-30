use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key};
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

pub const SEAL_VERSION: u8 = 2;

const WRAP_INFO: &[u8] = b"lince.seal.v1 wrap";
const CONTENT_AAD: &[u8] = b"lince.seal.v1 content";
const TRANSCRIPT_TAG: &[u8] = b"lince.seal.v2 transcript";

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealError {
    UnknownVersion(u8),
    BadRecipients(String),
    NotForUs,
    Unauthenticated,
    Undecipherable,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealingKey {
    pub key_id: String,
    pub public: String,
    pub not_after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedTo {
    pub key_id: String,
    pub nonce: String,
    pub wrapped: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedBundle {
    pub v: u8,
    pub uid: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub sender_key: String,
    pub to_organ: String,
    pub from_organ: String,
    pub from_cell: String,
    pub ephemeral: String,
    pub nonce: String,
    pub ciphertext: String,
    pub recipients: Vec<SealedTo>,
    pub signature: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MailedBatch {
    #[serde(default)]
    pub root: Option<String>,
    pub batch: crate::sync::OpBatch,
}

#[derive(Debug, Clone)]
pub struct OpenedBundle {
    pub from_cell: String,
    pub from_organ: String,
    pub batch: crate::sync::OpBatch,
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

pub fn transcript(bundle: &SealedBundle) -> Vec<u8> {
    let mut out = Vec::new();
    let mut push = |part: &[u8]| {
        out.extend_from_slice(&(part.len() as u64).to_be_bytes());
        out.extend_from_slice(part);
    };
    push(TRANSCRIPT_TAG);
    push(&[bundle.v]);
    push(bundle.uid.as_bytes());
    push(&bundle.created_at.to_be_bytes());
    push(&bundle.expires_at.to_be_bytes());
    push(bundle.sender_key.as_bytes());
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
    out.into()
}

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
    let aead = ChaCha20Poly1305::new((&content_key).into());
    let aad = content_aad(SEAL_VERSION, &batch.from_organ, to_organ);
    let ciphertext = aead
        .encrypt(
            (&content_nonce).into(),
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
        if !shared.was_contributory() {
            return Err(SealError::BadRecipients(
                "recipient key produces an unsafe shared secret".into(),
            ));
        }
        let key = wrap_key(
            shared.as_bytes(),
            ephemeral_public.as_bytes(),
            &point,
            &recipient.key_id,
        );
        let nonce = random_bytes::<12>();
        let wrapped = ChaCha20Poly1305::new(&key)
            .encrypt(
                (&nonce).into(),
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
    drop(ephemeral_secret);

    let created_at = nucleus::execution::now().timestamp();
    let mut bundle = SealedBundle {
        v: SEAL_VERSION,
        uid: nucleus::new_uid("r"),
        created_at,
        expires_at: created_at + RETENTION_DAYS * 86_400,
        sender_key: b64().encode(signing.verifying_key().as_bytes()),
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

pub fn verifying_key(published: &str) -> Option<VerifyingKey> {
    let raw = b64().decode(published).ok()?;
    VerifyingKey::from_bytes(&<[u8; 32]>::try_from(raw.as_slice()).ok()?).ok()
}

pub fn validate_envelope(bundle: &SealedBundle, now: i64) -> Result<(), SealError> {
    if bundle.v != SEAL_VERSION {
        return Err(SealError::UnknownVersion(bundle.v));
    }
    if !nucleus::valid_uid(&bundle.uid, "r")
        || bundle.created_at > now.saturating_add(300)
        || bundle.expires_at <= now
        || bundle.expires_at <= bundle.created_at
        || bundle.expires_at.saturating_sub(bundle.created_at) > RETENTION_DAYS * 86_400
        || bundle.recipients.is_empty()
        || bundle.recipients.len() > 256
    {
        return Err(SealError::Malformed(
            "invalid identity, lifetime or recipient bounds".into(),
        ));
    }
    let bounded_identity = |value: &str| {
        !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
    };
    let encoded_length =
        |value: &str, bytes: usize| b64().decode(value).is_ok_and(|raw| raw.len() == bytes);
    let mut keys = std::collections::HashSet::new();
    if !bounded_identity(&bundle.to_organ)
        || !bounded_identity(&bundle.from_organ)
        || !bounded_identity(&bundle.from_cell)
        || !encoded_length(&bundle.ephemeral, 32)
        || !encoded_length(&bundle.nonce, 12)
        || bundle.ciphertext.len() > crate::mailbox::MAX_BUNDLE_BYTES
        || !b64()
            .decode(&bundle.ciphertext)
            .is_ok_and(|raw| raw.len() >= 16)
        || bundle.recipients.iter().any(|recipient| {
            recipient.key_id.is_empty()
                || recipient.key_id.len() > 256
                || !keys.insert(&recipient.key_id)
                || !encoded_length(&recipient.nonce, 12)
                || !encoded_length(&recipient.wrapped, 48)
        })
    {
        return Err(SealError::Malformed(
            "invalid routing or encryption fields".into(),
        ));
    }
    let key = verifying_key(&bundle.sender_key).ok_or(SealError::Unauthenticated)?;
    let signature = b64()
        .decode(&bundle.signature)
        .map_err(|_| SealError::Unauthenticated)?;
    let signature = Signature::from_slice(&signature).map_err(|_| SealError::Unauthenticated)?;
    key.verify_strict(&transcript(bundle), &signature)
        .map_err(|_| SealError::Unauthenticated)
}

pub fn validate_recoverable(bundle: &SealedBundle, now: i64) -> Result<(), SealError> {
    if now > bundle.expires_at.saturating_add(GRACE_DAYS * 86_400) {
        return Err(SealError::Malformed("the recovery window expired".into()));
    }
    validate_envelope(bundle, now.min(bundle.expires_at.saturating_sub(1)))
}

pub fn delivery_id(bundle: &SealedBundle) -> String {
    let mut hasher = Sha256::new();
    for part in [
        "lince.mailbox.id.v2",
        &bundle.to_organ,
        &bundle.sender_key,
        &bundle.uid,
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("mb-{digest}")
}

pub fn envelope_hash(body: &str) -> String {
    nucleus::fact::sha256_hex(body.as_bytes())
}

pub fn open(
    bundle: &SealedBundle,
    sender_key: &VerifyingKey,
    ours: &[(String, [u8; 32])],
) -> Result<OpenedBundle, SealError> {
    if bundle.v != SEAL_VERSION {
        return Err(SealError::UnknownVersion(bundle.v));
    }
    validate_recoverable(bundle, nucleus::execution::now().timestamp())?;
    if bundle.sender_key != b64().encode(sender_key.as_bytes()) {
        return Err(SealError::Unauthenticated);
    }
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
    if !shared.was_contributory() {
        return Err(SealError::Undecipherable);
    }
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
            nonce
                .as_slice()
                .try_into()
                .map_err(|_| SealError::Undecipherable)?,
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
    let plaintext = ChaCha20Poly1305::new((&content_key).into())
        .decrypt(
            content_nonce
                .as_slice()
                .try_into()
                .map_err(|_| SealError::Undecipherable)?,
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| SealError::Undecipherable)?;

    let mail: MailedBatch =
        serde_json::from_slice(&plaintext).map_err(|why| SealError::Malformed(format!("{why}")))?;
    let MailedBatch { root, batch } = mail;
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

pub const RETENTION_DAYS: i64 = 30;

pub const ROTATE_AFTER_DAYS: i64 = 30;

pub const GRACE_DAYS: i64 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyringEntry {
    pub key_id: String,
    pub secret: String,
    pub public: String,
    pub not_after: String,
    pub delete_after: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keyring {
    pub entries: Vec<KeyringEntry>,
    #[serde(default)]
    pub generation: u32,
}

impl Keyring {
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

    pub fn prune(&mut self) -> usize {
        let before = self.entries.len();
        let moment = now();
        self.entries.retain(|entry| entry.delete_after > moment);
        before - self.entries.len()
    }

    pub fn ensure_current(&mut self, cell_uid: &str) -> bool {
        let dropped = self.prune();
        if let Some(current) = self.current() {
            if current.not_after > days_from_now(ROTATE_AFTER_DAYS) {
                return dropped > 0;
            }
        }
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

fn now() -> String {
    nucleus::execution::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn days_from_now(days: i64) -> String {
    (nucleus::execution::now() + chrono::Duration::days(days))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

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
        std::fs::rename(&temporary, path)?;
    }
    Ok(keyring)
}
