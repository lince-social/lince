use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use nucleus::{DecimalValue, RecordKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::query::Predicate;

/// Identifies a field in a Record extension namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionProperty {
    pub namespace: String,
    pub field: String,
}

/// Names a Record property that a mutation grant can authorize.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Kind,
    Slug,
    Head,
    Body,
    Quantity,
    Unit,
    Place,
    Organ,
    Extension(ExtensionProperty),
}

/// Describes the transition between the current and proposed Record states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Create,
    Update,
    Delete,
    Restore,
}

/// Distinguishes an ordinary Assertion from a Record identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionRole {
    Ordinary,
    Identity,
}

/// Specifies the object a grant can cover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionTarget {
    Unary,
    Record(String),
    Uri(String),
    AnyReadableRecord,
}

/// Names the optional data a grant can cover on an ordinary Assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionProperty {
    Quantity,
    Unit,
}

/// Permits an Assertion with a matching predicate, target, role, and properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionGrant {
    pub predicate_uid: String,
    pub target: AssertionTarget,
    pub role: AssertionRole,
    pub properties: BTreeSet<AssertionProperty>,
}

/// Supplies permissions for Records selected for one operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationGrant {
    pub operation: Operation,
    pub selector: Predicate,
    pub properties: BTreeSet<Property>,
    pub assertions_add: Vec<AssertionGrant>,
    pub assertions_remove: Vec<AssertionGrant>,
}

/// Defines the read selector and operation grants for a Role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RolePolicy {
    pub read: Predicate,
    pub grants: Vec<MutationGrant>,
}

/// Supplies a Record identity, lifecycle state, and optional content.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordState {
    pub uid: String,
    pub kind: RecordKind,
    pub organ_uid: Option<String>,
    pub deleted: bool,
    pub content: Option<RecordContent>,
}

/// Supplies the Record properties used for selectors and mutation checks.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordContent {
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub quantity: DecimalValue,
    pub unit_uid: Option<String>,
    pub place_uid: Option<String>,
    pub extensions: BTreeMap<ExtensionProperty, Value>,
}

/// Supplies the Concept name and parent relations used by selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptState {
    pub uid: String,
    pub name: String,
    pub parents: BTreeSet<String>,
}

/// Supplies one Assertion in the authority snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionState {
    pub uid: String,
    pub subject_uid: String,
    pub predicate_uid: String,
    pub object_uid: Option<String>,
    pub object_uri: Option<String>,
    pub role: AssertionRole,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
}

impl AssertionState {
    pub(super) fn relation_key(&self) -> (&str, &str, Option<&str>, Option<&str>) {
        (
            &self.subject_uid,
            &self.predicate_uid,
            self.object_uid.as_deref(),
            self.object_uri.as_deref(),
        )
    }

    pub(super) fn invalid_identity_shape(&self) -> bool {
        self.role == AssertionRole::Identity
            && (self.object_uid.is_some()
                || self.object_uri.is_some()
                || self.quantity.is_some()
                || self.unit_uid.is_some())
    }
}

/// Supplies the Records, Concepts, Places, and Assertions needed to evaluate policies.
#[derive(Debug, Clone, Default)]
pub struct GraphSnapshot {
    pub records: Vec<RecordState>,
    pub concepts: Vec<ConceptState>,
    pub assertions: Vec<AssertionState>,
    /// Maps Assertion UIDs with URI objects to resolved Records in the same Organ.
    pub resolved_links: BTreeMap<String, String>,
    pub places: BTreeSet<String>,
}

/// Bounds the Records, Concepts, and Places a policy can expose.
#[derive(Debug, Clone, Default)]
pub struct VisibilityCeiling {
    pub records: BTreeSet<String>,
    pub concepts: BTreeSet<String>,
    pub places: BTreeSet<String>,
}

/// Bounds the amount of input and work for an authorization evaluation.
#[derive(Debug, Clone)]
pub struct Limits {
    pub records: usize,
    pub concepts: usize,
    pub places: usize,
    pub assertions: usize,
    pub grants: usize,
    pub predicate_nodes: usize,
    pub predicate_depth: usize,
    pub graph_edges: usize,
    pub steps: usize,
    pub bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            records: 4096,
            concepts: 4096,
            places: 4096,
            assertions: 32768,
            grants: 128,
            predicate_nodes: 1024,
            predicate_depth: 12,
            graph_edges: 65536,
            steps: 1_000_000,
            bytes: 16 * 1024 * 1024,
        }
    }
}

/// Reports a refusal without exposing the evaluated content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityError {
    MissingPolicy,
    InvalidPolicy,
    UnsupportedPredicate,
    MissingDependency,
    InvalidIdentity,
    InvalidGraph,
    CyclicGraph,
    LimitExceeded,
    InvalidMutation,
    IncompleteRecord,
    Denied,
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingPolicy => "authority policy is missing",
            Self::InvalidPolicy => "authority policy is invalid",
            Self::UnsupportedPredicate => "authority predicate is unsupported",
            Self::MissingDependency => "authority dependency is unavailable",
            Self::InvalidIdentity => "authority requires durable identities",
            Self::InvalidGraph => "authority graph is invalid",
            Self::CyclicGraph => "authority traversal contains a cycle",
            Self::LimitExceeded => "authority evaluation limit exceeded",
            Self::InvalidMutation => "authority mutation states are invalid",
            Self::IncompleteRecord => "authority mutation needs complete Record content",
            Self::Denied => "operation is not permitted",
        })
    }
}

impl Error for AuthorityError {}

/// Reports the operation and exact changes allowed for one Record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDecision {
    pub operation: Operation,
    /// Index of the first matching policy grant.
    pub grant_index: usize,
    pub properties: BTreeSet<Property>,
    pub assertions_added: BTreeSet<String>,
    pub assertions_removed: BTreeSet<String>,
}

/// Declares a Record and properties touched by a mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationTarget {
    pub record_uid: String,
    pub touched_properties: BTreeSet<Property>,
}

/// Describes a declared Assertion transition and the properties it touches.
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionIntent {
    pub before: Option<AssertionState>,
    pub after: Option<AssertionState>,
    pub touched_properties: BTreeSet<AssertionProperty>,
}
