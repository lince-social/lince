use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub claim: super::super::commands::Draft,
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
    pub profiles: Vec<crate::trust_analysis::commands::ProfileVersion>,
}

impl Review {
    pub fn hash(&self) -> Result<CanonicalHash, String> {
        self.recipe.validate_unsigned()?;
        canonical_hash(
            "lince.profile-claim.recipe-review.v1",
            &(
                &self.recipe.definition,
                self.recipe.permit.signing_bytes()?,
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
