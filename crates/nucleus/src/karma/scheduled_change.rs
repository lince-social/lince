// non-lean-yet
// Scope: enum DateInput, enum Purpose, struct BoundaryInput.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 7, 25.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 7, 8,
//   9, 11, 12, 14, 15, 16, 17, 18, 19, 25, 26, 27, 28, 32, 42, 43, 44, 45, 46.

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
