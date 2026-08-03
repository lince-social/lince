//! Peer identity (Ontology §11 "Peers"): an Organ IS its keypair; addresses
//! are hints. Nothing flows on any connection until the far side proves
//! possession of the private key — every sync request and response is signed
//! by an ORGAN key and verified against the keys stored at introduction.
//!
//! Wire shape (stateless, replay-bounded — no server-side nonce store):
//! requests carry `x-lince-organ` / `x-lince-ts` / `x-lince-sig`, where the
//! signature covers `"{method}\n{path_and_query}\n{ts}\n{sha256_hex(body)}"`;
//! responses sign `"response\n{ts}\n{sha256_hex(body)}"`. A timestamp older
//! than the freshness window is rejected, so a captured request cannot be
//! replayed later.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::Engine;
use crate::error::EngineError;

/// How far a request/response timestamp may drift from local now.
pub const PEER_FRESHNESS_SECS: i64 = 120;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Domain separation prefix (Ontology §11, decided 2026-08-02). Every payload
/// this Organ key signs starts with it, so a signature produced for one purpose
/// can never be replayed as another. Colliding with TLS 1.3 CertificateVerify
/// was already impossible — that blob is 64 spaces plus a fixed context string
/// — so this replaces safe-by-luck with safe-by-design. Bump the version
/// suffix if the payload SHAPE ever changes; old and new must not verify alike.
pub const PEER_SIGNING_DOMAIN: &str = "lince/peer/1\n";

/// The exact bytes a peer signs for a request.
pub fn request_signing_payload(method: &str, path_and_query: &str, ts: &str, body: &[u8]) -> Vec<u8> {
    format!(
        "{PEER_SIGNING_DOMAIN}{method}\n{path_and_query}\n{ts}\n{}",
        sha256_hex(body)
    )
    .into_bytes()
}

/// The exact bytes a peer signs for a response body.
pub fn response_signing_payload(ts: &str, body: &[u8]) -> Vec<u8> {
    format!("{PEER_SIGNING_DOMAIN}response\n{ts}\n{}", sha256_hex(body)).into_bytes()
}

/// `|now - ts| <= window` — bounds replay of a captured exchange.
pub fn timestamp_fresh(ts: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return false;
    };
    (now - parsed.with_timezone(&chrono::Utc))
        .num_seconds()
        .abs()
        <= PEER_FRESHNESS_SECS
}

/// Verify `sig_b64` over `payload` against ANY key published for `organ_uid`
/// (introduction stores them in `identity_key`). Unknown organ = false.
pub async fn verify_peer_signature(
    store: &store::Store,
    organ_uid: &str,
    payload: &[u8],
    sig_b64: &str,
) -> Result<bool, EngineError> {
    let Ok(sig_bytes) = B64.decode(sig_b64) else {
        return Ok(false);
    };
    let Ok(sig) = Signature::from_slice(&sig_bytes) else {
        return Ok(false);
    };
    let keys: Vec<String> =
        store::sqlx::query_scalar("SELECT public_key FROM identity_key WHERE actor_uid = ?")
            .bind(organ_uid)
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
        if key.verify(payload, &sig).is_ok() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A public key's fingerprint as it travels in discovery announces:
/// base64(sha256(raw key bytes)). No secrets — the key is already public.
pub fn key_fingerprint(public_key_b64: &str) -> String {
    let raw = B64
        .decode(public_key_b64)
        .unwrap_or_else(|_| public_key_b64.as_bytes().to_vec());
    let mut hasher = Sha256::new();
    hasher.update(&raw);
    B64.encode(hasher.finalize())
}

/// The pairing verification code (the Signal safety-number pattern): base32 of
/// the first 25 bits of `sha256(sorted both organs' public keys)` — 5 chars
/// like `Y3HS4`. DERIVED on each side from the introduction's keys, never
/// transmitted: a code that travels in an announce proves nothing. Hashing
/// BOTH keys sorted makes exactly one code per pairing, so the humans speak
/// the same string; a man-in-the-middle gave each side a different key and is
/// caught by the mismatch.
pub fn verification_code(pub_a_b64: &str, pub_b_b64: &str) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let (first, second) = if pub_a_b64 <= pub_b_b64 {
        (pub_a_b64, pub_b_b64)
    } else {
        (pub_b_b64, pub_a_b64)
    };
    let mut hasher = Sha256::new();
    hasher.update(first.as_bytes());
    hasher.update(second.as_bytes());
    let digest = hasher.finalize();
    // 25 bits = five 5-bit base32 symbols.
    let mut bits: u32 = 0;
    for byte in digest.iter().take(4) {
        bits = (bits << 8) | u32::from(*byte);
    }
    let bits = bits >> 7; // keep the top 25 of 32
    let mut code = String::with_capacity(5);
    for slot in (0..5).rev() {
        let index = ((bits >> (slot * 5)) & 0x1F) as usize;
        code.push(ALPHABET[index] as char);
    }
    code
}

impl Engine {
    /// Sign a peer REQUEST with the local Organ key. `None` when no organ
    /// signer is installed (a Cell that cannot prove itself sends nothing).
    pub async fn sign_peer_request(
        &self,
        method: &str,
        path_and_query: &str,
        body: &[u8],
    ) -> Option<(String, String, String)> {
        let signer = self.organ_signer.lock().await.clone()?;
        let ts = chrono::Utc::now().to_rfc3339();
        let payload = request_signing_payload(method, path_and_query, &ts, body);
        let sig = signer.sign_bytes(&payload);
        Some((signer.actor_uid.clone(), ts, sig))
    }

    /// Sign a peer RESPONSE body with the local Organ key.
    pub async fn sign_peer_response(&self, body: &[u8]) -> Option<(String, String, String)> {
        let signer = self.organ_signer.lock().await.clone()?;
        let ts = chrono::Utc::now().to_rfc3339();
        let payload = response_signing_payload(&ts, body);
        let sig = signer.sign_bytes(&payload);
        Some((signer.actor_uid.clone(), ts, sig))
    }

    /// Verify a peer RESPONSE against the contact's stored keys: fresh
    /// timestamp, signature over the body, and the claimed organ must be the
    /// contact we called. A failure means a stranger answered at a known
    /// address — nothing gets imported.
    pub async fn verify_peer_response(
        &self,
        expected_organ: &str,
        claimed_organ: &str,
        ts: &str,
        sig_b64: &str,
        body: &[u8],
    ) -> Result<bool, EngineError> {
        if claimed_organ != expected_organ || !timestamp_fresh(ts, chrono::Utc::now()) {
            return Ok(false);
        }
        let payload = response_signing_payload(ts, body);
        verify_peer_signature(&self.store, expected_organ, &payload, sig_b64).await
    }

    /// The local organ's discovery fingerprint: sha256 of its transport public
    /// key, preferring the organ key over person keys.
    pub async fn local_fingerprint(&self) -> Result<Option<String>, EngineError> {
        let Some(organ) = store::organs::local(&self.store.pool).await? else {
            return Ok(None);
        };
        let keys = crate::trust::keys_of(&self.store, &organ.uid).await?;
        let organ_key = keys
            .iter()
            .find(|(key_id, _)| key_id.contains(":organ:"))
            .or_else(|| keys.first());
        Ok(organ_key.map(|(_, public_key)| key_fingerprint(public_key)))
    }

    /// The verification code for a pairing with `contact_organ`, derived from
    /// our organ key and theirs (as adopted at introduction).
    pub async fn pairing_code(&self, contact_organ: &str) -> Result<Option<String>, EngineError> {
        let Some(local) = store::organs::local(&self.store.pool).await? else {
            return Ok(None);
        };
        let ours = crate::trust::keys_of(&self.store, &local.uid).await?;
        let theirs = crate::trust::keys_of(&self.store, contact_organ).await?;
        let pick = |keys: &[(String, String)]| {
            keys.iter()
                .find(|(key_id, _)| key_id.contains(":organ:"))
                .or_else(|| keys.first())
                .map(|(_, public_key)| public_key.clone())
        };
        let (Some(our_key), Some(their_key)) = (pick(&ours), pick(&theirs)) else {
            return Ok(None);
        };
        Ok(Some(verification_code(&our_key, &their_key)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_symmetric_five_chars_deterministic() {
        let a = "AAAAC3NzaC1lZDI1NTE5AAAAIExampleKeyOne=";
        let b = "AAAAC3NzaC1lZDI1NTE5AAAAIExampleKeyTwo=";
        let ab = verification_code(a, b);
        let ba = verification_code(b, a);
        assert_eq!(ab, ba, "one code per pairing, either side derives it");
        assert_eq!(ab.len(), 5);
        assert!(ab.bytes().all(|c| c.is_ascii_uppercase() || (b'2'..=b'7').contains(&c)));
        assert_eq!(ab, verification_code(a, b), "deterministic");
    }

    #[test]
    fn code_differs_for_different_keys() {
        let a = "keyA";
        let b = "keyB";
        let c = "keyC";
        assert_ne!(verification_code(a, b), verification_code(a, c));
    }

    #[test]
    fn request_signature_round_trips_and_rejects_tamper() {
        let signer = crate::trust::Signer::generate("o-test", "ed25519:organ:v1");
        let ts = chrono::Utc::now().to_rfc3339();
        let payload = request_signing_payload("POST", "/organ/inbox", &ts, b"{\"x\":1}");
        let sig = signer.sign_bytes(&payload);

        let key_b64 = signer.public_key_b64();
        let key = VerifyingKey::from_bytes(
            &<[u8; 32]>::try_from(B64.decode(&key_b64).unwrap().as_slice()).unwrap(),
        )
        .unwrap();
        let sig_bytes = Signature::from_slice(&B64.decode(&sig).unwrap()).unwrap();
        assert!(key.verify(&payload, &sig_bytes).is_ok());

        let tampered = request_signing_payload("POST", "/organ/inbox", &ts, b"{\"x\":2}");
        assert!(key.verify(&tampered, &sig_bytes).is_err());
    }

    #[test]
    fn stale_timestamps_are_rejected() {
        let now = chrono::Utc::now();
        assert!(timestamp_fresh(&now.to_rfc3339(), now));
        let stale = now - chrono::TimeDelta::seconds(PEER_FRESHNESS_SECS + 1);
        assert!(!timestamp_fresh(&stale.to_rfc3339(), now));
        assert!(!timestamp_fresh("not-a-date", now));
    }
}
