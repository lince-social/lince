// non-lean-yet
// Scope: fn post_id.
// BeautifulRust (baseline lines):
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 14.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   21, 23.
// - [Clippy probe: clippy::indexing_slicing] Verify bounds or use checked access on fallible
//   inputs. Sites 21, 23.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 14.

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
    let mut entropy = [0; 16];
    entropy.copy_from_slice(&hash[..16]);
    Ok(format!(
        "post_{}",
        nucleus::id::ulid_from(u64::from_be_bytes(time), u128::from_be_bytes(entropy))
    ))
}
