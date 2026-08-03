//! Trust (blueprint XI): make every delta verifiable. Ed25519 signatures over
//! the fact hash; the private key lives OUTSIDE the database (caller supplies
//! the bytes — file, keychain, or test fixture); the public key is published
//! in `identity_key` so any Cell can verify authorship.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use nucleus::Fact;
use std::io::Write as _;
use std::path::Path;
use store::Store;

use crate::Engine;
use crate::error::EngineError;

#[derive(Clone)]
pub struct Signer {
    key: SigningKey,
    pub actor_uid: String,
    pub key_id: String,
}

impl Signer {
    /// Key material comes from the caller — never from the db (blueprint XI.1).
    pub fn from_bytes(actor_uid: &str, key_id: &str, secret: [u8; 32]) -> Signer {
        Signer {
            key: SigningKey::from_bytes(&secret),
            actor_uid: actor_uid.to_string(),
            key_id: key_id.to_string(),
        }
    }

    /// Fresh key from non-deterministic entropy (uuid-backed; no rand dep).
    pub fn generate(actor_uid: &str, key_id: &str) -> Signer {
        let mut secret = [0u8; 32];
        secret[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        secret[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        Self::from_bytes(actor_uid, key_id, secret)
    }

    pub fn public_key_b64(&self) -> String {
        B64.encode(self.key.verifying_key().as_bytes())
    }

    pub fn sign_hash(&self, hash: &str) -> String {
        B64.encode(self.key.sign(hash.as_bytes()).to_bytes())
    }

    pub fn sign_bytes(&self, bytes: &[u8]) -> String {
        B64.encode(self.key.sign(bytes).to_bytes())
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.key.to_bytes()
    }

    /// Load a private key from the Cell data directory, creating it once when
    /// absent. Key bytes stay outside SQLite and are never returned by a read
    /// API or cross-Cell envelope.
    pub fn load_or_create(path: &Path, actor_uid: &str, key_id: &str) -> Result<Self, EngineError> {
        Ok(Self::from_bytes(
            actor_uid,
            key_id,
            load_or_create_secret(path)?,
        ))
    }
}

/// The 32 raw secret bytes at `path`, generated at mode 0600 on first call.
///
/// Factored out of `Signer::load_or_create` so the iroh NODE key can reuse the
/// exact same on-disk discipline without being a `Signer` — a node key
/// authenticates a live connection and must never be installed as something
/// that signs Facts or op batches (Ontology §11: node key ≠ identity key).
pub fn load_or_create_secret(path: &Path) -> Result<[u8; 32], EngineError> {
    match std::fs::read(path) {
        Ok(bytes) => bytes.try_into().map_err(|_| {
            EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Cell key file must contain exactly 32 bytes",
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let secret = Signer::generate("", "").secret_bytes();
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            match options.open(path) {
                Ok(mut file) => {
                    file.write_all(&secret)?;
                    file.sync_all()?;
                    Ok(secret)
                }
                // Another process won the race and wrote first — read theirs,
                // so two Cells never disagree about which key this file holds.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    load_or_create_secret(path)
                }
                Err(error) => Err(EngineError::Io(error)),
            }
        }
        Err(error) => Err(EngineError::Io(error)),
    }
}

impl Engine {
    /// Install the Cell's signer: publishes the public key, signs every fact
    /// sealed from now on.
    pub async fn set_signer(&self, signer: Signer) -> Result<(), EngineError> {
        store::sqlx::query(
            "INSERT INTO identity_key (actor_uid, key_id, public_key) VALUES (?, ?, ?)
             ON CONFLICT(actor_uid, key_id) DO NOTHING",
        )
        .bind(&signer.actor_uid)
        .bind(&signer.key_id)
        .bind(signer.public_key_b64())
        .execute(&self.store.pool)
        .await?;
        require_published_key(
            &self.store,
            &signer.actor_uid,
            &signer.key_id,
            &signer.public_key_b64(),
        )
        .await?;
        *self.signer.lock().await = Some(signer);
        Ok(())
    }

    /// The actor whose private key is currently available to this Engine.
    /// Published identity keys are verification material and do not imply
    /// that this process can author a Fact for their actor.
    pub async fn signer_actor_uid(&self) -> Option<String> {
        self.signer
            .lock()
            .await
            .as_ref()
            .map(|signer| signer.actor_uid.clone())
    }

    /// Install the Cell Organ key used only for cross-Cell transport envelopes.
    /// It is deliberately separate from the Person signer used for Facts.
    pub async fn set_organ_signer(&self, signer: Signer) -> Result<(), EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
        if signer.actor_uid != organ.uid {
            return Err(EngineError::Conflict {
                code: "organ_signer_identity_mismatch",
                message: "the transport signer must belong to the local Organ".into(),
            });
        }
        store::sqlx::query(
            "INSERT INTO identity_key (actor_uid, key_id, public_key) VALUES (?, ?, ?)
             ON CONFLICT(actor_uid, key_id) DO NOTHING",
        )
        .bind(&signer.actor_uid)
        .bind(&signer.key_id)
        .bind(signer.public_key_b64())
        .execute(&self.store.pool)
        .await?;
        require_published_key(
            &self.store,
            &signer.actor_uid,
            &signer.key_id,
            &signer.public_key_b64(),
        )
        .await?;
        *self.organ_signer.lock().await = Some(signer);
        Ok(())
    }
}

/// A peer's published keys, for the introduction export.
pub async fn keys_of(store: &Store, actor_uid: &str) -> Result<Vec<(String, String)>, EngineError> {
    Ok(store::sqlx::query_as::<_, (String, String)>(
        "SELECT key_id, public_key FROM identity_key WHERE actor_uid = ?",
    )
    .bind(actor_uid)
    .fetch_all(&store.pool)
    .await?)
}

/// Store a foreign actor's public key (introduction, blueprint XI.1) so their
/// signed facts verify on import.
pub async fn adopt_key(
    store: &Store,
    actor_uid: &str,
    key_id: &str,
    public_key_b64: &str,
) -> Result<(), EngineError> {
    store::sqlx::query(
        "INSERT INTO identity_key (actor_uid, key_id, public_key) VALUES (?, ?, ?)
         ON CONFLICT(actor_uid, key_id) DO NOTHING",
    )
    .bind(actor_uid)
    .bind(key_id)
    .bind(public_key_b64)
    .execute(&store.pool)
    .await?;
    require_published_key(store, actor_uid, key_id, public_key_b64).await
}

async fn require_published_key(
    store: &Store,
    actor_uid: &str,
    key_id: &str,
    expected_public_key: &str,
) -> Result<(), EngineError> {
    let published: String = store::sqlx::query_scalar(
        "SELECT public_key FROM identity_key WHERE actor_uid = ? AND key_id = ?",
    )
    .bind(actor_uid)
    .bind(key_id)
    .fetch_one(&store.pool)
    .await?;
    if published != expected_public_key {
        return Err(EngineError::Conflict {
            code: "identity_key_id_conflict",
            message: "published identity key ids are immutable; use a new key id for rotation"
                .into(),
        });
    }
    Ok(())
}

/// Verify either a direct Fact-hash signature or a distinct, committed signed
/// Action intent linked to this Fact. The two evidence types are never
/// relabeled: `fact.signature` remains exclusively a Fact-hash signature.
pub async fn verify_fact(store: &Store, fact: &Fact) -> Result<bool, EngineError> {
    if let (Some(signature), Some(actor)) = (&fact.signature, &fact.actor_uid) {
        if let Ok(sig_bytes) = B64.decode(signature) {
            if let Ok(sig) = Signature::from_slice(&sig_bytes) {
                let keys: Vec<String> = store::sqlx::query_scalar(
                    "SELECT public_key FROM identity_key WHERE actor_uid = ?",
                )
                .bind(actor)
                .fetch_all(&store.pool)
                .await?;
                for key_b64 in keys {
                    let Ok(bytes) = B64.decode(&key_b64) else {
                        continue;
                    };
                    let Ok(bytes32) = <[u8; 32]>::try_from(bytes.as_slice()) else {
                        continue;
                    };
                    let Ok(key) = VerifyingKey::from_bytes(&bytes32) else {
                        continue;
                    };
                    if key.verify(fact.hash.as_bytes(), &sig).is_ok() {
                        return Ok(true);
                    }
                }
            }
        }
    }

    verify_fact_action_intent(store, fact).await
}

async fn verify_fact_action_intent(store: &Store, fact: &Fact) -> Result<bool, EngineError> {
    let Some(actor) = fact.actor_uid.as_deref() else {
        return Ok(false);
    };
    let Some(intent) = store::action_intents::for_fact(&store.pool, &fact.uid).await? else {
        return Ok(false);
    };
    if intent.actor_person_uid != actor {
        return Ok(false);
    }
    let public_key: Option<String> = store::sqlx::query_scalar(
        "SELECT public_key FROM identity_key WHERE actor_uid = ? AND key_id = ?",
    )
    .bind(actor)
    .bind(&intent.key_id)
    .fetch_optional(&store.pool)
    .await?;
    let Some(public_key) = public_key else {
        return Ok(false);
    };
    let Ok(public_key) = B64.decode(public_key) else {
        return Ok(false);
    };
    let Ok(public_key) = <[u8; 32]>::try_from(public_key.as_slice()) else {
        return Ok(false);
    };
    let Ok(public_key) = VerifyingKey::from_bytes(&public_key) else {
        return Ok(false);
    };
    let Ok(signature) = B64.decode(&intent.signature) else {
        return Ok(false);
    };
    let Ok(signature) = Signature::from_slice(&signature) else {
        return Ok(false);
    };
    let bytes = nucleus::action_intent::signing_bytes(
        &intent.session_id,
        &intent.session_challenge,
        intent.sequence,
        &intent.message_id,
        &intent.action_base64,
    );
    Ok(public_key.verify(&bytes, &signature).is_ok())
}
