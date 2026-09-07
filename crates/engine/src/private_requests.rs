use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use nucleus::{DecimalValue, RecordKind};
use protein::authority::{AssertionRole, ExtensionProperty, Operation, Property};
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::private_json;

pub const REQUEST_VERSION: u16 = 1;
pub const DIGEST_DOMAIN: &[u8] = b"lince.private.ordinary-request.v1\0";

#[derive(Debug, Clone, Copy)]
pub struct RequestLimits {
    pub bytes: usize,
    pub canonical_bytes: usize,
    pub depth: usize,
    pub nodes: usize,
    pub string_bytes: usize,
    pub commands: usize,
    pub subjects: usize,
    pub changes_per_record: usize,
}

impl Default for RequestLimits {
    fn default() -> Self {
        Self {
            bytes: 1024 * 1024,
            canonical_bytes: 1024 * 1024,
            depth: 32,
            nodes: 32768,
            string_bytes: 256 * 1024,
            commands: 256,
            subjects: 256,
            changes_per_record: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    InvalidLimits,
    LimitExceeded,
    InvalidJson,
    InvalidRequest,
    InvalidIdentity,
    ScopeMismatch,
    UnsupportedVersion,
    UnsupportedKind,
    ProtectedProperty,
    ContradictoryCommands,
    RevisionCoverage,
}

impl From<private_json::Error> for RequestError {
    fn from(value: private_json::Error) -> Self {
        match value {
            private_json::Error::InvalidLimits => Self::InvalidLimits,
            private_json::Error::LimitExceeded => Self::LimitExceeded,
            private_json::Error::InvalidJson => Self::InvalidJson,
        }
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Private request refused: {self:?}")
    }
}

impl std::error::Error for RequestError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldChange<T> {
    Set { value: T },
    Clear {},
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum QuantityChange {
    Set {
        #[serde(deserialize_with = "decimal")]
        value: DecimalValue,
    },
    Clear {},
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionChange {
    pub namespace: String,
    pub field: String,
    pub change: FieldChange<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "property", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordChange {
    Head {
        value: String,
    },
    Body {
        value: String,
    },
    Slug {
        change: FieldChange<String>,
    },
    Quantity {
        #[serde(deserialize_with = "decimal")]
        value: DecimalValue,
    },
    Unit {
        change: FieldChange<String>,
    },
    Place {
        change: FieldChange<String>,
    },
    Extension(ExtensionChange),
}

impl RecordChange {
    pub fn property(&self) -> Property {
        match self {
            Self::Head { .. } => Property::Head,
            Self::Body { .. } => Property::Body,
            Self::Slug { .. } => Property::Slug,
            Self::Quantity { .. } => Property::Quantity,
            Self::Unit { .. } => Property::Unit,
            Self::Place { .. } => Property::Place,
            Self::Extension(change) => Property::Extension(change.property()),
        }
    }
}

impl ExtensionChange {
    pub fn property(&self) -> ExtensionProperty {
        ExtensionProperty {
            namespace: self.namespace.clone(),
            field: self.field.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    CreateRecord {
        uid: String,
        kind: RecordKind,
        head: String,
        body: String,
        #[serde(deserialize_with = "decimal")]
        quantity: DecimalValue,
        #[serde(default)]
        slug: Option<String>,
        #[serde(default)]
        unit_uid: Option<String>,
        #[serde(default)]
        place_uid: Option<String>,
        #[serde(default)]
        extensions: Vec<ExtensionChange>,
    },
    UpdateRecord {
        uid: String,
        changes: Vec<RecordChange>,
    },
    DeleteRecord {
        uid: String,
    },
    RestoreRecord {
        uid: String,
    },
    InsertAssertion {
        uid: String,
        subject_uid: String,
        predicate_uid: String,
        #[serde(default)]
        object_uid: Option<String>,
        role: AssertionRole,
        #[serde(default, deserialize_with = "optional_decimal")]
        quantity: Option<DecimalValue>,
        #[serde(default)]
        unit_uid: Option<String>,
    },
    RetractAssertion {
        uid: String,
        expected_subject_uid: String,
    },
    SetAssertionQuantity {
        uid: String,
        expected_subject_uid: String,
        quantity: QuantityChange,
        unit: FieldChange<String>,
    },
    PromoteIdentity {
        uid: String,
        expected_subject_uid: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedRevision {
    pub record_uid: String,
    pub revision: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u16,
    expected_organ_uid: String,
    expected_person_uid: String,
    operation_uid: String,
    expected_revisions: Vec<ExpectedRevision>,
    commands: Vec<Command>,
}

pub struct SubjectIntent {
    operation: Operation,
    properties: BTreeSet<Property>,
    assertion_commands: Vec<usize>,
    expected_revision: Option<i64>,
}

impl SubjectIntent {
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn properties(&self) -> &BTreeSet<Property> {
        &self.properties
    }
    pub fn assertion_commands(&self) -> &[usize] {
        &self.assertion_commands
    }
    pub fn expected_revision(&self) -> Option<i64> {
        self.expected_revision
    }
    pub fn requires_stored_kind_check(&self) -> bool {
        self.operation != Operation::Create
    }
    pub fn requires_protein_kind(&self) -> bool {
        self.properties.iter().any(|property| matches!(property, Property::Extension(value) if value.namespace == "lince.protein"))
    }
}

#[derive(Default)]
pub struct ReferenceRequirements {
    records: BTreeSet<String>,
    concepts: BTreeSet<String>,
    places: BTreeSet<String>,
}

impl ReferenceRequirements {
    pub fn records(&self) -> &BTreeSet<String> {
        &self.records
    }
    pub fn concepts(&self) -> &BTreeSet<String> {
        &self.concepts
    }
    pub fn places(&self) -> &BTreeSet<String> {
        &self.places
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionExpectation {
    pub assertion_uid: String,
    pub expected_subject_uid: String,
}

pub struct ValidatedRequest {
    envelope: Envelope,
    subjects: BTreeMap<String, SubjectIntent>,
    references: ReferenceRequirements,
    assertion_expectations: Vec<AssertionExpectation>,
    canonical: Vec<u8>,
    digest: [u8; 32],
}

impl ValidatedRequest {
    pub fn expected_organ_uid(&self) -> &str {
        &self.envelope.expected_organ_uid
    }
    pub fn expected_person_uid(&self) -> &str {
        &self.envelope.expected_person_uid
    }
    pub fn operation_uid(&self) -> &str {
        &self.envelope.operation_uid
    }
    pub fn commands(&self) -> &[Command] {
        &self.envelope.commands
    }
    pub fn expected_revisions(&self) -> &[ExpectedRevision] {
        &self.envelope.expected_revisions
    }
    pub fn subjects(&self) -> &BTreeMap<String, SubjectIntent> {
        &self.subjects
    }
    pub fn references(&self) -> &ReferenceRequirements {
        &self.references
    }
    pub fn assertion_expectations(&self) -> &[AssertionExpectation] {
        &self.assertion_expectations
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    pub fn require_scope(&self, organ_uid: &str, person_uid: &str) -> Result<(), RequestError> {
        uid(organ_uid, "r")?;
        uid(person_uid, "r")?;
        if organ_uid != self.expected_organ_uid() || person_uid != self.expected_person_uid() {
            return Err(RequestError::ScopeMismatch);
        }
        Ok(())
    }
}

pub fn ordinary_kind(kind: RecordKind) -> bool {
    matches!(kind, RecordKind::Plain | RecordKind::Protein)
}

pub fn decode(
    bytes: &[u8],
    expected_organ_uid: &str,
    expected_person_uid: &str,
    limits: &RequestLimits,
) -> Result<ValidatedRequest, RequestError> {
    limits.validate()?;
    let json_limits = limits.json();
    let value = private_json::decode(bytes, &json_limits)?;
    let envelope: Envelope =
        serde_json::from_value(value).map_err(|_| RequestError::InvalidRequest)?;
    let mut request = validate(envelope, limits)?;
    request.require_scope(expected_organ_uid, expected_person_uid)?;
    let value =
        serde_json::to_value(&request.envelope).map_err(|_| RequestError::InvalidRequest)?;
    let canonical =
        private_json::canonical_normalized_envelope_output_bytes(&value, limits.canonical_bytes)?;
    request.digest = Sha256::new()
        .chain_update(DIGEST_DOMAIN)
        .chain_update(&canonical)
        .finalize()
        .into();
    request.canonical = canonical;
    Ok(request)
}

impl RequestLimits {
    fn validate(&self) -> Result<(), RequestError> {
        let json = self.json();
        json.validate().map_err(RequestError::from)?;
        let hard = Self::default();
        let pairs = [
            (self.commands, hard.commands),
            (self.subjects, hard.subjects),
            (self.changes_per_record, hard.changes_per_record),
        ];
        if pairs
            .iter()
            .any(|(value, maximum)| *value == 0 || value > maximum)
        {
            return Err(RequestError::InvalidLimits);
        }
        Ok(())
    }

    fn json(&self) -> private_json::Limits {
        private_json::Limits {
            bytes: self.bytes,
            canonical_bytes: self.canonical_bytes,
            depth: self.depth,
            nodes: self.nodes,
            string_bytes: self.string_bytes,
        }
    }
}

fn uid(value: &str, prefix: &str) -> Result<(), RequestError> {
    if !nucleus::valid_uid(value, prefix)
        || value
            .as_bytes()
            .get(prefix.len() + 1)
            .is_none_or(|byte| *byte > b'7')
    {
        return Err(RequestError::InvalidIdentity);
    }
    Ok(())
}

fn valid_slug(value: &str) -> Result<(), RequestError> {
    if value.len() > 200 || !nucleus::valid_slug(value) {
        return Err(RequestError::InvalidRequest);
    }
    Ok(())
}

fn extension(change: &ExtensionChange) -> Result<(), RequestError> {
    let name = &change.namespace;
    let field = &change.field;
    if name.len() > 200
        || !nucleus::valid_slug(name)
        || field.is_empty()
        || field.len() > 200
        || field.trim() != field
        || field.contains('.')
        || field.chars().any(char::is_control)
    {
        return Err(RequestError::InvalidRequest);
    }
    if name == "lince.protein" {
        if !matches!(
            field.as_str(),
            "source" | "where" | "fields" | "include" | "aggregate" | "order" | "limit"
        ) {
            return Err(RequestError::ProtectedProperty);
        }
    } else if name == "lince"
        || name.starts_with("lince.")
        || name == "communication"
        || name.starts_with("communication.")
    {
        return Err(RequestError::ProtectedProperty);
    }
    Ok(())
}

fn optional_reference(
    value: &Option<String>,
    prefix: &str,
    into: &mut BTreeSet<String>,
) -> Result<(), RequestError> {
    if let Some(value) = value {
        uid(value, prefix)?;
        into.insert(value.clone());
    }
    Ok(())
}

fn changed_reference(
    change: &FieldChange<String>,
    prefix: &str,
    into: &mut BTreeSet<String>,
) -> Result<(), RequestError> {
    if let FieldChange::Set { value } = change {
        uid(value, prefix)?;
        into.insert(value.clone());
    }
    Ok(())
}

fn validate(
    mut envelope: Envelope,
    limits: &RequestLimits,
) -> Result<ValidatedRequest, RequestError> {
    if envelope.version != REQUEST_VERSION {
        return Err(RequestError::UnsupportedVersion);
    }
    uid(&envelope.expected_organ_uid, "r")?;
    uid(&envelope.expected_person_uid, "r")?;
    uid(&envelope.operation_uid, "op")?;
    if envelope.commands.is_empty() {
        return Err(RequestError::InvalidRequest);
    }
    if envelope.commands.len() > limits.commands
        || envelope.expected_revisions.len() > limits.subjects
    {
        return Err(RequestError::LimitExceeded);
    }
    let mut subjects = BTreeMap::new();
    let mut references = ReferenceRequirements::default();
    for command in &envelope.commands {
        let (record_uid, operation, properties) = match command {
            Command::CreateRecord {
                uid: record_uid,
                kind,
                head: _,
                body: _,
                quantity: _,
                slug,
                unit_uid,
                place_uid,
                extensions,
            } => {
                if !ordinary_kind(*kind) {
                    return Err(RequestError::UnsupportedKind);
                }
                if extensions.len() > limits.changes_per_record {
                    return Err(RequestError::LimitExceeded);
                }
                let mut properties = BTreeSet::from([
                    Property::Kind,
                    Property::Organ,
                    Property::Head,
                    Property::Body,
                    Property::Quantity,
                ]);
                if let Some(slug) = slug {
                    valid_slug(slug)?;
                    properties.insert(Property::Slug);
                }
                optional_reference(unit_uid, "c", &mut references.concepts)?;
                optional_reference(place_uid, "pl", &mut references.places)?;
                if unit_uid.is_some() {
                    properties.insert(Property::Unit);
                }
                if place_uid.is_some() {
                    properties.insert(Property::Place);
                }
                for change in extensions {
                    extension(change)?;
                    if change.namespace == "lince.protein" && *kind != RecordKind::Protein {
                        return Err(RequestError::UnsupportedKind);
                    }
                    if !matches!(change.change, FieldChange::Set { .. }) {
                        return Err(RequestError::ContradictoryCommands);
                    }
                    if !properties.insert(Property::Extension(change.property())) {
                        return Err(RequestError::ContradictoryCommands);
                    }
                }
                (record_uid, Operation::Create, properties)
            }
            Command::UpdateRecord {
                uid: record_uid,
                changes,
            } => {
                if changes.is_empty() {
                    return Err(RequestError::InvalidRequest);
                }
                if changes.len() > limits.changes_per_record {
                    return Err(RequestError::LimitExceeded);
                }
                let mut properties = BTreeSet::new();
                for change in changes {
                    if !properties.insert(change.property()) {
                        return Err(RequestError::ContradictoryCommands);
                    }
                    match change {
                        RecordChange::Slug {
                            change: FieldChange::Set { value },
                        } => valid_slug(value)?,
                        RecordChange::Unit { change } => {
                            changed_reference(change, "c", &mut references.concepts)?
                        }
                        RecordChange::Place { change } => {
                            changed_reference(change, "pl", &mut references.places)?
                        }
                        RecordChange::Extension(change) => extension(change)?,
                        _ => {}
                    }
                }
                (record_uid, Operation::Update, properties)
            }
            Command::DeleteRecord { uid } => (uid, Operation::Delete, BTreeSet::new()),
            Command::RestoreRecord { uid } => (uid, Operation::Restore, BTreeSet::new()),
            _ => continue,
        };
        uid(record_uid, "r")?;
        if subjects
            .insert(
                record_uid.clone(),
                SubjectIntent {
                    operation,
                    properties,
                    assertion_commands: Vec::new(),
                    expected_revision: None,
                },
            )
            .is_some()
        {
            return Err(RequestError::ContradictoryCommands);
        }
    }
    let mut assertion_ids = BTreeSet::new();
    let mut assertion_tuples = BTreeSet::new();
    let mut identity_subjects = BTreeSet::new();
    let mut assertion_expectations = Vec::new();
    for (index, command) in envelope.commands.iter().enumerate() {
        let (assertion_uid, subject_uid, existing) = match command {
            Command::InsertAssertion {
                uid: assertion_uid,
                subject_uid,
                predicate_uid,
                object_uid,
                role,
                quantity,
                unit_uid,
            } => {
                uid(predicate_uid, "c")?;
                references.concepts.insert(predicate_uid.clone());
                optional_reference(object_uid, "r", &mut references.records)?;
                optional_reference(unit_uid, "c", &mut references.concepts)?;
                if quantity.is_none() && unit_uid.is_some() {
                    return Err(RequestError::InvalidRequest);
                }
                if *role == AssertionRole::Identity {
                    if object_uid.is_some() || quantity.is_some() || unit_uid.is_some() {
                        return Err(RequestError::InvalidRequest);
                    }
                    if !identity_subjects.insert(subject_uid.clone()) {
                        return Err(RequestError::ContradictoryCommands);
                    }
                }
                if !assertion_tuples.insert((subject_uid, predicate_uid, object_uid)) {
                    return Err(RequestError::ContradictoryCommands);
                }
                (assertion_uid, subject_uid, false)
            }
            Command::RetractAssertion {
                uid,
                expected_subject_uid,
            } => (uid, expected_subject_uid, true),
            Command::SetAssertionQuantity {
                uid: assertion_uid,
                expected_subject_uid,
                quantity,
                unit,
            } => {
                if matches!(quantity, QuantityChange::Clear {})
                    && matches!(unit, FieldChange::Set { .. })
                {
                    return Err(RequestError::InvalidRequest);
                }
                changed_reference(unit, "c", &mut references.concepts)?;
                (assertion_uid, expected_subject_uid, true)
            }
            Command::PromoteIdentity {
                uid,
                expected_subject_uid,
            } => {
                if !identity_subjects.insert(expected_subject_uid.clone()) {
                    return Err(RequestError::ContradictoryCommands);
                }
                (uid, expected_subject_uid, true)
            }
            _ => continue,
        };
        uid(assertion_uid, "a")?;
        uid(subject_uid, "r")?;
        if !assertion_ids.insert(assertion_uid) {
            return Err(RequestError::ContradictoryCommands);
        }
        let intent = subjects
            .entry(subject_uid.clone())
            .or_insert_with(|| SubjectIntent {
                operation: Operation::Update,
                properties: BTreeSet::new(),
                assertion_commands: Vec::new(),
                expected_revision: None,
            });
        if matches!(intent.operation, Operation::Delete | Operation::Restore)
            || (existing && intent.operation == Operation::Create)
        {
            return Err(RequestError::ContradictoryCommands);
        }
        intent.assertion_commands.push(index);
        if existing {
            assertion_expectations.push(AssertionExpectation {
                assertion_uid: assertion_uid.clone(),
                expected_subject_uid: subject_uid.clone(),
            });
        }
    }
    if subjects.len() > limits.subjects {
        return Err(RequestError::LimitExceeded);
    }
    envelope
        .expected_revisions
        .sort_by(|left, right| left.record_uid.cmp(&right.record_uid));
    for revision in &envelope.expected_revisions {
        uid(&revision.record_uid, "r")?;
        if revision.revision <= 0 {
            return Err(RequestError::RevisionCoverage);
        }
        let intent = subjects
            .get_mut(&revision.record_uid)
            .ok_or(RequestError::RevisionCoverage)?;
        if intent.operation == Operation::Create
            || intent
                .expected_revision
                .replace(revision.revision)
                .is_some()
        {
            return Err(RequestError::RevisionCoverage);
        }
    }
    if subjects
        .values()
        .any(|intent| intent.operation != Operation::Create && intent.expected_revision.is_none())
    {
        return Err(RequestError::RevisionCoverage);
    }
    Ok(ValidatedRequest {
        envelope,
        subjects,
        references,
        assertion_expectations,
        canonical: Vec::new(),
        digest: [0; 32],
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactDecimal {
    scale: u8,
    value: String,
}

fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DecimalValue, D::Error> {
    let value = ExactDecimal::deserialize(deserializer)?;
    DecimalValue::parse_canonical(value.scale, &value.value)
        .map_err(|_| de::Error::custom("invalid exact decimal"))
}

fn optional_decimal<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<DecimalValue>, D::Error> {
    Option::<ExactDecimal>::deserialize(deserializer)?
        .map(|value| {
            DecimalValue::parse_canonical(value.scale, &value.value)
                .map_err(|_| de::Error::custom("invalid exact decimal"))
        })
        .transpose()
}
