use nucleus::karma::{CivilDateTime, FoldPolicy, FrequencyAst, GapPolicy, TimeZoneId};
use serde::{Deserialize, Serialize};

pub const TUTORIAL: &str = "cleaning-room.v1";
pub const GUIDE: &str = "r_K7T3ZG08G5EBWRSTF8NBWMTZYV";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub time: String,
    pub timezone: TimeZoneId,
    pub gap: GapPolicy,
    pub fold: FoldPolicy,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            time: "18:00".into(),
            timezone: TimeZoneId::new("UTC").expect("UTC is a valid timezone identity"),
            gap: GapPolicy::ShiftForward,
            fold: FoldPolicy::First,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Record,
    Frequency,
    Rule,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Frequency => "frequency",
            Self::Rule => "rule",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub kind: Kind,
    pub uid: String,
    pub name: String,
    pub slug: String,
    pub exists: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub fingerprint: String,
    pub input: Input,
    pub first_local: CivilDateTime,
    pub first_at_ms: i64,
    pub frequency: FrequencyAst,
    pub objects: Vec<Object>,
    pub imported: bool,
    pub current_quantity: Option<nucleus::DecimalValue>,
    pub conflicts: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Definition {
    pub input: Input,
    pub first_local: CivilDateTime,
    pub first_at_ms: i64,
    pub frequency: FrequencyAst,
    pub source: String,
    pub objects: Vec<Object>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Imported {
    pub tutorial: String,
    pub objects: Vec<Object>,
    pub quantity_at_import: nucleus::DecimalValue,
}
