use serde::{Deserialize, Serialize};

use super::{CivilDateTime, Consequence, FoldPolicy, GapPolicy, TimeZoneId, TzdbRevision};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DateInput {
    Instant {
        at_ms: i64,
    },
    After {
        milliseconds: u64,
    },
    Local {
        date: CivilDateTime,
        timezone: TimeZoneId,
        tzdb: TzdbRevision,
        gap: GapPolicy,
        fold: FoldPolicy,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Purpose {
    Once,
    Start,
    End,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Start => "start",
            Self::End => "end",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BoundaryInput {
    pub purpose: Purpose,
    pub date: DateInput,
    pub target: String,
    pub consequences: Vec<Consequence>,
}
