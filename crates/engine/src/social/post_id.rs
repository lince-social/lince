use super::*;
use sha2::{Digest, Sha256};

pub(super) fn post_id(
    key: &str,
    nonce: &str,
    mode: AuthorMode,
    alias: &str,
    organ: Option<&str>,
    destinations: &[String],
) -> Result<String, EngineError> {
    let bytes = B64
        .decode(nonce)
        .map_err(|_| invalid("Invalid public post nonce"))?;
    if bytes.len() != 16 || B64.encode(&bytes) != nonce {
        return Err(invalid("Use a canonical 128-bit public post nonce"));
    }
    let identity = json!({"key":key,"nonce":nonce,"mode":mode,"alias":alias,"organ":organ,"destinations":destinations});
    let hash = Sha256::digest(signing_bytes("post-id", &identity)?);
    let mut time = [0u8; 8];
    time[2..].copy_from_slice(&hash[..6]);
    let entropy = u128::from_be_bytes(hash[..16].try_into().unwrap());
    Ok(format!(
        "post_{}",
        nucleus::id::ulid_from(u64::from_be_bytes(time), entropy)
    ))
}
