use std::{collections::BTreeSet, error::Error, fmt};

use chrono::{DateTime, FixedOffset, NaiveDate};
use nucleus::{DecimalValue, RecordKind};

/// Identity sets that the caller has authorized for this query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadableIdentities {
    pub records: BTreeSet<String>,
    pub concepts: BTreeSet<String>,
    pub places: BTreeSet<String>,
    pub assertions: BTreeSet<String>,
}

/// Scalar data for one readable Record, consistent with the authority snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordScalarRow {
    pub uid: String,
    pub kind: RecordKind,
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub quantity: DecimalValue,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
}

/// Per-query ceilings. Each field must be positive and at most its default.
#[derive(Debug, Clone)]
pub struct QueryLimits {
    pub records: usize,
    pub concepts: usize,
    pub places: usize,
    pub assertions: usize,
    pub predicate_nodes: usize,
    pub predicate_depth: usize,
    pub graph_edges: usize,
    pub steps: usize,
    pub bytes: usize,
    pub string_bytes: usize,
    pub results: usize,
    pub order_fields: usize,
    pub projection_fields: usize,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            records: 4096,
            concepts: 4096,
            places: 4096,
            assertions: 32768,
            predicate_nodes: 1024,
            predicate_depth: 12,
            graph_edges: 65536,
            steps: 1_000_000,
            bytes: 16 * 1024 * 1024,
            string_bytes: 1024 * 1024,
            results: 4096,
            order_fields: 8,
            projection_fields: 32,
        }
    }
}

impl QueryLimits {
    pub(super) fn validate(&self) -> Result<(), QueryError> {
        let maximum = Self::default();
        for (value, hard) in [
            (self.records, maximum.records),
            (self.concepts, maximum.concepts),
            (self.places, maximum.places),
            (self.assertions, maximum.assertions),
            (self.predicate_nodes, maximum.predicate_nodes),
            (self.predicate_depth, maximum.predicate_depth),
            (self.graph_edges, maximum.graph_edges),
            (self.steps, maximum.steps),
            (self.bytes, maximum.bytes),
            (self.string_bytes, maximum.string_bytes),
            (self.results, maximum.results),
            (self.order_fields, maximum.order_fields),
            (self.projection_fields, maximum.projection_fields),
        ] {
            if value == 0 || value > hard {
                return Err(QueryError::InvalidLimits);
            }
        }
        Ok(())
    }
}

/// A query refusal that can be reported without disclosing input data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryError {
    InvalidLimits,
    LimitExceeded,
    InvalidInput,
    Unavailable,
    Unsupported,
    CyclicGraph,
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "record query limits are invalid",
            Self::LimitExceeded => "record query limit exceeded",
            Self::InvalidInput => "record query input is invalid",
            Self::Unavailable => "record query dependency is unavailable",
            Self::Unsupported => "record query feature is unsupported",
            Self::CyclicGraph => "record query relation graph is cyclic",
        })
    }
}

impl Error for QueryError {}

/// A scalar field or assertion set the caller may project from a selected Record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordField {
    Uid,
    Kind,
    Slug,
    Head,
    Body,
    Quantity,
    Concept,
    Unit,
    Place,
    Organ,
    StartDate,
    DueDate,
    CreatedAt,
    UpdatedAt,
    Revision,
    Assertions,
}

/// Validated output fields, including the mandatory UID and kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordProjection {
    pub fields: BTreeSet<RecordField>,
    pub extension_namespace: Option<String>,
}

/// The total, a Record kind, or a visible identity concept used for counting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountGroup {
    Total,
    Kind(RecordKind),
    Concept(Option<String>),
}

/// The number of selected Records in one group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountRow {
    pub group: CountGroup,
    pub count: usize,
}

/// Ordered Record UIDs or grouped counts produced by a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryRows {
    Records(Vec<String>),
    Counts(Vec<CountRow>),
}

/// The selected rows and the validated shape of any subsequent projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    pub rows: QueryRows,
    pub projection: RecordProjection,
}
