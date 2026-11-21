//! Staged visibility of a reviewed Record publication.
//! Each stage changes the inclusions while retaining the plan's exclusions.
//! A review commitment binds its source, definition, and replies.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Condition, Data, Policy, rules::PolicyView};
use crate::{
    json::encoded_len,
    karma::canonical_hash,
    social::{AuthorMode, PostDraft},
    valid_uid,
};

/// Maximum number of audience stages in one plan.
pub const MAX_STAGES: usize = 8;
/// Maximum lifetime of an active plan in milliseconds.
pub const MAX_LIFETIME_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Byte limit for a saved plan and its metadata.
pub const MAX_BYTES: usize = 128 * 1024;
/// Record extension namespace for the private plan data.
pub const PRIVATE_NAMESPACE: &str = "lince.audience-plan.private";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub after_ms: u64,
    pub include: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub uid: String,
    pub projection_uid: String,
    pub controller_uid: String,
    pub source_uid: String,
    pub source_revision: u64,
    pub projection: PostDraft,
    pub stages: Vec<Stage>,
    pub exclude: Vec<Condition>,
    pub expires_at_ms: i64,
}

impl Definition {
    /// Validates the source, stages and expiry against the current timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid identifiers, publication rules, expiry, or size.
    pub fn validate(&self, now_ms: i64) -> Result<(), String> {
        let [plan, projection, source] = [&self.uid, &self.projection_uid, &self.source_uid];
        if [plan, projection, source, &self.controller_uid]
            .iter()
            .any(|uid| !valid_uid(uid, "r"))
            || plan == projection
            || plan == source
            || projection == source
            || self.source_revision == 0
        {
            return Err(
                "Choose distinct source, publication and plan Records and a controller".into(),
            );
        }
        if self.stages.is_empty()
            || self.stages.len() > MAX_STAGES
            || self.stages.iter().enumerate().any(|(index, stage)| {
                if index == 0 {
                    stage.after_ms != 0
                } else {
                    stage.after_ms == 0 || stage.after_ms > MAX_LIFETIME_MS.unsigned_abs()
                }
            })
        {
            return Err(
                "Choose one to eight stages, starting now and using positive delays afterward"
                    .into(),
            );
        }
        if self.expires_at_ms <= now_ms
            || self.expires_at_ms > now_ms.saturating_add(MAX_LIFETIME_MS)
        {
            return Err("Choose an expiry within the next seven days".into());
        }
        self.projection.validate()?;
        if self.projection.mode != AuthorMode::Anonymous || !self.projection.destinations.is_empty()
        {
            return Err("Audience stages use authenticated Record visibility; keep identity and directory publication separate".into());
        }
        for stage in &self.stages {
            PolicyView {
                include: &stage.include,
                exclude: &self.exclude,
            }
            .validate_for(Data::Record)
            .map_err(str::to_owned)?;
        }
        if encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES / 2 {
            return Err("Audience plan exceeds its size limit".into());
        }
        Ok(())
    }

    /// Builds the owned policy for the supplied stage.
    pub fn policy(&self, stage: &Stage) -> Policy {
        Policy {
            include: stage.include.clone(),
            exclude: self.exclude.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub definition: Definition,
    pub expected_revision: u64,
    pub source: crate::transfer::karma::Snapshot,
    pub replies: String,
}

impl Review {
    /// Calculates the canonical commitment for this review.
    ///
    /// # Errors
    ///
    /// Returns an error if the review exceeds its byte budget or cannot be encoded.
    pub fn commitment(&self) -> Result<String, String> {
        if encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES - 4096 {
            return Err("Audience review exceeds its byte limit".into());
        }
        canonical_hash("lince.record-audience-plan.v1", self)
            .map(|hash| hash.as_str().to_owned())
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Paused,
    Stopped,
    ReviewRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub review: Review,
    pub commitment: String,
    pub revision: u64,
    pub stage: Option<usize>,
    pub next_at_ms: i64,
    pub status: Status,
    pub reason: String,
    pub rule_uid: Option<String>,
    pub policy_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[expect(
    clippy::large_enum_variant,
    reason = "Review retains its public Definition payload type"
)]
pub enum Command {
    Inspect {
        source_uid: String,
        controller_uid: String,
    },
    Review {
        definition: Definition,
    },
    Enable {
        review: Box<Review>,
        commitment: String,
    },
    Pause {
        plan_uid: String,
        controller_uid: String,
        expected_revision: u64,
    },
    Stop {
        plan_uid: String,
        controller_uid: String,
        expected_revision: u64,
    },
}
