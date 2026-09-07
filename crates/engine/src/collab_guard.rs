use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use loro::{
    ContainerID, ContainerType, ExportMode, Frontiers, ID, IdSpan, JsonChange, JsonOp,
    JsonOpContent, JsonSchema, JsonTextOp, LoroDoc, LoroValue, VersionVector,
};
use protein::authority::Property;
use serde::de::{Error as _, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub delta_bytes: usize,
    pub delta_changes: usize,
    pub delta_operations: usize,
    pub delta_atoms: usize,
    pub delta_dependencies: usize,
    pub peers: usize,
    pub text_bytes: usize,
    pub message_bytes: usize,
    pub history_changes: usize,
    pub history_atoms: usize,
    pub history_bytes: usize,
    pub snapshot_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            delta_bytes: 2 * 1024 * 1024,
            delta_changes: 256,
            delta_operations: 4096,
            delta_atoms: 65_536,
            delta_dependencies: 4096,
            peers: 64,
            text_bytes: 1024 * 1024,
            message_bytes: 4096,
            history_changes: 16_384,
            history_atoms: 1_048_576,
            history_bytes: 16 * 1024 * 1024,
            snapshot_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    InvalidLimits,
    Limit(&'static str),
    InvalidJson,
    UnsupportedSchema,
    InvalidId,
    InvalidCounter,
    InvalidDependency,
    InvalidDeleteTarget,
    OverlappingChanges,
    ConflictingOperation,
    UnsupportedContainer,
    UnsupportedOperation,
    Forbidden(Property),
    PendingDependencies,
    UncommittedState,
    DetachedState,
    ShallowState,
    InvalidState,
    ImportRefused,
    ExportRefused,
}

impl fmt::Display for GuardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "collaborative delta refused: {self:?}")
    }
}

impl std::error::Error for GuardError {}

pub struct AcceptedDoc {
    doc: LoroDoc,
    version: VersionVector,
    head: String,
    body: String,
    snapshot: Vec<u8>,
}

pub struct PreparedDelta {
    candidate: AcceptedDoc,
    base_version: VersionVector,
    normalized_delta: Vec<u8>,
    touched_properties: BTreeSet<Property>,
    duplicate: bool,
}

impl AcceptedDoc {
    pub fn empty(limits: &Limits) -> Result<Self, GuardError> {
        Self::from_document(LoroDoc::new(), limits)
    }

    pub fn from_trusted_text(head: &str, body: &str, limits: &Limits) -> Result<Self, GuardError> {
        validate_limits(limits)?;
        let text_bytes = bounded_sum(head.len(), body.len(), limits.text_bytes, "text bytes")?;
        bounded(text_bytes, limits.history_bytes, "history bytes")?;
        let atoms = bounded_sum(
            head.chars().count(),
            body.chars().count(),
            limits.history_atoms,
            "history atoms",
        )?;
        bounded(
            usize::from(atoms != 0),
            limits.history_changes,
            "history changes",
        )?;
        let doc = LoroDoc::new();
        for (root, text) in [("head", head), ("body", body)] {
            let container = doc.get_text(root);
            if !text.is_empty() {
                container
                    .insert(0, text)
                    .map_err(|_| GuardError::InvalidState)?;
            }
        }
        doc.commit();
        Self::from_document(doc, limits)
    }

    pub fn from_trusted_snapshot(bytes: &[u8], limits: &Limits) -> Result<Self, GuardError> {
        validate_limits(limits)?;
        bounded(bytes.len(), limits.snapshot_bytes, "snapshot bytes")?;
        let doc = import_snapshot(bytes)?;
        validate_document(&doc, limits)?;
        let history = doc.export_json_updates_without_peer_compression(
            &VersionVector::default(),
            &doc.oplog_vv(),
        );
        let reconstructed = import_complete(LoroDoc::new(), history)?;
        Self::from_document(reconstructed, limits)
    }

    fn from_document(doc: LoroDoc, limits: &Limits) -> Result<Self, GuardError> {
        validate_limits(limits)?;
        validate_document(&doc, limits)?;
        let head = doc.get_text("head").to_string();
        let body = doc.get_text("body").to_string();
        let snapshot = doc
            .export(ExportMode::Snapshot)
            .map_err(|_| GuardError::ExportRefused)?;
        bounded(snapshot.len(), limits.snapshot_bytes, "snapshot bytes")?;
        Ok(Self {
            version: doc.oplog_vv(),
            doc,
            head,
            body,
            snapshot,
        })
    }

    pub fn head(&self) -> &str {
        &self.head
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn snapshot(&self) -> &[u8] {
        &self.snapshot
    }

    pub fn version(&self) -> VersionVector {
        self.version.clone()
    }

    pub fn prepare_delta(
        &self,
        raw_json: &[u8],
        selected_grant_properties: &BTreeSet<Property>,
        limits: &Limits,
    ) -> Result<PreparedDelta, GuardError> {
        validate_limits(limits)?;
        bounded(raw_json.len(), limits.delta_bytes, "delta bytes")?;
        bounded(self.snapshot.len(), limits.snapshot_bytes, "snapshot bytes")?;
        validate_document(&self.doc, limits)?;
        let raw: RawSchema =
            serde_json::from_slice(raw_json).map_err(|_| GuardError::InvalidJson)?;
        let schema = raw.into_schema(limits)?;
        let base_version = self.version.clone();
        let inspection = inspect_schema(
            &schema,
            selected_grant_properties,
            limits,
            false,
            &base_version,
        )?;
        verify_known_operations(&self.doc, &schema, &base_version)?;
        verify_delete_targets(&self.doc, &schema, &base_version)?;
        let new_atoms = novel_atoms(&schema, &base_version)?;
        let expected_atoms = bounded_sum(
            self.doc.len_ops(),
            new_atoms,
            limits.history_atoms,
            "history atoms",
        )?;
        bounded_sum(
            self.doc.len_changes(),
            schema
                .changes
                .iter()
                .filter(|change| {
                    change.id.counter + change.op_len() as i32
                        > base_version.get(&change.id.peer).copied().unwrap_or(0)
                })
                .count(),
            limits.history_changes,
            "history changes",
        )?;
        let candidate = import_snapshot(&self.snapshot)?;
        if candidate.oplog_vv() != base_version {
            return Err(GuardError::InvalidState);
        }
        let candidate = import_complete(candidate, schema.clone())?;
        if candidate.len_ops() != expected_atoms {
            return Err(GuardError::InvalidState);
        }
        let version = candidate.oplog_vv();
        verify_known_operations(&candidate, &schema, &version)?;
        let normalized =
            candidate.export_json_updates_without_peer_compression(&base_version, &version);
        let normalized_inspection = inspect_schema(
            &normalized,
            selected_grant_properties,
            limits,
            false,
            &base_version,
        )?;
        if !normalized_inspection.touched.is_subset(&inspection.touched) {
            return Err(GuardError::InvalidState);
        }
        let normalized_delta = encode_bounded(&normalized, limits.delta_bytes, "delta bytes")?;
        Ok(PreparedDelta {
            duplicate: version == base_version,
            candidate: Self::from_document(candidate, limits)?,
            base_version,
            normalized_delta,
            touched_properties: inspection.touched,
        })
    }
}

impl PreparedDelta {
    pub fn head(&self) -> &str {
        self.candidate.head()
    }

    pub fn body(&self) -> &str {
        self.candidate.body()
    }

    pub fn snapshot(&self) -> &[u8] {
        self.candidate.snapshot()
    }

    pub fn base_version(&self) -> &VersionVector {
        &self.base_version
    }

    pub fn normalized_delta(&self) -> &[u8] {
        &self.normalized_delta
    }

    pub fn touched_properties(&self) -> &BTreeSet<Property> {
        &self.touched_properties
    }

    pub fn is_duplicate(&self) -> bool {
        self.duplicate
    }

    pub fn into_accepted_after_commit(self) -> AcceptedDoc {
        self.candidate
    }
}

fn validate_limits(limits: &Limits) -> Result<(), GuardError> {
    if limits.history_atoms > i32::MAX as usize
        || limits.delta_atoms > limits.history_atoms
        || limits.delta_changes > limits.history_changes
        || limits.delta_bytes > limits.history_bytes
        || limits.peers == 0
        || limits.history_bytes == 0
        || limits.snapshot_bytes == 0
    {
        return Err(GuardError::InvalidLimits);
    }
    Ok(())
}

fn bounded(value: usize, maximum: usize, name: &'static str) -> Result<(), GuardError> {
    if value > maximum {
        Err(GuardError::Limit(name))
    } else {
        Ok(())
    }
}

fn bounded_sum(
    left: usize,
    right: usize,
    maximum: usize,
    name: &'static str,
) -> Result<usize, GuardError> {
    let sum = left.checked_add(right).ok_or(GuardError::Limit(name))?;
    bounded(sum, maximum, name)?;
    Ok(sum)
}

fn both_properties() -> BTreeSet<Property> {
    BTreeSet::from([Property::Head, Property::Body])
}

fn container_property(container: &ContainerID) -> Result<Property, GuardError> {
    match container {
        ContainerID::Root {
            name,
            container_type: ContainerType::Text,
        } => match name.as_str() {
            "head" => Ok(Property::Head),
            "body" => Ok(Property::Body),
            _ => Err(GuardError::UnsupportedContainer),
        },
        _ => Err(GuardError::UnsupportedContainer),
    }
}

fn isolate_import(
    doc: LoroDoc,
    import: impl FnOnce(&LoroDoc) -> Result<(), GuardError>,
) -> Result<LoroDoc, GuardError> {
    let result =
        catch_unwind(AssertUnwindSafe(|| import(&doc))).unwrap_or(Err(GuardError::ImportRefused));
    match result {
        Ok(()) => Ok(doc),
        Err(error) => {
            let _ = catch_unwind(AssertUnwindSafe(|| drop(doc)));
            Err(error)
        }
    }
}

fn import_snapshot(bytes: &[u8]) -> Result<LoroDoc, GuardError> {
    isolate_import(LoroDoc::new(), |doc| {
        let status = doc.import(bytes).map_err(|_| GuardError::ImportRefused)?;
        if status.pending.is_some_and(|pending| !pending.is_empty()) {
            return Err(GuardError::PendingDependencies);
        }
        Ok(())
    })
}

fn import_complete(doc: LoroDoc, schema: JsonSchema) -> Result<LoroDoc, GuardError> {
    isolate_import(doc, |doc| {
        let status = doc
            .import_json_updates(schema)
            .map_err(|_| GuardError::ImportRefused)?;
        if status.pending.is_some_and(|pending| !pending.is_empty()) {
            return Err(GuardError::PendingDependencies);
        }
        Ok(())
    })
}

fn validate_document(doc: &LoroDoc, limits: &Limits) -> Result<(), GuardError> {
    if doc.get_pending_txn_len() != 0 {
        return Err(GuardError::UncommittedState);
    }
    if doc.is_detached() {
        return Err(GuardError::DetachedState);
    }
    if doc.is_shallow() {
        return Err(GuardError::ShallowState);
    }
    bounded(doc.len_ops(), limits.history_atoms, "history atoms")?;
    bounded(doc.len_changes(), limits.history_changes, "history changes")?;
    bounded(doc.oplog_vv().len(), limits.peers, "peers")?;
    let LoroValue::Map(roots) = doc.get_value() else {
        return Err(GuardError::InvalidState);
    };
    for value in roots.values() {
        let LoroValue::Container(container) = value else {
            return Err(GuardError::InvalidState);
        };
        container_property(container)?;
    }
    bounded_sum(
        doc.get_text("head").len_utf8(),
        doc.get_text("body").len_utf8(),
        limits.text_bytes,
        "text bytes",
    )?;
    let history = doc
        .export_json_updates_without_peer_compression(&VersionVector::default(), &doc.oplog_vv());
    inspect_schema(
        &history,
        &both_properties(),
        limits,
        true,
        &VersionVector::default(),
    )?;
    verify_delete_targets(doc, &history, &doc.oplog_vv())?;
    encode_bounded(&history, limits.history_bytes, "history bytes")?;
    Ok(())
}

struct Inspection {
    touched: BTreeSet<Property>,
}

fn inspect_schema(
    schema: &JsonSchema,
    permitted: &BTreeSet<Property>,
    limits: &Limits,
    history: bool,
    accepted: &VersionVector,
) -> Result<Inspection, GuardError> {
    if schema.schema_version != 1 || schema.peers.is_some() {
        return Err(GuardError::UnsupportedSchema);
    }
    let (max_changes, max_operations, max_atoms, max_dependencies) = if history {
        (
            limits.history_changes,
            limits.history_atoms,
            limits.history_atoms,
            limits.history_bytes,
        )
    } else {
        (
            limits.delta_changes,
            limits.delta_operations,
            limits.delta_atoms,
            limits.delta_dependencies,
        )
    };
    bounded(schema.changes.len(), max_changes, "changes")?;
    let mut peers = accepted.keys().copied().collect::<BTreeSet<_>>();
    for id in schema.start_version.iter() {
        inspect_id(id, limits)?;
        peers.insert(id.peer);
        if id.counter >= accepted.get(&id.peer).copied().unwrap_or(0) {
            return Err(GuardError::InvalidDependency);
        }
    }
    bounded(schema.start_version.len(), limits.peers, "frontiers")?;
    let mut operations = 0;
    let mut atoms = 0;
    let mut dependencies = 0;
    let mut touched = BTreeSet::new();
    let mut intervals: BTreeMap<u64, Vec<(i32, i32)>> = BTreeMap::new();
    for change in &schema.changes {
        inspect_id(change.id, limits)?;
        peers.insert(change.id.peer);
        bounded(change.lamport as usize, limits.history_atoms, "lamport")?;
        bounded(
            change.msg.as_ref().map_or(0, String::len),
            limits.message_bytes,
            "message bytes",
        )?;
        if change.ops.is_empty() {
            return Err(GuardError::InvalidCounter);
        }
        dependencies = bounded_sum(
            dependencies,
            change.deps.len(),
            max_dependencies,
            "dependencies",
        )?;
        let mut distinct_deps = BTreeSet::new();
        for dep in &change.deps {
            inspect_id(*dep, limits)?;
            peers.insert(dep.peer);
            if !distinct_deps.insert(dep.peer)
                || (dep.peer == change.id.peer && dep.counter >= change.id.counter)
            {
                return Err(GuardError::InvalidDependency);
            }
        }
        let mut counter = change.id.counter;
        for op in &change.ops {
            operations = bounded_sum(operations, 1, max_operations, "operations")?;
            let property = container_property(&op.container)?;
            if !permitted.contains(&property) {
                return Err(GuardError::Forbidden(property));
            }
            touched.insert(property);
            if op.counter != counter {
                return Err(GuardError::InvalidCounter);
            }
            let len = inspect_text_op(&op.content, limits, &mut peers)?;
            atoms = bounded_sum(atoms, len, max_atoms, "atoms")?;
            counter = counter
                .checked_add(i32::try_from(len).map_err(|_| GuardError::InvalidCounter)?)
                .ok_or(GuardError::InvalidCounter)?;
            bounded(counter as usize, limits.history_atoms, "counter")?;
        }
        intervals
            .entry(change.id.peer)
            .or_default()
            .push((change.id.counter, counter));
    }
    bounded(peers.len(), limits.peers, "peers")?;
    for (peer, spans) in &mut intervals {
        spans.sort_unstable();
        let mut known = accepted.get(peer).copied().unwrap_or(0);
        let mut prior_end = None;
        for &(start, end) in spans.iter() {
            if prior_end.is_some_and(|prior| start < prior) {
                return Err(GuardError::OverlappingChanges);
            }
            if start > known {
                return Err(GuardError::InvalidCounter);
            }
            known = known.max(end);
            prior_end = Some(end);
        }
    }
    Ok(Inspection { touched })
}

fn inspect_id(id: ID, limits: &Limits) -> Result<(), GuardError> {
    if id.counter < 0 {
        return Err(GuardError::InvalidCounter);
    }
    bounded(id.counter as usize, limits.history_atoms, "counter")
}

fn inspect_text_op(
    content: &JsonOpContent,
    limits: &Limits,
    peers: &mut BTreeSet<u64>,
) -> Result<usize, GuardError> {
    let len = match content {
        JsonOpContent::Text(JsonTextOp::Insert { pos, text }) => {
            bounded(text.len(), limits.history_bytes, "insert bytes")?;
            let len = text.chars().count();
            bounded_sum(*pos as usize, len, limits.history_atoms, "position")?;
            len
        }
        JsonOpContent::Text(JsonTextOp::Delete { pos, len, start_id }) => {
            inspect_id(*start_id, limits)?;
            peers.insert(start_id.peer);
            let len_abs = len.unsigned_abs() as usize;
            let (start, end) = if *len > 0 {
                (i64::from(*pos), i64::from(*pos) + i64::from(*len))
            } else {
                (i64::from(*pos) + 1 + i64::from(*len), i64::from(*pos) + 1)
            };
            if start < 0 || end < start {
                return Err(GuardError::InvalidCounter);
            }
            bounded(end as usize, limits.history_atoms, "position")?;
            bounded_sum(
                start_id.counter as usize,
                len_abs,
                limits.history_atoms,
                "delete counter",
            )?;
            len_abs
        }
        _ => return Err(GuardError::UnsupportedOperation),
    };
    if len == 0 {
        return Err(GuardError::InvalidCounter);
    }
    Ok(len)
}

fn novel_atoms(schema: &JsonSchema, accepted: &VersionVector) -> Result<usize, GuardError> {
    schema.changes.iter().try_fold(0usize, |total, change| {
        let end = change
            .id
            .counter
            .checked_add(i32::try_from(change.op_len()).map_err(|_| GuardError::InvalidCounter)?)
            .ok_or(GuardError::InvalidCounter)?;
        let start = change
            .id
            .counter
            .max(accepted.get(&change.id.peer).copied().unwrap_or(0));
        total
            .checked_add(end.saturating_sub(start).max(0) as usize)
            .ok_or(GuardError::InvalidCounter)
    })
}

#[derive(Debug, PartialEq, Eq)]
enum AtomContent {
    Insert { pos: u32, character: char },
    Delete { pos: i32, target: ID },
}

#[derive(Debug, PartialEq, Eq)]
struct Atom {
    counter: i32,
    container: ContainerID,
    content: AtomContent,
    metadata: AtomMetadata,
}

#[derive(Debug, PartialEq, Eq)]
struct AtomMetadata {
    lamport: u32,
    timestamp: i64,
    message: Option<Arc<str>>,
    dependencies: BTreeSet<ID>,
}

fn atom_metadata(
    change: &JsonChange,
    counter: i32,
    message: &Option<Arc<str>>,
) -> Result<AtomMetadata, GuardError> {
    let offset = counter
        .checked_sub(change.id.counter)
        .and_then(|offset| u32::try_from(offset).ok())
        .ok_or(GuardError::InvalidCounter)?;
    Ok(AtomMetadata {
        lamport: change
            .lamport
            .checked_add(offset)
            .ok_or(GuardError::InvalidCounter)?,
        timestamp: change.timestamp,
        message: message.clone(),
        dependencies: if offset == 0 {
            change.deps.iter().copied().collect()
        } else {
            BTreeSet::from([ID::new(change.id.peer, counter - 1)])
        },
    })
}

fn atoms_in(change: &JsonChange, start: i32, end: i32) -> Result<Vec<Atom>, GuardError> {
    let mut atoms = Vec::new();
    let message = change.msg.as_deref().map(Arc::<str>::from);
    for op in &change.ops {
        let from = start.saturating_sub(op.counter).max(0) as usize;
        let to = (end.saturating_sub(op.counter).max(0) as usize).min(op.content.op_len());
        if from >= to {
            continue;
        }
        match &op.content {
            JsonOpContent::Text(JsonTextOp::Insert { pos, text }) => {
                for (index, character) in text.chars().enumerate().take(to).skip(from) {
                    atoms.push(Atom {
                        counter: op.counter + index as i32,
                        container: op.container.clone(),
                        content: AtomContent::Insert {
                            pos: *pos + index as u32,
                            character,
                        },
                        metadata: atom_metadata(change, op.counter + index as i32, &message)?,
                    });
                }
            }
            JsonOpContent::Text(JsonTextOp::Delete { pos, len, start_id }) => {
                for index in from..to {
                    let (position, offset) = if *len > 0 {
                        (*pos, index)
                    } else {
                        (*pos - index as i32, len.unsigned_abs() as usize - index - 1)
                    };
                    atoms.push(Atom {
                        counter: op.counter + index as i32,
                        container: op.container.clone(),
                        content: AtomContent::Delete {
                            pos: position,
                            target: ID::new(start_id.peer, start_id.counter + offset as i32),
                        },
                        metadata: atom_metadata(change, op.counter + index as i32, &message)?,
                    });
                }
            }
            _ => return Err(GuardError::UnsupportedOperation),
        }
    }
    Ok(atoms)
}

fn verify_known_operations(
    doc: &LoroDoc,
    schema: &JsonSchema,
    accepted: &VersionVector,
) -> Result<(), GuardError> {
    for change in &schema.changes {
        let start = change.id.counter;
        let end = (start + change.op_len() as i32)
            .min(accepted.get(&change.id.peer).copied().unwrap_or(0));
        if end <= start {
            continue;
        }
        let submitted = atoms_in(change, start, end)?;
        let mut known = Vec::new();
        for existing in doc.export_json_in_id_span(IdSpan::new(change.id.peer, start, end)) {
            known.extend(atoms_in(&existing, start, end)?);
        }
        known.sort_unstable_by_key(|atom| atom.counter);
        if known != submitted || known.len() != (end - start) as usize {
            return Err(GuardError::ConflictingOperation);
        }
    }
    Ok(())
}

fn verify_delete_targets(
    doc: &LoroDoc,
    schema: &JsonSchema,
    accepted: &VersionVector,
) -> Result<(), GuardError> {
    let mut incoming: BTreeMap<u64, Vec<(i32, i32, &JsonOp)>> = BTreeMap::new();
    for change in &schema.changes {
        let known = accepted.get(&change.id.peer).copied().unwrap_or(0);
        for op in &change.ops {
            let end = op.counter + op.content.op_len() as i32;
            if end > known {
                incoming
                    .entry(change.id.peer)
                    .or_default()
                    .push((op.counter, end, op));
            }
        }
    }
    for spans in incoming.values_mut() {
        spans.sort_unstable_by_key(|span| span.0);
    }
    for change in &schema.changes {
        for op in &change.ops {
            let JsonOpContent::Text(JsonTextOp::Delete { len, start_id, .. }) = &op.content else {
                continue;
            };
            let start = start_id.counter;
            let end = start
                .checked_add(
                    i32::try_from(len.unsigned_abs())
                        .map_err(|_| GuardError::InvalidDeleteTarget)?,
                )
                .ok_or(GuardError::InvalidDeleteTarget)?;
            let known = accepted.get(&start_id.peer).copied().unwrap_or(0);
            let mut covered = Vec::new();
            if start < known {
                for existing in
                    doc.export_json_in_id_span(IdSpan::new(start_id.peer, start, end.min(known)))
                {
                    for origin in &existing.ops {
                        cover_insert(origin, &op.container, start, end.min(known), &mut covered)?;
                    }
                }
            }
            if end > known {
                if let Some(ops) = incoming.get(&start_id.peer) {
                    let first = ops.partition_point(|span| span.1 <= start.max(known));
                    for &(from, to, origin) in ops[first..].iter().take_while(|span| span.0 < end) {
                        if origin.container != op.container
                            || !matches!(
                                origin.content,
                                JsonOpContent::Text(JsonTextOp::Insert { .. })
                            )
                        {
                            return Err(GuardError::InvalidDeleteTarget);
                        }
                        covered.push((from.max(start).max(known), to.min(end)));
                    }
                }
            }
            covered.sort_unstable();
            let mut through = start;
            for (from, to) in covered {
                if from != through {
                    return Err(GuardError::InvalidDeleteTarget);
                }
                through = to;
            }
            if through != end {
                return Err(GuardError::InvalidDeleteTarget);
            }
        }
    }
    Ok(())
}

fn cover_insert(
    origin: &JsonOp,
    container: &ContainerID,
    start: i32,
    end: i32,
    covered: &mut Vec<(i32, i32)>,
) -> Result<(), GuardError> {
    let from = start.max(origin.counter);
    let to = end.min(origin.counter + origin.content.op_len() as i32);
    if from >= to {
        return Ok(());
    }
    if &origin.container != container
        || !matches!(
            origin.content,
            JsonOpContent::Text(JsonTextOp::Insert { .. })
        )
    {
        return Err(GuardError::InvalidDeleteTarget);
    }
    covered.push((from, to));
    Ok(())
}

struct CappedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl Write for CappedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("JSON byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode_bounded(
    value: &impl Serialize,
    maximum: usize,
    name: &'static str,
) -> Result<Vec<u8>, GuardError> {
    let mut writer = CappedWriter {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| GuardError::Limit(name))?;
    Ok(writer.bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSchema {
    schema_version: u8,
    start_version: RawFrontiers,
    peers: (),
    changes: Vec<RawChange>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChange {
    id: String,
    timestamp: i64,
    deps: Vec<String>,
    lamport: u32,
    msg: Option<String>,
    ops: Vec<RawOp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOp {
    container: String,
    content: RawTextOp,
    counter: i32,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum RawTextOp {
    Insert {
        pos: u32,
        text: String,
    },
    Delete {
        pos: i32,
        len: i32,
        start_id: String,
    },
}

struct RawFrontiers(BTreeMap<u64, i32>);

impl<'de> Deserialize<'de> for RawFrontiers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FrontiersVisitor;

        impl<'de> Visitor<'de> for FrontiersVisitor {
            type Value = RawFrontiers;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("unique canonical peer counters")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut frontiers = BTreeMap::new();
                while let Some((peer, counter)) = map.next_entry::<String, i32>()? {
                    let parsed = peer
                        .parse::<u64>()
                        .map_err(|_| A::Error::custom("invalid peer"))?;
                    if parsed.to_string() != peer || frontiers.insert(parsed, counter).is_some() {
                        return Err(A::Error::custom("duplicate or noncanonical peer"));
                    }
                }
                Ok(RawFrontiers(frontiers))
            }
        }

        deserializer.deserialize_map(FrontiersVisitor)
    }
}

fn parse_id(raw: &str, limits: &Limits) -> Result<ID, GuardError> {
    if raw.len() > 32 {
        return Err(GuardError::InvalidId);
    }
    let id = ID::try_from(raw).map_err(|_| GuardError::InvalidId)?;
    if id.to_string() != raw {
        return Err(GuardError::InvalidId);
    }
    inspect_id(id, limits)?;
    Ok(id)
}

impl RawSchema {
    fn into_schema(self, limits: &Limits) -> Result<JsonSchema, GuardError> {
        let Self {
            schema_version,
            start_version,
            peers: (),
            changes,
        } = self;
        if schema_version != 1 {
            return Err(GuardError::UnsupportedSchema);
        }
        bounded(changes.len(), limits.delta_changes, "changes")?;
        bounded(start_version.0.len(), limits.peers, "frontiers")?;
        let mut operation_count = 0;
        let mut dependency_count = 0;
        let mut converted = Vec::with_capacity(changes.len());
        for change in changes {
            operation_count = bounded_sum(
                operation_count,
                change.ops.len(),
                limits.delta_operations,
                "operations",
            )?;
            dependency_count = bounded_sum(
                dependency_count,
                change.deps.len(),
                limits.delta_dependencies,
                "dependencies",
            )?;
            let mut ops = Vec::with_capacity(change.ops.len());
            for op in change.ops {
                let container = match op.container.as_str() {
                    "cid:root-head:Text" => ContainerID::new_root("head", ContainerType::Text),
                    "cid:root-body:Text" => ContainerID::new_root("body", ContainerType::Text),
                    _ => return Err(GuardError::UnsupportedContainer),
                };
                let content = match op.content {
                    RawTextOp::Insert { pos, text } => JsonTextOp::Insert { pos, text },
                    RawTextOp::Delete { pos, len, start_id } => JsonTextOp::Delete {
                        pos,
                        len,
                        start_id: parse_id(&start_id, limits)?,
                    },
                };
                ops.push(JsonOp {
                    content: JsonOpContent::Text(content),
                    container,
                    counter: op.counter,
                });
            }
            converted.push(JsonChange {
                id: parse_id(&change.id, limits)?,
                timestamp: change.timestamp,
                deps: change
                    .deps
                    .iter()
                    .map(|raw| parse_id(raw, limits))
                    .collect::<Result<_, _>>()?,
                lamport: change.lamport,
                msg: change.msg,
                ops,
            });
        }
        Ok(JsonSchema {
            schema_version,
            start_version: Frontiers::from_iter(
                start_version
                    .0
                    .into_iter()
                    .map(|(peer, counter)| ID::new(peer, counter)),
            ),
            peers: None,
            changes: converted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn accepted(head: &str, body: &str) -> AcceptedDoc {
        let doc = LoroDoc::new();
        doc.set_peer_id(1).unwrap();
        if !head.is_empty() {
            doc.get_text("head").insert(0, head).unwrap();
        }
        if !body.is_empty() {
            doc.get_text("body").insert(0, body).unwrap();
        }
        doc.commit();
        AcceptedDoc::from_trusted_snapshot(
            &doc.export(ExportMode::Snapshot).unwrap(),
            &Limits::default(),
        )
        .unwrap()
    }

    fn client(accepted: &AcceptedDoc, peer: u64) -> LoroDoc {
        let doc = LoroDoc::new();
        doc.import(accepted.snapshot()).unwrap();
        doc.set_peer_id(peer).unwrap();
        doc
    }

    fn delta(doc: &LoroDoc, base: &VersionVector) -> Vec<u8> {
        doc.commit();
        serde_json::to_vec(&doc.export_json_updates_without_peer_compression(base, &doc.oplog_vv()))
            .unwrap()
    }

    fn body_only() -> BTreeSet<Property> {
        BTreeSet::from([Property::Body])
    }

    fn insertion() -> Value {
        json!({
            "schema_version": 1,
            "start_version": {},
            "peers": null,
            "changes": [{
                "id": "0@7", "timestamp": 0, "deps": [], "lamport": 0, "msg": null,
                "ops": [{
                    "container": "cid:root-body:Text", "counter": 0,
                    "content": {"type": "insert", "pos": 0, "text": "x"}
                }]
            }]
        })
    }

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    fn refusal(
        accepted: &AcceptedDoc,
        raw: &[u8],
        permitted: &BTreeSet<Property>,
        limits: &Limits,
    ) -> GuardError {
        let version = accepted.version();
        let actual_version = accepted.doc.oplog_vv();
        let head = accepted.head().to_owned();
        let body = accepted.body().to_owned();
        let state = accepted.doc.get_deep_value();
        let snapshot = accepted.snapshot().to_vec();
        let error = match accepted.prepare_delta(raw, permitted, limits) {
            Ok(_) => panic!("expected refusal"),
            Err(error) => error,
        };
        assert_eq!(accepted.version(), version);
        assert_eq!(accepted.doc.oplog_vv(), actual_version);
        assert_eq!(accepted.head(), head);
        assert_eq!(accepted.body(), body);
        assert_eq!(accepted.doc.get_deep_value(), state);
        assert_eq!(accepted.snapshot(), snapshot);
        assert_eq!(accepted.doc.get_pending_txn_len(), 0);
        error
    }

    #[test]
    fn collab_guard_body_candidate_is_provisional_until_consumed() {
        let accepted = accepted("Title", "old");
        let client = client(&accepted, 7);
        client.get_text("body").insert(3, " new").unwrap();
        let prepared = accepted
            .prepare_delta(
                &delta(&client, &accepted.version()),
                &body_only(),
                &Limits::default(),
            )
            .unwrap();
        assert_eq!(accepted.body(), "old");
        assert_eq!(prepared.head(), "Title");
        assert_eq!(prepared.body(), "old new");
        assert_eq!(prepared.touched_properties(), &body_only());
        assert_eq!(prepared.base_version(), &accepted.version());
        assert!(!prepared.is_duplicate());
        let replacement = prepared.into_accepted_after_commit();
        assert_eq!(replacement.body(), "old new");
        assert_eq!(accepted.body(), "old");
    }

    #[test]
    fn collab_guard_concurrent_real_loro_body_edits_converge() {
        let base = accepted("T", "abc");
        let alice = client(&base, 2);
        let bob = client(&base, 3);
        alice.get_text("body").insert(1, "😀").unwrap();
        bob.get_text("body").insert(1, "é").unwrap();
        let alice_delta = delta(&alice, &base.version());
        let bob_delta = delta(&bob, &base.version());
        let alice_first = base
            .prepare_delta(&alice_delta, &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        let both_a = alice_first
            .prepare_delta(&bob_delta, &body_only(), &Limits::default())
            .unwrap();
        let bob_first = base
            .prepare_delta(&bob_delta, &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        let both_b = bob_first
            .prepare_delta(&alice_delta, &body_only(), &Limits::default())
            .unwrap();
        assert_eq!(both_a.body(), both_b.body());
        assert!(both_a.body().contains('😀'));
        assert!(both_a.body().contains('é'));
        assert_eq!(both_a.candidate.version(), both_b.candidate.version());
        assert_eq!(base.body(), "abc");
    }

    #[test]
    fn collab_guard_exact_duplicate_keeps_footprint_and_has_no_new_operations() {
        let base = accepted("", "");
        let raw = bytes(&insertion());
        let updated = base
            .prepare_delta(&raw, &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        let duplicate = updated
            .prepare_delta(&raw, &body_only(), &Limits::default())
            .unwrap();
        assert!(duplicate.is_duplicate());
        assert_eq!(duplicate.body(), "x");
        assert_eq!(duplicate.touched_properties(), &body_only());
        let normalized: JsonSchema = serde_json::from_slice(duplicate.normalized_delta()).unwrap();
        assert!(normalized.changes.is_empty());
        assert_eq!(duplicate.candidate.version(), updated.version());
    }

    #[test]
    fn collab_guard_forbidden_head_and_mixed_batches_leave_accepted_unchanged() {
        let base = accepted("title", "body");
        let client = client(&base, 7);
        client.get_text("head").insert(0, "new ").unwrap();
        client.get_text("body").insert(0, "new ").unwrap();
        assert_eq!(
            refusal(
                &base,
                &delta(&client, &base.version()),
                &body_only(),
                &Limits::default(),
            ),
            GuardError::Forbidden(Property::Head)
        );
    }

    #[test]
    fn collab_guard_canceled_head_edit_still_requires_head_permission() {
        let base = accepted("title", "body");
        let client = client(&base, 7);
        client.get_text("head").insert(0, "x").unwrap();
        client.commit();
        client.get_text("head").delete(0, 1).unwrap();
        client.get_text("body").insert(0, "new ").unwrap();
        let raw = delta(&client, &base.version());
        assert_eq!(client.get_text("head").to_string(), "title");
        assert_eq!(
            refusal(&base, &raw, &body_only(), &Limits::default()),
            GuardError::Forbidden(Property::Head)
        );
        let permitted = base
            .prepare_delta(&raw, &both_properties(), &Limits::default())
            .unwrap();
        assert_eq!(permitted.head(), "title");
        assert_eq!(permitted.touched_properties(), &both_properties());
    }

    #[test]
    fn collab_guard_empty_grant_never_authorizes_duplicate_writes() {
        let raw = bytes(&insertion());
        let base = accepted("", "")
            .prepare_delta(&raw, &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        assert_eq!(
            refusal(&base, &raw, &BTreeSet::new(), &Limits::default()),
            GuardError::Forbidden(Property::Body)
        );
    }

    #[test]
    fn collab_guard_unknown_nested_and_wrong_type_containers_are_refused() {
        let base = accepted("", "");
        for container in [
            "cid:root-secret:Text",
            "cid:root-body:Map",
            "cid:0@7:Text",
            "cid:root-body/child:Text",
            "cid:root-Head:Text",
        ] {
            let mut raw = insertion();
            raw["changes"][0]["ops"][0]["container"] = json!(container);
            assert_eq!(
                refusal(&base, &bytes(&raw), &both_properties(), &Limits::default()),
                GuardError::UnsupportedContainer
            );
        }
    }

    #[test]
    fn collab_guard_styles_and_nontext_operations_are_refused() {
        let base = accepted("", "");
        for content in [
            json!({"type": "mark_end"}),
            json!({"type": "mark", "start": 0, "end": 1, "style_key": "bold", "style_value": true, "info": 0}),
            json!({"type": "insert", "pos": 0, "value": ["x"]}),
            json!({"type": "set", "key": "head", "value": "x"}),
        ] {
            let mut raw = insertion();
            raw["changes"][0]["ops"][0]["content"] = content;
            assert_eq!(
                refusal(&base, &bytes(&raw), &both_properties(), &Limits::default()),
                GuardError::InvalidJson
            );
        }
    }

    #[test]
    fn collab_guard_unknown_fields_at_every_schema_level_are_refused() {
        let base = accepted("", "");
        for path in [
            "",
            "/changes/0",
            "/changes/0/ops/0",
            "/changes/0/ops/0/content",
        ] {
            let mut raw = insertion();
            raw.pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), json!(true));
            assert_eq!(
                refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
                GuardError::InvalidJson
            );
        }
    }

    #[test]
    fn collab_guard_duplicate_fields_including_null_and_frontiers_are_refused() {
        let base = accepted("", "");
        let raw = String::from_utf8(bytes(&insertion())).unwrap();
        for (from, to) in [
            (
                "\"schema_version\":1",
                "\"schema_version\":1,\"schema_version\":1",
            ),
            ("\"peers\":null", "\"peers\":null,\"peers\":null"),
            ("\"msg\":null", "\"msg\":null,\"msg\":null"),
            ("\"counter\":0", "\"counter\":0,\"counter\":0"),
            ("\"text\":\"x\"", "\"text\":\"x\",\"text\":\"y\""),
            (
                "\"type\":\"insert\"",
                "\"type\":\"insert\",\"type\":\"insert\"",
            ),
            (
                "\"start_version\":{}",
                "\"start_version\":{\"7\":0,\"7\":0}",
            ),
        ] {
            assert!(raw.contains(from));
            assert_eq!(
                refusal(
                    &base,
                    raw.replacen(from, to, 1).as_bytes(),
                    &body_only(),
                    &Limits::default(),
                ),
                GuardError::InvalidJson
            );
        }
    }

    #[test]
    fn collab_guard_binary_snapshots_updates_and_non_json_are_not_client_deltas() {
        let base = accepted("title", "body");
        let client = client(&base, 7);
        client.get_text("body").insert(0, "x").unwrap();
        for raw in [
            base.snapshot().to_vec(),
            client.export(ExportMode::all_updates()).unwrap(),
            b"{broken".to_vec(),
            vec![0xff, 0xfe],
            b"null".to_vec(),
            b"[]".to_vec(),
        ] {
            assert_eq!(
                refusal(&base, &raw, &body_only(), &Limits::default()),
                GuardError::InvalidJson
            );
        }
    }

    #[test]
    fn collab_guard_version_and_peer_compression_are_refused() {
        let base = accepted("", "");
        let mut raw = insertion();
        raw["schema_version"] = json!(2);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::UnsupportedSchema
        );
        raw = insertion();
        raw["peers"] = json!(["7"]);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::InvalidJson
        );
    }

    #[test]
    fn collab_guard_missing_dependencies_never_remain_in_accepted_state() {
        let base = accepted("", "");
        let mut raw = insertion();
        raw["changes"][0]["deps"] = json!(["0@99"]);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::PendingDependencies
        );
        let mut later = insertion();
        later["changes"][0]["id"] = json!("0@99");
        later["changes"][0]["ops"][0]["content"]["text"] = json!("z");
        let prepared = base
            .prepare_delta(&bytes(&later), &body_only(), &Limits::default())
            .unwrap();
        assert_eq!(prepared.body(), "z");
        assert_eq!(prepared.candidate.doc.len_ops(), 1);
    }

    #[test]
    fn collab_guard_pending_head_cannot_be_smuggled_through_body_permission() {
        let base = accepted("", "");
        let mut raw = insertion();
        raw["changes"][0]["deps"] = json!(["0@99"]);
        raw["changes"][0]["ops"][0]["container"] = json!("cid:root-head:Text");
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::Forbidden(Property::Head)
        );
    }

    #[test]
    fn collab_guard_bad_ids_counters_and_noncontiguous_operations_are_refused() {
        let base = accepted("", "");
        for id in ["-1@7", "00@7", "0@+7", "0@07", "0@18446744073709551616"] {
            let mut raw = insertion();
            raw["changes"][0]["id"] = json!(id);
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default());
        }
        for counter in [-1, 1, i32::MAX] {
            let mut raw = insertion();
            raw["changes"][0]["ops"][0]["counter"] = json!(counter);
            assert_eq!(
                refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
                GuardError::InvalidCounter
            );
        }
        let mut raw = insertion();
        raw["changes"][0]["id"] = json!("1@7");
        raw["changes"][0]["ops"][0]["counter"] = json!(1);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::InvalidCounter
        );
    }

    #[test]
    fn collab_guard_zero_length_overflow_and_invalid_delete_ranges_are_refused() {
        let base = accepted("", "");
        for content in [
            json!({"type":"insert", "pos":0, "text":""}),
            json!({"type":"insert", "pos":u32::MAX, "text":"x"}),
            json!({"type":"delete", "pos":0, "len":0, "start_id":"0@1"}),
            json!({"type":"delete", "pos":0, "len":i32::MIN, "start_id":"0@1"}),
            json!({"type":"delete", "pos":-1, "len":1, "start_id":"0@1"}),
            json!({"type":"delete", "pos":0, "len":-2, "start_id":"0@1"}),
            json!({"type":"delete", "pos":0, "len":1, "start_id":"2147483647@1"}),
        ] {
            let mut raw = insertion();
            raw["changes"][0]["ops"][0]["content"] = content;
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default());
        }
        let mut raw = insertion();
        raw["changes"][0]["ops"] = json!([]);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::InvalidCounter
        );
        let mut raw = insertion();
        raw["changes"][0]["id"] = json!("2147483647@7");
        raw["changes"][0]["ops"][0]["counter"] = json!(i32::MAX);
        assert_eq!(
            refusal(
                &base,
                &bytes(&raw),
                &body_only(),
                &Limits {
                    history_atoms: i32::MAX as usize,
                    ..Limits::default()
                },
            ),
            GuardError::InvalidCounter
        );
    }

    #[test]
    fn collab_guard_overlapping_changes_and_dependency_cycles_are_refused() {
        let base = accepted("", "");
        let mut raw = insertion();
        let change = raw["changes"][0].clone();
        raw["changes"].as_array_mut().unwrap().push(change);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::OverlappingChanges
        );
        let mut raw = insertion();
        raw["changes"][0]["deps"] = json!(["0@7"]);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::InvalidDependency
        );
    }

    #[test]
    fn collab_guard_malformed_text_positions_are_refused_without_poisoning_accepted() {
        let base = accepted("", "");
        let mut raw = insertion();
        raw["changes"][0]["ops"][0]["content"]["pos"] = json!(100);
        refusal(&base, &bytes(&raw), &body_only(), &Limits::default());
        raw["changes"][0]["ops"][0]["content"] =
            json!({"type":"delete", "pos":0, "len":1, "start_id":"0@99"});
        refusal(&base, &bytes(&raw), &body_only(), &Limits::default());
        let valid = base
            .prepare_delta(&bytes(&insertion()), &body_only(), &Limits::default())
            .unwrap();
        assert_eq!(valid.body(), "x");
    }

    #[test]
    fn collab_guard_split_unicode_operation_replay_matches_known_atoms() {
        let base = accepted("", "");
        let mut raw = insertion();
        raw["changes"][0]["ops"][0]["content"]["text"] = json!("a😀é");
        let updated = base
            .prepare_delta(&bytes(&raw), &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        let first = raw["changes"][0]["ops"][0].clone();
        let mut second = first.clone();
        raw["changes"][0]["ops"][0]["content"]["text"] = json!("a");
        second["counter"] = json!(1);
        second["content"]["pos"] = json!(1);
        second["content"]["text"] = json!("😀é");
        raw["changes"][0]["ops"]
            .as_array_mut()
            .unwrap()
            .push(second);
        let duplicate = updated
            .prepare_delta(&bytes(&raw), &body_only(), &Limits::default())
            .unwrap();
        assert!(duplicate.is_duplicate());
        assert_eq!(duplicate.body(), "a😀é");
    }

    #[test]
    fn collab_guard_changed_payload_at_known_loro_id_is_a_conflict_not_saved() {
        let raw = bytes(&insertion());
        let base = accepted("", "")
            .prepare_delta(&raw, &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        let mut altered = insertion();
        altered["changes"][0]["ops"][0]["content"]["text"] = json!("y");
        assert_eq!(
            refusal(&base, &bytes(&altered), &body_only(), &Limits::default()),
            GuardError::ConflictingOperation
        );
        altered = insertion();
        altered["changes"][0]["ops"][0]["container"] = json!("cid:root-head:Text");
        assert_eq!(
            refusal(
                &base,
                &bytes(&altered),
                &both_properties(),
                &Limits::default()
            ),
            GuardError::ConflictingOperation
        );
    }

    #[test]
    fn collab_guard_unicode_partial_overlap_compares_atoms_not_bytes_or_utf16() {
        let base = accepted("", "");
        let client = client(&base, 7);
        client.get_text("body").insert(0, "a😀").unwrap();
        let first_delta = delta(&client, &base.version());
        let first = base
            .prepare_delta(&first_delta, &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        client.get_text("body").insert(2, "éZ").unwrap();
        let full = delta(&client, &base.version());
        let prepared = first
            .prepare_delta(&full, &body_only(), &Limits::default())
            .unwrap();
        assert_eq!(prepared.body(), "a😀éZ");
        assert_eq!(prepared.candidate.doc.len_ops(), 4);
        let duplicate = prepared
            .into_accepted_after_commit()
            .prepare_delta(&full, &body_only(), &Limits::default())
            .unwrap();
        assert!(duplicate.is_duplicate());
        let mut altered: Value = serde_json::from_slice(&full).unwrap();
        altered["changes"][0]["ops"][0]["content"]["text"] = json!("a😈");
        assert_eq!(
            refusal(&first, &bytes(&altered), &body_only(), &Limits::default()),
            GuardError::ConflictingOperation
        );
    }

    #[test]
    fn collab_guard_reverse_delete_and_partial_replay_preserve_target_ids() {
        let base = accepted("", "a😀éZ");
        let client = client(&base, 7);
        client.get_text("body").delete(3, 1).unwrap();
        client.get_text("body").delete(2, 1).unwrap();
        let first_raw = delta(&client, &base.version());
        let first = base
            .prepare_delta(&first_raw, &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        assert_eq!(first.body(), "a😀");
        client.get_text("body").delete(1, 1).unwrap();
        let full = delta(&client, &base.version());
        let schema: JsonSchema = serde_json::from_slice(&full).unwrap();
        assert!(schema.changes.iter().flat_map(|c| &c.ops).any(|op| {
            matches!(&op.content, JsonOpContent::Text(JsonTextOp::Delete { len, .. }) if *len < 0)
        }));
        let prepared = first
            .prepare_delta(&full, &body_only(), &Limits::default())
            .unwrap();
        assert_eq!(prepared.body(), "a");
        assert!(
            prepared
                .into_accepted_after_commit()
                .prepare_delta(&full, &body_only(), &Limits::default())
                .unwrap()
                .is_duplicate()
        );
    }

    #[test]
    fn collab_guard_input_bytes_are_bounded_before_json_parsing() {
        let base = accepted("", "");
        let limits = Limits {
            delta_bytes: 1,
            ..Limits::default()
        };
        assert_eq!(
            refusal(&base, b"not JSON", &body_only(), &limits),
            GuardError::Limit("delta bytes")
        );
    }

    #[test]
    fn collab_guard_preimport_change_operation_atom_and_metadata_limits() {
        let base = accepted("", "");
        let raw = bytes(&insertion());
        for (limits, expected) in [
            (
                Limits {
                    delta_changes: 0,
                    ..Limits::default()
                },
                "changes",
            ),
            (
                Limits {
                    delta_operations: 0,
                    ..Limits::default()
                },
                "operations",
            ),
            (
                Limits {
                    delta_atoms: 0,
                    ..Limits::default()
                },
                "atoms",
            ),
        ] {
            assert_eq!(
                refusal(&base, &raw, &body_only(), &limits),
                GuardError::Limit(expected)
            );
        }
        let mut raw = insertion();
        raw["changes"][0]["msg"] = json!("xx");
        assert_eq!(
            refusal(
                &base,
                &bytes(&raw),
                &body_only(),
                &Limits {
                    message_bytes: 1,
                    ..Limits::default()
                }
            ),
            GuardError::Limit("message bytes")
        );
    }

    #[test]
    fn collab_guard_preimport_dependency_peer_frontier_and_counter_limits() {
        let base = accepted("", "");
        let mut raw = insertion();
        raw["changes"][0]["deps"] = json!(["0@99"]);
        assert_eq!(
            refusal(
                &base,
                &bytes(&raw),
                &body_only(),
                &Limits {
                    delta_dependencies: 0,
                    ..Limits::default()
                }
            ),
            GuardError::Limit("dependencies")
        );
        assert_eq!(
            refusal(
                &base,
                &bytes(&raw),
                &body_only(),
                &Limits {
                    peers: 1,
                    ..Limits::default()
                }
            ),
            GuardError::Limit("peers")
        );
        raw = insertion();
        raw["start_version"] = json!({"7": 0});
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::InvalidDependency
        );
        raw = insertion();
        raw["changes"][0]["lamport"] = json!(u32::MAX);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::Limit("lamport")
        );
    }

    #[test]
    fn collab_guard_output_text_and_history_limits_discard_candidates() {
        let base = accepted("T", "a");
        let client = client(&base, 7);
        client.get_text("body").insert(1, "bc").unwrap();
        let raw = delta(&client, &base.version());
        assert_eq!(
            refusal(
                &base,
                &raw,
                &body_only(),
                &Limits {
                    text_bytes: 3,
                    ..Limits::default()
                }
            ),
            GuardError::Limit("text bytes")
        );
        assert_eq!(
            refusal(
                &base,
                &raw,
                &body_only(),
                &Limits {
                    history_atoms: 3,
                    delta_atoms: 3,
                    ..Limits::default()
                }
            ),
            GuardError::Limit("history atoms")
        );
    }

    #[test]
    fn collab_guard_full_snapshot_retains_history_and_does_not_reset_budget() {
        let base = accepted("", "");
        let client = client(&base, 7);
        client.get_text("body").insert(0, "xx").unwrap();
        client.get_text("body").delete(0, 2).unwrap();
        let prepared = base
            .prepare_delta(
                &delta(&client, &base.version()),
                &body_only(),
                &Limits::default(),
            )
            .unwrap();
        assert_eq!(prepared.body(), "");
        let loaded =
            AcceptedDoc::from_trusted_snapshot(prepared.snapshot(), &Limits::default()).unwrap();
        assert_eq!(loaded.doc.len_ops(), 4);
        let limits = Limits {
            history_atoms: 3,
            delta_atoms: 3,
            ..Limits::default()
        };
        assert!(matches!(
            AcceptedDoc::from_trusted_snapshot(prepared.snapshot(), &limits),
            Err(GuardError::Limit("history atoms"))
        ));
    }

    #[test]
    fn collab_guard_snapshot_constructor_refuses_unsupported_history_and_shallow_state() {
        let doc = LoroDoc::new();
        doc.get_text("secret").insert(0, "x").unwrap();
        doc.get_text("secret").delete(0, 1).unwrap();
        doc.commit();
        assert!(matches!(
            AcceptedDoc::from_trusted_snapshot(
                &doc.export(ExportMode::Snapshot).unwrap(),
                &Limits::default()
            ),
            Err(GuardError::UnsupportedContainer)
        ));
        let base = accepted("title", "body");
        let shallow = base.doc.export(ExportMode::StateOnly(None)).unwrap();
        assert!(matches!(
            AcceptedDoc::from_trusted_snapshot(&shallow, &Limits::default()),
            Err(GuardError::ShallowState)
        ));
    }

    #[test]
    fn collab_guard_detached_and_uncommitted_states_are_refused_before_implicit_commit() {
        let base = accepted("", "body");
        base.doc.detach();
        assert!(matches!(
            base.prepare_delta(&bytes(&insertion()), &body_only(), &Limits::default()),
            Err(GuardError::DetachedState)
        ));
        let base = accepted("", "body");
        let before = base.version();
        base.doc.get_text("body").insert(0, "pending").unwrap();
        let pending_len = base.doc.get_pending_txn_len();
        let pending_version = base.doc.oplog_vv();
        assert!(pending_len > 0);
        assert!(matches!(
            base.prepare_delta(&bytes(&insertion()), &body_only(), &Limits::default()),
            Err(GuardError::UncommittedState)
        ));
        assert_eq!(base.doc.get_pending_txn_len(), pending_len);
        assert_eq!(base.doc.oplog_vv(), pending_version);
        assert_eq!(base.version(), before);
    }

    #[test]
    fn collab_guard_normalized_delta_and_trusted_snapshot_reconstruct_candidate() {
        let base = accepted("T", "a");
        let editor = client(&base, 7);
        editor.get_text("body").insert(1, "😀").unwrap();
        let prepared = base
            .prepare_delta(
                &delta(&editor, &base.version()),
                &body_only(),
                &Limits::default(),
            )
            .unwrap();
        let receiver = client(&base, 9);
        let schema: JsonSchema = serde_json::from_slice(prepared.normalized_delta()).unwrap();
        receiver.import_json_updates(schema).unwrap();
        assert_eq!(receiver.get_text("body").to_string(), prepared.body());
        let loaded =
            AcceptedDoc::from_trusted_snapshot(prepared.snapshot(), &Limits::default()).unwrap();
        assert_eq!(loaded.body(), prepared.body());
        assert_eq!(loaded.version(), prepared.candidate.version());
    }

    #[test]
    fn collab_guard_body_delete_cannot_name_accepted_head_atoms_in_either_direction() {
        for reverse in [false, true] {
            let base = accepted("H😀", "bc");
            let editor = client(&base, 7);
            if reverse {
                editor.get_text("body").delete(1, 1).unwrap();
                editor.get_text("body").delete(0, 1).unwrap();
            } else {
                editor.get_text("body").delete(0, 1).unwrap();
            }
            let mut raw: Value = serde_json::from_slice(&delta(&editor, &base.version())).unwrap();
            raw["changes"][0]["ops"][0]["content"]["start_id"] = json!("0@1");
            if reverse {
                assert!(
                    raw["changes"][0]["ops"][0]["content"]["len"]
                        .as_i64()
                        .unwrap()
                        < 0
                );
            }
            assert_eq!(
                refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
                GuardError::InvalidDeleteTarget
            );
        }
    }

    #[test]
    fn collab_guard_delete_targets_must_be_same_root_incoming_insertions() {
        let base = accepted("", "");
        let mut raw = insertion();
        raw["changes"][0]["ops"] = json!([
            {"container":"cid:root-head:Text", "counter":0, "content":{"type":"insert", "pos":0, "text":"H"}},
            {"container":"cid:root-body:Text", "counter":1, "content":{"type":"insert", "pos":0, "text":"B"}},
            {"container":"cid:root-body:Text", "counter":2, "content":{"type":"delete", "pos":0, "len":1, "start_id":"0@7"}}
        ]);
        assert_eq!(
            refusal(&base, &bytes(&raw), &both_properties(), &Limits::default()),
            GuardError::InvalidDeleteTarget
        );
        raw["changes"][0]["ops"][2]["content"]["start_id"] = json!("1@7");
        let valid = base
            .prepare_delta(&bytes(&raw), &both_properties(), &Limits::default())
            .unwrap();
        assert_eq!(valid.head(), "H");
        assert_eq!(valid.body(), "");
        assert_eq!(valid.touched_properties(), &both_properties());
        let updated = valid.into_accepted_after_commit();
        let mut invalid = insertion();
        invalid["changes"][0]["id"] = json!("0@8");
        invalid["changes"][0]["deps"] = json!(["2@7"]);
        invalid["changes"][0]["lamport"] = json!(3);
        invalid["changes"][0]["ops"][0]["content"] =
            json!({"type":"delete", "pos":0, "len":1, "start_id":"2@7"});
        assert_eq!(
            refusal(&updated, &bytes(&invalid), &body_only(), &Limits::default()),
            GuardError::InvalidDeleteTarget
        );
    }

    #[test]
    fn collab_guard_known_change_metadata_conflicts_are_not_successful_retries() {
        let base = accepted("", "")
            .prepare_delta(&bytes(&insertion()), &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        for (field, value) in [
            ("timestamp", json!(1)),
            ("lamport", json!(1)),
            ("msg", json!("changed")),
            ("deps", json!(["0@99"])),
        ] {
            let mut raw = insertion();
            raw["changes"][0][field] = value;
            assert_eq!(
                refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
                GuardError::ConflictingOperation
            );
        }
    }

    #[test]
    fn collab_guard_new_metadata_must_survive_normalization_exactly() {
        let base = accepted("", "");
        let mut raw = insertion();
        raw["changes"][0]["lamport"] = json!(42);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::ConflictingOperation
        );
        raw = insertion();
        raw["changes"][0]["timestamp"] = json!(7);
        raw["changes"][0]["msg"] = json!("retained");
        let updated = base
            .prepare_delta(&bytes(&raw), &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        assert!(
            updated
                .prepare_delta(&bytes(&raw), &body_only(), &Limits::default())
                .unwrap()
                .is_duplicate()
        );
    }

    #[test]
    fn collab_guard_retained_timestamp_metadata_matches_normalized_history() {
        let mut first = insertion();
        first["changes"][0]["timestamp"] = json!(10);
        let base = accepted("", "")
            .prepare_delta(&bytes(&first), &body_only(), &Limits::default())
            .unwrap()
            .into_accepted_after_commit();
        let mut next = insertion();
        next["changes"][0]["id"] = json!("1@7");
        next["changes"][0]["deps"] = json!(["0@7"]);
        next["changes"][0]["lamport"] = json!(1);
        next["changes"][0]["timestamp"] = json!(11);
        next["changes"][0]["ops"][0]["counter"] = json!(1);
        next["changes"][0]["ops"][0]["content"]["pos"] = json!(1);
        let prepared = base
            .prepare_delta(&bytes(&next), &body_only(), &Limits::default())
            .unwrap();
        assert_eq!(prepared.body(), "xx");
        let normalized: JsonSchema = serde_json::from_slice(prepared.normalized_delta()).unwrap();
        assert_eq!(normalized.changes.len(), 1);
        assert_eq!(normalized.changes[0].timestamp, 11);
        assert_eq!(normalized.changes[0].lamport, 1);
        assert_eq!(normalized.changes[0].deps, vec![ID::new(7, 0)]);
        let updated = prepared.into_accepted_after_commit();
        assert!(
            updated
                .prepare_delta(&bytes(&next), &body_only(), &Limits::default())
                .unwrap()
                .is_duplicate()
        );
    }

    #[test]
    fn collab_guard_valid_first_invalid_second_import_cannot_poison_accepted() {
        let base = accepted("", "");
        let mut raw = insertion();
        let mut second = raw["changes"][0].clone();
        second["id"] = json!("0@8");
        second["deps"] = json!(["0@7"]);
        second["lamport"] = json!(1);
        second["ops"][0]["content"]["pos"] = json!(100);
        raw["changes"].as_array_mut().unwrap().push(second);
        assert_eq!(
            refusal(&base, &bytes(&raw), &body_only(), &Limits::default()),
            GuardError::ImportRefused
        );
        let valid = base
            .prepare_delta(&bytes(&insertion()), &body_only(), &Limits::default())
            .unwrap();
        assert_eq!(valid.body(), "x");
        assert_eq!(valid.candidate.doc.len_ops(), 1);
    }

    #[test]
    fn collab_guard_poisoned_candidate_cleanup_releases_owned_resources() {
        for _ in 0..3 {
            let doc = LoroDoc::new();
            let lifetime = Arc::new(());
            let weak = Arc::downgrade(&lifetime);
            doc.subscribe_root(Arc::new(move |_| {
                let _ = &lifetime;
            }))
            .detach();
            let mut raw = insertion();
            raw["changes"][0]["ops"][0]["content"]["pos"] = json!(100);
            let schema: JsonSchema = serde_json::from_value(raw).unwrap();
            assert!(weak.upgrade().is_some());
            assert!(matches!(
                import_complete(doc, schema),
                Err(GuardError::ImportRefused)
            ));
            assert!(weak.upgrade().is_none());
        }
        let base = accepted("", "");
        let valid = base
            .prepare_delta(&bytes(&insertion()), &body_only(), &Limits::default())
            .unwrap();
        assert_eq!(valid.body(), "x");
    }

    #[test]
    fn collab_guard_invalid_limit_configurations_fail_before_preparation() {
        let base = accepted("", "");
        for limits in [
            Limits {
                peers: 0,
                ..Limits::default()
            },
            Limits {
                history_atoms: i32::MAX as usize + 1,
                ..Limits::default()
            },
            Limits {
                delta_atoms: 1_048_577,
                ..Limits::default()
            },
            Limits {
                delta_changes: 16_385,
                ..Limits::default()
            },
            Limits {
                delta_bytes: 17 * 1024 * 1024,
                ..Limits::default()
            },
            Limits {
                history_bytes: 0,
                ..Limits::default()
            },
            Limits {
                snapshot_bytes: 0,
                ..Limits::default()
            },
        ] {
            assert_eq!(
                refusal(&base, &bytes(&insertion()), &body_only(), &limits),
                GuardError::InvalidLimits
            );
        }
    }

    #[test]
    fn collab_guard_candidate_history_change_and_byte_overflow_is_recoverable() {
        let base = accepted("", &"a".repeat(2048));
        let editor = client(&base, 7);
        editor.get_text("body").insert(0, "x").unwrap();
        let raw = delta(&editor, &base.version());
        assert_eq!(
            refusal(
                &base,
                &raw,
                &body_only(),
                &Limits {
                    history_changes: 1,
                    delta_changes: 1,
                    ..Limits::default()
                }
            ),
            GuardError::Limit("history changes")
        );
        let history = base.doc.export_json_updates_without_peer_compression(
            &VersionVector::default(),
            &base.version(),
        );
        let history_bytes = serde_json::to_vec(&history).unwrap().len();
        assert!(raw.len() < history_bytes);
        assert_eq!(
            refusal(
                &base,
                &raw,
                &body_only(),
                &Limits {
                    history_bytes,
                    delta_bytes: history_bytes,
                    ..Limits::default()
                }
            ),
            GuardError::Limit("history bytes")
        );
        assert!(
            base.prepare_delta(&raw, &body_only(), &Limits::default())
                .is_ok()
        );
    }

    #[test]
    fn collab_guard_candidate_snapshot_and_normalized_output_expansion_are_bounded() {
        let base = accepted("", "ab");
        let editor = client(&base, 7);
        let text: String = (0x400..0x800).filter_map(char::from_u32).collect();
        editor.get_text("body").insert(0, &text).unwrap();
        let raw = delta(&editor, &base.version());
        let unrestricted = base
            .prepare_delta(&raw, &body_only(), &Limits::default())
            .unwrap();
        assert!(unrestricted.snapshot().len() > base.snapshot().len());
        assert_eq!(
            refusal(
                &base,
                &raw,
                &body_only(),
                &Limits {
                    snapshot_bytes: unrestricted.snapshot().len() - 1,
                    ..Limits::default()
                }
            ),
            GuardError::Limit("snapshot bytes")
        );
        let raw = bytes(&insertion());
        let unrestricted = base
            .prepare_delta(&raw, &body_only(), &Limits::default())
            .unwrap();
        assert!(unrestricted.normalized_delta().len() > raw.len());
        assert_eq!(
            refusal(
                &base,
                &raw,
                &body_only(),
                &Limits {
                    delta_bytes: raw.len(),
                    ..Limits::default()
                }
            ),
            GuardError::Limit("delta bytes")
        );
    }
}
