use super::{
    Identity,
    contacts::{Audience, Document},
    paths,
};
use crate::karma::{CanonicalHash, canonical_hash};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    MatchSuppliers {
        request: crate::transfer::matching::Request,
    },
    Remote {
        request: super::remote::Command,
    },
    Serving {
        request: super::serving::Command,
    },
    Custody {
        request: super::custody::Command,
    },
    Claims {
        request: super::claims::commands::Command,
    },
    Context,
    PreviewContact {
        draft: ContactDraft,
    },
    PublishContact {
        review: ContactReview,
        expected_review: CanonicalHash,
    },
    ImportContact {
        document: String,
        #[serde(default)]
        as_organ: bool,
    },
    InspectContact {
        record: String,
        #[serde(default)]
        as_organ: bool,
    },
    ExportContact {
        record: String,
        #[serde(default)]
        as_organ: bool,
    },
    ListContacts {
        before: Option<String>,
        #[serde(default)]
        as_organ: bool,
    },
    ContactPaths {
        policy: paths::Policy,
        #[serde(default)]
        as_organ: bool,
    },
    SaveQuery {
        name: String,
        query: super::query::Query,
    },
    LoadQuery {
        record: String,
    },
    ListQueries {
        before: Option<String>,
    },
    AnalyzeQuery {
        query: super::query::Query,
        #[serde(default)]
        as_organ: bool,
    },
}

impl Command {
    pub fn is_mutation(&self) -> bool {
        if let Self::Custody { request } = self {
            return matches!(
                request,
                super::custody::Command::SetPolicy { .. }
                    | super::custody::Command::Cleanup
                    | super::custody::Command::CancelDelivery { .. }
            );
        }
        if let Self::Serving { request } = self {
            return matches!(request, super::serving::Command::Publish { .. });
        }
        if let Self::Claims { request } = self {
            return request.is_mutation();
        }
        matches!(
            self,
            Self::PublishContact { .. } | Self::ImportContact { .. } | Self::SaveQuery { .. }
        )
    }

    pub fn permission(&self) -> &'static str {
        if matches!(
            self,
            Self::Custody {
                request: super::custody::Command::CancelDelivery { .. }
            }
        ) {
            return "record:update";
        }
        if let Self::Custody {
            request: super::custody::Command::SetPolicy { .. } | super::custody::Command::Cleanup,
        } = self
        {
            return "organ:update";
        }
        if matches!(
            self,
            Self::Claims {
                request: super::claims::commands::Command::Recipes {
                    request: super::claims::recipes::commands::Command::Control { .. }
                }
            }
        ) {
            return "record:update";
        }
        if self.is_mutation() {
            "record:create"
        } else {
            "record:read"
        }
    }
}
