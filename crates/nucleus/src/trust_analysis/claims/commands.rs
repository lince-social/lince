//! Profile-claim commands and review commitments.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Category, Comparator, Descriptor, Document, recipes};
use crate::trust_analysis::commands::ProfileVersion;
use crate::{
    karma::{CanonicalHash, canonical_hash},
    trust_analysis::{Identity, contacts::Audience, query::Query},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    #[serde(default)]
    pub category: Category,
    pub issuer: Identity,
    pub claim: Option<String>,
    pub title: String,
    pub descriptor: Descriptor,
    pub comparator: Comparator,
    pub amount: String,
    pub as_of: Option<i64>,
    pub lifetime_seconds: u32,
    pub expected_revision: u64,
    pub withdrawn: bool,
    pub audience: Audience,
    #[serde(default)]
    pub key_id: String,
    #[serde(default)]
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub person: String,
    pub document: Document,
    pub expected_revision: u64,
    pub reviewed_at: i64,
    pub profiles: Vec<ProfileVersion>,
}

impl Review {
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        canonical_hash(
            "lince.profile-claim.review.v1",
            &(
                &self.person,
                self.document.signing_bytes()?,
                self.expected_revision,
                self.reviewed_at,
                &self.profiles,
            ),
        )
        .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    clippy::large_enum_variant,
    reason = "Keep the public claim command variants compatible with existing callers"
)]
pub enum Command {
    Recipes {
        request: recipes::commands::Command,
    },
    Preview {
        draft: Draft,
    },
    Publish {
        review: Review,
        expected_review: CanonicalHash,
    },
    Import {
        document: String,
        #[serde(default)]
        as_organ: bool,
    },
    Inspect {
        record: String,
        #[serde(default)]
        as_organ: bool,
    },
    Export {
        record: String,
        #[serde(default)]
        as_organ: bool,
    },
    List {
        subject: Identity,
        before: Option<String>,
        #[serde(default)]
        as_organ: bool,
    },
    Check {
        record: String,
        sources: Query,
        #[serde(default)]
        as_organ: bool,
    },
    InspectDocument {
        document: String,
        #[serde(default)]
        as_organ: bool,
    },
    CheckDocument {
        document: String,
        sources: Query,
        #[serde(default)]
        as_organ: bool,
    },
}

impl Command {
    pub fn is_mutation(&self) -> bool {
        match self {
            Self::Recipes { request } => request.is_mutation(),
            Self::Publish { .. } | Self::Import { .. } => true,
            _ => false,
        }
    }
}
