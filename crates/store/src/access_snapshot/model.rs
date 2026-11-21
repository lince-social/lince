use std::collections::{BTreeMap, BTreeSet};

use nucleus::{DecimalValue, RecordKind};
use serde_json::{Map, Value};

/// Row and byte limits for metadata and selected Record content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessSnapshotLimits {
    pub records: usize,
    pub concepts: usize,
    pub concept_parents: usize,
    pub assertions: usize,
    pub places: usize,
    pub roles: usize,
    pub visibility_rules: usize,
    pub linguas: usize,
    pub lingua_concepts: usize,
    pub message_drafts: usize,
    pub targets: usize,
    pub extensions: usize,
    /// Maximum accounted byte length of any measured row.
    pub row_bytes: usize,
    /// Shared byte budget across all metadata tables.
    pub metadata_bytes: usize,
    /// Shared byte budget across targeted Records and their extensions.
    pub content_bytes: usize,
}

impl Default for AccessSnapshotLimits {
    fn default() -> Self {
        Self {
            records: 4096,
            concepts: 4096,
            concept_parents: 65536,
            assertions: 32768,
            places: 4096,
            roles: 4096,
            visibility_rules: 32768,
            linguas: 4096,
            lingua_concepts: 65536,
            message_drafts: 4096,
            targets: 256,
            extensions: 4096,
            row_bytes: 1024 * 1024,
            metadata_bytes: 16 * 1024 * 1024,
            content_bytes: 16 * 1024 * 1024,
        }
    }
}

/// A Record's kind, ownership, and replication metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordMetadata {
    pub uid: String,
    pub kind: RecordKind,
    pub organ_uid: Option<String>,
    pub deleted: bool,
    pub replica_root: Option<String>,
}

/// A Concept's name and owning Organ, when one is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptMetadata {
    pub uid: String,
    pub name: String,
    pub origin_organ: Option<String>,
}

/// A directed edge from a Concept to its parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptParentMetadata {
    pub concept_uid: String,
    pub parent_uid: String,
}

/// The stored roles understood by the authorization snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionRole {
    Ordinary,
    Identity,
}

/// An active Assertion with its exact quantity and endpoint metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionMetadata {
    pub uid: String,
    pub subject_uid: String,
    pub predicate_uid: String,
    pub object_uid: Option<String>,
    pub object_uri: Option<String>,
    pub role: AssertionRole,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
}

/// A Lingua's stored visibility policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinguaVisibility {
    Private,
    Shared,
    Public,
}

/// A Lingua and the Organ responsible for its visibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinguaMetadata {
    pub uid: String,
    pub name: String,
    pub owner_organ: Option<String>,
    pub visibility: LinguaVisibility,
}

/// A Concept's membership in a Lingua.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinguaConceptMetadata {
    pub lingua_uid: String,
    pub concept_uid: String,
}

/// The identity a stored visibility rule applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilitySubject {
    Public,
    Actor(String),
    Organ(String),
    Role(i64),
    /// A stored subject kind the evaluator handles conservatively.
    Unsupported {
        kind: String,
        uid: Option<String>,
    },
}

/// The Record, Concept, or Place a visibility rule covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityTarget {
    Record(String),
    Concept(String),
    Place(String),
}

/// Whether a rule exposes or hides its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityGrant {
    Visible,
    Hidden,
}

/// A visibility grant and its subject, target, and optional field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilityRuleMetadata {
    pub uid: String,
    pub subject: VisibilitySubject,
    pub target: VisibilityTarget,
    pub field: Option<String>,
    pub grant: VisibilityGrant,
}

/// Draft ownership and thread metadata used for access checks.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageDraftMetadata {
    pub record_uid: String,
    pub author_uid: String,
    pub operator_uid: String,
    pub thread_uid: Option<String>,
    pub metadata: Map<String, Value>,
}

/// Complete authorization metadata from a bounded database read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AccessMetadataSnapshot {
    pub records: Vec<RecordMetadata>,
    pub concepts: Vec<ConceptMetadata>,
    pub concept_parents: Vec<ConceptParentMetadata>,
    pub assertions: Vec<AssertionMetadata>,
    /// Maps URI Assertion UIDs to resolved Record UIDs.
    pub resolved_links: BTreeMap<String, String>,
    pub places: BTreeSet<String>,
    pub role_ids: BTreeSet<i64>,
    pub visibility_rules: Vec<VisibilityRuleMetadata>,
    pub linguas: Vec<LinguaMetadata>,
    pub lingua_concepts: Vec<LinguaConceptMetadata>,
    pub message_drafts: Vec<MessageDraftMetadata>,
}

/// The stored version and fields of a targeted Record extension.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionContent {
    pub version: i64,
    pub fields: Map<String, Value>,
}

/// Full content and extensions for a selected Record.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetRecordContent {
    pub uid: String,
    pub kind: RecordKind,
    pub organ_uid: Option<String>,
    pub deleted: bool,
    pub replica_root: Option<String>,
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub quantity: DecimalValue,
    pub unit_uid: Option<String>,
    pub place_uid: Option<String>,
    pub extensions: BTreeMap<String, ExtensionContent>,
}
