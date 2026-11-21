// non-lean-yet
// Scope: struct Draft, struct Review, enum Command.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 54.
// - [Clippy: clippy::large_enum_variant] large size difference between variants. Sites 54.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 37.
// - [Clippy: clippy::wildcard_imports] Use explicit imports while preserving framework and test
//   conventions. Sites 1.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 6, 8,
//   9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 23, 28, 29, 30, 31, 32, 33, 37, 54, 55, 56, 58,
//   59, 61, 62, 63, 65, 66, 68, 70, 71, 73, 75, 76, 78, 80, 81, 82, 84, 86, 87, 88, 90, 92, 93, 95,
//   97, 98, 99, 101, 106.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites hash: String
//   (37).

use super::*;
use crate::trust_analysis::commands::ProfileVersion;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    #[serde(default)]
    pub category: super::Category,
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
pub enum Command {
    Recipes {
        request: super::recipes::commands::Command,
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
        if let Self::Recipes { request } = self {
            return request.is_mutation();
        }
        matches!(self, Self::Publish { .. } | Self::Import { .. })
    }
}
