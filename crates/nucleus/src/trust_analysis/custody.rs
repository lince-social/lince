// non-lean-yet
// Scope: mod forwarding, const NAMESPACE, const MAX_STORAGE_BYTES, const MAX_ITEMS.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 115, 163.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 97, 135.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 181.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 46, 90, 114, 120, 123, 162.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 9, 11,
//   12, 13, 14, 15, 16, 20, 21, 23, 24, 25, 26, 27, 28, 46, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82,
//   83, 84, 85, 86, 90, 114, 120, 123, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 162,
//   168, 181, 182, 183, 185, 186, 188, 189, 190, 192, 193, 194, 195, 197, 198, 200.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites accepts: String
//   (123), hash: String (120), signing_bytes: String (114, 162), validate: String (46, 90).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    Filter,
    contacts::Audience,
    serving::{Artifact, SourceKind},
};
use crate::karma::{CanonicalHash, canonical_hash, canonical_json_bytes};
pub mod forwarding;

pub const NAMESPACE: &str = "lince.evidence-custody";
pub const MAX_STORAGE_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ITEMS: u32 = 4096;
pub const CONTROL_SECONDS: i64 = 30 * 86400;
pub const UPLOAD_SECONDS: u64 = 15;
pub const MAX_UPLOADS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub enabled: bool,
    #[serde(default)]
    pub forwarding: forwarding::Policy,
    pub senders: Filter,
    pub readers: Filter,
    pub storage_bytes: u64,
    pub items: u32,
    pub retention_seconds: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            forwarding: Default::default(),
            senders: Filter::default(),
            readers: Filter::default(),
            storage_bytes: 8 * 1024 * 1024,
            items: 256,
            retention_seconds: 365 * 86400,
        }
    }
}

impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        self.senders.validate()?;
        self.readers.validate()?;
        self.forwarding.validate()?;
        if self.storage_bytes == 0
            || self.storage_bytes > MAX_STORAGE_BYTES
            || self.items == 0
            || self.items > MAX_ITEMS
            || self.retention_seconds == 0
            || self.retention_seconds > 365 * 86400
        {
            return Err(
                "Choose bounded custody storage, item count and retention up to one year".into(),
            );
        }
        if self.enabled && (self.senders.include.is_empty() || self.readers.include.is_empty()) {
            return Err(
                "Choose explicit sender and reader include conditions before enabling custody"
                    .into(),
            );
        }
        Ok(())
    }
}

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
        if !crate::valid_uid(&self.nonce, "cust")
            || !crate::valid_uid(&self.sender, "r")
            || !crate::valid_uid(&self.custodian, "r")
            || self.sender == self.custodian
            || self.bytes == 0
            || self.bytes as usize > super::serving::MAX_ARTIFACT_BYTES
            || self.retained_until <= now
            || self.retained_until > now.saturating_add(365 * 86400)
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
            .map_err(|e| e.to_string())
    }
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        canonical_hash("lince.evidence-custody.signed-offer.v1", self).map_err(|e| e.to_string())
    }
    pub fn accepts(&self, artifact: &Artifact, now: i64) -> Result<(), String> {
        artifact.validate(now)?;
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
            || artifact.bytes()?.len() != self.bytes as usize
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
            .map_err(|e| e.to_string())
    }
    pub fn matches(&self, offer: &Offer) -> bool {
        offer.hash().is_ok_and(|hash| self.offer == hash)
            && self.artifact == offer.artifact
            && self.sender == offer.sender
            && self.custodian == offer.custodian
            && self.bytes == offer.bytes
            && self.retained_until == offer.retained_until
            && self.stored_at < self.retained_until
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    InspectOriginal {
        record: String,
    },
    Outbox {
        before: Option<i64>,
    },
    CancelDelivery {
        record: String,
        expected_offer: CanonicalHash,
    },
    InspectPolicy,
    SetPolicy {
        policy: Policy,
        expected_version: u64,
    },
    Inventory {
        before: Option<i64>,
    },
    Cleanup,
}

#[cfg(test)]
mod tests;
