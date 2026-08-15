//! Peer identity (Ontology §11 "Peers"): an Organ IS its keypair.
//!
//! **The signed-HTTP transport that used to live here was DELETED 2026-08-03**,
//! once the iroh path had run end to end. What went: `request_signing_payload`
//! / `response_signing_payload`, the 120s freshness window, `timestamp_fresh`,
//! `verify_peer_signature`, `sign_peer_request` / `sign_peer_response`,
//! `verify_peer_response`, and the `peer_auth` layer in the web crate.
//!
//! None of it is needed any more: an iroh connection is mutually authenticated
//! at the QUIC/TLS handshake against raw public keys, so
//! `Connection::remote_id()` gives what a signature plus a replay window used
//! to establish, and there is no unauthenticated moment on the wire to defend.
//! See `engine::wire`.
//!
//! What did NOT go with it, because it answers a different question: op-batch
//! and Fact signing in `trust.rs`. Transport auth says "who is on this socket";
//! payload signing says "who wrote this", which must still hold a year later,
//! from a backup, with no connection in sight.
//!
//! What remains here is the human-facing pairing helper.

use sha2::{Digest, Sha256};

use crate::Engine;
use crate::error::EngineError;

/// The pairing verification code (the Signal safety-number pattern): base32 of
/// the first 25 bits of `sha256(sorted both organs' public keys)` — 5 chars
/// like `Y3HS4`. DERIVED on each side from the introduction's keys, never
/// transmitted: a code that travels in an announce proves nothing. Hashing
/// BOTH keys sorted makes exactly one code per pairing, so the humans speak
/// the same string; a man-in-the-middle gave each side a different key and is
/// caught by the mismatch.
///
/// **Retired from every normal flow** (2026-08-03). Under iroh the address IS
/// the key, so dialing a NodeId reaches that keypair or nothing — the residual
/// risk is only being handed the WRONG NodeId, and a QR scanned in person or a
/// key pasted into an already-trusted chat defeats that completely. Both
/// channels are unrelayable, both are what this product actually does, and a
/// security step users are taught to click past is worse than no step.
///
/// Kept for the one case those two do not cover: pairing with someone remote
/// with no other trusted channel, as an optional "verify this contact" panel.
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
                // The OPERATIONAL key, which is now per-Cell
                // (`ed25519:cell:<uid>:v1`). An Organ with several devices has
                // several, and the code is between the two Cells actually
                // talking — which is what a verification code compared out
                // loud has always meant.
                .find(|(key_id, _)| key_id.contains(":cell:"))
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
        assert!(
            ab.bytes()
                .all(|c| c.is_ascii_uppercase() || (b'2'..=b'7').contains(&c))
        );
        assert_eq!(ab, verification_code(a, b), "deterministic");
    }

    #[test]
    fn code_differs_for_different_keys() {
        let a = "keyA";
        let b = "keyB";
        let c = "keyC";
        assert_ne!(verification_code(a, b), verification_code(a, c));
    }
}
