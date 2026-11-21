//! Conditions that select rows, including exact-decimal comparisons.

use nucleus::{
    DecimalValue,
    projection::{RollingWindow, Window},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{LinkDirection, decimal_operand};

fn part_of_kind() -> String {
    "feature:containment".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Predicate {
    ProjectionWindow(Window),
    RollingWindow(RollingWindow),
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
    QuantityLt(
        #[serde(with = "decimal_operand")]
        #[schemars(with = "String")]
        DecimalValue,
    ),
    QuantityLte(
        #[serde(with = "decimal_operand")]
        #[schemars(with = "String")]
        DecimalValue,
    ),
    QuantityGt(
        #[serde(with = "decimal_operand")]
        #[schemars(with = "String")]
        DecimalValue,
    ),
    QuantityGte(
        #[serde(with = "decimal_operand")]
        #[schemars(with = "String")]
        DecimalValue,
    ),
    QuantityEq(
        #[serde(with = "decimal_operand")]
        #[schemars(with = "String")]
        DecimalValue,
    ),
    UidEq(String),
    OccurrenceIn(Vec<String>),
    KindEq(String),
    SlugEq(String),
    ConceptIn(String),
    Relation {
        kind: String,
        #[serde(default)]
        direction: LinkDirection,
        #[serde(default)]
        other: Option<String>,
    },
    Under {
        record: String,
        #[serde(default = "part_of_kind")]
        kind: String,
        #[serde(default)]
        include_self: bool,
    },
    TextContains(String),
    WorkDate {
        field: WorkDateField,
        op: DateComparison,
        #[serde(default)]
        value: Option<String>,
    },
    StateIn(Vec<String>),
    RevisionEq(u64),
    RevisionLt(u64),
    RevisionLte(u64),
    RevisionGt(u64),
    RevisionGte(u64),
    StatusIn(Vec<String>),
    ViewerRoleIn(Vec<String>),
    InvitationStateIn(Vec<String>),
    PersonEq(String),
    UnitEq(String),
    WindowEndBefore(String),
    WindowEndAfter(String),
    AtSince(String),
    AtBefore(String),
    ClassifiedIn(String),
    CauseKindEq(String),
    RecordEq(String),
    Near {
        of: String,
        meters: f64,
    },
    OrganEq(String),
    OrganIn(Vec<String>),
    FileOriginEq(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkDateField {
    Start,
    Due,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DateComparison {
    Eq,
    Lt,
    Lte,
    Gt,
    Gte,
    Exists,
}
