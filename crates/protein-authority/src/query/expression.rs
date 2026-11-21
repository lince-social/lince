//! The top-level query, source, and optional aggregation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Include, Order, Predicate};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Protein {
    pub source: Source,
    #[serde(default, rename = "where")]
    pub filter: Vec<Predicate>,
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    #[serde(default)]
    pub include: Include,
    #[serde(default)]
    pub aggregate: Option<Aggregate>,
    #[serde(default)]
    pub order: Vec<Order>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Aggregate {
    pub op: AggregateOp,
    pub by: GroupBy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOp {
    Sum,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    Total,
    Concept,
    Classification,
    Kind,
    CauseKind,
    Day,
    Month,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Record,
    Promise,
    Decision,
    Fact,
    Concept,
    Lingua,
    Assertion,
    Transfer,
    TransferSettlementPreview,
    TransferBulkCompletionPreview,
    Auth,
    Karma,
    KarmaRule,
    Timeline,
    Calendar,
    Schedule,
    Entry,
    Frequency,
    Recurrence,
    Nearby,
}
