use sha2::{Digest, Sha256};

use crate::Engine;
use crate::error::EngineError;

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
    let mut bits: u32 = 0;
    for byte in digest.iter().take(4) {
        bits = (bits << 8) | u32::from(*byte);
    }
    let bits = bits >> 7;
    let mut code = String::with_capacity(5);
    for slot in (0..5).rev() {
        let index = ((bits >> (slot * 5)) & 0x1F) as usize;
        code.push(ALPHABET[index] as char);
    }
    code
}

impl Engine {
    pub async fn pairing_code(&self, contact_organ: &str) -> Result<Option<String>, EngineError> {
        let Some(local) = store::organs::local(&self.store.pool).await? else {
            return Ok(None);
        };
        let ours = crate::trust::keys_of(&self.store, &local.uid).await?;
        let theirs = crate::trust::keys_of(&self.store, contact_organ).await?;
        let pick = |keys: &[(String, String)]| {
            keys.iter()
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
