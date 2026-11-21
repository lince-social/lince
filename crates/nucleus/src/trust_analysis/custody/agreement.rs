//! Signed custody offers and acknowledgments for exact original evidence.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    super::{
        Filter,
        contacts::Audience,
        serving::{Artifact, MAX_ARTIFACT_BYTES, SourceKind},
    },
    MAX_RETENTION_SECONDS,
};
use crate::{
    karma::{CanonicalHash, canonical_hash, canonical_json_bytes},
    valid_uid,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub nonce: String,
    pub sender: String,
    pub custodian: String,
    pub artifact: CanonicalHash,
    pub source_kind: SourceKind,
    pub bytes: u32,
    pub retained_until: i64,
    pub deliver_by: i64,
    pub readers: Filter,
    pub redistribute: bool,
    pub key_id: String,
    pub public_key: String,
    pub signature: String,
}

impl Offer {
    pub fn validate(&self, now: i64) -> Result<(), String> {
        self.readers.validate()?;
        if !valid_uid(&self.nonce, "cust")
            || !valid_uid(&self.sender, "r")
            || !valid_uid(&self.custodian, "r")
            || self.sender == self.custodian
            || self.bytes == 0
            || usize::try_from(self.bytes).map_or(true, |bytes| bytes > MAX_ARTIFACT_BYTES)
            || self.retained_until <= now
            || self.retained_until > now.saturating_add(i64::from(MAX_RETENTION_SECONDS))
            || self.deliver_by <= now
            || self.deliver_by > now.saturating_add(300)
            || self.retained_until < self.deliver_by
            || self.readers.include.is_empty()
            || !self.redistribute
            || self.key_id.is_empty()
            || self.key_id.len() > 256
            || self.public_key.len() > 256
            || self.signature.len() > 256
        {
            return Err("Review an unexpired bounded public-evidence custody offer with redistribution consent".into());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut unsigned = self.clone();
        unsigned.signature.clear();
        canonical_json_bytes(&("lince.evidence-custody.offer.v1", unsigned))
            .map_err(|error| error.to_string())
    }
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        canonical_hash("lince.evidence-custody.signed-offer.v1", self)
            .map_err(|error| error.to_string())
    }
    /// Check that the artifact is the exact Public original named by this offer.
    ///
    /// # Errors
    ///
    /// The original is invalid or differs from the offer's hash, kind, bytes, or term.
    pub fn accepts(&self, artifact: &Artifact, now: i64) -> Result<(), String> {
        let bytes = artifact.validated_bytes(now)?;
        let public = match artifact {
            Artifact::Receipt(document) => document
                .public_bundle()
                .is_some_and(|bundle| bundle.publication.scope.audience == Audience::Public),
            Artifact::Contact(document) => document.statement.audience == Audience::Public,
            Artifact::Claim(document) => document.statement.audience == Audience::Public,
        };
        if !public
            || artifact.hash()? != self.artifact
            || artifact.kind() != self.source_kind
            || u32::try_from(bytes.len()).ok() != Some(self.bytes)
            || self.retained_until > artifact.expires_at()?
        {
            return Err(
                "Custody retains only the exact approved Public original inside its expiry".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgment {
    pub offer: CanonicalHash,
    pub artifact: CanonicalHash,
    pub sender: String,
    pub custodian: String,
    pub bytes: u32,
    pub retained_until: i64,
    pub stored_at: i64,
    pub key_id: String,
    pub public_key: String,
    pub signature: String,
}

impl Acknowledgment {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut unsigned = self.clone();
        unsigned.signature.clear();
        canonical_json_bytes(&("lince.evidence-custody.acknowledgment.v1", unsigned))
            .map_err(|error| error.to_string())
    }
    pub fn matches(&self, offer: &Offer) -> bool {
        self.artifact == offer.artifact
            && self.sender == offer.sender
            && self.custodian == offer.custodian
            && self.bytes == offer.bytes
            && self.retained_until == offer.retained_until
            && self.stored_at < self.retained_until
            && offer.hash().is_ok_and(|hash| self.offer == hash)
    }
}
