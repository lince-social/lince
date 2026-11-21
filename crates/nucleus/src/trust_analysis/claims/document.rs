//! Signed profile-claim envelope and canonical signing input.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{super::signature::canonical_base64, Authorship, Statement};
use crate::{
    json::encoded_len,
    karma::{CanonicalHash, canonical_hash, canonical_json_bytes},
};

pub const MAX_DOCUMENT_BYTES: usize = 32_768;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub statement: Statement,
    pub key_id: String,
    pub public_key: String,
    pub signature: String,
}

pub(super) fn validate_key(key_id: &str, public_key: &str) -> Result<(), String> {
    if key_id.is_empty()
        || key_id.len() > 128
        || key_id.chars().any(char::is_control)
        || public_key.len() != 44
        || !canonical_base64::<32>(public_key)
    {
        return Err("Choose a canonical independently authorized signing key".into());
    }
    Ok(())
}

pub(super) fn validate_signature(signature: &str) -> Result<(), String> {
    if signature.len() != 88 || !canonical_base64::<64>(signature) {
        return Err("Use a canonical profile-claim signature".into());
    }
    Ok(())
}

impl Document {
    /// Encode the statement and its signer as canonical signing input.
    ///
    /// # Errors
    ///
    /// The statement, signing key, or reviewed delegation is invalid.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.statement.validate()?;
        validate_key(&self.key_id, &self.public_key)?;
        if let Authorship::Generated { permit, .. } = &self.statement.authorship
            && (self.key_id != permit.terms.delegate.key_id
                || self.public_key != permit.terms.delegate.public_key)
        {
            return Err("Sign a generated claim only with its reviewed publishing key".into());
        }
        canonical_json_bytes(&(
            "lince.profile-claim.signature.v1",
            &self.statement,
            &self.key_id,
            &self.public_key,
        ))
        .map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.signing_bytes()?;
        validate_signature(&self.signature)?;
        if encoded_len(self).map_err(|error| error.to_string())? > MAX_DOCUMENT_BYTES {
            return Err("Profile claim exceeds 32 KiB".into());
        }
        Ok(())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err("Profile claim exceeds 32 KiB".into());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        document.validate()?;
        Ok(document)
    }

    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.profile-claim.document.v1", self).map_err(|error| error.to_string())
    }
}
