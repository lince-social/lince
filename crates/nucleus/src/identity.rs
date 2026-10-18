use crate::karma::{CanonicalHash, canonical_hash};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub mod roots;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PersonKey {
    pub subject: String,
    pub key_id: String,
    pub public_key: String,
}

impl PersonKey {
    pub fn fingerprint(&self) -> Result<CanonicalHash, String> {
        canonical_hash("lince.identity.person-key.v1", self).map_err(|error| error.to_string())
    }

    pub fn review_hash(&self, profile_revision: i64) -> Result<CanonicalHash, String> {
        canonical_hash(
            "lince.identity.person-key-review.v1",
            &(self, profile_revision),
        )
        .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    OrganRoots { subject: String },
    ExportOrganRoot { subject: String },
    PreviewOrganRoot { subject: String, document: String },
    AcceptOrganRoot { request_id: String, review: roots::Review, expected_review: CanonicalHash },
    PersonKeys {
        subject: String,
    },
    PreviewPersonKey {
        key: PersonKey,
    },
    AcceptPersonKey {
        key: PersonKey,
        expected_profile_revision: i64,
        expected_review: CanonicalHash,
    },
}

impl Command {
    pub fn subject(&self) -> &str {
        match self {
            Self::OrganRoots {subject} | Self::ExportOrganRoot {subject} | Self::PreviewOrganRoot {subject,..} => subject,
            Self::AcceptOrganRoot {review,..} => &review.proof.organ,
            Self::PersonKeys { subject } => subject,
            Self::PreviewPersonKey { key } | Self::AcceptPersonKey { key, .. } => &key.subject,
        }
    }

    pub fn is_mutation(&self) -> bool {
        matches!(self, Self::AcceptPersonKey { .. } | Self::AcceptOrganRoot {..})
    }

    pub fn permission(&self) -> &'static str {
        if self.is_mutation() || matches!(self,Self::ExportOrganRoot {..}) {
            "organ:update"
        } else {
            "record:read"
        }
    }
}
