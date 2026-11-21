//! Stored visibility rules and their wire format.
//!
//! A [Policy] allows a principal when at least one inclusion matches and no
//! exclusion matches. [Condition] intersects identities with a proximity range.
//! [Decision] retains the matching condition indices and the most precise admitted
//! radius. The [plan] module makes these rules available in timed audience stages.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::location::Choice;

pub mod plan;
mod rules;

/// Maximum total inclusion and exclusion rules in one policy.
pub const MAX_CONDITIONS: usize = 64;
/// Maximum number of Organs or Persons in either list of a condition.
pub const MAX_ORGANS_PER_CONDITION: usize = 64;
/// Maximum encoded policy size.
pub const MAX_POLICY_BYTES: usize = 12 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Data {
    Record,
    Place,
    LiveLocation,
}

impl Data {
    /// Returns the stable wire name for this visibility domain.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Place => "place",
            Self::LiveLocation => "live_location",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    pub value: u32,
    pub inclusive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Condition {
    pub organs: Vec<String>,
    pub persons: Vec<String>,
    pub lower: Option<Bound>,
    pub upper: Option<Bound>,
    pub radius_metres: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub include: Vec<Condition>,
    pub exclude: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Decision {
    pub allowed: bool,
    pub included_by: Vec<usize>,
    pub excluded_by: Vec<usize>,
    pub radius_metres: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SavedPolicy {
    pub controller_uid: String,
    pub revision: u64,
    pub policy: Policy,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Organ {
    pub uid: String,
    pub label: String,
    pub proximity: Option<u32>,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Context {
    pub record_uid: String,
    pub data: Data,
    pub saved: Option<SavedPolicy>,
    pub organs: Vec<Organ>,
    pub persons: Vec<Choice>,
    pub records: Vec<Choice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Context {
        record_uid: String,
        data: Data,
    },
    Save {
        record_uid: String,
        data: Data,
        policy: Policy,
        expected_revision: u64,
    },
}

#[cfg(test)]
mod tests;
