use crate::{
    composition::{
        CompositionArtifact, CompositionBinding, CompositionPlacement, DefinitionLineage,
        DefinitionOrigin, DefinitionRevisionRecord, DefinitionUpdate, NodeAddress,
        composition_workbench_fixture, composition_workbench_package,
    },
    primitive_gallery::{PRIMITIVE_COUNT, primitive_gallery_package},
    sand::{
        AccessibilityRole, AccessibilitySpec, BehaviorBinding, DeclarativeBehavior,
        DefinitionChild, DefinitionRef, ExportedPort, Isolation, OutputPort, PortDirection,
        ProjectionKind, ProjectionManifest, RendererCapability, SAND_ABI_VERSION,
        SAND_SCHEMA_VERSION, SandCapability, SandDefinition, SandElement, SandPackage, SandValue,
        Transform2d, ValueType,
    },
    style::{
        ResolvedStyle, STYLE_CONTRACT_VERSION, STYLE_TOKEN_SPECS, ScopedStyleLayer, StyleLayer,
        StyleScope, StyleValue, StyleValueKind, ThemeManifest, lynx_theme, partial_theme_fixture,
        resolve_style,
    },
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Display, Formatter},
    fs,
    path::{Path, PathBuf},
};

pub const CONFIGURATION_SCHEMA_VERSION: u32 = 1;
pub const EXTERNAL_AUTHOR_CONTRACT_VERSION: u32 = 1;
pub const LAUNCH_RECIPE_SCHEMA_VERSION: u32 = 1;
pub const CONFIGURATION_DEFINITION_COUNT: usize = PRIMITIVE_COUNT + 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigurationError {
    detail: String,
}

impl ConfigurationError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for ConfigurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for ConfigurationError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeIdentity {
    pub uid: String,
    pub manifest_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeCatalogEntry {
    pub identity: ThemeIdentity,
    pub manifest: ThemeManifest,
}

impl ThemeCatalogEntry {
    pub fn from_manifest(manifest: ThemeManifest) -> Result<Self, ConfigurationError> {
        manifest
            .validate()
            .map_err(|error| ConfigurationError::new(error.to_string()))?;
        Ok(Self {
            identity: ThemeIdentity {
                uid: manifest.uid.clone(),
                manifest_sha256: manifest_sha256(&manifest)?,
            },
            manifest,
        })
    }

    pub fn validate(&self) -> Result<(), ConfigurationError> {
        self.manifest
            .validate()
            .map_err(|error| ConfigurationError::new(error.to_string()))?;
        if self.identity.uid != self.manifest.uid
            || self.identity.manifest_sha256 != manifest_sha256(&self.manifest)?
        {
            return Err(ConfigurationError::new(
                "theme identity disagrees with its manifest",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopedCssPatch {
    pub root_uid: String,
    pub declarations: String,
}

impl ScopedCssPatch {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        validate_identifier("CSS root", &self.root_uid)?;
        validate_css_declarations(&self.declarations)
    }

    pub fn wrapped(&self) -> Result<String, ConfigurationError> {
        self.validate()?;
        Ok(format!(
            "[data-lince-root=\"{}\"]{{{}}}",
            self.root_uid, self.declarations
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LaunchReceipt {
    pub recipe_uid: String,
    pub domain_kind: String,
    pub domain_uid: String,
    pub placement_uids: Vec<String>,
    pub focus_count: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationDocument {
    pub schema_version: u32,
    pub uid: String,
    pub developer_mode: bool,
    pub mode: String,
    pub selected_theme: Option<ThemeIdentity>,
    pub themes: Vec<ThemeCatalogEntry>,
    pub workspace_style: StyleLayer,
    pub group_styles: BTreeMap<String, StyleLayer>,
    pub raw_css: BTreeMap<String, ScopedCssPatch>,
    pub launches: BTreeMap<String, LaunchReceipt>,
}

impl ConfigurationDocument {
    pub fn fixture() -> Result<Self, ConfigurationError> {
        let theme = ThemeCatalogEntry::from_manifest(partial_theme_fixture())?;
        Ok(Self {
            schema_version: CONFIGURATION_SCHEMA_VERSION,
            uid: "lince-interface-configuration".into(),
            developer_mode: false,
            mode: "dark".into(),
            selected_theme: None,
            themes: vec![theme],
            workspace_style: StyleLayer::default(),
            group_styles: BTreeMap::new(),
            raw_css: BTreeMap::new(),
            launches: BTreeMap::new(),
        })
    }

    pub fn validate(&self, composition: &CompositionArtifact) -> Result<(), ConfigurationError> {
        if self.schema_version != CONFIGURATION_SCHEMA_VERSION {
            return Err(ConfigurationError::new(
                "unsupported Configuration schema version",
            ));
        }
        validate_identifier("Configuration", &self.uid)?;
        let base = lynx_theme();
        if !base.modes.contains_key(&self.mode) {
            return Err(ConfigurationError::new("Configuration mode is unavailable"));
        }
        unique(
            "theme",
            self.themes.iter().map(|entry| entry.identity.uid.as_str()),
        )?;
        for entry in &self.themes {
            entry.validate()?;
        }
        if let Some(selected) = &self.selected_theme {
            self.theme(selected)?;
        }
        for group_uid in self.group_styles.keys() {
            validate_identifier("group style", group_uid)?;
        }
        if !self.developer_mode && !self.raw_css.is_empty() {
            return Err(ConfigurationError::new(
                "raw CSS exists while developer mode is disabled",
            ));
        }
        for (root_uid, patch) in &self.raw_css {
            if root_uid != &patch.root_uid {
                return Err(ConfigurationError::new("raw CSS root identity disagrees"));
            }
            patch.validate()?;
        }
        for (key, receipt) in &self.launches {
            validate_identifier("launch recipe", &receipt.recipe_uid)?;
            validate_identifier("domain kind", &receipt.domain_kind)?;
            validate_identifier("domain object", &receipt.domain_uid)?;
            if key != &domain_key(&receipt.domain_kind, &receipt.domain_uid)
                || receipt.placement_uids.is_empty()
                || receipt.focus_count == 0
            {
                return Err(ConfigurationError::new("launch receipt is invalid"));
            }
            for placement_uid in &receipt.placement_uids {
                if !composition
                    .document
                    .placements
                    .iter()
                    .any(|placement| &placement.instance_uid == placement_uid)
                {
                    return Err(ConfigurationError::new(
                        "launch receipt names a missing placement",
                    ));
                }
            }
        }
        for placement in &composition.document.placements {
            self.resolved_for(composition, &placement.instance_uid)?;
        }
        Ok(())
    }

    pub fn resolved_for(
        &self,
        composition: &CompositionArtifact,
        instance_uid: &str,
    ) -> Result<ResolvedStyle, ConfigurationError> {
        let placement = composition
            .document
            .placements
            .iter()
            .find(|placement| placement.instance_uid == instance_uid)
            .ok_or_else(|| ConfigurationError::new("style target placement is missing"))?;
        let definition = composition
            .catalog
            .definition(&placement.definition)
            .map_err(|error| ConfigurationError::new(error.to_string()))?;
        let projection = ScopedStyleLayer {
            scope: StyleScope::Projection,
            label: format!("{} projection", placement.projection),
            layer: StyleLayer::default(),
        };
        let definition = ScopedStyleLayer {
            scope: StyleScope::Definition,
            label: format!("{} definition", definition.uid),
            layer: definition.style.clone(),
        };
        let workspace = ScopedStyleLayer {
            scope: StyleScope::Workspace,
            label: self.uid.clone(),
            layer: self.workspace_style.clone(),
        };
        let group = ScopedStyleLayer {
            scope: StyleScope::Group,
            label: format!("{} group", placement.instance_uid),
            layer: self
                .group_styles
                .get(instance_uid)
                .cloned()
                .unwrap_or_default(),
        };
        let instance = ScopedStyleLayer {
            scope: StyleScope::Instance,
            label: format!("{} instance", placement.instance_uid),
            layer: placement.style.clone(),
        };
        resolve_style(
            &lynx_theme(),
            self.selected_theme
                .as_ref()
                .map(|identity| self.theme(identity).map(|entry| &entry.manifest))
                .transpose()?,
            &self.mode,
            &[projection, definition, workspace, group, instance],
        )
        .map_err(|error| ConfigurationError::new(error.to_string()))
    }

    pub fn projected_styles(
        &self,
        composition: &CompositionArtifact,
        instance_uid: &str,
    ) -> Result<Vec<ProjectedStyle>, ConfigurationError> {
        let style = self.resolved_for(composition, instance_uid)?;
        let declarations = style.css_declarations();
        Ok([
            ProjectionSurface::Native,
            ProjectionSurface::SharedHtml,
            ProjectionSurface::InstalledHtml,
            ProjectionSurface::BrowserDom,
        ]
        .into_iter()
        .map(|surface| ProjectedStyle {
            surface,
            contract_version: style.contract_version,
            declarations: declarations.clone(),
        })
        .collect())
    }

    pub fn css_for_root(
        &self,
        composition: &CompositionArtifact,
        instance_uid: &str,
        root_uid: &str,
    ) -> Result<String, ConfigurationError> {
        let mut declarations = self
            .resolved_for(composition, instance_uid)?
            .css_declarations();
        if let Some(patch) = self.raw_css.get(root_uid) {
            declarations.push_str(&patch.declarations);
        }
        Ok(declarations)
    }

    fn theme(&self, identity: &ThemeIdentity) -> Result<&ThemeCatalogEntry, ConfigurationError> {
        self.themes
            .iter()
            .find(|entry| &entry.identity == identity)
            .ok_or_else(|| ConfigurationError::new("selected theme identity is unavailable"))
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionSurface {
    Native,
    SharedHtml,
    InstalledHtml,
    BrowserDom,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectedStyle {
    pub surface: ProjectionSurface,
    pub contract_version: u32,
    pub declarations: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationArtifact {
    pub schema_version: u32,
    pub configuration: ConfigurationDocument,
    pub composition: CompositionArtifact,
}

impl ConfigurationArtifact {
    pub fn fixture() -> Result<Self, ConfigurationError> {
        let mut composition = composition_workbench_fixture()
            .map_err(|error| ConfigurationError::new(error.to_string()))?;
        let definition = configuration_sand_definition();
        composition.catalog.revisions.insert(
            definition.uid.clone(),
            BTreeMap::from([(
                definition.revision,
                DefinitionRevisionRecord {
                    definition: definition.clone(),
                    origin: DefinitionOrigin::Rust,
                    lineage: DefinitionLineage::CodeOwned {
                        constructor: "native::configuration".into(),
                    },
                },
            )]),
        );
        composition
            .catalog
            .active
            .insert(definition.uid.clone(), definition.revision);
        composition.document.placements.push(CompositionPlacement {
            instance_uid: "configuration-sand".into(),
            definition: DefinitionRef {
                uid: definition.uid,
                revision: definition.revision,
            },
            follow_active: true,
            projection: "native-retained".into(),
            transform: Transform2d {
                x: 820.0,
                y: 380.0,
                width: 390.0,
                height: 520.0,
                rotation_radians: 0.0,
            },
            locked: true,
            inputs: BTreeMap::new(),
            configuration: BTreeMap::new(),
            style: StyleLayer::default(),
        });
        let artifact = Self {
            schema_version: CONFIGURATION_SCHEMA_VERSION,
            configuration: ConfigurationDocument::fixture()?,
            composition,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if self.schema_version != CONFIGURATION_SCHEMA_VERSION {
            return Err(ConfigurationError::new(
                "unsupported Configuration artifact schema",
            ));
        }
        self.composition
            .validate()
            .map_err(|error| ConfigurationError::new(error.to_string()))?;
        self.configuration.validate(&self.composition)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, ConfigurationError> {
        self.validate()?;
        serde_json::to_vec_pretty(self)
            .map_err(|error| ConfigurationError::new(format!("serialize Configuration: {error}")))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, ConfigurationError> {
        let artifact = serde_json::from_slice::<Self>(bytes)
            .map_err(|error| ConfigurationError::new(format!("decode Configuration: {error}")))?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn apply_launch_recipe(
        &mut self,
        recipe: &DomainLaunchRecipe,
    ) -> Result<LaunchResult, ConfigurationError> {
        recipe.validate(&self.composition)?;
        let key = domain_key(&recipe.domain_kind, &recipe.domain_uid);
        if let Some(receipt) = self.configuration.launches.get_mut(&key) {
            if receipt.recipe_uid != recipe.uid {
                return Err(ConfigurationError::new(
                    "domain object is already owned by another launch recipe",
                ));
            }
            receipt.focus_count = receipt.focus_count.saturating_add(1);
            return Ok(LaunchResult::FocusedExisting {
                placement_uids: receipt.placement_uids.clone(),
            });
        }
        let mut placement_uids = Vec::new();
        for placement in &recipe.placements {
            let instance_uid = recipe_instance_uid(recipe, &placement.local_uid);
            placement_uids.push(instance_uid.clone());
            self.composition
                .document
                .placements
                .push(CompositionPlacement {
                    instance_uid,
                    definition: placement.definition.clone(),
                    follow_active: false,
                    projection: placement.projection.clone(),
                    transform: placement.transform,
                    locked: placement.locked,
                    inputs: placement.inputs.clone(),
                    configuration: placement.configuration.clone(),
                    style: placement.style.clone(),
                });
        }
        for input in &recipe.record_inputs {
            self.composition
                .document
                .bindings
                .push(CompositionBinding::ProteinRead {
                    uid: format!("{}--{}", key, input.uid),
                    protein_uid: input.protein_uid.clone(),
                    field: input.field.clone(),
                    value: SandValue::Record(input.record_uid.clone()),
                    target: NodeAddress {
                        instance_uid: recipe_instance_uid(recipe, &input.placement_uid),
                        node_path: input.node_path.clone(),
                    }
                    .port(input.port.clone()),
                });
        }
        for export in &recipe.exports {
            self.composition
                .document
                .bindings
                .push(CompositionBinding::ActionWrite {
                    uid: format!("{}--{}", key, export.uid),
                    source: NodeAddress {
                        instance_uid: recipe_instance_uid(recipe, &export.placement_uid),
                        node_path: export.node_path.clone(),
                    }
                    .port(export.port.clone()),
                    action: export.action.clone(),
                });
        }
        self.configuration.launches.insert(
            key,
            LaunchReceipt {
                recipe_uid: recipe.uid.clone(),
                domain_kind: recipe.domain_kind.clone(),
                domain_uid: recipe.domain_uid.clone(),
                placement_uids: placement_uids.clone(),
                focus_count: 1,
            },
        );
        self.validate()?;
        Ok(LaunchResult::Created { placement_uids })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LaunchRecipePlacement {
    pub local_uid: String,
    pub definition: DefinitionRef,
    pub projection: String,
    pub transform: crate::sand::Transform2d,
    pub locked: bool,
    pub inputs: BTreeMap<String, SandValue>,
    pub configuration: BTreeMap<String, SandValue>,
    pub style: StyleLayer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LaunchRecordInput {
    pub uid: String,
    pub protein_uid: String,
    pub field: String,
    pub record_uid: String,
    pub placement_uid: String,
    pub node_path: Vec<String>,
    pub port: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LaunchExport {
    pub uid: String,
    pub placement_uid: String,
    pub node_path: Vec<String>,
    pub port: String,
    pub action: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainLaunchRecipe {
    pub schema_version: u32,
    pub uid: String,
    pub domain_kind: String,
    pub domain_uid: String,
    pub placements: Vec<LaunchRecipePlacement>,
    pub record_inputs: Vec<LaunchRecordInput>,
    pub exports: Vec<LaunchExport>,
}

impl DomainLaunchRecipe {
    pub fn validate(&self, composition: &CompositionArtifact) -> Result<(), ConfigurationError> {
        if self.schema_version != LAUNCH_RECIPE_SCHEMA_VERSION {
            return Err(ConfigurationError::new(
                "unsupported launch recipe schema version",
            ));
        }
        validate_identifier("launch recipe", &self.uid)?;
        validate_identifier("domain kind", &self.domain_kind)?;
        validate_identifier("domain object", &self.domain_uid)?;
        if self.placements.is_empty() {
            return Err(ConfigurationError::new("launch recipe has no placements"));
        }
        unique(
            "recipe placement",
            self.placements
                .iter()
                .map(|placement| placement.local_uid.as_str()),
        )?;
        unique(
            "recipe input",
            self.record_inputs.iter().map(|input| input.uid.as_str()),
        )?;
        unique(
            "recipe export",
            self.exports.iter().map(|export| export.uid.as_str()),
        )?;
        let placements = self
            .placements
            .iter()
            .map(|placement| placement.local_uid.as_str())
            .collect::<BTreeSet<_>>();
        for placement in &self.placements {
            validate_identifier("recipe placement", &placement.local_uid)?;
            composition
                .catalog
                .definition(&placement.definition)
                .map_err(|error| ConfigurationError::new(error.to_string()))?;
        }
        for input in &self.record_inputs {
            validate_identifier("recipe input", &input.uid)?;
            validate_identifier("Protein", &input.protein_uid)?;
            validate_identifier("Protein field", &input.field)?;
            validate_identifier("Record", &input.record_uid)?;
            if !placements.contains(input.placement_uid.as_str()) {
                return Err(ConfigurationError::new(
                    "recipe input names an unknown placement",
                ));
            }
        }
        for export in &self.exports {
            validate_identifier("recipe export", &export.uid)?;
            validate_identifier("Action", &export.action)?;
            if !placements.contains(export.placement_uid.as_str()) {
                return Err(ConfigurationError::new(
                    "recipe export names an unknown placement",
                ));
            }
        }
        let mut candidate = ConfigurationArtifact {
            schema_version: CONFIGURATION_SCHEMA_VERSION,
            configuration: ConfigurationDocument::fixture()?,
            composition: composition.clone(),
        };
        candidate.configuration.launches.clear();
        let recipe_instances = self
            .placements
            .iter()
            .map(|placement| recipe_instance_uid(self, &placement.local_uid))
            .collect::<BTreeSet<_>>();
        candidate
            .composition
            .document
            .placements
            .retain(|placement| !recipe_instances.contains(&placement.instance_uid));
        let binding_prefix = format!("{}--", domain_key(&self.domain_kind, &self.domain_uid));
        candidate
            .composition
            .document
            .bindings
            .retain(|binding| !binding.uid().starts_with(&binding_prefix));
        candidate.apply_launch_recipe_without_validation(self)?;
        candidate
            .composition
            .validate()
            .map_err(|error| ConfigurationError::new(error.to_string()))
    }
}

impl ConfigurationArtifact {
    fn apply_launch_recipe_without_validation(
        &mut self,
        recipe: &DomainLaunchRecipe,
    ) -> Result<(), ConfigurationError> {
        for placement in &recipe.placements {
            self.composition
                .document
                .placements
                .push(CompositionPlacement {
                    instance_uid: recipe_instance_uid(recipe, &placement.local_uid),
                    definition: placement.definition.clone(),
                    follow_active: false,
                    projection: placement.projection.clone(),
                    transform: placement.transform,
                    locked: placement.locked,
                    inputs: placement.inputs.clone(),
                    configuration: placement.configuration.clone(),
                    style: placement.style.clone(),
                });
        }
        for input in &recipe.record_inputs {
            self.composition
                .document
                .bindings
                .push(CompositionBinding::ProteinRead {
                    uid: format!(
                        "{}--{}",
                        domain_key(&recipe.domain_kind, &recipe.domain_uid),
                        input.uid
                    ),
                    protein_uid: input.protein_uid.clone(),
                    field: input.field.clone(),
                    value: SandValue::Record(input.record_uid.clone()),
                    target: NodeAddress {
                        instance_uid: recipe_instance_uid(recipe, &input.placement_uid),
                        node_path: input.node_path.clone(),
                    }
                    .port(input.port.clone()),
                });
        }
        for export in &recipe.exports {
            self.composition
                .document
                .bindings
                .push(CompositionBinding::ActionWrite {
                    uid: format!(
                        "{}--{}",
                        domain_key(&recipe.domain_kind, &recipe.domain_uid),
                        export.uid
                    ),
                    source: NodeAddress {
                        instance_uid: recipe_instance_uid(recipe, &export.placement_uid),
                        node_path: export.node_path.clone(),
                    }
                    .port(export.port.clone()),
                    action: export.action.clone(),
                });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum LaunchResult {
    Created { placement_uids: Vec<String> },
    FocusedExisting { placement_uids: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalAuthorManifest {
    pub contract_version: u32,
    pub package_uid: String,
    pub package_graph_sha256: String,
    pub roots: Vec<DefinitionRef>,
}

impl ExternalAuthorManifest {
    pub fn for_package(
        package: &SandPackage,
        roots: Vec<DefinitionRef>,
    ) -> Result<Self, ConfigurationError> {
        let manifest = Self {
            contract_version: EXTERNAL_AUTHOR_CONTRACT_VERSION,
            package_uid: package.uid.clone(),
            package_graph_sha256: package
                .graph_sha256()
                .map_err(|error| ConfigurationError::new(error.to_string()))?,
            roots,
        };
        manifest.validate(package)?;
        Ok(manifest)
    }

    pub fn validate(&self, package: &SandPackage) -> Result<(), ConfigurationError> {
        if self.contract_version != EXTERNAL_AUTHOR_CONTRACT_VERSION {
            return Err(ConfigurationError::new(
                "unsupported external-author contract version",
            ));
        }
        package
            .validate()
            .map_err(|error| ConfigurationError::new(error.to_string()))?;
        if self.package_uid != package.uid
            || self.package_graph_sha256
                != package
                    .graph_sha256()
                    .map_err(|error| ConfigurationError::new(error.to_string()))?
            || self.roots.is_empty()
        {
            return Err(ConfigurationError::new(
                "external-author manifest disagrees with package identity",
            ));
        }
        for root in &self.roots {
            let definition = package
                .graph
                .definitions
                .get(&root.uid)
                .ok_or_else(|| ConfigurationError::new("external root is unavailable"))?;
            if definition.revision != root.revision {
                return Err(ConfigurationError::new(
                    "external root revision is unavailable",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TokenReference {
    pub contract_version: u32,
    pub default_theme_uid: String,
    pub tokens: Vec<TokenReferenceEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TokenReferenceEntry {
    pub name: String,
    pub family: String,
    pub kind: StyleValueKind,
    pub dark: StyleValue,
    pub light: StyleValue,
}

pub fn token_reference() -> Result<TokenReference, ConfigurationError> {
    let base = lynx_theme();
    let dark = resolve_style(&base, None, "dark", &[])
        .map_err(|error| ConfigurationError::new(error.to_string()))?;
    let light = resolve_style(&base, None, "light", &[])
        .map_err(|error| ConfigurationError::new(error.to_string()))?;
    let tokens = STYLE_TOKEN_SPECS
        .iter()
        .map(|specification| {
            Ok(TokenReferenceEntry {
                name: specification.name.into(),
                family: specification.family.into(),
                kind: specification.kind,
                dark: dark
                    .value(specification.name)
                    .map_err(|error| ConfigurationError::new(error.to_string()))?
                    .clone(),
                light: light
                    .value(specification.name)
                    .map_err(|error| ConfigurationError::new(error.to_string()))?
                    .clone(),
            })
        })
        .collect::<Result<Vec<_>, ConfigurationError>>()?;
    Ok(TokenReference {
        contract_version: STYLE_CONTRACT_VERSION,
        default_theme_uid: base.uid,
        tokens,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExternalAuthorSchemas {
    pub contract_version: u32,
    pub sand_schema_version: u32,
    pub sand_abi_version: u32,
    pub style_contract_version: u32,
    pub external_manifest: serde_json::Value,
    pub configuration: serde_json::Value,
    pub launch_recipe: serde_json::Value,
}

pub fn external_author_schemas() -> Result<ExternalAuthorSchemas, ConfigurationError> {
    Ok(ExternalAuthorSchemas {
        contract_version: EXTERNAL_AUTHOR_CONTRACT_VERSION,
        sand_schema_version: SAND_SCHEMA_VERSION,
        sand_abi_version: SAND_ABI_VERSION,
        style_contract_version: STYLE_CONTRACT_VERSION,
        external_manifest: serde_json::to_value(schema_for!(ExternalAuthorManifest))
            .map_err(|error| ConfigurationError::new(error.to_string()))?,
        configuration: serde_json::to_value(schema_for!(ConfigurationArtifact))
            .map_err(|error| ConfigurationError::new(error.to_string()))?,
        launch_recipe: serde_json::to_value(schema_for!(DomainLaunchRecipe))
            .map_err(|error| ConfigurationError::new(error.to_string()))?,
    })
}

pub fn external_author_fixture() -> Result<(SandPackage, ExternalAuthorManifest), ConfigurationError>
{
    let package = primitive_gallery_package();
    let manifest = ExternalAuthorManifest::for_package(
        &package,
        vec![DefinitionRef {
            uid: "button".into(),
            revision: 1,
        }],
    )?;
    Ok((package, manifest))
}

pub fn launch_recipe_fixture() -> DomainLaunchRecipe {
    DomainLaunchRecipe {
        schema_version: LAUNCH_RECIPE_SCHEMA_VERSION,
        uid: "conversation-workroom".into(),
        domain_kind: "conversation".into(),
        domain_uid: "fixture-conversation".into(),
        placements: vec![LaunchRecipePlacement {
            local_uid: "workroom".into(),
            definition: DefinitionRef {
                uid: "video-call-room".into(),
                revision: 1,
            },
            projection: "native-retained".into(),
            transform: crate::sand::Transform2d {
                x: 820.0,
                y: 130.0,
                width: 360.0,
                height: 220.0,
                rotation_radians: 0.0,
            },
            locked: true,
            inputs: BTreeMap::new(),
            configuration: BTreeMap::from([("compact".into(), SandValue::Boolean(true))]),
            style: StyleLayer::default(),
        }],
        record_inputs: vec![LaunchRecordInput {
            uid: "record".into(),
            protein_uid: "conversation-record".into(),
            field: "uid".into(),
            record_uid: "fixture-record".into(),
            placement_uid: "workroom".into(),
            node_path: Vec::new(),
            port: "record".into(),
        }],
        exports: vec![LaunchExport {
            uid: "open-record".into(),
            placement_uid: "workroom".into(),
            node_path: Vec::new(),
            port: "record-clicked".into(),
            action: "record.open".into(),
        }],
    }
}

pub fn configuration_sand_package() -> SandPackage {
    let mut package = composition_workbench_package();
    package.uid = "lince-configuration-sand".into();
    let definition = configuration_sand_definition();
    package
        .graph
        .definitions
        .insert(definition.uid.clone(), definition);
    package
}

pub fn configuration_sand_definition() -> SandDefinition {
    let operations = ConfigurationOperation::ALL;
    let mut children = Vec::with_capacity(operations.len() + 2);
    children.push(configuration_child(
        "background",
        "panel",
        0.0,
        0.0,
        390.0,
        520.0,
        0,
    ));
    children.push(configuration_child(
        "title", "title", 18.0, 14.0, 350.0, 28.0, 1,
    ));
    for (index, operation) in operations.into_iter().enumerate() {
        children.push(configuration_child(
            operation.uid(),
            "button",
            18.0,
            52.0 + index as f64 * 27.0,
            350.0,
            24.0,
            index as u32 + 2,
        ));
    }
    let outputs = operations
        .into_iter()
        .map(|operation| OutputPort {
            name: operation.uid().into(),
            value_type: ValueType::Record,
        })
        .collect::<Vec<_>>();
    let exports = operations
        .into_iter()
        .map(|operation| ExportedPort {
            name: operation.uid().into(),
            child_uid: operation.uid().into(),
            child_port: "pressed".into(),
            direction: PortDirection::Output,
        })
        .collect::<Vec<_>>();
    let capabilities = operations
        .into_iter()
        .map(|operation| SandCapability::EmitEvent {
            event: format!("configuration.{}", operation.uid()),
        })
        .collect::<BTreeSet<_>>();
    let behaviors = operations
        .into_iter()
        .map(|operation| BehaviorBinding::Declarative {
            behavior: DeclarativeBehavior::EmitEvent {
                source_output: operation.uid().into(),
                event: format!("configuration.{}", operation.uid()),
            },
        })
        .collect::<Vec<_>>();
    let projected_nodes = std::iter::once("root".to_string())
        .chain(children.iter().map(|child| child.local_uid.clone()))
        .collect::<BTreeSet<_>>();
    SandDefinition {
        uid: "configuration".into(),
        revision: 1,
        display_name: "Configuration".into(),
        element: SandElement::Compound,
        inputs: Vec::new(),
        outputs,
        children,
        connections: Vec::new(),
        exports,
        behaviors,
        configuration: Vec::new(),
        style: StyleLayer {
            values: BTreeMap::from([
                ("--lynx-gap-content".into(), StyleValue::LengthPx(3.0)),
                ("--lynx-padding-record".into(), StyleValue::LengthPx(8.0)),
            ]),
        },
        accessibility: AccessibilitySpec {
            role: AccessibilityRole::Group,
            label: "Configuration".into(),
            description: Some(
                "Edits inherited and local Sand presentation and authoring contracts".into(),
            ),
            live: true,
        },
        capabilities,
        projections: vec![
            ProjectionManifest {
                key: "native-retained".into(),
                kind: ProjectionKind::NativeRetained,
                isolation: Isolation::Trusted,
                required: BTreeSet::from([
                    RendererCapability::RetainedControls,
                    RendererCapability::Accessibility,
                ]),
                capabilities: BTreeSet::new(),
                assets: Vec::new(),
                projected_nodes: projected_nodes.clone(),
            },
            ProjectionManifest {
                key: "browser-dom".into(),
                kind: ProjectionKind::BrowserDom,
                isolation: Isolation::Trusted,
                required: BTreeSet::from([
                    RendererCapability::BrowserDom,
                    RendererCapability::Accessibility,
                ]),
                capabilities: BTreeSet::new(),
                assets: Vec::new(),
                projected_nodes,
            },
        ],
    }
}

fn configuration_child(
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationOperation {
    ToggleMode,
    SelectTheme,
    SetTokenFamilies,
    SetGroupGap,
    SetInstanceRadius,
    CreateExtension,
    EditDefinitionDefault,
    EditBehaviorAndPort,
    EditIsolationAndCapability,
    PreviewRawCss,
    RefuseUnsafeCss,
    ClearRawCss,
    ResetToInherited,
    Undo,
    LaunchDomainCompound,
    SaveAndReopen,
}

impl ConfigurationOperation {
    pub const ALL: [Self; 16] = [
        Self::ToggleMode,
        Self::SelectTheme,
        Self::SetTokenFamilies,
        Self::SetGroupGap,
        Self::SetInstanceRadius,
        Self::CreateExtension,
        Self::EditDefinitionDefault,
        Self::EditBehaviorAndPort,
        Self::EditIsolationAndCapability,
        Self::PreviewRawCss,
        Self::RefuseUnsafeCss,
        Self::ClearRawCss,
        Self::ResetToInherited,
        Self::Undo,
        Self::LaunchDomainCompound,
        Self::SaveAndReopen,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleMode => "toggle Dark / Light mode",
            Self::SelectTheme => "select theme by identity",
            Self::SetTokenFamilies => "edit spacing, margin, type, density and thickness",
            Self::SetGroupGap => "set group gap override",
            Self::SetInstanceRadius => "set instance radius override",
            Self::CreateExtension => "declare and apply a typed extension",
            Self::EditDefinitionDefault => "edit definition configuration default",
            Self::EditBehaviorAndPort => "add typed port and Behavior",
            Self::EditIsolationAndCapability => "edit isolation and capability grant",
            Self::PreviewRawCss => "preview safe developer CSS",
            Self::RefuseUnsafeCss => "attempt unsafe developer CSS",
            Self::ClearRawCss => "safe reset developer CSS",
            Self::ResetToInherited => "reset instance radius to inherited",
            Self::Undo => "undo previous accepted edit",
            Self::LaunchDomainCompound => "launch or focus domain compound",
            Self::SaveAndReopen => "save and reopen Configuration",
        }
    }

    pub fn uid(self) -> &'static str {
        match self {
            Self::ToggleMode => "toggle-mode",
            Self::SelectTheme => "select-theme",
            Self::SetTokenFamilies => "set-token-families",
            Self::SetGroupGap => "set-group-gap",
            Self::SetInstanceRadius => "set-instance-radius",
            Self::CreateExtension => "create-extension",
            Self::EditDefinitionDefault => "edit-definition-default",
            Self::EditBehaviorAndPort => "edit-behavior-port",
            Self::EditIsolationAndCapability => "edit-isolation-capability",
            Self::PreviewRawCss => "preview-raw-css",
            Self::RefuseUnsafeCss => "refuse-unsafe-css",
            Self::ClearRawCss => "clear-raw-css",
            Self::ResetToInherited => "reset-to-inherited",
            Self::Undo => "undo",
            Self::LaunchDomainCompound => "launch-domain-compound",
            Self::SaveAndReopen => "save-reopen",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ConfigurationFacts {
    pub accepted_edits: u64,
    pub refused_edits: u64,
    pub undo_count: u64,
    pub save_reopens: u64,
    pub live_previews: u64,
    pub definition_publications: u64,
    pub launches_created: u64,
    pub launches_focused: u64,
    pub persisted_bytes: usize,
}

pub struct ConfigurationWorkbenchState {
    artifact: ConfigurationArtifact,
    path: PathBuf,
    history: Vec<ConfigurationArtifact>,
    selected: usize,
    last_result: String,
    facts: ConfigurationFacts,
}

impl ConfigurationWorkbenchState {
    pub fn open(path: PathBuf) -> Result<Self, ConfigurationError> {
        let fallback = ConfigurationArtifact::fixture()?;
        let (artifact, last_result, write_default) = match fs::read(&path) {
            Ok(bytes) => match ConfigurationArtifact::from_json(&bytes) {
                Ok(artifact) => (artifact, "restored persisted Configuration".into(), false),
                Err(error) => (
                    fallback,
                    format!("SAFE DEFAULT · refused persisted Configuration: {error}"),
                    false,
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (fallback, "new Configuration".into(), true)
            }
            Err(error) => (
                fallback,
                format!("SAFE DEFAULT · Configuration unreadable: {error}"),
                false,
            ),
        };
        let mut state = Self {
            artifact,
            path,
            history: Vec::new(),
            selected: 0,
            last_result,
            facts: ConfigurationFacts::default(),
        };
        if write_default {
            state.persist()?;
        }
        Ok(state)
    }

    pub fn fixture(path: PathBuf) -> Result<Self, ConfigurationError> {
        let mut state = Self {
            artifact: ConfigurationArtifact::fixture()?,
            path,
            history: Vec::new(),
            selected: 0,
            last_result: "Configuration fixture ready".into(),
            facts: ConfigurationFacts::default(),
        };
        state.persist()?;
        Ok(state)
    }

    pub fn focus_next(&mut self, reverse: bool) {
        self.selected = if reverse {
            self.selected
                .checked_sub(1)
                .unwrap_or(ConfigurationOperation::ALL.len() - 1)
        } else {
            (self.selected + 1) % ConfigurationOperation::ALL.len()
        };
    }

    pub fn focus_at(&mut self, index: usize) {
        if index < ConfigurationOperation::ALL.len() {
            self.selected = index;
        }
    }

    pub fn selected(&self) -> ConfigurationOperation {
        ConfigurationOperation::ALL[self.selected]
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn artifact(&self) -> &ConfigurationArtifact {
        &self.artifact
    }

    pub fn facts(&self) -> &ConfigurationFacts {
        &self.facts
    }

    pub fn resolved_style(&self) -> Result<ResolvedStyle, ConfigurationError> {
        self.artifact
            .configuration
            .resolved_for(&self.artifact.composition, "room-native")
    }

    pub fn css_declarations(&self) -> Result<String, ConfigurationError> {
        self.artifact.configuration.css_for_root(
            &self.artifact.composition,
            "room-native",
            "installed-gallery",
        )
    }

    pub fn activate(&mut self) -> Result<(), ConfigurationError> {
        let operation = self.selected();
        match operation {
            ConfigurationOperation::Undo => self.undo(),
            ConfigurationOperation::SaveAndReopen => self.save_and_reopen(),
            ConfigurationOperation::RefuseUnsafeCss => self.refuse_unsafe_css(),
            _ => {
                let mut candidate = self.artifact.clone();
                let result = apply_operation(&mut candidate, operation)?;
                candidate.validate()?;
                let persisted_bytes = persist_artifact(&self.path, &candidate)?;
                self.history.push(self.artifact.clone());
                self.artifact = candidate;
                self.facts.persisted_bytes = persisted_bytes;
                self.facts.accepted_edits = self.facts.accepted_edits.saturating_add(1);
                self.facts.live_previews = self.facts.live_previews.saturating_add(1);
                if matches!(
                    operation,
                    ConfigurationOperation::EditDefinitionDefault
                        | ConfigurationOperation::EditBehaviorAndPort
                        | ConfigurationOperation::EditIsolationAndCapability
                ) {
                    self.facts.definition_publications =
                        self.facts.definition_publications.saturating_add(1);
                }
                match result {
                    OperationResult::Edited(detail) => self.last_result = detail,
                    OperationResult::Launch(LaunchResult::Created { placement_uids }) => {
                        self.facts.launches_created = self.facts.launches_created.saturating_add(1);
                        self.last_result = format!("launched {} placement", placement_uids.len());
                    }
                    OperationResult::Launch(LaunchResult::FocusedExisting { placement_uids }) => {
                        self.facts.launches_focused = self.facts.launches_focused.saturating_add(1);
                        self.last_result =
                            format!("focused {} existing placement", placement_uids.len());
                    }
                }
                Ok(())
            }
        }
    }

    pub fn exercise(&mut self) -> Result<(), ConfigurationError> {
        for index in 0..ConfigurationOperation::ALL.len() {
            self.selected = index;
            self.activate()?;
        }
        self.selected = 0;
        let mut candidate = self.artifact.clone();
        let result = candidate.apply_launch_recipe(&launch_recipe_fixture())?;
        if matches!(result, LaunchResult::FocusedExisting { .. }) {
            let persisted_bytes = persist_artifact(&self.path, &candidate)?;
            self.history.push(self.artifact.clone());
            self.artifact = candidate;
            self.facts.persisted_bytes = persisted_bytes;
            self.facts.accepted_edits = self.facts.accepted_edits.saturating_add(1);
            self.facts.launches_focused = self.facts.launches_focused.saturating_add(1);
        }
        Ok(())
    }

    pub fn status_line(&self) -> String {
        let resolved = self.resolved_style();
        let accent = resolved
            .as_ref()
            .ok()
            .and_then(|style| style.origin("--lynx-accent").ok())
            .map(|origin| origin.label.as_str())
            .unwrap_or("unavailable");
        format!(
            "{} · {} accepted · {} refused · {} undo · {} reopen · accent from {}",
            self.last_result,
            self.facts.accepted_edits,
            self.facts.refused_edits,
            self.facts.undo_count,
            self.facts.save_reopens,
            accent,
        )
    }

    pub fn origin_lines(&self) -> Vec<String> {
        let Ok(style) = self.resolved_style() else {
            return vec!["REFUSED unresolved style".into()];
        };
        [
            "--lynx-accent",
            "--lynx-gap-content",
            "--lynx-margin-sand",
            "--lynx-radius-control",
            "--lynx-font-body",
            "--lynx-density-scale",
        ]
        .into_iter()
        .map(|name| match (style.value(name), style.origin(name)) {
            (Ok(value), Ok(origin)) => {
                format!("{name} = {value:?} · {:?} / {}", origin.scope, origin.label)
            }
            _ => format!("{name} = REFUSED"),
        })
        .collect()
    }

    fn refuse_unsafe_css(&mut self) -> Result<(), ConfigurationError> {
        let patch = ScopedCssPatch {
            root_uid: "installed-gallery".into(),
            declarations:
                "@import url(https://example.com/theme.css);position:fixed;z-index:999999;".into(),
        };
        if patch.validate().is_ok() {
            return Err(ConfigurationError::new("unsafe CSS was admitted"));
        }
        self.facts.refused_edits = self.facts.refused_edits.saturating_add(1);
        self.last_result = "REFUSED unsafe CSS · previous preview remains live".into();
        Ok(())
    }

    fn undo(&mut self) -> Result<(), ConfigurationError> {
        let Some(previous) = self.history.last().cloned() else {
            self.last_result = "nothing to undo".into();
            return Ok(());
        };
        previous.validate()?;
        let persisted_bytes = persist_artifact(&self.path, &previous)?;
        self.history.pop();
        self.artifact = previous;
        self.facts.persisted_bytes = persisted_bytes;
        self.facts.undo_count = self.facts.undo_count.saturating_add(1);
        self.facts.live_previews = self.facts.live_previews.saturating_add(1);
        self.last_result = "undid previous accepted edit".into();
        Ok(())
    }

    fn save_and_reopen(&mut self) -> Result<(), ConfigurationError> {
        self.persist()?;
        let bytes = fs::read(&self.path)
            .map_err(|error| ConfigurationError::new(format!("read Configuration: {error}")))?;
        self.artifact = ConfigurationArtifact::from_json(&bytes)?;
        self.facts.save_reopens = self.facts.save_reopens.saturating_add(1);
        self.last_result = "saved and reopened exact Configuration".into();
        Ok(())
    }

    fn persist(&mut self) -> Result<(), ConfigurationError> {
        self.facts.persisted_bytes = persist_artifact(&self.path, &self.artifact)?;
        Ok(())
    }
}

fn persist_artifact(
    path: &Path,
    artifact: &ConfigurationArtifact,
) -> Result<usize, ConfigurationError> {
    let bytes = artifact.to_json()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ConfigurationError::new(format!("create Configuration directory: {error}"))
        })?;
    }
    let temporary = path.with_extension("json.pending");
    fs::write(&temporary, &bytes).map_err(|error| {
        ConfigurationError::new(format!("write Configuration candidate: {error}"))
    })?;
    fs::rename(&temporary, path)
        .map_err(|error| ConfigurationError::new(format!("commit Configuration: {error}")))?;
    Ok(bytes.len())
}

enum OperationResult {
    Edited(String),
    Launch(LaunchResult),
}

fn apply_operation(
    artifact: &mut ConfigurationArtifact,
    operation: ConfigurationOperation,
) -> Result<OperationResult, ConfigurationError> {
    match operation {
        ConfigurationOperation::ToggleMode => {
            artifact.configuration.mode = if artifact.configuration.mode == "dark" {
                "light"
            } else {
                "dark"
            }
            .into();
            Ok(OperationResult::Edited(format!(
                "mode {}",
                artifact.configuration.mode
            )))
        }
        ConfigurationOperation::SelectTheme => {
            artifact.configuration.selected_theme =
                if artifact.configuration.selected_theme.is_some() {
                    None
                } else {
                    artifact
                        .configuration
                        .themes
                        .first()
                        .map(|entry| entry.identity.clone())
                };
            Ok(OperationResult::Edited("theme identity selected".into()))
        }
        ConfigurationOperation::SetTokenFamilies => {
            artifact.configuration.workspace_style.values.extend([
                ("--lynx-space-2".into(), StyleValue::LengthPx(9.0)),
                ("--lynx-margin-sand".into(), StyleValue::LengthPx(3.0)),
                ("--lynx-border-hairline".into(), StyleValue::LengthPx(1.0)),
                ("--lynx-text-size-body".into(), StyleValue::LengthPx(15.0)),
                ("--lynx-density-scale".into(), StyleValue::Scalar(0.92)),
            ]);
            Ok(OperationResult::Edited(
                "workspace token families previewed".into(),
            ))
        }
        ConfigurationOperation::SetGroupGap => {
            artifact
                .configuration
                .group_styles
                .entry("room-native".into())
                .or_default()
                .values
                .insert("--lynx-gap-content".into(), StyleValue::LengthPx(11.0));
            Ok(OperationResult::Edited(
                "group gap override previewed".into(),
            ))
        }
        ConfigurationOperation::SetInstanceRadius => {
            placement_mut(artifact, "room-native")?
                .style
                .values
                .insert("--lynx-radius-control".into(), StyleValue::LengthPx(13.0));
            Ok(OperationResult::Edited(
                "instance radius override previewed".into(),
            ))
        }
        ConfigurationOperation::CreateExtension => {
            let index = selected_theme_index(&artifact.configuration)?;
            let mut manifest = artifact.configuration.themes[index].manifest.clone();
            manifest
                .extensions
                .insert("--lynx-local-workroom-signal".into(), StyleValueKind::Color);
            manifest.common.values.insert(
                "--lynx-local-workroom-signal".into(),
                StyleValue::Color("#918CF5".into()),
            );
            let entry = ThemeCatalogEntry::from_manifest(manifest)?;
            artifact.configuration.selected_theme = Some(entry.identity.clone());
            artifact.configuration.themes[index] = entry;
            artifact.configuration.workspace_style.values.insert(
                "--lynx-local-workroom-signal".into(),
                StyleValue::Color("#A7B4C2".into()),
            );
            Ok(OperationResult::Edited(
                "typed extension declared and applied".into(),
            ))
        }
        ConfigurationOperation::EditDefinitionDefault => {
            let update = publish_definition(artifact, "video-call-room", |definition| {
                if let Some(field) = definition
                    .configuration
                    .iter_mut()
                    .find(|field| field.name == "compact")
                {
                    field.default = Some(SandValue::Boolean(true));
                }
            })?;
            Ok(OperationResult::Edited(format_update(
                "definition default",
                update,
            )))
        }
        ConfigurationOperation::EditBehaviorAndPort => {
            let update = publish_definition(artifact, "video-call-room", |definition| {
                if !definition
                    .outputs
                    .iter()
                    .any(|output| output.name == "configured")
                {
                    definition.outputs.push(OutputPort {
                        name: "configured".into(),
                        value_type: ValueType::Boolean,
                    });
                    definition.behaviors.push(BehaviorBinding::Declarative {
                        behavior: DeclarativeBehavior::SetLocalState {
                            source_output: "configured".into(),
                            key: "configuration-applied".into(),
                            value: SandValue::Boolean(true),
                        },
                    });
                }
            })?;
            Ok(OperationResult::Edited(format_update(
                "typed port and Behavior",
                update,
            )))
        }
        ConfigurationOperation::EditIsolationAndCapability => {
            let update = publish_definition(artifact, "video-call-room", |definition| {
                definition.capabilities.insert(SandCapability::LocalStorage);
                if let Some(projection) = definition
                    .projections
                    .iter_mut()
                    .find(|projection| projection.key == "installed-html")
                {
                    projection.isolation = Isolation::InstalledHtml;
                    projection.capabilities.insert(SandCapability::LocalStorage);
                }
            })?;
            Ok(OperationResult::Edited(format_update(
                "Installed HTML isolation and storage capability",
                update,
            )))
        }
        ConfigurationOperation::PreviewRawCss => {
            artifact.configuration.developer_mode = true;
            let patch = ScopedCssPatch {
                root_uid: "installed-gallery".into(),
                declarations: "letter-spacing:0.01em;border-radius:3px;".into(),
            };
            patch.validate()?;
            artifact
                .configuration
                .raw_css
                .insert(patch.root_uid.clone(), patch);
            Ok(OperationResult::Edited(
                "developer CSS previewed in disposable root".into(),
            ))
        }
        ConfigurationOperation::ClearRawCss => {
            artifact.configuration.raw_css.clear();
            artifact.configuration.developer_mode = false;
            Ok(OperationResult::Edited(
                "developer CSS reset from native Configuration Sand".into(),
            ))
        }
        ConfigurationOperation::ResetToInherited => {
            placement_mut(artifact, "room-native")?
                .style
                .values
                .remove("--lynx-radius-control");
            Ok(OperationResult::Edited(
                "instance radius reset to inherited origin".into(),
            ))
        }
        ConfigurationOperation::LaunchDomainCompound => Ok(OperationResult::Launch(
            artifact.apply_launch_recipe(&launch_recipe_fixture())?,
        )),
        ConfigurationOperation::Undo
        | ConfigurationOperation::SaveAndReopen
        | ConfigurationOperation::RefuseUnsafeCss => {
            Err(ConfigurationError::new("operation needs workbench state"))
        }
    }
}

fn publish_definition(
    artifact: &mut ConfigurationArtifact,
    uid: &str,
    mutate: impl FnOnce(&mut crate::sand::SandDefinition),
) -> Result<DefinitionUpdate, ConfigurationError> {
    let update = artifact
        .composition
        .catalog
        .publish_shared(uid, mutate)
        .map_err(|error| ConfigurationError::new(error.to_string()))?;
    for placement in &mut artifact.composition.document.placements {
        if placement.follow_active
            && let Some([old, new]) = update.revisions.get(&placement.definition.uid)
            && placement.definition.revision == *old
        {
            placement.definition.revision = *new;
        }
    }
    Ok(update)
}

fn format_update(label: &str, update: DefinitionUpdate) -> String {
    format!(
        "published {label} across {} definition",
        update.revisions.len()
    )
}

fn placement_mut<'a>(
    artifact: &'a mut ConfigurationArtifact,
    uid: &str,
) -> Result<&'a mut CompositionPlacement, ConfigurationError> {
    artifact
        .composition
        .document
        .placements
        .iter_mut()
        .find(|placement| placement.instance_uid == uid)
        .ok_or_else(|| ConfigurationError::new("Configuration placement is unavailable"))
}

fn selected_theme_index(
    configuration: &ConfigurationDocument,
) -> Result<usize, ConfigurationError> {
    let selected = configuration
        .selected_theme
        .as_ref()
        .ok_or_else(|| ConfigurationError::new("select a theme before extending it"))?;
    configuration
        .themes
        .iter()
        .position(|entry| &entry.identity == selected)
        .ok_or_else(|| ConfigurationError::new("selected theme identity is unavailable"))
}

fn recipe_instance_uid(recipe: &DomainLaunchRecipe, local_uid: &str) -> String {
    format!(
        "{}--{}--{}",
        recipe.domain_kind, recipe.domain_uid, local_uid
    )
}

fn domain_key(kind: &str, uid: &str) -> String {
    format!("{kind}--{uid}")
}

fn manifest_sha256(manifest: &ThemeManifest) -> Result<String, ConfigurationError> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|error| ConfigurationError::new(format!("serialize theme: {error}")))?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn validate_identifier(label: &str, value: &str) -> Result<(), ConfigurationError> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err(ConfigurationError::new(format!("invalid {label} identity")));
    }
    Ok(())
}

fn unique<'a>(
    label: &str,
    values: impl Iterator<Item = &'a str>,
) -> Result<(), ConfigurationError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ConfigurationError::new(format!(
                "duplicate {label} {value}"
            )));
        }
    }
    Ok(())
}

fn validate_css_declarations(declarations: &str) -> Result<(), ConfigurationError> {
    if declarations.is_empty() || declarations.len() > 64 * 1024 {
        return Err(ConfigurationError::new("raw CSS size is invalid"));
    }
    let lower = declarations.to_ascii_lowercase();
    if [
        "@import",
        "url(",
        "src(",
        "http:",
        "https:",
        "blob:",
        "ftp:",
        "//",
        "javascript:",
        "data:",
        "file:",
        "chrome:",
        "resource:",
        "extension:",
        "expression(",
        "-moz-binding",
        "!important",
        "--lynx-stack-security",
    ]
    .iter()
    .any(|blocked| lower.contains(blocked))
        || declarations
            .chars()
            .any(|character| matches!(character, '{' | '}' | '@' | '<' | '>' | '\\'))
    {
        return Err(ConfigurationError::new(
            "raw CSS contains an escaping or executable construct",
        ));
    }
    let mut count = 0usize;
    for declaration in declarations.split(';') {
        if declaration.trim().is_empty() {
            continue;
        }
        count = count.saturating_add(1);
        let (property, value) = declaration
            .split_once(':')
            .ok_or_else(|| ConfigurationError::new("raw CSS declaration is malformed"))?;
        let property = property.trim();
        let value = value.trim();
        if !allowed_css_property(property)
            || value.is_empty()
            || value.len() > 1024
            || property != property.to_ascii_lowercase()
        {
            return Err(ConfigurationError::new(
                "raw CSS property or value is outside the presentation policy",
            ));
        }
    }
    if count == 0 || count > 256 {
        return Err(ConfigurationError::new(
            "raw CSS declaration count is invalid",
        ));
    }
    Ok(())
}

fn allowed_css_property(property: &str) -> bool {
    property.starts_with("--lynx-local-")
        || matches!(
            property,
            "color"
                | "background"
                | "background-color"
                | "border"
                | "border-color"
                | "border-style"
                | "border-width"
                | "border-radius"
                | "box-shadow"
                | "margin"
                | "margin-block"
                | "margin-inline"
                | "padding"
                | "padding-block"
                | "padding-inline"
                | "gap"
                | "row-gap"
                | "column-gap"
                | "font-family"
                | "font-size"
                | "font-style"
                | "font-weight"
                | "font-variant-numeric"
                | "line-height"
                | "letter-spacing"
                | "text-align"
                | "text-decoration"
                | "opacity"
                | "transform"
                | "transform-origin"
                | "filter"
                | "display"
                | "flex-direction"
                | "flex-wrap"
                | "align-items"
                | "justify-content"
                | "grid-template-columns"
                | "grid-template-rows"
                | "width"
                | "height"
                | "min-width"
                | "min-height"
                | "max-width"
                | "max-height"
                | "overflow"
        )
}

pub fn default_configuration_path() -> Result<PathBuf, ConfigurationError> {
    dirs::config_dir()
        .map(|directory| directory.join("lince/interface/configuration.json"))
        .ok_or_else(|| ConfigurationError::new("Lince Configuration directory is unavailable"))
}

pub fn external_author_readme() -> String {
    format!(
        "# Lince external Sand author kit\n\nContract version: {EXTERNAL_AUTHOR_CONTRACT_VERSION}\nSand schema: {SAND_SCHEMA_VERSION}\nSand ABI: {SAND_ABI_VERSION}\nStyle contract: {STYLE_CONTRACT_VERSION}\n\nAuthor ordinary HTML, CSS and native JavaScript modules as package assets. Declare one Installed HTML root, every semantic node, exact revisions, typed ports, capabilities and asset hashes in the package. Keep CSS and JavaScript external to markup. Relative static module imports must close inside the package. Website projections receive no Lince authority. Unknown versions, unknown fields, stale hashes, dynamic imports and undeclared capabilities fail closed. Maud is optional and never required outside Lince.\n"
    )
}

pub fn write_external_author_kit(root: &Path) -> Result<Vec<PathBuf>, ConfigurationError> {
    fs::create_dir_all(root)
        .map_err(|error| ConfigurationError::new(format!("create author kit: {error}")))?;
    let (package, manifest) = external_author_fixture()?;
    let mut stale = manifest.clone();
    stale.contract_version = EXTERNAL_AUTHOR_CONTRACT_VERSION.saturating_add(1);
    if stale.validate(&package).is_ok() {
        return Err(ConfigurationError::new(
            "stale external-author contract was admitted",
        ));
    }
    let files = [
        (
            root.join("schemas.json"),
            serde_json::to_vec_pretty(&external_author_schemas()?),
        ),
        (
            root.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest),
        ),
        (
            root.join("package.json"),
            serde_json::to_vec_pretty(&package),
        ),
        (
            root.join("unknown-version-manifest.json"),
            serde_json::to_vec_pretty(&stale),
        ),
        (
            root.join("token-reference.json"),
            serde_json::to_vec_pretty(&token_reference()?),
        ),
        (
            root.join("launch-recipe.json"),
            serde_json::to_vec_pretty(&launch_recipe_fixture()),
        ),
        (
            root.join("README.md"),
            Ok(external_author_readme().into_bytes()),
        ),
    ];
    let mut paths = Vec::new();
    for (path, bytes) in files {
        let bytes = bytes.map_err(|error| ConfigurationError::new(error.to_string()))?;
        fs::write(&path, bytes)
            .map_err(|error| ConfigurationError::new(format!("write author kit: {error}")))?;
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lince-interface-{}-{}-{name}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("configuration")
        ))
    }

    #[test]
    fn default_contract_generates_every_typed_token() {
        let reference = token_reference().unwrap();
        assert_eq!(reference.tokens.len(), STYLE_TOKEN_SPECS.len());
        assert_eq!(reference.tokens.len(), 92);
        assert!(
            reference
                .tokens
                .iter()
                .any(|token| token.name == "--lynx-margin-sand")
        );
        let package = configuration_sand_package();
        package.validate().unwrap();
        assert_eq!(
            package.graph.definitions.len(),
            CONFIGURATION_DEFINITION_COUNT
        );
    }

    #[test]
    fn broken_persisted_theme_falls_back_without_blocking_configuration() {
        let path = temporary_path("broken-theme");
        let mut artifact = ConfigurationArtifact::fixture().unwrap();
        let selected = artifact.configuration.themes[0].identity.clone();
        artifact.configuration.selected_theme = Some(ThemeIdentity {
            uid: selected.uid,
            manifest_sha256: "sha256:missing".into(),
        });
        fs::write(&path, serde_json::to_vec_pretty(&artifact).unwrap()).unwrap();
        let workbench = ConfigurationWorkbenchState::open(path.clone()).unwrap();
        assert!(workbench.resolved_style().is_ok());
        assert!(
            workbench
                .status_line()
                .contains("SAFE DEFAULT · refused persisted Configuration")
        );
        assert!(workbench.artifact().configuration.selected_theme.is_none());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn configuration_persists_global_local_provenance_and_undo() {
        let path = temporary_path("persistence");
        let mut workbench = ConfigurationWorkbenchState::fixture(path.clone()).unwrap();
        workbench.selected = ConfigurationOperation::ALL
            .iter()
            .position(|operation| *operation == ConfigurationOperation::SetTokenFamilies)
            .unwrap();
        workbench.activate().unwrap();
        workbench.selected = ConfigurationOperation::ALL
            .iter()
            .position(|operation| *operation == ConfigurationOperation::SetInstanceRadius)
            .unwrap();
        workbench.activate().unwrap();
        let style = workbench.resolved_style().unwrap();
        assert_eq!(
            style.origin("--lynx-margin-sand").unwrap().scope,
            StyleScope::Workspace
        );
        assert_eq!(
            style.origin("--lynx-radius-control").unwrap().scope,
            StyleScope::Instance
        );
        workbench.selected = ConfigurationOperation::ALL
            .iter()
            .position(|operation| *operation == ConfigurationOperation::Undo)
            .unwrap();
        workbench.activate().unwrap();
        assert_ne!(
            workbench
                .resolved_style()
                .unwrap()
                .origin("--lynx-radius-control")
                .unwrap()
                .scope,
            StyleScope::Instance
        );
        let reopened = ConfigurationWorkbenchState::open(path.clone()).unwrap();
        assert_eq!(
            reopened
                .resolved_style()
                .unwrap()
                .origin("--lynx-margin-sand")
                .unwrap()
                .scope,
            StyleScope::Workspace
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn unsafe_css_and_unknown_contract_versions_fail_closed() {
        assert!(
            ScopedCssPatch {
                root_uid: "sand".into(),
                declarations: "@import url(https://example.com/theme.css);".into(),
            }
            .validate()
            .is_err()
        );
        assert!(
            ScopedCssPatch {
                root_uid: "sand".into(),
                declarations: "background:image-set(\"https://example.com/pixel\" 1x);".into(),
            }
            .validate()
            .is_err()
        );
        assert!(
            ScopedCssPatch {
                root_uid: "sand".into(),
                declarations: "letter-spacing:0.01em;border-radius:3px;".into(),
            }
            .wrapped()
            .unwrap()
            .starts_with("[data-lince-root=\"sand\"]")
        );
        let (package, mut manifest) = external_author_fixture().unwrap();
        manifest.contract_version += 1;
        assert!(manifest.validate(&package).is_err());
    }

    #[test]
    fn launch_recipe_is_typed_idempotent_persisted_and_renderer_neutral() {
        let mut artifact = ConfigurationArtifact::fixture().unwrap();
        let recipe = launch_recipe_fixture();
        assert!(matches!(
            artifact.apply_launch_recipe(&recipe).unwrap(),
            LaunchResult::Created { .. }
        ));
        assert!(matches!(
            artifact.apply_launch_recipe(&recipe).unwrap(),
            LaunchResult::FocusedExisting { .. }
        ));
        let bytes = artifact.to_json().unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(!text.contains("renderer_handle"));
        let recipe_text = serde_json::to_string(&recipe).unwrap();
        assert!(!recipe_text.contains("cef"));
        assert!(!recipe_text.contains("bevy"));
        assert!(!recipe_text.contains("renderer_handle"));
        let reopened = ConfigurationArtifact::from_json(&bytes).unwrap();
        assert_eq!(reopened.configuration.launches.len(), 1);
        assert_eq!(
            reopened
                .configuration
                .launches
                .values()
                .next()
                .unwrap()
                .focus_count,
            2
        );
    }

    #[test]
    fn definition_authoring_edits_share_the_catalog_and_refuse_invalid_isolation() {
        let path = temporary_path("definition-editing");
        let mut workbench = ConfigurationWorkbenchState::fixture(path.clone()).unwrap();
        for operation in [
            ConfigurationOperation::EditDefinitionDefault,
            ConfigurationOperation::EditBehaviorAndPort,
            ConfigurationOperation::EditIsolationAndCapability,
        ] {
            workbench.selected = ConfigurationOperation::ALL
                .iter()
                .position(|candidate| *candidate == operation)
                .unwrap();
            workbench.activate().unwrap();
        }
        let reference = workbench
            .artifact
            .composition
            .catalog
            .active_ref("video-call-room")
            .unwrap();
        let definition = workbench
            .artifact
            .composition
            .catalog
            .definition(&reference)
            .unwrap();
        assert!(
            definition
                .outputs
                .iter()
                .any(|port| port.name == "configured")
        );
        assert!(
            definition
                .capabilities
                .contains(&SandCapability::LocalStorage)
        );
        let mut invalid = workbench.artifact.clone();
        assert!(
            publish_definition(&mut invalid, "video-call-room", |definition| {
                if let Some(projection) = definition
                    .projections
                    .iter_mut()
                    .find(|projection| projection.key == "installed-html")
                {
                    projection.isolation = Isolation::Website;
                }
            })
            .and_then(|_| invalid.validate())
            .is_err()
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn failed_persistence_keeps_the_last_good_artifact_and_history() {
        let path = temporary_path("failed-persistence-source");
        let blocked = temporary_path("failed-persistence-target");
        fs::create_dir_all(&blocked).unwrap();
        let mut workbench = ConfigurationWorkbenchState::fixture(path.clone()).unwrap();
        let original = workbench.artifact.clone();
        let original_history = workbench.history.len();
        workbench.path = blocked.clone();
        assert!(workbench.activate().is_err());
        assert_eq!(workbench.artifact, original);
        assert_eq!(workbench.history.len(), original_history);
        let _ = fs::remove_file(blocked.with_extension("json.pending"));
        let _ = fs::remove_dir(blocked);
        let _ = fs::remove_file(path);
    }
}
