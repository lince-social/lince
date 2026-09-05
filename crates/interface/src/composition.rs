use crate::{
    primitive_gallery::{
        INSTALLED_GALLERY_ABI_JS_PATH, INSTALLED_GALLERY_COMPOSITION_JS_PATH,
        INSTALLED_GALLERY_CSS_PATH, INSTALLED_GALLERY_HTML_PATH, INSTALLED_GALLERY_JS_PATH,
        INSTALLED_GALLERY_PROBES_JS_PATH, primitive_gallery_package,
    },
    sand::{
        AccessibilityRole, AccessibilitySpec, BehaviorBinding, ConfigurationField,
        DeclarativeBehavior, DefinitionChild, DefinitionGraph, DefinitionRef, ExportedPort,
        Isolation, ModuleBehavior, PortDirection, ProjectionKind, ProjectionManifest,
        RendererCapability, RuntimeAdapter, RuntimeSandInstance, SAND_SCHEMA_VERSION,
        SandCapability, SandDefinition, SandElement, SandInstance, SandPackage, SandValue,
        Transform2d, ValueType,
    },
    style::{StyleLayer, StyleValue},
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Display, Formatter},
};

pub const COMPOSITION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionError {
    detail: String,
}

impl CompositionError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for CompositionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for CompositionError {}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionOrigin {
    Rust,
    Maud,
    Workbench,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "lineage", rename_all = "snake_case", deny_unknown_fields)]
pub enum DefinitionLineage {
    CodeOwned {
        constructor: String,
    },
    SavedGroup {
        source_group_uid: String,
    },
    Forked {
        source_uid: String,
        source_revision: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionRevisionRecord {
    pub definition: SandDefinition,
    pub origin: DefinitionOrigin,
    pub lineage: DefinitionLineage,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionCatalog {
    pub schema_version: u32,
    pub revisions: BTreeMap<String, BTreeMap<u64, DefinitionRevisionRecord>>,
    pub active: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionUpdate {
    pub revisions: BTreeMap<String, [u64; 2]>,
}

impl DefinitionCatalog {
    pub fn from_records(
        records: impl IntoIterator<Item = DefinitionRevisionRecord>,
    ) -> Result<Self, CompositionError> {
        let mut catalog = Self {
            schema_version: COMPOSITION_SCHEMA_VERSION,
            revisions: BTreeMap::new(),
            active: BTreeMap::new(),
        };
        for record in records {
            let uid = record.definition.uid.clone();
            let revision = record.definition.revision;
            if catalog
                .revisions
                .entry(uid.clone())
                .or_default()
                .insert(revision, record)
                .is_some()
            {
                return Err(CompositionError::new("duplicate definition revision"));
            }
            catalog.active.insert(uid, revision);
        }
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), CompositionError> {
        if self.schema_version != COMPOSITION_SCHEMA_VERSION {
            return Err(CompositionError::new(
                "unsupported composition catalog schema",
            ));
        }
        if self.active.is_empty() {
            return Err(CompositionError::new("definition catalog is empty"));
        }
        for (uid, revision) in &self.active {
            self.graph_for(&DefinitionRef {
                uid: uid.clone(),
                revision: *revision,
            })?;
        }
        Ok(())
    }

    pub fn active_ref(&self, uid: &str) -> Result<DefinitionRef, CompositionError> {
        self.active
            .get(uid)
            .copied()
            .map(|revision| DefinitionRef {
                uid: uid.into(),
                revision,
            })
            .ok_or_else(|| CompositionError::new(format!("no active definition {uid}")))
    }

    pub fn record(
        &self,
        reference: &DefinitionRef,
    ) -> Result<&DefinitionRevisionRecord, CompositionError> {
        self.revisions
            .get(&reference.uid)
            .and_then(|revisions| revisions.get(&reference.revision))
            .ok_or_else(|| {
                CompositionError::new(format!(
                    "definition {} revision {} is unavailable",
                    reference.uid, reference.revision
                ))
            })
    }

    pub fn definition(
        &self,
        reference: &DefinitionRef,
    ) -> Result<&SandDefinition, CompositionError> {
        Ok(&self.record(reference)?.definition)
    }

    pub fn graph_for(&self, root: &DefinitionRef) -> Result<DefinitionGraph, CompositionError> {
        let mut definitions = BTreeMap::new();
        let mut visiting = BTreeSet::new();
        self.collect_graph(root, &mut definitions, &mut visiting)?;
        let graph = DefinitionGraph {
            schema_version: SAND_SCHEMA_VERSION,
            definitions,
        };
        graph
            .validate()
            .map_err(|error| CompositionError::new(error.to_string()))?;
        Ok(graph)
    }

    fn collect_graph(
        &self,
        reference: &DefinitionRef,
        definitions: &mut BTreeMap<String, SandDefinition>,
        visiting: &mut BTreeSet<String>,
    ) -> Result<(), CompositionError> {
        if let Some(existing) = definitions.get(&reference.uid) {
            if existing.revision != reference.revision {
                return Err(CompositionError::new(format!(
                    "one graph requests two revisions of {}",
                    reference.uid
                )));
            }
            return Ok(());
        }
        if !visiting.insert(reference.uid.clone()) {
            return Err(CompositionError::new("recursive definition cycle"));
        }
        let definition = self.definition(reference)?.clone();
        for child in &definition.children {
            self.collect_graph(&child.definition, definitions, visiting)?;
        }
        visiting.remove(&reference.uid);
        definitions.insert(reference.uid.clone(), definition);
        Ok(())
    }

    pub fn publish_shared(
        &mut self,
        uid: &str,
        mutate: impl FnOnce(&mut SandDefinition),
    ) -> Result<DefinitionUpdate, CompositionError> {
        let mut candidate = self.clone();
        let active = candidate.active_ref(uid)?;
        let mut next = candidate.definition(&active)?.clone();
        mutate(&mut next);
        next.revision = active.revision.saturating_add(1);
        let next_revision = next.revision;
        let previous = candidate.record(&active)?.clone();
        candidate.revisions.entry(uid.into()).or_default().insert(
            next_revision,
            DefinitionRevisionRecord {
                definition: next,
                origin: previous.origin,
                lineage: previous.lineage,
            },
        );
        candidate.active.insert(uid.into(), next_revision);
        let mut revisions = BTreeMap::from([(uid.to_string(), [active.revision, next_revision])]);
        loop {
            let active_snapshot = candidate.active.clone();
            let mut propagated = false;
            for (parent_uid, parent_revision) in active_snapshot {
                if revisions.contains_key(&parent_uid) {
                    continue;
                }
                let reference = DefinitionRef {
                    uid: parent_uid.clone(),
                    revision: parent_revision,
                };
                let record = candidate.record(&reference)?.clone();
                let mut definition = record.definition;
                let mut changed = false;
                for child in &mut definition.children {
                    if let Some([old, new]) = revisions.get(&child.definition.uid)
                        && child.definition.revision == *old
                    {
                        child.definition.revision = *new;
                        changed = true;
                    }
                }
                if changed {
                    let new_revision = parent_revision.saturating_add(1);
                    definition.revision = new_revision;
                    candidate
                        .revisions
                        .entry(parent_uid.clone())
                        .or_default()
                        .insert(
                            new_revision,
                            DefinitionRevisionRecord {
                                definition,
                                origin: record.origin,
                                lineage: record.lineage,
                            },
                        );
                    candidate.active.insert(parent_uid.clone(), new_revision);
                    revisions.insert(parent_uid, [parent_revision, new_revision]);
                    propagated = true;
                }
            }
            if !propagated {
                break;
            }
        }
        candidate.validate()?;
        *self = candidate;
        Ok(DefinitionUpdate { revisions })
    }

    pub fn fork_definition(
        &mut self,
        source_uid: &str,
        fork_uid: &str,
    ) -> Result<DefinitionRef, CompositionError> {
        let source = self.active_ref(source_uid)?;
        self.copy_definition(
            &source,
            fork_uid,
            DefinitionOrigin::Workbench,
            DefinitionLineage::Forked {
                source_uid: source.uid.clone(),
                source_revision: source.revision,
            },
        )
    }

    pub fn save_group_definition(
        &mut self,
        source_uid: &str,
        saved_uid: &str,
        source_group_uid: &str,
    ) -> Result<DefinitionRef, CompositionError> {
        let source = self.active_ref(source_uid)?;
        self.copy_definition(
            &source,
            saved_uid,
            DefinitionOrigin::Workbench,
            DefinitionLineage::SavedGroup {
                source_group_uid: source_group_uid.into(),
            },
        )
    }

    fn copy_definition(
        &mut self,
        source: &DefinitionRef,
        copy_uid: &str,
        origin: DefinitionOrigin,
        lineage: DefinitionLineage,
    ) -> Result<DefinitionRef, CompositionError> {
        if self.revisions.contains_key(copy_uid) {
            return Err(CompositionError::new("copied definition already exists"));
        }
        let mut definition = self.definition(source)?.clone();
        definition.uid = copy_uid.into();
        definition.revision = 1;
        definition.display_name = format!("{} copy", definition.display_name);
        let record = DefinitionRevisionRecord {
            definition,
            origin,
            lineage,
        };
        let mut candidate = self.clone();
        candidate
            .revisions
            .insert(copy_uid.into(), BTreeMap::from([(1, record)]));
        candidate.active.insert(copy_uid.into(), 1);
        candidate.validate()?;
        *self = candidate;
        Ok(DefinitionRef {
            uid: copy_uid.into(),
            revision: 1,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeAddress {
    pub instance_uid: String,
    pub node_path: Vec<String>,
}

impl NodeAddress {
    pub fn key(&self) -> String {
        if self.node_path.is_empty() {
            self.instance_uid.clone()
        } else {
            format!("{}/{}", self.instance_uid, self.node_path.join("/"))
        }
    }

    pub fn port(&self, port: impl Into<String>) -> NodePortAddress {
        NodePortAddress {
            node: self.clone(),
            port: port.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodePortAddress {
    pub node: NodeAddress,
    pub port: String,
}

impl NodePortAddress {
    pub fn label(&self) -> String {
        format!("{}.{}", self.node.key(), self.port)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompositionPlacement {
    pub instance_uid: String,
    pub definition: DefinitionRef,
    pub follow_active: bool,
    pub projection: String,
    pub transform: Transform2d,
    pub locked: bool,
    pub inputs: BTreeMap<String, SandValue>,
    pub configuration: BTreeMap<String, SandValue>,
    pub style: StyleLayer,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "binding", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompositionBinding {
    ProteinRead {
        uid: String,
        protein_uid: String,
        field: String,
        value: SandValue,
        target: NodePortAddress,
    },
    OutputRoute {
        uid: String,
        source: NodePortAddress,
        target: NodePortAddress,
    },
    ActionWrite {
        uid: String,
        source: NodePortAddress,
        action: String,
    },
}

impl CompositionBinding {
    pub fn uid(&self) -> &str {
        match self {
            Self::ProteinRead { uid, .. }
            | Self::OutputRoute { uid, .. }
            | Self::ActionWrite { uid, .. } => uid,
        }
    }

    pub fn visible_arrow(&self) -> String {
        match self {
            Self::ProteinRead {
                protein_uid,
                field,
                target,
                ..
            } => format!("READ  {protein_uid}.{field} ──▶ {}", target.label()),
            Self::OutputRoute { source, target, .. } => {
                format!("EVENT {} ──▶ {}", source.label(), target.label())
            }
            Self::ActionWrite { source, action, .. } => {
                format!("WRITE {} ══▶ Action {action}", source.label())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompositionDocument {
    pub schema_version: u32,
    pub uid: String,
    pub placements: Vec<CompositionPlacement>,
    pub bindings: Vec<CompositionBinding>,
}

impl CompositionDocument {
    pub fn validate(&self, catalog: &DefinitionCatalog) -> Result<(), CompositionError> {
        if self.schema_version != COMPOSITION_SCHEMA_VERSION || self.uid.is_empty() {
            return Err(CompositionError::new("invalid composition document"));
        }
        unique(
            "placement",
            self.placements
                .iter()
                .map(|placement| placement.instance_uid.as_str()),
        )?;
        unique("binding", self.bindings.iter().map(CompositionBinding::uid))?;
        let available = BTreeSet::from([
            RendererCapability::RetainedControls,
            RendererCapability::InstancedNodes,
            RendererCapability::ExternalHtml,
            RendererCapability::BrowserDom,
            RendererCapability::ThreeDimensional,
            RendererCapability::Accessibility,
            RendererCapability::Wasm,
        ]);
        for placement in &self.placements {
            let graph = catalog.graph_for(&placement.definition)?;
            let root = NodeAddress {
                instance_uid: placement.instance_uid.clone(),
                node_path: Vec::new(),
            };
            SandInstance {
                uid: placement.instance_uid.clone(),
                definition: placement.definition.clone(),
                projection: placement.projection.clone(),
                inputs: self.inputs_for(&root),
                configuration: placement.configuration.clone(),
                style: placement.style.clone(),
            }
            .validate(&graph, &available)
            .map_err(|error| CompositionError::new(error.to_string()))?;
        }
        let mut read_targets = BTreeSet::new();
        for placement in &self.placements {
            for port in placement.inputs.keys() {
                let target = self.terminal_input_target(
                    catalog,
                    NodePortAddress {
                        node: NodeAddress {
                            instance_uid: placement.instance_uid.clone(),
                            node_path: Vec::new(),
                        },
                        port: port.clone(),
                    },
                )?;
                if !read_targets.insert(target) {
                    return Err(CompositionError::new(
                        "one Sand input receives more than one value",
                    ));
                }
            }
        }
        for binding in &self.bindings {
            match binding {
                CompositionBinding::ProteinRead { value, target, .. } => {
                    let expected = self.port_type(catalog, target, PortDirection::Input)?;
                    let target = self.terminal_input_target(catalog, target.clone())?;
                    if expected != value.value_type() || !read_targets.insert(target) {
                        return Err(CompositionError::new(
                            "Protein binding type or destination is invalid",
                        ));
                    }
                }
                CompositionBinding::OutputRoute { source, target, .. } => {
                    let output = self.port_type(catalog, source, PortDirection::Output)?;
                    let input = self.port_type(catalog, target, PortDirection::Input)?;
                    if output != input {
                        return Err(CompositionError::new("event route types disagree"));
                    }
                }
                CompositionBinding::ActionWrite { source, action, .. } => {
                    self.port_type(catalog, source, PortDirection::Output)?;
                    let definition = self.definition_at(catalog, &source.node)?;
                    if !definition
                        .capabilities
                        .contains(&SandCapability::RequestAction {
                            action: action.clone(),
                        })
                    {
                        return Err(CompositionError::new(
                            "Action write lacks an exact capability",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn terminal_input_target(
        &self,
        catalog: &DefinitionCatalog,
        mut target: NodePortAddress,
    ) -> Result<NodePortAddress, CompositionError> {
        loop {
            let definition = self.definition_at(catalog, &target.node)?;
            let Some(export) = definition.exports.iter().find(|export| {
                export.direction == PortDirection::Input && export.name == target.port
            }) else {
                return Ok(target);
            };
            target.node.node_path.push(export.child_uid.clone());
            target.port = export.child_port.clone();
        }
    }

    fn placement(&self, uid: &str) -> Result<&CompositionPlacement, CompositionError> {
        self.placements
            .iter()
            .find(|placement| placement.instance_uid == uid)
            .ok_or_else(|| CompositionError::new(format!("unknown placement {uid}")))
    }

    fn definition_at<'a>(
        &self,
        catalog: &'a DefinitionCatalog,
        address: &NodeAddress,
    ) -> Result<&'a SandDefinition, CompositionError> {
        let placement = self.placement(&address.instance_uid)?;
        let mut definition = catalog.definition(&placement.definition)?;
        for local_uid in &address.node_path {
            let child = definition
                .children
                .iter()
                .find(|child| child.local_uid == *local_uid)
                .ok_or_else(|| CompositionError::new("node path names an unknown child"))?;
            definition = catalog.definition(&child.definition)?;
        }
        Ok(definition)
    }

    fn port_type(
        &self,
        catalog: &DefinitionCatalog,
        address: &NodePortAddress,
        direction: PortDirection,
    ) -> Result<ValueType, CompositionError> {
        let definition = self.definition_at(catalog, &address.node)?;
        match direction {
            PortDirection::Input => definition
                .inputs
                .iter()
                .find(|port| port.name == address.port)
                .map(|port| port.value_type),
            PortDirection::Output => definition
                .outputs
                .iter()
                .find(|port| port.name == address.port)
                .map(|port| port.value_type),
        }
        .ok_or_else(|| CompositionError::new("binding names an unknown typed port"))
    }

    fn inputs_for(&self, address: &NodeAddress) -> BTreeMap<String, SandValue> {
        let mut inputs = if address.node_path.is_empty() {
            self.placement(&address.instance_uid)
                .map(|placement| placement.inputs.clone())
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        for binding in &self.bindings {
            if let CompositionBinding::ProteinRead { value, target, .. } = binding
                && target.node == *address
            {
                inputs.insert(target.port.clone(), value.clone());
            }
        }
        inputs
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompositionArtifact {
    pub schema_version: u32,
    pub catalog: DefinitionCatalog,
    pub document: CompositionDocument,
}

impl CompositionArtifact {
    pub fn validate(&self) -> Result<(), CompositionError> {
        if self.schema_version != COMPOSITION_SCHEMA_VERSION {
            return Err(CompositionError::new(
                "unsupported composition artifact schema",
            ));
        }
        self.catalog.validate()?;
        self.document.validate(&self.catalog)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, CompositionError> {
        self.validate()?;
        serde_json::to_vec_pretty(self)
            .map_err(|error| CompositionError::new(format!("serialize composition: {error}")))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, CompositionError> {
        let artifact = serde_json::from_slice::<Self>(bytes)
            .map_err(|error| CompositionError::new(format!("decode composition: {error}")))?;
        artifact.validate()?;
        Ok(artifact)
    }
}

pub fn composition_schema() -> Result<serde_json::Value, CompositionError> {
    serde_json::to_value(schema_for!(CompositionArtifact))
        .map_err(|error| CompositionError::new(format!("serialize composition schema: {error}")))
}

#[derive(Clone, Debug, PartialEq)]
pub struct MountedNode {
    pub address: NodeAddress,
    pub definition: DefinitionRef,
    pub projection: String,
    pub adapter: RuntimeAdapter,
    pub renderer_handle: u64,
    pub dom_uid: String,
    pub behavior_handles: Vec<u64>,
    pub inputs: BTreeMap<String, SandValue>,
    pub configured: bool,
    pub style: StyleLayer,
    pub children: Vec<MountedNode>,
}

impl MountedNode {
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Self::count).sum::<usize>()
    }

    pub fn behavior_count(&self) -> usize {
        self.behavior_handles.len()
            + self
                .children
                .iter()
                .map(Self::behavior_count)
                .sum::<usize>()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CompositionRuntimeFacts {
    pub mounted_placements: usize,
    pub mounted_nodes: usize,
    pub active_behavior_handles: usize,
    pub configured_nodes: usize,
    pub styled_nodes: usize,
    pub emitted_events: u64,
    pub routed_outputs: u64,
    pub action_requests: u64,
    pub teardown_count: u64,
    pub retired_behavior_handles: u64,
    pub rejected_publications: u64,
    pub save_reopens: u64,
    pub lock_changes: u64,
    pub style_override_sets: u64,
    pub style_override_resets: u64,
    pub shared_publications: u64,
    pub saved_definitions: u64,
    pub forked_definitions: u64,
    pub scoped_dom_identities: usize,
}

pub struct CompositionHost {
    pub catalog: DefinitionCatalog,
    pub document: CompositionDocument,
    available: BTreeSet<RendererCapability>,
    mounted: BTreeMap<String, MountedNode>,
    next_handle: u64,
    emitted_events: u64,
    routed_outputs: u64,
    action_requests: u64,
    teardown_count: u64,
    retired_behavior_handles: u64,
    rejected_publications: u64,
    save_reopens: u64,
    lock_changes: u64,
    style_override_sets: u64,
    style_override_resets: u64,
    shared_publications: u64,
    saved_definitions: u64,
    forked_definitions: u64,
    routed_values: BTreeMap<NodePortAddress, SandValue>,
}

impl CompositionHost {
    pub fn open(
        artifact: CompositionArtifact,
        available: BTreeSet<RendererCapability>,
    ) -> Result<Self, CompositionError> {
        artifact.validate()?;
        let mut host = Self {
            catalog: artifact.catalog,
            document: artifact.document,
            available,
            mounted: BTreeMap::new(),
            next_handle: 1,
            emitted_events: 0,
            routed_outputs: 0,
            action_requests: 0,
            teardown_count: 0,
            retired_behavior_handles: 0,
            rejected_publications: 0,
            save_reopens: 0,
            lock_changes: 0,
            style_override_sets: 0,
            style_override_resets: 0,
            shared_publications: 0,
            saved_definitions: 0,
            forked_definitions: 0,
            routed_values: BTreeMap::new(),
        };
        host.mount_all()?;
        Ok(host)
    }

    pub fn artifact(&self) -> CompositionArtifact {
        CompositionArtifact {
            schema_version: COMPOSITION_SCHEMA_VERSION,
            catalog: self.catalog.clone(),
            document: self.document.clone(),
        }
    }

    pub fn save_json(&self) -> Result<Vec<u8>, CompositionError> {
        self.artifact().to_json()
    }

    pub fn reopen(&mut self) -> Result<(), CompositionError> {
        let bytes = self.save_json()?;
        let artifact = CompositionArtifact::from_json(&bytes)?;
        self.teardown();
        self.catalog = artifact.catalog;
        self.document = artifact.document;
        self.save_reopens = self.save_reopens.saturating_add(1);
        self.mount_all()
    }

    pub fn mount_all(&mut self) -> Result<(), CompositionError> {
        self.document.validate(&self.catalog)?;
        let placements = self.document.placements.clone();
        let mut mounted = BTreeMap::new();
        for placement in placements {
            let root = self.mount_node(
                &placement,
                placement.definition.clone(),
                NodeAddress {
                    instance_uid: placement.instance_uid.clone(),
                    node_path: Vec::new(),
                },
                &placement.projection,
                placement.configuration.clone(),
                placement.style.clone(),
                BTreeMap::new(),
            )?;
            mounted.insert(placement.instance_uid, root);
        }
        self.mounted = mounted;
        Ok(())
    }

    fn mount_node(
        &mut self,
        placement: &CompositionPlacement,
        reference: DefinitionRef,
        address: NodeAddress,
        projection_key: &str,
        configuration: BTreeMap<String, SandValue>,
        style: StyleLayer,
        inherited_inputs: BTreeMap<String, SandValue>,
    ) -> Result<MountedNode, CompositionError> {
        let definition = self.catalog.definition(&reference)?.clone();
        let graph = self.catalog.graph_for(&reference)?;
        let projection = definition
            .projections
            .iter()
            .find(|projection| projection.key == projection_key)
            .or_else(|| ProjectionManifest::select(&definition.projections, &self.available).ok())
            .ok_or_else(|| CompositionError::new("no usable renderer projection"))?;
        let projection_key = projection.key.clone();
        let mut inputs = self.document.inputs_for(&address);
        for (name, value) in inherited_inputs {
            if inputs.insert(name, value).is_some() {
                return Err(CompositionError::new(
                    "one Sand input receives more than one value",
                ));
            }
        }
        for input in &definition.inputs {
            if !inputs.contains_key(&input.name)
                && let Some(default) = &input.default
            {
                inputs.insert(input.name.clone(), default.clone());
            }
        }
        let configured = !configuration.is_empty();
        let style = merged_style(&definition.style, &style)?;
        let instance = SandInstance {
            uid: dom_identity(&address, "instance"),
            definition: reference.clone(),
            projection: projection_key.clone(),
            inputs: inputs.clone(),
            configuration,
            style: style.clone(),
        };
        let renderer_handle = self.take_handle();
        let runtime =
            RuntimeSandInstance::mount(&instance, &graph, &self.available, renderer_handle)
                .map_err(|error| CompositionError::new(error.to_string()))?;
        let behavior_handles = definition
            .behaviors
            .iter()
            .map(|_| self.take_handle())
            .collect::<Vec<_>>();
        let mut children = Vec::new();
        let mut ordered = definition.children.clone();
        ordered.sort_by_key(|child| child.sibling_order);
        for child in ordered {
            let child_inputs = definition
                .exports
                .iter()
                .filter(|export| {
                    export.direction == PortDirection::Input && export.child_uid == child.local_uid
                })
                .filter_map(|export| {
                    inputs
                        .get(&export.name)
                        .cloned()
                        .map(|value| (export.child_port.clone(), value))
                })
                .collect::<BTreeMap<_, _>>();
            let mut path = address.node_path.clone();
            path.push(child.local_uid);
            children.push(self.mount_node(
                placement,
                child.definition,
                NodeAddress {
                    instance_uid: address.instance_uid.clone(),
                    node_path: path,
                },
                &projection_key,
                child.configuration,
                child.style,
                child_inputs,
            )?);
        }
        Ok(MountedNode {
            dom_uid: dom_identity(&address, "root"),
            address,
            definition: reference,
            projection: projection_key,
            adapter: runtime.adapter,
            renderer_handle: runtime.renderer_handle,
            behavior_handles,
            inputs,
            configured,
            style,
            children,
        })
    }

    fn take_handle(&mut self) -> u64 {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.saturating_add(1);
        handle
    }

    pub fn teardown(&mut self) {
        if !self.mounted.is_empty() {
            let retired = self
                .mounted
                .values()
                .map(MountedNode::behavior_count)
                .sum::<usize>();
            self.retired_behavior_handles =
                self.retired_behavior_handles.saturating_add(retired as u64);
            self.mounted.clear();
            self.teardown_count = self.teardown_count.saturating_add(1);
        }
    }

    pub fn publish_shared(
        &mut self,
        uid: &str,
        mutate: impl FnOnce(&mut SandDefinition),
    ) -> Result<DefinitionUpdate, CompositionError> {
        match self.catalog.publish_shared(uid, mutate) {
            Ok(update) => {
                for placement in &mut self.document.placements {
                    if placement.follow_active
                        && let Some([old, new]) = update.revisions.get(&placement.definition.uid)
                        && placement.definition.revision == *old
                    {
                        placement.definition.revision = *new;
                    }
                }
                self.teardown();
                self.mount_all()?;
                self.shared_publications = self.shared_publications.saturating_add(1);
                Ok(update)
            }
            Err(error) => {
                self.rejected_publications = self.rejected_publications.saturating_add(1);
                Err(error)
            }
        }
    }

    pub fn fork_placement(
        &mut self,
        source_instance_uid: &str,
        fork_instance_uid: &str,
        fork_definition_uid: &str,
    ) -> Result<(), CompositionError> {
        if self
            .document
            .placements
            .iter()
            .any(|placement| placement.instance_uid == fork_instance_uid)
        {
            return Err(CompositionError::new("fork placement already exists"));
        }
        let source = self.document.placement(source_instance_uid)?.clone();
        let definition = self
            .catalog
            .fork_definition(&source.definition.uid, fork_definition_uid)?;
        let mut placement = source;
        placement.instance_uid = fork_instance_uid.into();
        placement.definition = definition;
        placement.follow_active = true;
        placement.locked = false;
        placement.transform.x += 40.0;
        placement.transform.y += 40.0;
        self.document.placements.push(placement);
        let mut cloned_bindings = Vec::new();
        for binding in &self.document.bindings {
            if let CompositionBinding::ProteinRead {
                uid,
                protein_uid,
                field,
                value,
                target,
            } = binding
                && target.node.instance_uid == source_instance_uid
            {
                let mut target = target.clone();
                target.node.instance_uid = fork_instance_uid.into();
                cloned_bindings.push(CompositionBinding::ProteinRead {
                    uid: format!("{uid}-{fork_instance_uid}"),
                    protein_uid: protein_uid.clone(),
                    field: field.clone(),
                    value: value.clone(),
                    target,
                });
            }
        }
        self.document.bindings.extend(cloned_bindings);
        self.teardown();
        self.mount_all()?;
        self.forked_definitions = self.forked_definitions.saturating_add(1);
        Ok(())
    }

    pub fn save_group_as_definition(
        &mut self,
        source_instance_uid: &str,
        saved_instance_uid: &str,
        saved_definition_uid: &str,
    ) -> Result<(), CompositionError> {
        if self
            .document
            .placements
            .iter()
            .any(|placement| placement.instance_uid == saved_instance_uid)
        {
            return Err(CompositionError::new("saved placement already exists"));
        }
        let source = self.document.placement(source_instance_uid)?.clone();
        let definition = self.catalog.save_group_definition(
            &source.definition.uid,
            saved_definition_uid,
            source_instance_uid,
        )?;
        let mut placement = source;
        placement.instance_uid = saved_instance_uid.into();
        placement.definition = definition;
        placement.follow_active = true;
        placement.locked = true;
        placement.transform.x += 20.0;
        placement.transform.y += 20.0;
        self.document.placements.push(placement);
        let mut cloned_bindings = Vec::new();
        for binding in &self.document.bindings {
            if let CompositionBinding::ProteinRead {
                uid,
                protein_uid,
                field,
                value,
                target,
            } = binding
                && target.node.instance_uid == source_instance_uid
            {
                let mut target = target.clone();
                target.node.instance_uid = saved_instance_uid.into();
                cloned_bindings.push(CompositionBinding::ProteinRead {
                    uid: format!("{uid}-{saved_instance_uid}"),
                    protein_uid: protein_uid.clone(),
                    field: field.clone(),
                    value: value.clone(),
                    target,
                });
            }
        }
        self.document.bindings.extend(cloned_bindings);
        self.teardown();
        self.mount_all()?;
        self.saved_definitions = self.saved_definitions.saturating_add(1);
        Ok(())
    }

    pub fn toggle_lock(&mut self, instance_uid: &str) -> Result<bool, CompositionError> {
        let placement = self
            .document
            .placements
            .iter_mut()
            .find(|placement| placement.instance_uid == instance_uid)
            .ok_or_else(|| CompositionError::new("unknown placement"))?;
        placement.locked = !placement.locked;
        self.lock_changes = self.lock_changes.saturating_add(1);
        Ok(placement.locked)
    }

    pub fn set_instance_style(
        &mut self,
        instance_uid: &str,
        name: impl Into<String>,
        value: StyleValue,
    ) -> Result<(), CompositionError> {
        let name = name.into();
        let index = self
            .document
            .placements
            .iter()
            .position(|placement| placement.instance_uid == instance_uid)
            .ok_or_else(|| CompositionError::new("unknown placement"))?;
        let previous = self.document.placements[index]
            .style
            .values
            .insert(name.clone(), value);
        if let Err(error) = self.document.placements[index].style.validate_standard() {
            match previous {
                Some(value) => {
                    self.document.placements[index]
                        .style
                        .values
                        .insert(name, value);
                }
                None => {
                    self.document.placements[index].style.values.remove(&name);
                }
            }
            return Err(CompositionError::new(error.to_string()));
        }
        self.teardown();
        self.mount_all()?;
        self.style_override_sets = self.style_override_sets.saturating_add(1);
        Ok(())
    }

    pub fn reset_instance_style(
        &mut self,
        instance_uid: &str,
        name: &str,
    ) -> Result<(), CompositionError> {
        let index = self
            .document
            .placements
            .iter()
            .position(|placement| placement.instance_uid == instance_uid)
            .ok_or_else(|| CompositionError::new("unknown placement"))?;
        let removed = self.document.placements[index].style.values.remove(name);
        self.teardown();
        self.mount_all()?;
        if removed.is_some() {
            self.style_override_resets = self.style_override_resets.saturating_add(1);
        }
        Ok(())
    }

    pub fn trigger_output(
        &mut self,
        source: NodePortAddress,
        value: SandValue,
    ) -> Result<(), CompositionError> {
        let expected = self
            .document
            .port_type(&self.catalog, &source, PortDirection::Output)?;
        if expected != value.value_type() {
            return Err(CompositionError::new("trigger output type is incompatible"));
        }
        let mut outputs = vec![source.clone()];
        let definition = self.document.definition_at(&self.catalog, &source.node)?;
        for binding in &definition.behaviors {
            if let BehaviorBinding::Declarative { behavior } = binding {
                match behavior {
                    DeclarativeBehavior::EmitEvent { source_output, .. }
                        if source_output == &source.port =>
                    {
                        self.emitted_events = self.emitted_events.saturating_add(1);
                    }
                    DeclarativeBehavior::SetLocalState {
                        source_output,
                        key,
                        value,
                    } if source_output == &source.port => {
                        self.routed_values
                            .insert(source.node.port(key), value.clone());
                    }
                    DeclarativeBehavior::RequestAction { source_output, .. }
                        if source_output == &source.port =>
                    {
                        self.action_requests = self.action_requests.saturating_add(1);
                    }
                    _ => {}
                }
            }
        }
        let mut current = source;
        while let Some((parent, exported)) = self.parent_export(&current)? {
            current = parent.port(exported);
            outputs.push(current.clone());
        }
        for binding in self.document.bindings.clone() {
            match binding {
                CompositionBinding::OutputRoute { source, target, .. }
                    if outputs.contains(&source) =>
                {
                    self.routed_values.insert(target, value.clone());
                    self.routed_outputs = self.routed_outputs.saturating_add(1);
                }
                CompositionBinding::ActionWrite { source, .. } if outputs.contains(&source) => {
                    self.action_requests = self.action_requests.saturating_add(1);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn parent_export(
        &self,
        source: &NodePortAddress,
    ) -> Result<Option<(NodeAddress, String)>, CompositionError> {
        let Some(child_uid) = source.node.node_path.last() else {
            return Ok(None);
        };
        let mut parent_path = source.node.node_path.clone();
        parent_path.pop();
        let parent = NodeAddress {
            instance_uid: source.node.instance_uid.clone(),
            node_path: parent_path,
        };
        let parent_definition = self.document.definition_at(&self.catalog, &parent)?;
        Ok(parent_definition
            .exports
            .iter()
            .find(|export| {
                export.direction == PortDirection::Output
                    && export.child_uid == *child_uid
                    && export.child_port == source.port
            })
            .map(|export| (parent, export.name.clone())))
    }

    pub fn facts(&self) -> CompositionRuntimeFacts {
        let mounted_nodes = self.mounted.values().map(MountedNode::count).sum();
        let active_behavior_handles = self.mounted.values().map(MountedNode::behavior_count).sum();
        let nodes = self
            .mounted
            .values()
            .flat_map(flatten_nodes)
            .collect::<Vec<_>>();
        let configured_nodes = nodes.iter().filter(|node| node.configured).count();
        let styled_nodes = nodes
            .iter()
            .filter(|node| !node.style.values.is_empty())
            .count();
        let scoped_dom_identities = self
            .mounted
            .values()
            .flat_map(flatten_nodes)
            .map(|node| node.dom_uid.as_str())
            .collect::<BTreeSet<_>>()
            .len();
        CompositionRuntimeFacts {
            mounted_placements: self.mounted.len(),
            mounted_nodes,
            active_behavior_handles,
            configured_nodes,
            styled_nodes,
            emitted_events: self.emitted_events,
            routed_outputs: self.routed_outputs,
            action_requests: self.action_requests,
            teardown_count: self.teardown_count,
            retired_behavior_handles: self.retired_behavior_handles,
            rejected_publications: self.rejected_publications,
            save_reopens: self.save_reopens,
            lock_changes: self.lock_changes,
            style_override_sets: self.style_override_sets,
            style_override_resets: self.style_override_resets,
            shared_publications: self.shared_publications,
            saved_definitions: self.saved_definitions,
            forked_definitions: self.forked_definitions,
            scoped_dom_identities,
        }
    }

    pub fn lineage(&self, uid: &str) -> Result<&DefinitionLineage, CompositionError> {
        let reference = self.catalog.active_ref(uid)?;
        Ok(&self.catalog.record(&reference)?.lineage)
    }

    pub fn adapters(&self) -> Vec<RuntimeAdapter> {
        self.mounted
            .values()
            .flat_map(flatten_nodes)
            .map(|node| node.adapter)
            .collect()
    }

    pub fn mounted_style(&self, address: &NodeAddress) -> Option<&StyleLayer> {
        self.mounted
            .get(&address.instance_uid)
            .and_then(|root| find_node(root, &address.node_path))
            .map(|node| &node.style)
    }

    pub fn mounted_inputs(&self, address: &NodeAddress) -> Option<&BTreeMap<String, SandValue>> {
        self.mounted
            .get(&address.instance_uid)
            .and_then(|root| find_node(root, &address.node_path))
            .map(|node| &node.inputs)
    }
}

impl Drop for CompositionHost {
    fn drop(&mut self) {
        self.teardown();
    }
}

fn flatten_nodes(root: &MountedNode) -> Vec<&MountedNode> {
    let mut nodes = vec![root];
    for child in &root.children {
        nodes.extend(flatten_nodes(child));
    }
    nodes
}

fn find_node<'a>(root: &'a MountedNode, path: &[String]) -> Option<&'a MountedNode> {
    let Some((local_uid, rest)) = path.split_first() else {
        return Some(root);
    };
    root.children
        .iter()
        .find(|child| child.address.node_path.last() == Some(local_uid))
        .and_then(|child| find_node(child, rest))
}

fn merged_style(base: &StyleLayer, patch: &StyleLayer) -> Result<StyleLayer, CompositionError> {
    let mut style = base.clone();
    style.values.extend(patch.values.clone());
    style
        .validate_standard()
        .map_err(|error| CompositionError::new(error.to_string()))?;
    Ok(style)
}

pub fn dom_identity(address: &NodeAddress, local_uid: &str) -> String {
    let mut parts = vec![address.instance_uid.as_str()];
    parts.extend(address.node_path.iter().map(String::as_str));
    parts.push(local_uid);
    parts
        .into_iter()
        .map(|part| {
            part.chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                        character
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("--")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkbenchOperation {
    ActivateNestedButton,
    ToggleLock,
    SetInstanceOverride,
    ResetToInherited,
    PublishSharedEdit,
    RefuseInvalidEdit,
    SaveGroupAsDefinition,
    SaveAndReopen,
    ForkAndDetach,
    TeardownAndRemount,
}

impl WorkbenchOperation {
    pub const ALL: [Self; 10] = [
        Self::ActivateNestedButton,
        Self::ToggleLock,
        Self::SetInstanceOverride,
        Self::ResetToInherited,
        Self::PublishSharedEdit,
        Self::RefuseInvalidEdit,
        Self::SaveGroupAsDefinition,
        Self::SaveAndReopen,
        Self::ForkAndDetach,
        Self::TeardownAndRemount,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ActivateNestedButton => "activate nested Button",
            Self::ToggleLock => "toggle group lock",
            Self::SetInstanceOverride => "set instance radius override",
            Self::ResetToInherited => "reset radius to inherited",
            Self::PublishSharedEdit => "publish shared Button revision",
            Self::RefuseInvalidEdit => "attempt invalid shared revision",
            Self::SaveGroupAsDefinition => "save locked group as definition",
            Self::SaveAndReopen => "save and reopen composition",
            Self::ForkAndDetach => "fork and detach room",
            Self::TeardownAndRemount => "teardown and remount all handles",
        }
    }
}

pub struct CompositionWorkbenchState {
    host: CompositionHost,
    selected: usize,
    last_result: String,
}

impl CompositionWorkbenchState {
    pub fn new() -> Result<Self, CompositionError> {
        let available = BTreeSet::from([
            RendererCapability::RetainedControls,
            RendererCapability::InstancedNodes,
            RendererCapability::ExternalHtml,
            RendererCapability::Accessibility,
        ]);
        Ok(Self {
            host: CompositionHost::open(composition_workbench_fixture()?, available)?,
            selected: 0,
            last_result: "ready".into(),
        })
    }

    pub fn focus_next(&mut self, reverse: bool) {
        self.selected = if reverse {
            self.selected
                .checked_sub(1)
                .unwrap_or(WorkbenchOperation::ALL.len() - 1)
        } else {
            (self.selected + 1) % WorkbenchOperation::ALL.len()
        };
    }

    pub fn selected(&self) -> WorkbenchOperation {
        WorkbenchOperation::ALL[self.selected]
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn activate(&mut self) -> Result<(), CompositionError> {
        let operation = self.selected();
        let result = match operation {
            WorkbenchOperation::ActivateNestedButton => self.host.trigger_output(
                nested_open().port("pressed"),
                SandValue::Record("fixture-record".into()),
            ),
            WorkbenchOperation::ToggleLock => self
                .host
                .toggle_lock("room-native")
                .map(|locked| self.last_result = format!("room lock {locked}")),
            WorkbenchOperation::SetInstanceOverride => self.host.set_instance_style(
                "room-native",
                "--lynx-radius-control",
                StyleValue::LengthPx(12.0),
            ),
            WorkbenchOperation::ResetToInherited => self
                .host
                .reset_instance_style("room-native", "--lynx-radius-control"),
            WorkbenchOperation::PublishSharedEdit => self
                .host
                .publish_shared("button", |definition| {
                    definition.display_name = format!("Button r{}", definition.revision + 1);
                })
                .map(|update| self.last_result = format!("published {:?}", update.revisions)),
            WorkbenchOperation::RefuseInvalidEdit => match self
                .host
                .publish_shared("button", |definition| definition.outputs.clear())
            {
                Ok(_) => Err(CompositionError::new("invalid revision was admitted")),
                Err(_) => {
                    self.last_result = "REFUSED invalid shared revision".into();
                    Ok(())
                }
            },
            WorkbenchOperation::SaveGroupAsDefinition => {
                if self
                    .host
                    .document
                    .placements
                    .iter()
                    .any(|placement| placement.instance_uid == "room-saved")
                {
                    Ok(())
                } else {
                    self.host.save_group_as_definition(
                        "room-native",
                        "room-saved",
                        "saved-video-call-room",
                    )
                }
            }
            WorkbenchOperation::SaveAndReopen => self.host.reopen(),
            WorkbenchOperation::ForkAndDetach => {
                if self
                    .host
                    .document
                    .placements
                    .iter()
                    .any(|placement| placement.instance_uid == "room-fork")
                {
                    Ok(())
                } else {
                    self.host
                        .fork_placement("room-native", "room-fork", "video-call-room-fork")
                }
            }
            WorkbenchOperation::TeardownAndRemount => {
                self.host.teardown();
                self.host.mount_all()
            }
        };
        if result.is_ok()
            && !matches!(
                operation,
                WorkbenchOperation::ToggleLock
                    | WorkbenchOperation::PublishSharedEdit
                    | WorkbenchOperation::RefuseInvalidEdit
            )
        {
            self.last_result = format!("{} complete", operation.label());
        }
        result
    }

    pub fn exercise(&mut self) -> Result<(), CompositionError> {
        for index in 0..WorkbenchOperation::ALL.len() {
            self.selected = index;
            self.activate()?;
        }
        self.selected = 0;
        Ok(())
    }

    pub fn facts(&self) -> CompositionRuntimeFacts {
        self.host.facts()
    }

    pub fn host(&self) -> &CompositionHost {
        &self.host
    }

    pub fn status_line(&self) -> String {
        let facts = self.facts();
        format!(
            "{} · {} placements · {} nested nodes · {} Behavior · {} events · {} Actions · {} teardowns · {} refused",
            self.last_result,
            facts.mounted_placements,
            facts.mounted_nodes,
            facts.active_behavior_handles,
            facts.emitted_events,
            facts.action_requests,
            facts.teardown_count,
            facts.rejected_publications,
        )
    }

    pub fn tree_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "standalone-button · Button@active".into(),
            "room-native · video-call-room@active · LOCKABLE".into(),
            "  └─ call · video-call@active".into(),
            "     ├─ frame · Panel@active".into(),
            "     └─ open · Button@active".into(),
            "room-html · same graph · Installed HTML projection".into(),
        ];
        if self
            .host
            .document
            .placements
            .iter()
            .any(|placement| placement.instance_uid == "room-saved")
        {
            lines.push("room-saved · lineage saved_group · LOCKED".into());
        }
        if self
            .host
            .document
            .placements
            .iter()
            .any(|placement| placement.instance_uid == "room-fork")
        {
            lines.push("room-fork · lineage forked · independent".into());
        }
        lines
    }

    pub fn arrow_lines(&self) -> Vec<String> {
        self.host
            .document
            .bindings
            .iter()
            .map(CompositionBinding::visible_arrow)
            .collect()
    }
}

pub fn composition_workbench_fixture() -> Result<CompositionArtifact, CompositionError> {
    let package = composition_workbench_package();
    package
        .validate()
        .map_err(|error| CompositionError::new(error.to_string()))?;
    let records =
        package
            .graph
            .definitions
            .into_values()
            .map(|definition| DefinitionRevisionRecord {
                lineage: DefinitionLineage::CodeOwned {
                    constructor: format!("native::{}", definition.uid),
                },
                definition,
                origin: DefinitionOrigin::Rust,
            });
    let catalog = DefinitionCatalog::from_records(records)?;
    let document = CompositionDocument {
        schema_version: COMPOSITION_SCHEMA_VERSION,
        uid: "composition-workbench".into(),
        placements: vec![
            placement("standalone-button", "button", "native-retained", 30.0, 50.0),
            locked_placement(
                "room-native",
                "video-call-room",
                "native-retained",
                30.0,
                130.0,
            ),
            placement(
                "room-html",
                "video-call-room",
                "installed-html",
                430.0,
                130.0,
            ),
        ],
        bindings: fixture_bindings(),
    };
    let artifact = CompositionArtifact {
        schema_version: COMPOSITION_SCHEMA_VERSION,
        catalog,
        document,
    };
    artifact.validate()?;
    Ok(artifact)
}

pub fn composition_workbench_package() -> SandPackage {
    let mut package = primitive_gallery_package();
    package.uid = "lince-composition-workbench".into();
    let video_call = video_call_definition();
    let room = video_call_room_definition();
    package
        .graph
        .definitions
        .insert(video_call.uid.clone(), video_call);
    package.graph.definitions.insert(room.uid.clone(), room);
    package
}

fn video_call_definition() -> SandDefinition {
    let frame = definition_child("frame", "panel", 0.0, 0.0, 340.0, 190.0, 0);
    let mut open = definition_child("open", "button", 180.0, 140.0, 140.0, 32.0, 1);
    open.style
        .values
        .insert("--lynx-accent".into(), StyleValue::Color("#5046e5".into()));
    SandDefinition {
        uid: "video-call".into(),
        revision: 1,
        display_name: "Video call".into(),
        element: SandElement::Compound,
        inputs: composable_record_inputs(),
        outputs: vec![crate::sand::OutputPort {
            name: "record-clicked".into(),
            value_type: ValueType::Record,
        }],
        children: vec![frame, open],
        connections: Vec::new(),
        exports: vec![
            ExportedPort {
                name: "label".into(),
                child_uid: "open".into(),
                child_port: "label".into(),
                direction: PortDirection::Input,
            },
            ExportedPort {
                name: "description".into(),
                child_uid: "open".into(),
                child_port: "description".into(),
                direction: PortDirection::Input,
            },
            ExportedPort {
                name: "record".into(),
                child_uid: "open".into(),
                child_port: "record".into(),
                direction: PortDirection::Input,
            },
            ExportedPort {
                name: "record-clicked".into(),
                child_uid: "open".into(),
                child_port: "pressed".into(),
                direction: PortDirection::Output,
            },
        ],
        behaviors: vec![BehaviorBinding::Module {
            module: ModuleBehavior {
                asset: INSTALLED_GALLERY_COMPOSITION_JS_PATH.into(),
                mount_export: "mountComposition".into(),
                teardown_export: "teardownComposition".into(),
                capabilities: BTreeSet::new(),
            },
        }],
        configuration: Vec::new(),
        style: StyleLayer {
            values: BTreeMap::from([("--lynx-gap-content".into(), StyleValue::LengthPx(6.0))]),
        },
        accessibility: AccessibilitySpec {
            role: AccessibilityRole::Group,
            label: "Video call".into(),
            description: Some("A compound Sand built from ordinary primitives".into()),
            live: false,
        },
        capabilities: BTreeSet::new(),
        projections: composition_projections(["root", "frame", "open"]),
    }
}

fn video_call_room_definition() -> SandDefinition {
    let action = SandCapability::RequestAction {
        action: "record.open".into(),
    };
    let mut call = definition_child("call", "video-call", 0.0, 0.0, 360.0, 220.0, 0);
    call.style
        .values
        .insert("--lynx-radius-control".into(), StyleValue::LengthPx(8.0));
    SandDefinition {
        uid: "video-call-room".into(),
        revision: 1,
        display_name: "Video call room".into(),
        element: SandElement::Compound,
        inputs: composable_record_inputs(),
        outputs: vec![crate::sand::OutputPort {
            name: "record-clicked".into(),
            value_type: ValueType::Record,
        }],
        children: vec![call],
        connections: Vec::new(),
        exports: vec![
            ExportedPort {
                name: "label".into(),
                child_uid: "call".into(),
                child_port: "label".into(),
                direction: PortDirection::Input,
            },
            ExportedPort {
                name: "description".into(),
                child_uid: "call".into(),
                child_port: "description".into(),
                direction: PortDirection::Input,
            },
            ExportedPort {
                name: "record".into(),
                child_uid: "call".into(),
                child_port: "record".into(),
                direction: PortDirection::Input,
            },
            ExportedPort {
                name: "record-clicked".into(),
                child_uid: "call".into(),
                child_port: "record-clicked".into(),
                direction: PortDirection::Output,
            },
        ],
        behaviors: Vec::new(),
        configuration: vec![ConfigurationField {
            name: "compact".into(),
            value_type: ValueType::Boolean,
            required: false,
            default: Some(SandValue::Boolean(false)),
        }],
        style: StyleLayer {
            values: BTreeMap::from([("--lynx-radius-control".into(), StyleValue::LengthPx(5.0))]),
        },
        accessibility: AccessibilitySpec {
            role: AccessibilityRole::Group,
            label: "Nested video call room".into(),
            description: Some("A Castle nested through the same Sand graph".into()),
            live: false,
        },
        capabilities: BTreeSet::from([action.clone()]),
        projections: composition_projections_with_capabilities(["root", "call"], [action]),
    }
}

fn composable_record_inputs() -> Vec<crate::sand::InputPort> {
    vec![
        crate::sand::InputPort {
            name: "label".into(),
            value_type: ValueType::Text,
            required: false,
            default: Some(SandValue::Text("Open Record".into())),
        },
        crate::sand::InputPort {
            name: "description".into(),
            value_type: ValueType::Text,
            required: false,
            default: Some(SandValue::Text("Open the bound Record".into())),
        },
        crate::sand::InputPort {
            name: "record".into(),
            value_type: ValueType::Record,
            required: false,
            default: Some(SandValue::Record("fixture-record".into())),
        },
    ]
}

fn composition_projections<const N: usize>(nodes: [&str; N]) -> Vec<ProjectionManifest> {
    composition_projections_with_capabilities(nodes, [])
}

fn composition_projections_with_capabilities<const N: usize, const C: usize>(
    nodes: [&str; N],
    capabilities: [SandCapability; C],
) -> Vec<ProjectionManifest> {
    let nodes = nodes
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let capabilities = capabilities.into_iter().collect::<BTreeSet<_>>();
    vec![
        ProjectionManifest {
            key: "native-retained".into(),
            kind: ProjectionKind::NativeRetained,
            isolation: Isolation::Trusted,
            required: BTreeSet::from([
                RendererCapability::RetainedControls,
                RendererCapability::Accessibility,
            ]),
            capabilities: capabilities.clone(),
            assets: Vec::new(),
            projected_nodes: nodes.clone(),
        },
        ProjectionManifest {
            key: "installed-html".into(),
            kind: ProjectionKind::InstalledHtml,
            isolation: Isolation::InstalledHtml,
            required: BTreeSet::from([
                RendererCapability::ExternalHtml,
                RendererCapability::Accessibility,
            ]),
            capabilities,
            assets: vec![
                INSTALLED_GALLERY_HTML_PATH.into(),
                INSTALLED_GALLERY_CSS_PATH.into(),
                INSTALLED_GALLERY_JS_PATH.into(),
                INSTALLED_GALLERY_ABI_JS_PATH.into(),
                INSTALLED_GALLERY_COMPOSITION_JS_PATH.into(),
                INSTALLED_GALLERY_PROBES_JS_PATH.into(),
            ],
            projected_nodes: nodes,
        },
    ]
}

fn definition_child(
    local_uid: &str,
    definition_uid: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    sibling_order: u32,
) -> DefinitionChild {
    DefinitionChild {
        local_uid: local_uid.into(),
        definition: DefinitionRef {
            uid: definition_uid.into(),
            revision: 1,
        },
        transform: Transform2d {
            x,
            y,
            width,
            height,
            rotation_radians: 0.0,
        },
        sibling_order,
        configuration: BTreeMap::new(),
        style: StyleLayer::default(),
    }
}

fn placement(
    instance_uid: &str,
    definition_uid: &str,
    projection: &str,
    x: f64,
    y: f64,
) -> CompositionPlacement {
    let inputs = if definition_uid == "button" {
        BTreeMap::from([
            (
                "label".into(),
                SandValue::Text("Open standalone Record".into()),
            ),
            (
                "description".into(),
                SandValue::Text("The same Button outside a Castle".into()),
            ),
            ("record".into(), SandValue::Record("fixture-record".into())),
        ])
    } else {
        BTreeMap::new()
    };
    CompositionPlacement {
        instance_uid: instance_uid.into(),
        definition: DefinitionRef {
            uid: definition_uid.into(),
            revision: 1,
        },
        follow_active: true,
        projection: projection.into(),
        transform: Transform2d {
            x,
            y,
            width: 360.0,
            height: 220.0,
            rotation_radians: 0.0,
        },
        locked: false,
        inputs,
        configuration: BTreeMap::new(),
        style: StyleLayer::default(),
    }
}

fn locked_placement(
    instance_uid: &str,
    definition_uid: &str,
    projection: &str,
    x: f64,
    y: f64,
) -> CompositionPlacement {
    let mut placement = placement(instance_uid, definition_uid, projection, x, y);
    placement.locked = true;
    placement
        .configuration
        .insert("compact".into(), SandValue::Boolean(true));
    placement
}

fn nested_open_for(instance_uid: &str) -> NodeAddress {
    NodeAddress {
        instance_uid: instance_uid.into(),
        node_path: vec!["call".into(), "open".into()],
    }
}

fn nested_open() -> NodeAddress {
    nested_open_for("room-native")
}

fn fixture_bindings() -> Vec<CompositionBinding> {
    let mut bindings = Vec::new();
    for instance_uid in ["room-native", "room-html"] {
        let target = NodeAddress {
            instance_uid: instance_uid.into(),
            node_path: Vec::new(),
        };
        for (field, port, value) in [
            (
                "title",
                "label",
                SandValue::Text("Build the Box together".into()),
            ),
            (
                "description",
                "description",
                SandValue::Text("Protein data crossed two compound boundaries".into()),
            ),
            ("uid", "record", SandValue::Record("fixture-record".into())),
        ] {
            bindings.push(CompositionBinding::ProteinRead {
                uid: format!("read-{instance_uid}-{field}"),
                protein_uid: "current-records".into(),
                field: field.into(),
                value,
                target: target.port(port),
            });
        }
    }
    bindings.push(CompositionBinding::OutputRoute {
        uid: "route-room-to-standalone".into(),
        source: NodeAddress {
            instance_uid: "room-native".into(),
            node_path: Vec::new(),
        }
        .port("record-clicked"),
        target: NodeAddress {
            instance_uid: "standalone-button".into(),
            node_path: Vec::new(),
        }
        .port("record"),
    });
    bindings.push(CompositionBinding::ActionWrite {
        uid: "action-open-record".into(),
        source: NodeAddress {
            instance_uid: "room-native".into(),
            node_path: Vec::new(),
        }
        .port("record-clicked"),
        action: "record.open".into(),
    });
    bindings
}

fn unique<'a>(label: &str, values: impl Iterator<Item = &'a str>) -> Result<(), CompositionError> {
    let mut unique = BTreeSet::new();
    for value in values {
        if !unique.insert(value) {
            return Err(CompositionError::new(format!("duplicate {label} {value}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_updates_revise_every_exact_ancestor_atomically() {
        let mut artifact = composition_workbench_fixture().unwrap();
        let update = artifact
            .catalog
            .publish_shared("button", |definition| {
                definition.display_name = "Shared Button revision".into();
            })
            .unwrap();
        assert_eq!(update.revisions["button"], [1, 2]);
        assert_eq!(update.revisions["video-call"], [1, 2]);
        assert_eq!(update.revisions["video-call-room"], [1, 2]);
        let before = artifact.catalog.clone();
        assert!(
            artifact
                .catalog
                .publish_shared("button", |definition| definition.outputs.clear())
                .is_err()
        );
        assert_eq!(artifact.catalog, before);
    }

    #[test]
    fn workbench_exercises_nested_behavior_save_fork_and_teardown() {
        let mut workbench = CompositionWorkbenchState::new().unwrap();
        workbench.exercise().unwrap();
        let facts = workbench.facts();
        assert_eq!(facts.mounted_placements, 5);
        assert_eq!(facts.scoped_dom_identities, facts.mounted_nodes);
        assert!(facts.emitted_events > 0);
        assert!(facts.routed_outputs > 0);
        assert!(facts.action_requests > 0);
        assert!(facts.active_behavior_handles > 0);
        assert!(facts.configured_nodes > 0);
        assert!(facts.styled_nodes > 0);
        assert!(facts.retired_behavior_handles > 0);
        assert!(facts.rejected_publications > 0);
        assert!(facts.save_reopens > 0);
        assert_eq!(facts.lock_changes, 1);
        assert_eq!(facts.style_override_sets, 1);
        assert_eq!(facts.style_override_resets, 1);
        assert_eq!(facts.shared_publications, 1);
        assert_eq!(facts.saved_definitions, 1);
        assert_eq!(facts.forked_definitions, 1);
        assert!(facts.teardown_count >= 4);
        assert_eq!(
            workbench
                .host()
                .mounted_style(&NodeAddress {
                    instance_uid: "room-native".into(),
                    node_path: Vec::new(),
                })
                .unwrap()
                .values["--lynx-radius-control"],
            StyleValue::LengthPx(5.0)
        );
        assert_eq!(
            workbench
                .host()
                .mounted_style(&NodeAddress {
                    instance_uid: "room-native".into(),
                    node_path: vec!["call".into()],
                })
                .unwrap()
                .values["--lynx-radius-control"],
            StyleValue::LengthPx(8.0)
        );
        assert!(matches!(
            workbench.host().lineage("video-call-room-fork").unwrap(),
            DefinitionLineage::Forked { .. }
        ));
        assert!(matches!(
            workbench.host().lineage("saved-video-call-room").unwrap(),
            DefinitionLineage::SavedGroup { .. }
        ));
    }

    #[test]
    fn artifact_round_trip_refuses_unknown_fields_and_preserves_locking() {
        let artifact = composition_workbench_fixture().unwrap();
        let bytes = artifact.to_json().unwrap();
        let reopened = CompositionArtifact::from_json(&bytes).unwrap();
        assert_eq!(artifact, reopened);
        assert!(reopened.document.placements[1].locked);
        let mut value = serde_json::to_value(reopened).unwrap();
        value["unknown"] = serde_json::Value::Bool(true);
        assert!(CompositionArtifact::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn binding_directions_are_typed_and_visually_distinct() {
        let artifact = composition_workbench_fixture().unwrap();
        artifact.validate().unwrap();
        let arrows = artifact
            .document
            .bindings
            .iter()
            .map(CompositionBinding::visible_arrow)
            .collect::<Vec<_>>();
        assert!(arrows.iter().any(|line| line.starts_with("READ")));
        assert!(arrows.iter().any(|line| line.starts_with("EVENT")));
        assert!(arrows.iter().any(|line| line.starts_with("WRITE")));
    }

    #[test]
    fn compound_inputs_cross_recursive_exports_once() {
        let workbench = CompositionWorkbenchState::new().unwrap();
        let inputs = workbench.host().mounted_inputs(&nested_open()).unwrap();
        assert_eq!(
            inputs["label"],
            SandValue::Text("Build the Box together".into())
        );
        assert_eq!(
            inputs["description"],
            SandValue::Text("Protein data crossed two compound boundaries".into())
        );
        assert_eq!(inputs["record"], SandValue::Record("fixture-record".into()));

        let mut duplicate = composition_workbench_fixture().unwrap();
        duplicate
            .document
            .bindings
            .push(CompositionBinding::ProteinRead {
                uid: "duplicate-deep-label".into(),
                protein_uid: "current-records".into(),
                field: "title".into(),
                value: SandValue::Text("Duplicate".into()),
                target: nested_open().port("label"),
            });
        assert!(duplicate.validate().is_err());
    }

    #[test]
    fn one_host_selects_every_declared_renderer_adapter() {
        let variants = [
            (
                "retained",
                ProjectionKind::NativeRetained,
                Isolation::Trusted,
                RendererCapability::RetainedControls,
            ),
            (
                "world",
                ProjectionKind::NativeWorld,
                Isolation::Trusted,
                RendererCapability::ThreeDimensional,
            ),
            (
                "installed",
                ProjectionKind::InstalledHtml,
                Isolation::InstalledHtml,
                RendererCapability::ExternalHtml,
            ),
            (
                "website",
                ProjectionKind::Website,
                Isolation::Website,
                RendererCapability::ExternalHtml,
            ),
            (
                "browser",
                ProjectionKind::BrowserDom,
                Isolation::Trusted,
                RendererCapability::BrowserDom,
            ),
            (
                "wasm",
                ProjectionKind::Wasm,
                Isolation::Trusted,
                RendererCapability::Wasm,
            ),
        ];
        let definition = SandDefinition {
            uid: "adapter-fixture".into(),
            revision: 1,
            display_name: "Adapter fixture".into(),
            element: SandElement::Specialized,
            inputs: Vec::new(),
            outputs: Vec::new(),
            children: Vec::new(),
            connections: Vec::new(),
            exports: Vec::new(),
            behaviors: Vec::new(),
            configuration: Vec::new(),
            style: StyleLayer::default(),
            accessibility: AccessibilitySpec {
                role: AccessibilityRole::Group,
                label: "Adapter fixture".into(),
                description: None,
                live: false,
            },
            capabilities: BTreeSet::new(),
            projections: variants
                .iter()
                .map(|(key, kind, isolation, capability)| ProjectionManifest {
                    key: (*key).into(),
                    kind: *kind,
                    isolation: *isolation,
                    required: BTreeSet::from([*capability]),
                    capabilities: BTreeSet::new(),
                    assets: Vec::new(),
                    projected_nodes: BTreeSet::from(["root".into()]),
                })
                .collect(),
        };
        let catalog = DefinitionCatalog::from_records([DefinitionRevisionRecord {
            definition,
            origin: DefinitionOrigin::Rust,
            lineage: DefinitionLineage::CodeOwned {
                constructor: "native::adapter-fixture".into(),
            },
        }])
        .unwrap();
        let document = CompositionDocument {
            schema_version: COMPOSITION_SCHEMA_VERSION,
            uid: "adapter-document".into(),
            placements: variants
                .iter()
                .enumerate()
                .map(|(index, (key, ..))| {
                    placement(
                        &format!("adapter-{index}"),
                        "adapter-fixture",
                        key,
                        index as f64 * 10.0,
                        0.0,
                    )
                })
                .collect(),
            bindings: Vec::new(),
        };
        let available = variants
            .iter()
            .map(|(_, _, _, capability)| *capability)
            .collect();
        let host = CompositionHost::open(
            CompositionArtifact {
                schema_version: COMPOSITION_SCHEMA_VERSION,
                catalog,
                document,
            },
            available,
        )
        .unwrap();
        let adapters = host.adapters();
        assert!(adapters.contains(&RuntimeAdapter::NativeRetained));
        assert!(adapters.contains(&RuntimeAdapter::NativeWorld));
        assert!(adapters.contains(&RuntimeAdapter::InstalledHtml));
        assert!(adapters.contains(&RuntimeAdapter::Website));
        assert!(adapters.contains(&RuntimeAdapter::BrowserDom));
        assert!(adapters.contains(&RuntimeAdapter::Wasm));
    }
}
