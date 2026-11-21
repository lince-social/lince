//! Public Trust Analysis commands and their permission classification.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    claims::{commands::Command as ClaimCommand, recipes::commands::Command as RecipeCommand},
    custody::Command as CustodyCommand,
    paths,
    query::Query,
    remote::Command as RemoteCommand,
    serving::Command as ServingCommand,
};
use crate::{karma::CanonicalHash, transfer::matching};

mod contact;

pub use contact::{ContactDraft, ContactReview, ProfileVersion};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    clippy::large_enum_variant,
    reason = "Keep the public trust analysis command variants compatible with existing callers"
)]
pub enum Command {
    MatchSuppliers {
        request: matching::Request,
    },
    Remote {
        request: RemoteCommand,
    },
    Serving {
        request: ServingCommand,
    },
    Custody {
        request: CustodyCommand,
    },
    Claims {
        request: ClaimCommand,
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
        query: Query,
    },
    LoadQuery {
        record: String,
    },
    ListQueries {
        before: Option<String>,
    },
    AnalyzeQuery {
        query: Query,
        #[serde(default)]
        as_organ: bool,
    },
}

impl Command {
    pub fn is_mutation(&self) -> bool {
        self.permission() != "record:read"
    }

    pub fn permission(&self) -> &'static str {
        match self {
            Self::Custody {
                request: CustodyCommand::CancelDelivery { .. },
            }
            | Self::Claims {
                request:
                    ClaimCommand::Recipes {
                        request: RecipeCommand::Control { .. },
                    },
            } => "record:update",
            Self::Custody {
                request: CustodyCommand::SetPolicy { .. } | CustodyCommand::Cleanup,
            } => "organ:update",
            Self::Serving {
                request: ServingCommand::Publish { .. },
            }
            | Self::PublishContact { .. }
            | Self::ImportContact { .. }
            | Self::SaveQuery { .. } => "record:create",
            Self::Claims { request } if request.is_mutation() => "record:create",
            _ => "record:read",
        }
    }
}
