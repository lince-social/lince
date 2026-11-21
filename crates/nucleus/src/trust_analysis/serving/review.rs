//! Bound serving requests and the reviews that authorize an exact scope.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    super::{Filter, Identity, claims::Authorship, contacts::MAX_TERM_SECONDS},
    Artifact, SourceKind,
};
use crate::{
    json::encoded_len,
    karma::{CanonicalHash, canonical_hash},
    valid_uid,
    visibility::MAX_POLICY_BYTES,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub record: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub source: Source,
    #[serde(default, skip_serializing_if = "Scope::is_fixed")]
    pub scope: Scope,
    #[serde(default)]
    pub as_organ: bool,
    pub audience: Filter,
    pub lifetime_seconds: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    #[default]
    Fixed,
    Recipe,
}

impl Scope {
    pub fn is_fixed(&self) -> bool {
        *self == Self::Fixed
    }
}

impl Request {
    pub fn validate(&self) -> Result<(), String> {
        self.audience.validate()?;
        if encoded_len(&self.audience).map_err(|error| error.to_string())? > MAX_POLICY_BYTES {
            return Err("Serving audience exceeds its 12 KiB policy limit".into());
        }
        if !valid_uid(&self.source.record, "r")
            || (self.scope == Scope::Recipe && self.source.kind != SourceKind::Claim)
            || !(1..=MAX_TERM_SECONDS).contains(&i64::from(self.lifetime_seconds))
            || self
                .audience
                .include
                .iter()
                .chain(&self.audience.exclude)
                .flat_map(|condition| &condition.identities)
                .any(|identity| !matches!(identity, Identity::Organ(_)))
        {
            return Err("Choose retained evidence, Organ audience conditions and a term of at most 366 days".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub person: String,
    pub previous: Option<String>,
    pub request: Request,
    pub artifact: Artifact,
    pub source_version: u64,
    pub source_revision: i64,
    pub reviewed_at: i64,
    pub expires_at: i64,
}

impl Review {
    pub fn term_boundary(&self) -> Result<i64, String> {
        match (self.request.scope, &self.artifact) {
            (Scope::Fixed, artifact) => artifact.expires_at(),
            (Scope::Recipe, Artifact::Claim(document)) => match &document.statement.authorship {
                Authorship::Generated { permit, .. } => Ok(permit.terms.expires_at),
                _ => Err("Recipe sharing requires an original generated claim and permit".into()),
            },
            _ => Err("Recipe sharing is only available for generated claims".into()),
        }
    }

    pub fn allows_current(&self, artifact: &Artifact, version: u64, revision: i64) -> bool {
        if self.source_version != version {
            return false;
        }
        match (self.request.scope, &self.artifact, artifact) {
            (Scope::Fixed, original, current) => {
                self.source_revision == revision && original == current
            }
            (Scope::Recipe, Artifact::Claim(original), Artifact::Claim(current)) => {
                match (
                    &original.statement.authorship,
                    &current.statement.authorship,
                ) {
                    (
                        Authorship::Generated {
                            recipe,
                            revision,
                            permit,
                            ..
                        },
                        Authorship::Generated {
                            recipe: current_recipe,
                            revision: current_revision,
                            permit: current_permit,
                            ..
                        },
                    ) => {
                        recipe == current_recipe
                            && revision == current_revision
                            && permit == current_permit
                            && permit.matches(&original.statement).is_ok()
                            && permit.matches(&current.statement).is_ok()
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub fn hash(&self) -> Result<CanonicalHash, String> {
        self.request.validate()?;
        self.artifact.validate(self.reviewed_at)?;
        if !valid_uid(&self.person, "r")
            || self
                .previous
                .as_ref()
                .is_some_and(|record| !valid_uid(record, "r"))
            || self.source_version == 0
            || self.source_version > i64::MAX as u64
            || self.source_revision < 0
            || self.reviewed_at <= 0
            || self.request.source.kind != self.artifact.kind()
            || self.expires_at
                != self.term_boundary()?.min(
                    self.reviewed_at
                        .saturating_add(i64::from(self.request.lifetime_seconds)),
                )
        {
            return Err("Review the exact original evidence, audience and bounded term".into());
        }
        canonical_hash("lince.evidence-serving.review.v1", self).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Preview {
        request: Request,
    },
    Publish {
        review: Box<Review>,
        expected_review: CanonicalHash,
    },
    List {
        before: Option<String>,
    },
    Inspect {
        record: String,
    },
}
