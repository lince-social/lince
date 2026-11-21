//! Generated-claim recipe commands and their review commitments.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Mode, Recipe};
use crate::{
    karma::{CanonicalHash, canonical_hash},
    trust_analysis::{
        claims::commands::Draft as ClaimDraft, commands::ProfileVersion, query::Query,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub claim: ClaimDraft,
    pub recipe: Option<String>,
    pub expected_revision: u64,
    pub inputs: Query,
    pub permit_seconds: u32,
    pub refresh_seconds: u32,
    pub start_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub recipe: Recipe,
    pub expected_revision: u64,
    pub reviewed_at: i64,
    pub profiles: Vec<ProfileVersion>,
}

impl Review {
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        let permit = self.recipe.checked_permit()?;
        canonical_hash(
            "lince.profile-claim.recipe-review.v1",
            &(
                &self.recipe.definition,
                permit,
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
    reason = "Keep the public claim recipe command variants compatible with existing callers"
)]
pub enum Command {
    Preview {
        draft: Draft,
    },
    Save {
        review: Review,
        expected_review: CanonicalHash,
    },
    Inspect {
        record: String,
    },
    List {
        before: Option<String>,
    },
    Control {
        record: String,
        expected_revision: u64,
        mode: Mode,
    },
    Recompute {
        record: String,
    },
}

impl Command {
    pub fn is_mutation(&self) -> bool {
        matches!(
            self,
            Self::Save { .. } | Self::Control { .. } | Self::Recompute { .. }
        )
    }
}
