//! Bind a remote draft, public questions, and pinned recipients to a review.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    super::{commands::ProfileVersion, discovery::Question},
    Draft, MAX_PEERS,
};
use crate::{
    karma::{CanonicalHash, canonical_hash},
    valid_uid,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub organ: String,
    pub endpoint: String,
    pub label: String,
    pub profile_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub nonce: String,
    pub person: String,
    pub draft: Draft,
    pub questions: Vec<Question>,
    pub endpoints: Vec<Endpoint>,
    pub profiles: Vec<ProfileVersion>,
    pub reviewed_at: i64,
}

impl Review {
    /// Identify the reviewed public questions, private reader policy, and pinned recipients.
    ///
    /// # Errors
    ///
    /// The reader, questions, recipients, or review bounds are invalid.
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        if !valid_uid(&self.nonce, "ask")
            || !valid_uid(&self.person, "r")
            || self.reviewed_at <= 0
            || self.endpoints.is_empty()
            || self.endpoints.len() > MAX_PEERS
            || self.profiles.len() > 70
            || self
                .profiles
                .iter()
                .any(|profile| profile.identity.validate().is_err() || profile.revision < 0)
            || self.questions != self.draft.questions()?
            || !valid_endpoints(&self.endpoints)
        {
            return Err("Review the exact questions, reader and distinct pinned recipients".into());
        }
        canonical_hash("lince.trust-remote.review.v1", self).map_err(|error| error.to_string())
    }
}

fn valid_endpoints(endpoints: &[Endpoint]) -> bool {
    let mut organs = BTreeSet::new();
    let mut addresses = BTreeSet::new();
    endpoints.iter().all(|peer| {
        valid_uid(&peer.organ, "r")
            && peer.endpoint.len() == 64
            && peer
                .endpoint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            && peer.profile_revision >= 0
            && peer.label.len() <= 640
            && organs.insert(&peer.organ)
            && addresses.insert(&peer.endpoint)
    })
}
