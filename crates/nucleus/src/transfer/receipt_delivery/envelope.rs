//! Receipt delivery envelopes and validation of their routing and term.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{DOMAIN, MAX_BYTES, MAX_SECONDS};
use crate::{
    json::encoded_len,
    karma::{CanonicalHash, canonical_hash},
    transfer::receipt::{Document, ParticipationRequest},
    valid_uid,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[allow(
    clippy::large_enum_variant,
    reason = "Keep the public receipt delivery variants compatible with existing callers"
)]
pub enum Artifact {
    Document(Document),
    Participation(ParticipationRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub domain: String,
    pub sender: String,
    pub recipients: Vec<String>,
    pub issued_at: i64,
    pub expires_at: i64,
    pub artifact: Artifact,
}

impl Delivery {
    pub fn private_audit(&self, message: &str) -> serde_json::Value {
        serde_json::json!({"domain":"lince.artifact-delivery.audit.v1","message":message,"artifact":self})
    }
    fn validate_envelope(&self, now: i64) -> Result<(), String> {
        if self.domain != DOMAIN
            || !valid_uid(&self.sender, "r")
            || self.recipients.is_empty()
            || self.recipients.len() > 64
            || self
                .recipients
                .iter()
                .any(|uid| !valid_uid(uid, "r") || uid == &self.sender)
            || self.recipients.windows(2).any(|pair| pair[0] >= pair[1])
            || self.issued_at <= 0
            || self.issued_at > now.saturating_add(300)
            || self.expires_at <= self.issued_at
            || self.expires_at.saturating_sub(self.issued_at) > MAX_SECONDS
        {
            return Err("Invalid delivery sender, recipients or term".into());
        }
        match &self.artifact {
            Artifact::Document(document) => {
                if self.expires_at > document.expires_at() {
                    return Err("Delivery exceeds the artifact's reviewed term".into());
                }
                if let Document::Review(review) = document
                    && self.recipients.iter().any(|uid| {
                        uid != &review.scope.source.origin
                            && uid != &review.scope.giver
                            && uid != &review.scope.receiver
                    })
                {
                    return Err(
                        "Private reviews can go only to their named approving Organs".into(),
                    );
                }
            }
            Artifact::Participation(request) => {
                request.validate()?;
                if !matches!(self.recipients.as_slice(), [organ] if organ == &request.terms.organ) {
                    return Err(
                        "A private participation request can go only to its named approving Organ"
                            .into(),
                    );
                }
            }
        }
        if encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES {
            return Err("Delivery exceeds its byte budget".into());
        }
        Ok(())
    }

    pub fn validate(&self, now: i64) -> Result<(), String> {
        self.validate_envelope(now)?;
        if let Artifact::Document(document) = &self.artifact {
            document.validate(now)?;
        }
        Ok(())
    }

    pub fn hash(&self) -> Result<CanonicalHash, String> {
        canonical_hash(DOMAIN, self).map_err(|error| error.to_string())
    }

    /// Parse the envelope and check its routing, term, and byte budget.
    ///
    /// Call `validate` to check an embedded receipt document as well.
    ///
    /// # Errors
    ///
    /// The envelope is malformed or outside its delivery rules.
    pub fn parse(bytes: &[u8], now: i64) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Delivery exceeds its byte budget".into());
        }
        let delivery: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        delivery.validate_envelope(now)?;
        Ok(delivery)
    }
}
