//! Canonical encodings for Ed25519 public keys and signatures.

use base64::{Engine as _, engine::general_purpose::STANDARD};

pub(super) fn canonical_base64<const N: usize>(value: &str) -> bool {
    let mut bytes = [0; N];
    let mut encoded = [0; 88];
    STANDARD.decode_slice(value, &mut bytes).ok() == Some(N)
        && STANDARD
            .encode_slice(bytes, &mut encoded)
            .is_ok_and(|len| encoded.get(..len) == Some(value.as_bytes()))
}
