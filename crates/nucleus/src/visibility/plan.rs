use super::{Condition, Data, Policy};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_STAGES: usize = 8;
pub const MAX_LIFETIME_MS: i64 = 7 * 24 * 60 * 60 * 1000;
pub const MAX_BYTES: usize = 128 * 1024;
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
    pub projection: crate::social::PostDraft,
    pub stages: Vec<Stage>,
    pub exclude: Vec<Condition>,
    pub expires_at_ms: i64,
}

impl Definition {
    pub fn validate(&self, now_ms: i64) -> Result<(), String> {
        let ids = [&self.uid, &self.projection_uid, &self.source_uid];
        if ids.iter().any(|uid| !crate::valid_uid(uid, "r"))
            || !crate::valid_uid(&self.controller_uid, "r")
            || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != 3
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
                    stage.after_ms == 0 || stage.after_ms > MAX_LIFETIME_MS as u64
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
        if self.projection.mode != crate::social::AuthorMode::Anonymous
            || !self.projection.destinations.is_empty()
        {
            return Err("Audience stages use authenticated Record visibility; keep identity and directory publication separate".into());
        }
        for stage in &self.stages {
            self.policy(stage)
                .validate_for(Data::Record)
                .map_err(str::to_owned)?;
        }
        if serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_BYTES / 2
        {
            return Err("Audience plan exceeds its size limit".into());
        }
        Ok(())
    }

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
    pub fn commitment(&self) -> Result<String, String> {
        if serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_BYTES - 4096
        {
            return Err("Audience review exceeds its byte limit".into());
        }
        crate::karma::canonical_hash("lince.record-audience-plan.v1", self)
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
