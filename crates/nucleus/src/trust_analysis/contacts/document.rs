//! Signed contact disclosure documents and signing input.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{super::signature::canonical_base64, MAX_DOCUMENT_BYTES, Statement};
use crate::karma::{CanonicalHash, canonical_hash, canonical_json_bytes};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub statement: Statement,
    pub key_id: String,
    pub public_key: String,
    pub signature: String,
}

impl Document {
    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.contact-disclosure.document.v1", self)
            .map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.signing_bytes()?;
        if self.signature.len() != 88 || !canonical_base64::<64>(&self.signature) {
            return Err("Use a canonical Ed25519 signature".into());
        }
        Ok(())
    }

    /// Encode the statement and the signer as canonical signing input.
    ///
    /// # Errors
    ///
    /// The statement, signer identifier, public key, or encoding is invalid.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.statement.validate()?;
        if self.key_id.is_empty()
            || self.key_id.len() > 160
            || !self.key_id.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err("Choose a bounded signing-key identifier".into());
        }
        if self.public_key.len() != 44 || !canonical_base64::<32>(&self.public_key) {
            return Err("Use a canonical Ed25519 public key".into());
        }
        canonical_json_bytes(&(
            "lince.contact-disclosure.signature.v1",
            &self.statement,
            &self.key_id,
            &self.public_key,
        ))
        .map_err(|error| error.to_string())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err("Contact disclosure exceeds 32 KiB".into());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        document.validate()?;
        Ok(document)
    }
}
