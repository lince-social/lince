use crate::style::StyleLayer;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Display, Formatter},
    path::{Component, Path},
};

pub const SAND_SCHEMA_VERSION: u32 = 1;
pub const SAND_ABI_VERSION: u32 = 1;

const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_ABI_BYTES: usize = 64 * 1024;
const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandError {
    detail: String,
}

impl SandError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for SandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for SandError {}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Text,
    Number,
    Boolean,
    Record,
    Json,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SandValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Record(String),
    Json(serde_json::Value),
}

impl SandValue {
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Text(_) => ValueType::Text,
            Self::Number(_) => ValueType::Number,
            Self::Boolean(_) => ValueType::Boolean,
            Self::Record(_) => ValueType::Record,
            Self::Json(_) => ValueType::Json,
        }
    }

    fn validate(&self) -> Result<(), SandError> {
        match self {
            Self::Text(value) if value.len() > MAX_TEXT_BYTES => {
                Err(SandError::new("text value exceeds its size limit"))
            }
            Self::Number(value) if !value.is_finite() => {
                Err(SandError::new("number value must be finite"))
            }
            Self::Record(value) => validate_identifier("Record", value),
            Self::Json(value)
                if serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() > MAX_TEXT_BYTES) =>
            {
                Err(SandError::new("JSON value exceeds its size limit"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputPort {
    pub name: String,
    pub value_type: ValueType,
    pub required: bool,
    pub default: Option<SandValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputPort {
    pub name: String,
    pub value_type: ValueType,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SandElement {
    Text,
    Heading,
    Quantity,
    Icon,
    Button,
    Badge,
    Field,
    Textarea,
    Checkbox,
    Radio,
    Disclosure,
    Select,
    Tooltip,
    ValidationMessage,
    List,
    Row,
    Card,
    Panel,
    Stack,
    Compound,
    Specialized,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AccessibilityRole {
    Text,
    Heading,
    Image,
    Button,
    Status,
    TextInput,
    MultilineTextInput,
    Checkbox,
    Radio,
    Disclosure,
    Select,
    Tooltip,
    Alert,
    List,
    Row,
    Article,
    Group,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccessibilitySpec {
    pub role: AccessibilityRole,
    pub label: String,
    pub description: Option<String>,
    pub live: bool,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    Trusted,
    InstalledHtml,
    Website,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "capability", rename_all = "snake_case", deny_unknown_fields)]
pub enum SandCapability {
    ProteinRead,
    EmitEvent { event: String },
    RequestAction { action: String },
    LocalStorage,
    Network,
    Media { medium: String },
    GpuProgram,
    WasmBehavior,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RendererCapability {
    RetainedControls,
    InstancedNodes,
    ExternalHtml,
    BrowserDom,
    ThreeDimensional,
    Accessibility,
    Wasm,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionKind {
    NativeRetained,
    NativeWorld,
    InstalledHtml,
    Website,
    BrowserDom,
    Wasm,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionManifest {
    pub key: String,
    pub kind: ProjectionKind,
    pub isolation: Isolation,
    pub required: BTreeSet<RendererCapability>,
    pub capabilities: BTreeSet<SandCapability>,
    pub assets: Vec<String>,
    pub projected_nodes: BTreeSet<String>,
}

impl ProjectionManifest {
    pub fn select<'a>(
        manifests: &'a [Self],
        available: &BTreeSet<RendererCapability>,
    ) -> Result<&'a Self, SandError> {
        manifests
            .iter()
            .find(|manifest| manifest.required.is_subset(available))
            .ok_or_else(|| SandError::new("no renderer projection satisfies capabilities"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionRef {
    pub uid: String,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Transform2d {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_radians: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionChild {
    pub local_uid: String,
    pub definition: DefinitionRef,
    pub transform: Transform2d,
    pub sibling_order: u32,
    pub configuration: BTreeMap<String, SandValue>,
    pub style: StyleLayer,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportedPort {
    pub name: String,
    pub child_uid: String,
    pub child_port: String,
    pub direction: PortDirection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortEndpoint {
    pub child_uid: String,
    pub port: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortConnection {
    pub from: PortEndpoint,
    pub to: PortEndpoint,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeclarativeBehavior {
    EmitEvent {
        source_output: String,
        event: String,
    },
    SetLocalState {
        source_output: String,
        key: String,
        value: SandValue,
    },
    RequestAction {
        source_output: String,
        action: String,
    },
    TogglePresentationOnEvent {
        event: String,
    },
}

impl DeclarativeBehavior {
    fn source_output(&self) -> Option<&str> {
        match self {
            Self::EmitEvent { source_output, .. }
            | Self::SetLocalState { source_output, .. }
            | Self::RequestAction { source_output, .. } => Some(source_output),
            Self::TogglePresentationOnEvent { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleBehavior {
    pub asset: String,
    pub mount_export: String,
    pub teardown_export: String,
    pub capabilities: BTreeSet<SandCapability>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "runtime", rename_all = "snake_case", deny_unknown_fields)]
pub enum BehaviorBinding {
    Declarative { behavior: DeclarativeBehavior },
    Module { module: ModuleBehavior },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationField {
    pub name: String,
    pub value_type: ValueType,
    pub required: bool,
    pub default: Option<SandValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SandDefinition {
    pub uid: String,
    pub revision: u64,
    pub display_name: String,
    pub element: SandElement,
    pub inputs: Vec<InputPort>,
    pub outputs: Vec<OutputPort>,
    pub children: Vec<DefinitionChild>,
    pub connections: Vec<PortConnection>,
    pub exports: Vec<ExportedPort>,
    pub behaviors: Vec<BehaviorBinding>,
    pub configuration: Vec<ConfigurationField>,
    pub style: StyleLayer,
    pub accessibility: AccessibilitySpec,
    pub capabilities: BTreeSet<SandCapability>,
    pub projections: Vec<ProjectionManifest>,
}

impl SandDefinition {
    fn validate_shape(&self) -> Result<(), SandError> {
        validate_identifier("Sand definition", &self.uid)?;
        if self.revision == 0 {
            return Err(SandError::new("Sand definition revision must be positive"));
        }
        validate_visible_text("Sand display name", &self.display_name)?;
        validate_visible_text("accessibility label", &self.accessibility.label)?;
        unique(
            "input port",
            self.inputs.iter().map(|port| port.name.as_str()),
        )?;
        unique(
            "output port",
            self.outputs.iter().map(|port| port.name.as_str()),
        )?;
        unique(
            "child",
            self.children.iter().map(|child| child.local_uid.as_str()),
        )?;
        unique(
            "export",
            self.exports.iter().map(|export| export.name.as_str()),
        )?;
        unique(
            "configuration",
            self.configuration.iter().map(|field| field.name.as_str()),
        )?;
        unique(
            "projection",
            self.projections
                .iter()
                .map(|projection| projection.key.as_str()),
        )?;
        for input in &self.inputs {
            validate_identifier("input port", &input.name)?;
            if let Some(default) = &input.default {
                validate_typed_value(input.value_type, default)?;
            }
            if input.required && input.default.is_some() {
                return Err(SandError::new(format!(
                    "required input {} cannot also have a default",
                    input.name
                )));
            }
        }
        for output in &self.outputs {
            validate_identifier("output port", &output.name)?;
        }
        for field in &self.configuration {
            validate_identifier("configuration field", &field.name)?;
            if let Some(default) = &field.default {
                validate_typed_value(field.value_type, default)?;
            }
            if field.required && field.default.is_none() {
                return Err(SandError::new(format!(
                    "required configuration {} needs a default",
                    field.name
                )));
            }
        }
        for child in &self.children {
            validate_identifier("child", &child.local_uid)?;
            validate_identifier("child definition", &child.definition.uid)?;
            validate_transform(child.transform)?;
            child
                .style
                .validate_standard()
                .map_err(|error| SandError::new(error.to_string()))?;
        }
        self.style
            .validate_standard()
            .map_err(|error| SandError::new(error.to_string()))?;
        for capability in &self.capabilities {
            validate_capability(capability)?;
        }
        let outputs = self
            .outputs
            .iter()
            .map(|port| port.name.as_str())
            .collect::<BTreeSet<_>>();
        for binding in &self.behaviors {
            match binding {
                BehaviorBinding::Declarative { behavior } => {
                    if behavior
                        .source_output()
                        .is_some_and(|source| !outputs.contains(source))
                    {
                        return Err(SandError::new("Behavior names an unknown output"));
                    }
                    match behavior {
                        DeclarativeBehavior::EmitEvent { event, .. } => {
                            validate_identifier("event", event)?;
                            require_capability(
                                &self.capabilities,
                                &SandCapability::EmitEvent {
                                    event: event.clone(),
                                },
                            )?;
                        }
                        DeclarativeBehavior::SetLocalState { key, value, .. } => {
                            validate_identifier("local state", key)?;
                            value.validate()?;
                        }
                        DeclarativeBehavior::RequestAction { action, .. } => {
                            validate_identifier("Action", action)?;
                            require_capability(
                                &self.capabilities,
                                &SandCapability::RequestAction {
                                    action: action.clone(),
                                },
                            )?;
                        }
                        DeclarativeBehavior::TogglePresentationOnEvent { event } => {
                            validate_identifier("event", event)?;
                        }
                    }
                }
                BehaviorBinding::Module { module } => {
                    validate_asset_path(&module.asset)?;
                    validate_identifier("module mount export", &module.mount_export)?;
                    validate_identifier("module teardown export", &module.teardown_export)?;
                    if module.teardown_export == module.mount_export {
                        return Err(SandError::new(
                            "module mount and teardown exports must differ",
                        ));
                    }
                    if !module.capabilities.is_subset(&self.capabilities) {
                        return Err(SandError::new("module requests undeclared capabilities"));
                    }
                }
            }
        }
        let expected_nodes = std::iter::once("root".to_string())
            .chain(self.children.iter().map(|child| child.local_uid.clone()))
            .collect::<BTreeSet<_>>();
        if self.projections.is_empty() {
            return Err(SandError::new("Sand definition has no renderer projection"));
        }
        for projection in &self.projections {
            validate_identifier("projection", &projection.key)?;
            if projection.projected_nodes != expected_nodes {
                return Err(SandError::new(format!(
                    "projection {} disagrees with semantic nodes",
                    projection.key
                )));
            }
            for asset in &projection.assets {
                validate_asset_path(asset)?;
            }
            if !projection.capabilities.is_subset(&self.capabilities) {
                return Err(SandError::new(
                    "projection requests undeclared capabilities",
                ));
            }
            if projection.isolation == Isolation::Website
                && projection.capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        SandCapability::ProteinRead
                            | SandCapability::EmitEvent { .. }
                            | SandCapability::RequestAction { .. }
                            | SandCapability::GpuProgram
                            | SandCapability::WasmBehavior
                    )
                })
            {
                return Err(SandError::new(
                    "Website projection requests Lince authority",
                ));
            }
            match (projection.isolation, projection.kind) {
                (Isolation::InstalledHtml, ProjectionKind::InstalledHtml)
                | (Isolation::Website, ProjectionKind::Website)
                | (Isolation::Trusted, ProjectionKind::NativeRetained)
                | (Isolation::Trusted, ProjectionKind::NativeWorld)
                | (Isolation::Trusted, ProjectionKind::BrowserDom)
                | (Isolation::Trusted, ProjectionKind::Wasm) => {}
                _ => return Err(SandError::new("projection conflicts with isolation")),
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefinitionGraph {
    pub schema_version: u32,
    pub definitions: BTreeMap<String, SandDefinition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SandInstance {
    pub uid: String,
    pub definition: DefinitionRef,
    pub projection: String,
    pub inputs: BTreeMap<String, SandValue>,
    pub configuration: BTreeMap<String, SandValue>,
    pub style: StyleLayer,
}

impl SandInstance {
    pub fn validate<'a>(
        &self,
        graph: &'a DefinitionGraph,
        available: &BTreeSet<RendererCapability>,
    ) -> Result<&'a ProjectionManifest, SandError> {
        validate_identifier("Sand instance", &self.uid)?;
        let definition = graph
            .definitions
            .get(&self.definition.uid)
            .ok_or_else(|| SandError::new("instance definition is missing"))?;
        if definition.revision != self.definition.revision {
            return Err(SandError::new(
                "instance definition revision is unavailable",
            ));
        }
        validate_inputs(definition, &self.inputs, true)?;
        validate_configuration(definition, &self.configuration)?;
        self.style
            .validate_standard()
            .map_err(|error| SandError::new(error.to_string()))?;
        let projection = definition
            .projections
            .iter()
            .find(|projection| projection.key == self.projection)
            .ok_or_else(|| SandError::new("instance projection is unavailable"))?;
        if !projection.required.is_subset(available) {
            return Err(SandError::new(
                "instance projection renderer capabilities are unavailable",
            ));
        }
        Ok(projection)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeAdapter {
    NativeRetained,
    NativeWorld,
    InstalledHtml,
    Website,
    BrowserDom,
    Wasm,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSandInstance {
    pub instance_uid: String,
    pub definition: DefinitionRef,
    pub projection: String,
    pub adapter: RuntimeAdapter,
    pub renderer_handle: u64,
    pub presented: bool,
}

impl RuntimeSandInstance {
    pub fn mount(
        instance: &SandInstance,
        graph: &DefinitionGraph,
        available: &BTreeSet<RendererCapability>,
        renderer_handle: u64,
    ) -> Result<Self, SandError> {
        if renderer_handle == 0 {
            return Err(SandError::new("runtime renderer handle must be positive"));
        }
        let projection = instance.validate(graph, available)?;
        let adapter = match projection.kind {
            ProjectionKind::NativeRetained => RuntimeAdapter::NativeRetained,
            ProjectionKind::NativeWorld => RuntimeAdapter::NativeWorld,
            ProjectionKind::InstalledHtml => RuntimeAdapter::InstalledHtml,
            ProjectionKind::Website => RuntimeAdapter::Website,
            ProjectionKind::BrowserDom => RuntimeAdapter::BrowserDom,
            ProjectionKind::Wasm => RuntimeAdapter::Wasm,
        };
        Ok(Self {
            instance_uid: instance.uid.clone(),
            definition: instance.definition.clone(),
            projection: instance.projection.clone(),
            adapter,
            renderer_handle,
            presented: true,
        })
    }
}

impl DefinitionGraph {
    pub fn validate(&self) -> Result<(), SandError> {
        if self.schema_version != SAND_SCHEMA_VERSION {
            return Err(SandError::new(format!(
                "unsupported Sand schema version {}",
                self.schema_version
            )));
        }
        if self.definitions.is_empty() {
            return Err(SandError::new("Sand graph is empty"));
        }
        for (uid, definition) in &self.definitions {
            if uid != &definition.uid {
                return Err(SandError::new("definition map key disagrees with uid"));
            }
            definition.validate_shape()?;
            for child in &definition.children {
                let referenced = self
                    .definitions
                    .get(&child.definition.uid)
                    .ok_or_else(|| SandError::new("child definition is missing"))?;
                if referenced.revision != child.definition.revision {
                    return Err(SandError::new("child definition revision is unavailable"));
                }
                validate_configuration(referenced, &child.configuration)?;
            }
            validate_exports(self, definition)?;
            validate_connections(self, definition)?;
        }
        for uid in self.definitions.keys() {
            self.validate_acyclic(uid, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        }
        Ok(())
    }

    fn validate_acyclic(
        &self,
        uid: &str,
        visiting: &mut BTreeSet<String>,
        complete: &mut BTreeSet<String>,
    ) -> Result<(), SandError> {
        if complete.contains(uid) {
            return Ok(());
        }
        if !visiting.insert(uid.to_string()) {
            return Err(SandError::new(format!(
                "recursive Sand definition cycle reaches {uid}"
            )));
        }
        let definition = self
            .definitions
            .get(uid)
            .ok_or_else(|| SandError::new("definition is missing"))?;
        for child in &definition.children {
            self.validate_acyclic(&child.definition.uid, visiting, complete)?;
        }
        visiting.remove(uid);
        complete.insert(uid.to_string());
        Ok(())
    }

    pub fn normalized_sha256(&self) -> Result<String, SandError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| SandError::new(format!("serialize Sand graph: {error}")))?;
        Ok(sha256(&bytes))
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Html,
    Css,
    JavaScriptModule,
    Wasm,
    Shader,
    Raster,
    Svg,
    Font,
    License,
    Notice,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "origin", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssetProvenance {
    FirstParty,
    User,
    Vendored {
        project: String,
        source: String,
        license_asset: String,
        credit: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SandAsset {
    pub path: String,
    pub kind: AssetKind,
    pub media_type: String,
    pub sha256: String,
    pub provenance: AssetProvenance,
    pub bytes: Vec<u8>,
}

impl SandAsset {
    pub fn first_party(
        path: impl Into<String>,
        kind: AssetKind,
        media_type: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        let bytes = bytes.into();
        Self {
            path: path.into(),
            kind,
            media_type: media_type.into(),
            sha256: sha256(&bytes),
            provenance: AssetProvenance::FirstParty,
            bytes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SandPackage {
    pub schema_version: u32,
    pub uid: String,
    pub graph: DefinitionGraph,
    pub assets: Vec<SandAsset>,
}

impl SandPackage {
    pub fn validate(&self) -> Result<(), SandError> {
        if self.schema_version != SAND_SCHEMA_VERSION {
            return Err(SandError::new("unsupported Sand package schema"));
        }
        validate_identifier("Sand package", &self.uid)?;
        self.graph.validate()?;
        unique("asset", self.assets.iter().map(|asset| asset.path.as_str()))?;
        let mut total = 0usize;
        let assets = self
            .assets
            .iter()
            .map(|asset| (asset.path.as_str(), asset))
            .collect::<BTreeMap<_, _>>();
        for asset in &self.assets {
            validate_asset_path(&asset.path)?;
            validate_media_type(&asset.media_type)?;
            if asset.bytes.len() > MAX_ASSET_BYTES {
                return Err(SandError::new("Sand asset exceeds its size limit"));
            }
            total = total.saturating_add(asset.bytes.len());
            if asset.sha256 != sha256(&asset.bytes) {
                return Err(SandError::new(format!(
                    "Sand asset {} hash disagrees with its bytes",
                    asset.path
                )));
            }
            if let AssetProvenance::Vendored {
                project,
                source,
                license_asset,
                credit,
            } = &asset.provenance
            {
                validate_visible_text("vendored project", project)?;
                validate_visible_text("vendored credit", credit)?;
                let source = url::Url::parse(source)
                    .map_err(|_| SandError::new("vendored source is not a URL"))?;
                if source.scheme() != "https" {
                    return Err(SandError::new("vendored source must use HTTPS"));
                }
                let license = assets
                    .get(license_asset.as_str())
                    .ok_or_else(|| SandError::new("vendored asset license is missing"))?;
                if !matches!(license.kind, AssetKind::License | AssetKind::Notice) {
                    return Err(SandError::new("vendored license asset has the wrong kind"));
                }
            }
        }
        if total > MAX_PACKAGE_BYTES {
            return Err(SandError::new("Sand package exceeds its size limit"));
        }
        for definition in self.graph.definitions.values() {
            for projection in &definition.projections {
                for path in &projection.assets {
                    if !assets.contains_key(path.as_str()) {
                        return Err(SandError::new(format!(
                            "projection {} references missing asset {path}",
                            projection.key
                        )));
                    }
                }
            }
            for binding in &definition.behaviors {
                if let BehaviorBinding::Module { module } = binding {
                    let asset = assets
                        .get(module.asset.as_str())
                        .ok_or_else(|| SandError::new("Behavior module asset is missing"))?;
                    if asset.kind != AssetKind::JavaScriptModule {
                        return Err(SandError::new("Behavior module asset is not JavaScript"));
                    }
                }
            }
            for projection in &definition.projections {
                for path in &projection.assets {
                    let asset = assets[path.as_str()];
                    if asset.kind == AssetKind::Shader
                        && !definition
                            .capabilities
                            .contains(&SandCapability::GpuProgram)
                    {
                        return Err(SandError::new("shader asset lacks GPU capability"));
                    }
                    if asset.kind == AssetKind::Wasm
                        && !definition
                            .capabilities
                            .contains(&SandCapability::WasmBehavior)
                    {
                        return Err(SandError::new("Wasm asset lacks Wasm capability"));
                    }
                }
                validate_projection_markup(definition, projection, &assets)?;
            }
        }
        Ok(())
    }

    pub fn graph_sha256(&self) -> Result<String, SandError> {
        self.graph.normalized_sha256()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum SandHostEvent {
    Mount {
        definition: DefinitionRef,
        inputs: BTreeMap<String, SandValue>,
        configuration: BTreeMap<String, SandValue>,
        presented: bool,
    },
    InputsChanged {
        values: BTreeMap<String, SandValue>,
    },
    BoxEvent {
        name: String,
        value: SandValue,
    },
    PresentationChanged {
        presented: bool,
    },
    Unmount,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SandInboundEnvelope {
    pub abi_version: u32,
    pub instance_uid: String,
    pub sequence: u64,
    pub event: SandHostEvent,
}

impl SandInboundEnvelope {
    pub fn validate(&self, definition: &SandDefinition) -> Result<(), SandError> {
        validate_envelope(self.abi_version, &self.instance_uid, self.sequence)?;
        match &self.event {
            SandHostEvent::Mount {
                definition: reference,
                inputs,
                configuration,
                ..
            } => {
                if reference.uid != definition.uid || reference.revision != definition.revision {
                    return Err(SandError::new("mount names another Sand revision"));
                }
                validate_inputs(definition, inputs, true)?;
                validate_configuration(definition, configuration)
            }
            SandHostEvent::InputsChanged { values } => validate_inputs(definition, values, false),
            SandHostEvent::BoxEvent { name, value } => {
                validate_identifier("Box event", name)?;
                value.validate()
            }
            SandHostEvent::PresentationChanged { .. } | SandHostEvent::Unmount => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "request", rename_all = "snake_case", deny_unknown_fields)]
pub enum SandHostRequest {
    EmitOutput { port: String, value: SandValue },
    SetLocalState { key: String, value: SandValue },
    RequestAction { action: String, payload: SandValue },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SandOutboundEnvelope {
    pub abi_version: u32,
    pub instance_uid: String,
    pub sequence: u64,
    pub request: SandHostRequest,
}

impl SandOutboundEnvelope {
    pub fn validate(&self, definition: &SandDefinition) -> Result<(), SandError> {
        validate_envelope(self.abi_version, &self.instance_uid, self.sequence)?;
        match &self.request {
            SandHostRequest::EmitOutput { port, value } => {
                let output = definition
                    .outputs
                    .iter()
                    .find(|candidate| candidate.name == *port)
                    .ok_or_else(|| SandError::new("request names an unknown output"))?;
                validate_typed_value(output.value_type, value)
            }
            SandHostRequest::SetLocalState { key, value } => {
                validate_identifier("local state", key)?;
                value.validate()
            }
            SandHostRequest::RequestAction { action, payload } => {
                validate_identifier("Action", action)?;
                require_capability(
                    &definition.capabilities,
                    &SandCapability::RequestAction {
                        action: action.clone(),
                    },
                )?;
                payload.validate()
            }
        }
    }

    pub fn decode_and_validate(
        bytes: &[u8],
        definition: &SandDefinition,
    ) -> Result<Self, SandError> {
        if bytes.len() > MAX_ABI_BYTES {
            return Err(SandError::new("Sand ABI message exceeds its size limit"));
        }
        let envelope = serde_json::from_slice::<Self>(bytes)
            .map_err(|error| SandError::new(format!("invalid Sand ABI message: {error}")))?;
        envelope.validate(definition)?;
        Ok(envelope)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SandContractSchemas {
    pub schema_version: u32,
    pub package: serde_json::Value,
    pub instance: serde_json::Value,
    pub inbound: serde_json::Value,
    pub outbound: serde_json::Value,
}

pub fn contract_schemas() -> Result<SandContractSchemas, SandError> {
    Ok(SandContractSchemas {
        schema_version: SAND_SCHEMA_VERSION,
        package: serde_json::to_value(schema_for!(SandPackage))
            .map_err(|error| SandError::new(format!("serialize package schema: {error}")))?,
        instance: serde_json::to_value(schema_for!(SandInstance))
            .map_err(|error| SandError::new(format!("serialize instance schema: {error}")))?,
        inbound: serde_json::to_value(schema_for!(SandInboundEnvelope))
            .map_err(|error| SandError::new(format!("serialize inbound schema: {error}")))?,
        outbound: serde_json::to_value(schema_for!(SandOutboundEnvelope))
            .map_err(|error| SandError::new(format!("serialize outbound schema: {error}")))?,
    })
}

fn validate_projection_markup(
    definition: &SandDefinition,
    projection: &ProjectionManifest,
    assets: &BTreeMap<&str, &SandAsset>,
) -> Result<(), SandError> {
    if projection.kind != ProjectionKind::InstalledHtml {
        return Ok(());
    }
    let html_assets = projection
        .assets
        .iter()
        .filter_map(|path| assets.get(path.as_str()))
        .filter(|asset| asset.kind == AssetKind::Html)
        .collect::<Vec<_>>();
    if html_assets.len() != 1 {
        return Err(SandError::new(
            "Installed HTML projection needs exactly one HTML asset",
        ));
    }
    let markup = std::str::from_utf8(&html_assets[0].bytes)
        .map_err(|_| SandError::new("Installed HTML markup is not UTF-8"))?;
    let definition_marker = format!("data-sand-definition=\"{}\"", definition.uid);
    if !markup.contains(&definition_marker) {
        return Err(SandError::new(format!(
            "Installed HTML omits definition {}",
            definition.uid
        )));
    }
    for node in &projection.projected_nodes {
        let marker = format!("data-sand-node=\"{}:{node}\"", definition.uid);
        if !markup.contains(&marker) {
            return Err(SandError::new(format!(
                "Installed HTML omits declared node {}:{node}",
                definition.uid
            )));
        }
    }
    if markup.contains("<style") {
        return Err(SandError::new(
            "Installed HTML must carry style in declared CSS assets",
        ));
    }
    for script in markup.split("<script").skip(1) {
        let opening = script
            .split_once('>')
            .map(|(opening, _)| opening)
            .ok_or_else(|| SandError::new("Installed HTML has a malformed script element"))?;
        if !opening.contains("src=") {
            return Err(SandError::new(
                "Installed HTML must carry Behavior in declared module assets",
            ));
        }
    }
    for handler in [
        " onclick=",
        " onchange=",
        " oninput=",
        " onload=",
        " onerror=",
        " onsubmit=",
        " onkeydown=",
        " onpointerdown=",
    ] {
        if markup.to_ascii_lowercase().contains(handler) {
            return Err(SandError::new(
                "Installed HTML contains an inline event handler",
            ));
        }
    }
    for asset in projection
        .assets
        .iter()
        .filter_map(|path| assets.get(path.as_str()))
        .filter(|asset| asset.kind == AssetKind::JavaScriptModule)
    {
        let source = std::str::from_utf8(&asset.bytes)
            .map_err(|_| SandError::new("JavaScript module is not UTF-8"))?;
        if source.contains("eval(") || source.contains("new Function(") {
            return Err(SandError::new(
                "JavaScript module contains dynamic source evaluation",
            ));
        }
        validate_module_imports(asset, source, projection, assets)?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum JavaScriptToken {
    Identifier(String),
    StringLiteral(String),
    Symbol(u8),
}

fn validate_module_imports(
    asset: &SandAsset,
    source: &str,
    projection: &ProjectionManifest,
    assets: &BTreeMap<&str, &SandAsset>,
) -> Result<(), SandError> {
    for specifier in static_module_specifiers(source)? {
        if !specifier.starts_with("./")
            || specifier.len() == 2
            || specifier.contains('\\')
            || specifier.split('/').any(|part| part == "..")
        {
            return Err(SandError::new(format!(
                "JavaScript module {} imports an unbounded specifier {specifier}",
                asset.path
            )));
        }
        let parent = Path::new(&asset.path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let resolved = parent.join(&specifier[2..]);
        let resolved = resolved
            .to_str()
            .ok_or_else(|| SandError::new("JavaScript import path is not UTF-8"))?;
        validate_asset_path(resolved)?;
        let imported = assets.get(resolved).ok_or_else(|| {
            SandError::new(format!(
                "JavaScript module {} imports missing asset {resolved}",
                asset.path
            ))
        })?;
        if imported.kind != AssetKind::JavaScriptModule {
            return Err(SandError::new(format!(
                "JavaScript module {} imports non-module asset {resolved}",
                asset.path
            )));
        }
        if !projection.assets.iter().any(|path| path == resolved) {
            return Err(SandError::new(format!(
                "JavaScript module {} imports undeclared projection asset {resolved}",
                asset.path
            )));
        }
    }
    Ok(())
}

fn static_module_specifiers(source: &str) -> Result<Vec<String>, SandError> {
    let tokens = javascript_tokens(source)?;
    let mut specifiers = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let JavaScriptToken::Identifier(keyword) = token else {
            continue;
        };
        if keyword == "import" {
            if matches!(
                tokens.get(index.wrapping_sub(1)),
                Some(JavaScriptToken::Symbol(b'.'))
            ) {
                continue;
            }
            match tokens.get(index + 1) {
                Some(JavaScriptToken::Symbol(b'(')) => {
                    return Err(SandError::new(
                        "Installed JavaScript uses a dynamic module import",
                    ));
                }
                Some(JavaScriptToken::StringLiteral(specifier)) => {
                    specifiers.push(specifier.clone());
                }
                Some(_) => specifiers.push(module_from_specifier(&tokens, index + 1)?),
                None => return Err(SandError::new("JavaScript import is incomplete")),
            }
        } else if keyword == "export" {
            if let Some(specifier) = optional_module_from_specifier(&tokens, index + 1)? {
                specifiers.push(specifier);
            }
        }
    }
    Ok(specifiers)
}

fn module_from_specifier(tokens: &[JavaScriptToken], start: usize) -> Result<String, SandError> {
    optional_module_from_specifier(tokens, start)?
        .ok_or_else(|| SandError::new("JavaScript import has no static module specifier"))
}

fn optional_module_from_specifier(
    tokens: &[JavaScriptToken],
    start: usize,
) -> Result<Option<String>, SandError> {
    let mut index = start;
    while let Some(token) = tokens.get(index) {
        match token {
            JavaScriptToken::Identifier(keyword) if keyword == "from" => {
                return match tokens.get(index + 1) {
                    Some(JavaScriptToken::StringLiteral(specifier)) => Ok(Some(specifier.clone())),
                    _ => Err(SandError::new(
                        "JavaScript from clause has no static module specifier",
                    )),
                };
            }
            JavaScriptToken::Symbol(b';') => return Ok(None),
            JavaScriptToken::Identifier(keyword)
                if index > start && (keyword == "import" || keyword == "export") =>
            {
                return Ok(None);
            }
            _ => index += 1,
        }
    }
    Ok(None)
}

fn javascript_tokens(source: &str) -> Result<Vec<JavaScriptToken>, SandError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                let mut closed = false;
                while index + 1 < bytes.len() {
                    if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                        index += 2;
                        closed = true;
                        break;
                    }
                    index += 1;
                }
                if !closed {
                    return Err(SandError::new("JavaScript block comment is incomplete"));
                }
            }
            quote @ (b'\'' | b'"') => {
                index += 1;
                let start = index;
                let mut escaped = false;
                let mut closed = false;
                while index < bytes.len() {
                    if bytes[index] == quote && !escaped {
                        let value = std::str::from_utf8(&bytes[start..index])
                            .map_err(|_| SandError::new("JavaScript string is not UTF-8"))?;
                        tokens.push(JavaScriptToken::StringLiteral(value.into()));
                        index += 1;
                        closed = true;
                        break;
                    }
                    escaped = bytes[index] == b'\\' && !escaped;
                    if bytes[index] != b'\\' {
                        escaped = false;
                    }
                    index += 1;
                }
                if !closed {
                    return Err(SandError::new("JavaScript string is incomplete"));
                }
            }
            b'`' => {
                index += 1;
                let mut escaped = false;
                let mut closed = false;
                while index < bytes.len() {
                    if bytes[index] == b'`' && !escaped {
                        index += 1;
                        closed = true;
                        break;
                    }
                    escaped = bytes[index] == b'\\' && !escaped;
                    if bytes[index] != b'\\' {
                        escaped = false;
                    }
                    index += 1;
                }
                if !closed {
                    return Err(SandError::new("JavaScript template string is incomplete"));
                }
            }
            byte if byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$') => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
                {
                    index += 1;
                }
                tokens.push(JavaScriptToken::Identifier(source[start..index].into()));
            }
            symbol => {
                tokens.push(JavaScriptToken::Symbol(symbol));
                index += 1;
            }
        }
    }
    Ok(tokens)
}

fn validate_exports(graph: &DefinitionGraph, definition: &SandDefinition) -> Result<(), SandError> {
    for export in &definition.exports {
        validate_identifier("export", &export.name)?;
        let child = definition
            .children
            .iter()
            .find(|child| child.local_uid == export.child_uid)
            .ok_or_else(|| SandError::new("export names an unknown child"))?;
        let child_definition = &graph.definitions[&child.definition.uid];
        let child_type = match export.direction {
            PortDirection::Input => child_definition
                .inputs
                .iter()
                .find(|port| port.name == export.child_port)
                .map(|port| port.value_type),
            PortDirection::Output => child_definition
                .outputs
                .iter()
                .find(|port| port.name == export.child_port)
                .map(|port| port.value_type),
        };
        let child_type =
            child_type.ok_or_else(|| SandError::new("export names an unknown child port"))?;
        let parent_type = match export.direction {
            PortDirection::Input => definition
                .inputs
                .iter()
                .find(|port| port.name == export.name)
                .map(|port| port.value_type),
            PortDirection::Output => definition
                .outputs
                .iter()
                .find(|port| port.name == export.name)
                .map(|port| port.value_type),
        }
        .ok_or_else(|| SandError::new("export has no matching parent port"))?;
        if child_type != parent_type {
            return Err(SandError::new("export port types are incompatible"));
        }
    }
    Ok(())
}

fn validate_connections(
    graph: &DefinitionGraph,
    definition: &SandDefinition,
) -> Result<(), SandError> {
    let mut destinations = BTreeSet::new();
    for connection in &definition.connections {
        let from = child_port(graph, definition, &connection.from, PortDirection::Output)?;
        let to = child_port(graph, definition, &connection.to, PortDirection::Input)?;
        if from != to {
            return Err(SandError::new("connection port types are incompatible"));
        }
        if !destinations.insert((
            connection.to.child_uid.as_str(),
            connection.to.port.as_str(),
        )) {
            return Err(SandError::new("connection input is wired more than once"));
        }
    }
    Ok(())
}

fn child_port(
    graph: &DefinitionGraph,
    definition: &SandDefinition,
    endpoint: &PortEndpoint,
    direction: PortDirection,
) -> Result<ValueType, SandError> {
    let child = definition
        .children
        .iter()
        .find(|child| child.local_uid == endpoint.child_uid)
        .ok_or_else(|| SandError::new("connection names an unknown child"))?;
    let child_definition = &graph.definitions[&child.definition.uid];
    match direction {
        PortDirection::Input => child_definition
            .inputs
            .iter()
            .find(|port| port.name == endpoint.port)
            .map(|port| port.value_type),
        PortDirection::Output => child_definition
            .outputs
            .iter()
            .find(|port| port.name == endpoint.port)
            .map(|port| port.value_type),
    }
    .ok_or_else(|| SandError::new("connection names an unknown child port"))
}

fn validate_inputs(
    definition: &SandDefinition,
    values: &BTreeMap<String, SandValue>,
    require_all: bool,
) -> Result<(), SandError> {
    for (name, value) in values {
        let input = definition
            .inputs
            .iter()
            .find(|input| input.name == *name)
            .ok_or_else(|| SandError::new("input value names an unknown port"))?;
        validate_typed_value(input.value_type, value)?;
    }
    if require_all
        && definition
            .inputs
            .iter()
            .any(|input| input.required && !values.contains_key(&input.name))
    {
        return Err(SandError::new("mount omits a required input"));
    }
    Ok(())
}

fn validate_configuration(
    definition: &SandDefinition,
    values: &BTreeMap<String, SandValue>,
) -> Result<(), SandError> {
    for (name, value) in values {
        let field = definition
            .configuration
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| SandError::new("configuration names an unknown field"))?;
        validate_typed_value(field.value_type, value)?;
    }
    Ok(())
}

fn validate_typed_value(expected: ValueType, value: &SandValue) -> Result<(), SandError> {
    value.validate()?;
    if value.value_type() != expected {
        return Err(SandError::new("value type does not match its declaration"));
    }
    Ok(())
}

fn validate_capability(capability: &SandCapability) -> Result<(), SandError> {
    match capability {
        SandCapability::EmitEvent { event } => validate_identifier("event capability", event),
        SandCapability::RequestAction { action } => {
            validate_identifier("Action capability", action)
        }
        SandCapability::Media { medium } => validate_identifier("media capability", medium),
        _ => Ok(()),
    }
}

fn require_capability(
    capabilities: &BTreeSet<SandCapability>,
    required: &SandCapability,
) -> Result<(), SandError> {
    if capabilities.contains(required) {
        Ok(())
    } else {
        Err(SandError::new("Sand lacks a required capability"))
    }
}

fn validate_envelope(version: u32, instance_uid: &str, sequence: u64) -> Result<(), SandError> {
    if version != SAND_ABI_VERSION {
        return Err(SandError::new(format!(
            "unsupported Sand ABI version {version}"
        )));
    }
    validate_identifier("Sand instance", instance_uid)?;
    if sequence == 0 {
        return Err(SandError::new("Sand ABI sequence must be positive"));
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<(), SandError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(SandError::new(format!("invalid {label} identity")));
    }
    Ok(())
}

fn validate_visible_text(label: &str, value: &str) -> Result<(), SandError> {
    if value.trim().is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(SandError::new(format!("invalid {label}")));
    }
    Ok(())
}

fn validate_transform(transform: Transform2d) -> Result<(), SandError> {
    let values = [
        transform.x,
        transform.y,
        transform.width,
        transform.height,
        transform.rotation_radians,
    ];
    if values.iter().any(|value| !value.is_finite())
        || transform.width <= 0.0
        || transform.height <= 0.0
    {
        return Err(SandError::new("child transform is invalid"));
    }
    Ok(())
}

fn validate_asset_path(value: &str) -> Result<(), SandError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 1024
        || value.contains("://")
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(SandError::new("asset path must be relative and local"));
    }
    Ok(())
}

fn validate_media_type(value: &str) -> Result<(), SandError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'+' | b'.'))
    {
        return Err(SandError::new("asset media type is invalid"));
    }
    Ok(())
}

fn unique<'a>(label: &str, values: impl Iterator<Item = &'a str>) -> Result<(), SandError> {
    let mut unique = BTreeSet::new();
    for value in values {
        if !unique.insert(value) {
            return Err(SandError::new(format!("duplicate {label} {value}")));
        }
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button_definition() -> SandDefinition {
        SandDefinition {
            uid: "button".into(),
            revision: 1,
            display_name: "Button".into(),
            element: SandElement::Button,
            inputs: vec![InputPort {
                name: "label".into(),
                value_type: ValueType::Text,
                required: true,
                default: None,
            }],
            outputs: vec![OutputPort {
                name: "clicked".into(),
                value_type: ValueType::Record,
            }],
            children: Vec::new(),
            connections: Vec::new(),
            exports: Vec::new(),
            behaviors: vec![BehaviorBinding::Declarative {
                behavior: DeclarativeBehavior::EmitEvent {
                    source_output: "clicked".into(),
                    event: "record-clicked".into(),
                },
            }],
            configuration: Vec::new(),
            style: StyleLayer::default(),
            accessibility: AccessibilitySpec {
                role: AccessibilityRole::Button,
                label: "Open Record".into(),
                description: None,
                live: false,
            },
            capabilities: BTreeSet::from([SandCapability::EmitEvent {
                event: "record-clicked".into(),
            }]),
            projections: vec![ProjectionManifest {
                key: "native".into(),
                kind: ProjectionKind::NativeRetained,
                isolation: Isolation::Trusted,
                required: BTreeSet::from([RendererCapability::RetainedControls]),
                capabilities: BTreeSet::from([SandCapability::EmitEvent {
                    event: "record-clicked".into(),
                }]),
                assets: Vec::new(),
                projected_nodes: BTreeSet::from(["root".into()]),
            }],
        }
    }

    #[test]
    fn authoritative_graph_and_abi_fail_closed() {
        let definition = button_definition();
        let graph = DefinitionGraph {
            schema_version: SAND_SCHEMA_VERSION,
            definitions: BTreeMap::from([("button".into(), definition.clone())]),
        };
        graph.validate().unwrap();
        let valid = SandOutboundEnvelope {
            abi_version: SAND_ABI_VERSION,
            instance_uid: "button-1".into(),
            sequence: 1,
            request: SandHostRequest::EmitOutput {
                port: "clicked".into(),
                value: SandValue::Record("record-a".into()),
            },
        };
        valid.validate(&definition).unwrap();
        let mut encoded = serde_json::to_value(&valid).unwrap();
        encoded["unknown"] = serde_json::Value::Bool(true);
        assert!(
            SandOutboundEnvelope::decode_and_validate(
                &serde_json::to_vec(&encoded).unwrap(),
                &definition
            )
            .is_err()
        );
        let mut invalid = valid;
        invalid.abi_version = 99;
        assert!(invalid.validate(&definition).is_err());
    }

    #[test]
    fn package_hashes_and_vendored_credits_are_enforced() {
        let definition = button_definition();
        let mut package = SandPackage {
            schema_version: SAND_SCHEMA_VERSION,
            uid: "button-package".into(),
            graph: DefinitionGraph {
                schema_version: SAND_SCHEMA_VERSION,
                definitions: BTreeMap::from([("button".into(), definition)]),
            },
            assets: Vec::new(),
        };
        package.validate().unwrap();
        package.assets.push(SandAsset {
            path: "vendor/widget.js".into(),
            kind: AssetKind::JavaScriptModule,
            media_type: "text/javascript".into(),
            sha256: sha256(b"export{}"),
            provenance: AssetProvenance::Vendored {
                project: "Widget".into(),
                source: "https://example.com/widget".into(),
                license_asset: "vendor/LICENSE".into(),
                credit: "Widget authors".into(),
            },
            bytes: b"export{}".to_vec(),
        });
        assert!(package.validate().is_err());
    }

    #[test]
    fn generated_contract_schemas_are_versioned() {
        let schemas = contract_schemas().unwrap();
        assert_eq!(schemas.schema_version, SAND_SCHEMA_VERSION);
        assert!(schemas.package.get("$schema").is_some());
        assert!(schemas.instance.to_string().contains("projection"));
        assert!(schemas.outbound.to_string().contains("abi_version"));
    }

    #[test]
    fn javascript_import_graph_is_static_and_relative() {
        assert_eq!(
            static_module_specifiers(
                "import{x}from'./x.js';import'./side.js';export{y}from'./y.js'"
            )
            .unwrap(),
            vec!["./x.js", "./side.js", "./y.js"]
        );
        assert!(static_module_specifiers("import('./late.js')").is_err());
    }

    #[test]
    fn persisted_instance_selects_projection_without_storing_runtime_handles() {
        let definition = button_definition();
        let graph = DefinitionGraph {
            schema_version: SAND_SCHEMA_VERSION,
            definitions: BTreeMap::from([("button".into(), definition)]),
        };
        let instance = SandInstance {
            uid: "button-1".into(),
            definition: DefinitionRef {
                uid: "button".into(),
                revision: 1,
            },
            projection: "native".into(),
            inputs: BTreeMap::from([("label".into(), SandValue::Text("Open".into()))]),
            configuration: BTreeMap::new(),
            style: StyleLayer::default(),
        };
        let capabilities = BTreeSet::from([RendererCapability::RetainedControls]);
        instance.validate(&graph, &capabilities).unwrap();
        let persisted = serde_json::to_string(&instance).unwrap();
        assert!(!persisted.contains("renderer_handle"));
        let runtime = RuntimeSandInstance::mount(&instance, &graph, &capabilities, 7).unwrap();
        assert_eq!(runtime.renderer_handle, 7);
    }
}
