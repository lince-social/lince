//! Contact drafts and reviewed disclosure commitments.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::{
    Identity,
    contacts::{Audience, Document},
};
use crate::karma::{CanonicalHash, canonical_hash};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactDraft {
    pub issuer: Identity,
    pub contact: Identity,
    #[serde(default)]
    pub audience: Audience,
    pub lifetime_seconds: u32,
    pub expected_revision: u64,
    #[serde(default)]
    pub withdrawn: bool,
    #[serde(default)]
    pub key_id: String,
    #[serde(default)]
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileVersion {
    pub identity: Identity,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactReview {
    pub person: String,
    pub document: Document,
    pub expected_revision: u64,
    pub reviewed_at: i64,
    pub profiles: Vec<ProfileVersion>,
}

impl ContactReview {
    /// Commit to the reviewed signing input, reader, revision, and profiles.
    ///
    /// # Errors
    ///
    /// The original disclosure cannot be encoded as signing input.
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        let unsigned = self.document.signing_bytes()?;
        canonical_hash(
            "lince.contact-disclosure.review.v1",
            &(
                &self.person,
                unsigned,
                self.expected_revision,
                self.reviewed_at,
                &self.profiles,
            ),
        )
        .map_err(|error| error.to_string())
    }
}
