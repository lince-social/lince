//! Bounded chunk decoding and assembly into an original evidence artifact.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Artifact, CHUNK_BYTES, MAX_ARTIFACT_BYTES};
use crate::karma::CanonicalHash;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Chunk {
    pub hash: CanonicalHash,
    pub offset: u32,
    pub total: u32,
    pub bytes: String,
}

impl Chunk {
    /// Decode a chunk bound to the expected artifact and offset.
    ///
    /// # Errors
    ///
    /// The chunk binding, length, or canonical base64 is invalid.
    pub fn decode(&self, hash: &CanonicalHash, offset: u32) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        self.append_to(hash, offset, &mut bytes)?;
        Ok(bytes)
    }

    fn append_to(
        &self,
        hash: &CanonicalHash,
        offset: u32,
        bytes: &mut Vec<u8>,
    ) -> Result<(), String> {
        let bound = "Invalid evidence chunk binding or byte limits";
        let total = usize::try_from(self.total).map_err(|_| bound)?;
        let start = usize::try_from(offset).map_err(|_| bound)?;
        if &self.hash != hash
            || self.offset != offset
            || total > MAX_ARTIFACT_BYTES
            || !start.is_multiple_of(CHUNK_BYTES)
            || start >= total
            || self.bytes.len() > CHUNK_BYTES.div_ceil(3) * 4
        {
            return Err(bound.into());
        }
        let before = bytes.len();
        // STANDARD requires canonical padding and rejects nonzero trailing bits.
        STANDARD
            .decode_vec(&self.bytes, bytes)
            .map_err(|_| "Invalid evidence chunk encoding")?;
        if bytes.len() - before != (total - start).min(CHUNK_BYTES) {
            return Err("Incomplete evidence chunk".into());
        }
        Ok(())
    }
}

/// Reassemble a complete artifact and verify its canonical content hash.
///
/// # Errors
///
/// A chunk is invalid, retrieval is incomplete, or the artifact fails validation or hash checks.
pub fn assemble(hash: &CanonicalHash, chunks: &[Chunk], now: i64) -> Result<Artifact, String> {
    let Some(first) = chunks
        .first()
        .filter(|_| chunks.len() <= MAX_ARTIFACT_BYTES.div_ceil(CHUNK_BYTES))
    else {
        return Err("Choose a bounded complete sequence of evidence chunks".into());
    };
    let mut bytes = Vec::new();
    for chunk in chunks {
        if chunk.total != first.total {
            return Err("Evidence changed size during retrieval".into());
        }
        let offset = u32::try_from(bytes.len())
            .map_err(|_| "Invalid evidence chunk binding or byte limits")?;
        chunk.append_to(hash, offset, &mut bytes)?;
    }
    if Some(bytes.len()) != usize::try_from(first.total).ok() {
        return Err("Evidence retrieval is incomplete".into());
    }
    let artifact = Artifact::parse(&bytes, now)?;
    if artifact.hash()? != *hash {
        return Err("Retrieved evidence differs from its requested content hash".into());
    }
    Ok(artifact)
}
