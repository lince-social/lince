use super::receipt::{Document, ParticipationRequest};
use crate::karma::{CanonicalHash, canonical_hash};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DOMAIN: &str = "lince.transfer-receipt-delivery.v1";
pub const MIME: &str = "application/vnd.lince.transfer-receipt+json";
pub const MAX_BYTES: usize = 850_000;
pub const MAX_SECONDS: i64 = 30 * 86400;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Document {
        record: String,
        expected_version: u64,
    },
    Participation {
        transfer: String,
        expected_revision: u64,
        party: Option<String>,
    },
}

impl Source {
    pub fn records(&self) -> Vec<&String> {
        match self {
            Self::Document { record, .. } => vec![record],
            Self::Participation {
                transfer, party, ..
            } => std::iter::once(transfer).chain(party.iter()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub source: Source,
    pub thread: String,
    pub recipients: Vec<String>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
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
            || !crate::valid_uid(&self.sender, "r")
            || self.recipients.is_empty()
            || self.recipients.len() > 64
            || self
                .recipients
                .iter()
                .any(|uid| !crate::valid_uid(uid, "r") || uid == &self.sender)
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
                if let Document::Review(review) = document {
                    if self.recipients.iter().any(|uid| {
                        uid != &review.scope.source.origin
                            && uid != &review.scope.giver
                            && uid != &review.scope.receiver
                    }) {
                        return Err(
                            "Private reviews can go only to their named approving Organs".into(),
                        );
                    }
                }
            }
            Artifact::Participation(request) => {
                request.validate()?;
                if self.recipients != [request.terms.organ.clone()] {
                    return Err(
                        "A private participation request can go only to its named approving Organ"
                            .into(),
                    );
                }
            }
        }
        if serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_BYTES
        {
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

    pub fn parse(bytes: &[u8], now: i64) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Delivery exceeds its byte budget".into());
        }
        let delivery: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        delivery.validate_envelope(now)?;
        Ok(delivery)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Inbox {
        #[serde(default)]
        before: Option<String>,
    },
    Options {
        source: Source,
    },
    Preview {
        request: Request,
    },
    Send {
        request: Request,
        reviewed_at: i64,
        expected_projection: CanonicalHash,
    },
    Inspect {
        message: String,
    },
    Import {
        message: String,
        expected_projection: CanonicalHash,
    },
    Cancel {
        message: String,
    },
    Retry {
        message: String,
    },
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::Inbox { .. }
            | Self::Options { .. }
            | Self::Preview { .. }
            | Self::Inspect { .. } => "record:read",
            Self::Send { .. } | Self::Import { .. } => "record:create",
            Self::Cancel { .. } | Self::Retry { .. } => "record:update",
        }
    }
    pub fn is_mutation(&self) -> bool {
        !matches!(
            self,
            Self::Inbox { .. } | Self::Options { .. } | Self::Preview { .. } | Self::Inspect { .. }
        )
    }
    pub fn records(&self) -> Vec<&String> {
        match self {
            Self::Inbox { .. } => vec![],
            Self::Options { source } => source.records(),
            Self::Preview { request } | Self::Send { request, .. } => {
                std::iter::once(&request.thread)
                    .chain(request.source.records())
                    .collect()
            }
            Self::Inspect { message }
            | Self::Import { message, .. }
            | Self::Cancel { message }
            | Self::Retry { message } => vec![message],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::receipt::Participation;

    fn private_request() -> Delivery {
        let sender = crate::new_uid("r");
        let recipient = crate::new_uid("r");
        let transfer = crate::new_uid("r");
        let party = crate::new_uid("r");
        let commitment = Participation::party_commitment(&sender, &transfer, 1, &party).unwrap();
        Delivery {
            domain: DOMAIN.into(),
            sender: sender.clone(),
            recipients: vec![recipient.clone()],
            issued_at: 1000,
            expires_at: 2000,
            artifact: Artifact::Participation(ParticipationRequest {
                public_history_terms: None,
                transfer,
                party: Some(party),
                terms: Participation {
                    public_history: None,
                    organ: recipient,
                    origin: sender,
                    agreement_revision: 1,
                    agreement_commitment: canonical_hash(DOMAIN, &"agreement").unwrap(),
                    party_commitment: Some(commitment),
                },
            }),
        }
    }

    #[test]
    fn participation_delivery_is_bound_to_the_named_approver_and_a_bounded_term() {
        let delivery = private_request();
        delivery.validate(1000).unwrap();
        let bytes = serde_json::to_vec(&delivery).unwrap();
        assert_eq!(Delivery::parse(&bytes, 1000).unwrap(), delivery);
        let mut changed = delivery.clone();
        changed.recipients = vec![crate::new_uid("r")];
        assert!(changed.validate(1000).is_err());
        changed = delivery.clone();
        changed.expires_at = changed.issued_at + MAX_SECONDS + 1;
        assert!(changed.validate(1000).is_err());
        changed = delivery.clone();
        changed.issued_at = 1301;
        assert!(changed.validate(1000).is_err());
        let mut unknown = serde_json::to_value(delivery).unwrap();
        unknown["execute"] = serde_json::json!({"action":"settle"});
        assert!(Delivery::parse(&serde_json::to_vec(&unknown).unwrap(), 1000).is_err());
        assert!(Delivery::parse(&vec![b' '; MAX_BYTES + 1], 1000).is_err());
    }

    #[test]
    fn delivery_controls_keep_inspection_and_preview_read_only() {
        let request = Request {
            source: Source::Document {
                record: crate::new_uid("r"),
                expected_version: 1,
            },
            thread: crate::new_uid("r"),
            recipients: vec![crate::new_uid("r")],
            expires_at: 2000,
        };
        assert!(
            !Command::Preview {
                request: request.clone()
            }
            .is_mutation()
        );
        assert!(
            !Command::Inspect {
                message: crate::new_uid("r")
            }
            .is_mutation()
        );
        assert!(
            Command::Send {
                request,
                reviewed_at: 1000,
                expected_projection: canonical_hash(DOMAIN, &"review").unwrap()
            }
            .is_mutation()
        );
        assert!(
            Command::Import {
                message: crate::new_uid("r"),
                expected_projection: canonical_hash(DOMAIN, &"artifact").unwrap()
            }
            .is_mutation()
        );
    }
}
