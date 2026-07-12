//! Trust (blueprint XI): make every delta verifiable. Ed25519 signatures over
//! the fact hash; the private key lives OUTSIDE the database (caller supplies
//! the bytes — file, keychain, or test fixture); the public key is published
//! in `identity_key` so any Cell can verify authorship.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use nucleus::Fact;
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
}

impl Engine {
    /// Install the Cell's signer: publishes the public key, signs every fact
    /// sealed from now on.
    pub async fn set_signer(&self, signer: Signer) -> Result<(), EngineError> {
        store::sqlx::query(
            "INSERT INTO identity_key (actor_uid, key_id, public_key) VALUES (?, ?, ?)
             ON CONFLICT(actor_uid, key_id) DO UPDATE SET public_key = excluded.public_key",
        )
        .bind(&signer.actor_uid)
        .bind(&signer.key_id)
        .bind(signer.public_key_b64())
        .execute(&self.store.pool)
        .await?;
        *self.signer.lock().await = Some(signer);
        Ok(())
    }
}

/// A peer's published keys, for the introduction export.
pub async fn keys_of(
    store: &Store,
    actor_uid: &str,
) -> Result<Vec<(String, String)>, EngineError> {
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
         ON CONFLICT(actor_uid, key_id) DO UPDATE SET public_key = excluded.public_key",
    )
    .bind(actor_uid)
    .bind(key_id)
    .bind(public_key_b64)
    .execute(&store.pool)
    .await?;
    Ok(())
}

/// Verify a fact's signature against the actor's published keys.
/// `Ok(true)` = verified; `Ok(false)` = no signature or no matching key;
/// `Err` only on storage failure. Tampering shows up as `false`.
pub async fn verify_fact(store: &Store, fact: &Fact) -> Result<bool, EngineError> {
    let (Some(signature), Some(actor)) = (&fact.signature, &fact.actor_uid) else {
        return Ok(false);
    };
    let Ok(sig_bytes) = B64.decode(signature) else {
        return Ok(false);
    };
    let Ok(sig) = Signature::from_slice(&sig_bytes) else {
        return Ok(false);
    };
    let keys: Vec<String> =
        store::sqlx::query_scalar("SELECT public_key FROM identity_key WHERE actor_uid = ?")
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
    Ok(false)
}
