use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Display, Formatter},
};

pub use crate::sand::{
    DefinitionChild, DefinitionGraph, DefinitionRef, InputPort, OutputPort, ProjectionManifest,
    RendererCapability, SandDefinition, SandValue as FieldValue, Transform2d, ValueType,
};
pub type RendererProjectionManifest = ProjectionManifest;
use crate::{
    sand::{
        AccessibilityRole, AccessibilitySpec, BehaviorBinding, DeclarativeBehavior, Isolation,
        ProjectionKind, SAND_SCHEMA_VERSION, SandCapability, SandElement,
    },
    style::{StyleLayer, StyleValue},
};

pub const SEMANTIC_SCHEMA_VERSION: u32 = SAND_SCHEMA_VERSION;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticError {
    detail: String,
}

impl SemanticError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for SemanticError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for SemanticError {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProteinField {
    pub name: String,
    pub value_type: ValueType,
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProteinRow {
    pub stable_key: String,
    pub fields: BTreeMap<String, FieldValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProteinProjection {
    pub uid: String,
    pub revision: u64,
    pub shape: Vec<ProteinField>,
    pub rows: Vec<ProteinRow>,
}

impl ProteinProjection {
    pub fn validate(&self) -> Result<(), SemanticError> {
        validate_uid("Protein projection", &self.uid)?;
        if self.revision == 0 {
            return Err(SemanticError::new(
                "Protein projection revision must be positive",
            ));
        }
        unique_names(
            "Protein field",
            self.shape.iter().map(|field| field.name.as_str()),
        )?;
        unique_names(
            "Protein stable row key",
            self.rows.iter().map(|row| row.stable_key.as_str()),
        )?;
        let shape = self
            .shape
            .iter()
            .map(|field| (field.name.as_str(), field))
            .collect::<BTreeMap<_, _>>();
        for row in &self.rows {
            validate_uid("Protein stable row key", &row.stable_key)?;
            for (name, value) in &row.fields {
                let field = shape.get(name.as_str()).ok_or_else(|| {
                    SemanticError::new(format!(
                        "row {} has undeclared field {name}",
                        row.stable_key
                    ))
                })?;
                if value.value_type() != field.value_type {
                    return Err(SemanticError::new(format!(
                        "row {} field {name} has incompatible type",
                        row.stable_key
                    )));
                }
            }
            for field in self.shape.iter().filter(|field| !field.optional) {
                if !row.fields.contains_key(&field.name) {
                    return Err(SemanticError::new(format!(
                        "row {} omits required field {}",
                        row.stable_key, field.name
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldMapping {
    pub protein_field: String,
    pub child_uid: String,
    pub input_port: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultTemplate {
    pub uid: String,
    pub definition_uid: String,
    pub protein_uid: String,
    pub mappings: Vec<FieldMapping>,
}

impl ResultTemplate {
    pub fn validate(
        &self,
        graph: &DefinitionGraph,
        protein: &ProteinProjection,
    ) -> Result<(), SemanticError> {
        validate_uid("result template", &self.uid)?;
        if self.protein_uid != protein.uid {
            return Err(SemanticError::new("result template names another Protein"));
        }
        let definition = graph
            .definitions
            .get(&self.definition_uid)
            .ok_or_else(|| SemanticError::new("result template definition is missing"))?;
        let fields = protein
            .shape
            .iter()
            .map(|field| (field.name.as_str(), field.value_type))
            .collect::<BTreeMap<_, _>>();
        let children = definition
            .children
            .iter()
            .map(|child| (child.local_uid.as_str(), child))
            .collect::<BTreeMap<_, _>>();
        let mut destinations = BTreeSet::new();
        for mapping in &self.mappings {
            let field_type = fields.get(mapping.protein_field.as_str()).ok_or_else(|| {
                SemanticError::new(format!(
                    "mapping references missing Protein field {}",
                    mapping.protein_field
                ))
            })?;
            let child = children.get(mapping.child_uid.as_str()).ok_or_else(|| {
                SemanticError::new(format!(
                    "mapping references missing template child {}",
                    mapping.child_uid
                ))
            })?;
            let child_definition = graph
                .definitions
                .get(&child.definition.uid)
                .ok_or_else(|| SemanticError::new("mapped child definition is missing"))?;
            let port = child_definition
                .inputs
                .iter()
                .find(|port| port.name == mapping.input_port)
                .ok_or_else(|| {
                    SemanticError::new(format!(
                        "mapping references missing input {}.{}",
                        mapping.child_uid, mapping.input_port
                    ))
                })?;
            if &port.value_type != field_type {
                return Err(SemanticError::new(format!(
                    "mapping {} to {}.{} has incompatible types",
                    mapping.protein_field, mapping.child_uid, mapping.input_port
                )));
            }
            if !destinations.insert((mapping.child_uid.as_str(), mapping.input_port.as_str())) {
                return Err(SemanticError::new(format!(
                    "input {}.{} is mapped more than once",
                    mapping.child_uid, mapping.input_port
                )));
            }
        }
        Ok(())
    }

    pub fn instantiate(
        &self,
        graph: &DefinitionGraph,
        protein: &ProteinProjection,
    ) -> Result<Vec<TemplateInstance>, SemanticError> {
        self.validate(graph, protein)?;
        protein
            .rows
            .iter()
            .map(|row| {
                let values = self
                    .mappings
                    .iter()
                    .filter_map(|mapping| {
                        row.fields.get(&mapping.protein_field).map(|value| {
                            (
                                format!("{}.{}", mapping.child_uid, mapping.input_port),
                                value.clone(),
                            )
                        })
                    })
                    .collect();
                Ok(TemplateInstance {
                    uid: format!("{}:{}:{}", self.protein_uid, self.uid, row.stable_key),
                    definition_uid: self.definition_uid.clone(),
                    protein_uid: self.protein_uid.clone(),
                    row_key: row.stable_key.clone(),
                    values,
                    why: vec![
                        WhyReason::ProteinRow {
                            protein_uid: self.protein_uid.clone(),
                            row_key: row.stable_key.clone(),
                        },
                        WhyReason::ResultTemplate {
                            template_uid: self.uid.clone(),
                        },
                    ],
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateInstance {
    pub uid: String,
    pub definition_uid: String,
    pub protein_uid: String,
    pub row_key: String,
    pub values: BTreeMap<String, FieldValue>,
    pub why: Vec<WhyReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case", deny_unknown_fields)]
pub enum WhyReason {
    ProteinRow {
        protein_uid: String,
        row_key: String,
    },
    ResultTemplate {
        template_uid: String,
    },
    ForceArea {
        area_uid: String,
    },
    SortArea {
        area_uid: String,
        order: usize,
    },
    MutationArea {
        area_uid: String,
        visit_uid: String,
    },
    ManualOffset,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticDiffOp {
    UpsertProteinRow { row: ProteinRow },
    RemoveProteinRow { stable_key: String },
    SetGlobalToken { name: String, value: StyleValue },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDiff {
    pub schema_version: u32,
    pub base_revision: u64,
    pub next_revision: u64,
    pub operations: Vec<SemanticDiffOp>,
}

impl SemanticDiff {
    pub fn apply(
        &self,
        current_revision: &mut u64,
        protein: &mut ProteinProjection,
        tokens: &mut StyleLayer,
    ) -> Result<(), SemanticError> {
        if self.schema_version != SEMANTIC_SCHEMA_VERSION {
            return Err(SemanticError::new("unsupported semantic diff schema"));
        }
        if self.base_revision != *current_revision || self.next_revision <= self.base_revision {
            return Err(SemanticError::new(
                "semantic diff revision is not contiguous",
            ));
        }
        let mut next_protein = protein.clone();
        let mut next_tokens = tokens.clone();
        for operation in &self.operations {
            match operation {
                SemanticDiffOp::UpsertProteinRow { row } => {
                    if let Some(existing) = next_protein
                        .rows
                        .iter_mut()
                        .find(|candidate| candidate.stable_key == row.stable_key)
                    {
                        *existing = row.clone();
                    } else {
                        next_protein.rows.push(row.clone());
                    }
                }
                SemanticDiffOp::RemoveProteinRow { stable_key } => {
                    next_protein
                        .rows
                        .retain(|row| row.stable_key != *stable_key);
                }
                SemanticDiffOp::SetGlobalToken { name, value } => {
                    next_tokens.values.insert(name.clone(), value.clone());
                }
            }
        }
        next_protein.revision = self.next_revision;
        next_protein.validate()?;
        next_tokens
            .validate_standard()
            .map_err(|error| SemanticError::new(error.to_string()))?;
        *protein = next_protein;
        *tokens = next_tokens;
        *current_revision = self.next_revision;
        Ok(())
    }
}

pub fn composition_fixture() -> (DefinitionGraph, ProteinProjection, ResultTemplate) {
    let text = primitive_definition(
        "text",
        "Text",
        vec![InputPort {
            name: "text".into(),
            value_type: ValueType::Text,
            required: true,
            default: None,
        }],
        Vec::new(),
    );
    let panel = primitive_definition("panel", "Panel", Vec::new(), Vec::new());
    let dropdown = primitive_definition(
        "dropdown",
        "Dropdown",
        vec![InputPort {
            name: "selected".into(),
            value_type: ValueType::Text,
            required: false,
            default: None,
        }],
        vec![OutputPort {
            name: "changed".into(),
            value_type: ValueType::Text,
        }],
    );
    let button = SandDefinition {
        uid: "button".into(),
        revision: 1,
        display_name: "Button".into(),
        element: SandElement::Button,
        inputs: vec![
            InputPort {
                name: "label".into(),
                value_type: ValueType::Text,
                required: true,
                default: None,
            },
            InputPort {
                name: "record".into(),
                value_type: ValueType::Record,
                required: true,
                default: None,
            },
        ],
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
        accessibility: accessibility(AccessibilityRole::Button, "Open Record"),
        capabilities: BTreeSet::from([SandCapability::EmitEvent {
            event: "record-clicked".into(),
        }]),
        projections: projections(
            ["root"],
            BTreeSet::from([SandCapability::EmitEvent {
                event: "record-clicked".into(),
            }]),
        ),
    };
    let castle = SandDefinition {
        uid: "record-card".into(),
        revision: 1,
        display_name: "Record card".into(),
        element: SandElement::Compound,
        inputs: Vec::new(),
        outputs: Vec::new(),
        children: vec![
            child("background", "panel", 0.0, 0.0, 320.0, 190.0, 0),
            child("title", "text", 18.0, 18.0, 284.0, 30.0, 1),
            child("description", "text", 18.0, 58.0, 284.0, 70.0, 2),
            child("status", "dropdown", 18.0, 140.0, 130.0, 32.0, 3),
            child("open", "button", 164.0, 140.0, 138.0, 32.0, 4),
        ],
        connections: Vec::new(),
        exports: Vec::new(),
        behaviors: Vec::new(),
        configuration: Vec::new(),
        style: StyleLayer::default(),
        accessibility: accessibility(AccessibilityRole::Article, "Record card"),
        capabilities: BTreeSet::new(),
        projections: projections(
            [
                "root",
                "background",
                "title",
                "description",
                "status",
                "open",
            ],
            BTreeSet::new(),
        ),
    };
    let definitions = [text, panel, dropdown, button, castle]
        .into_iter()
        .map(|definition| (definition.uid.clone(), definition))
        .collect();
    let graph = DefinitionGraph {
        schema_version: SEMANTIC_SCHEMA_VERSION,
        definitions,
    };
    let protein = ProteinProjection {
        uid: "current-records".into(),
        revision: 1,
        shape: vec![
            ProteinField {
                name: "uid".into(),
                value_type: ValueType::Record,
                optional: false,
            },
            ProteinField {
                name: "title".into(),
                value_type: ValueType::Text,
                optional: false,
            },
            ProteinField {
                name: "description".into(),
                value_type: ValueType::Text,
                optional: false,
            },
            ProteinField {
                name: "status".into(),
                value_type: ValueType::Text,
                optional: true,
            },
            ProteinField {
                name: "quantity".into(),
                value_type: ValueType::Number,
                optional: true,
            },
        ],
        rows: vec![record_row(
            "record-a",
            "Build the Box",
            "Keep meaning visible",
        )],
    };
    let template = ResultTemplate {
        uid: "record-card-template".into(),
        definition_uid: "record-card".into(),
        protein_uid: protein.uid.clone(),
        mappings: vec![
            mapping("title", "title", "text"),
            mapping("description", "description", "text"),
            mapping("status", "status", "selected"),
            mapping("title", "open", "label"),
            mapping("uid", "open", "record"),
        ],
    };
    (graph, protein, template)
}

pub fn record_row(uid: &str, title: &str, description: &str) -> ProteinRow {
    ProteinRow {
        stable_key: uid.into(),
        fields: BTreeMap::from([
            ("uid".into(), FieldValue::Record(uid.into())),
            ("title".into(), FieldValue::Text(title.into())),
            ("description".into(), FieldValue::Text(description.into())),
            ("status".into(), FieldValue::Text("open".into())),
        ]),
    }
}

fn primitive_definition(
    uid: &str,
    display_name: &str,
    inputs: Vec<InputPort>,
    outputs: Vec<OutputPort>,
) -> SandDefinition {
    let (element, role) = match uid {
        "text" => (SandElement::Text, AccessibilityRole::Text),
        "panel" => (SandElement::Panel, AccessibilityRole::Group),
        "dropdown" => (SandElement::Select, AccessibilityRole::Select),
        _ => (SandElement::Specialized, AccessibilityRole::Group),
    };
    SandDefinition {
        uid: uid.into(),
        revision: 1,
        display_name: display_name.into(),
        element,
        inputs,
        outputs,
        children: Vec::new(),
        connections: Vec::new(),
        exports: Vec::new(),
        behaviors: Vec::new(),
        configuration: Vec::new(),
        style: StyleLayer::default(),
        accessibility: accessibility(role, display_name),
        capabilities: BTreeSet::new(),
        projections: projections(["root"], BTreeSet::new()),
    }
}

fn accessibility(role: AccessibilityRole, label: &str) -> AccessibilitySpec {
    AccessibilitySpec {
        role,
        label: label.into(),
        description: None,
        live: false,
    }
}

fn projections<const N: usize>(
    nodes: [&str; N],
    capabilities: BTreeSet<SandCapability>,
) -> Vec<ProjectionManifest> {
    let projected_nodes = nodes.into_iter().map(String::from).collect::<BTreeSet<_>>();
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
            capabilities,
            assets: Vec::new(),
            projected_nodes,
        },
    ]
}

fn child(
    uid: &str,
    definition_uid: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    sibling_order: u32,
) -> DefinitionChild {
    DefinitionChild {
        local_uid: uid.into(),
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

fn mapping(protein_field: &str, child_uid: &str, input_port: &str) -> FieldMapping {
    FieldMapping {
        protein_field: protein_field.into(),
        child_uid: child_uid.into(),
        input_port: input_port.into(),
    }
}

fn validate_uid(label: &str, value: &str) -> Result<(), SemanticError> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character))
    {
        return Err(SemanticError::new(format!("invalid {label} identity")));
    }
    Ok(())
}

fn unique_names<'a>(
    label: &str,
    names: impl Iterator<Item = &'a str>,
) -> Result<(), SemanticError> {
    let mut unique = BTreeSet::new();
    for name in names {
        if !unique.insert(name) {
            return Err(SemanticError::new(format!("duplicate {label} {name}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_composition_and_field_mapping_validate() {
        let (graph, protein, template) = composition_fixture();
        assert!(graph.validate().is_ok());
        assert!(protein.validate().is_ok());
        assert!(template.validate(&graph, &protein).is_ok());
        let instances = template.instantiate(&graph, &protein).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].uid,
            "current-records:record-card-template:record-a"
        );
        assert_eq!(instances[0].why.len(), 2);
    }

    #[test]
    fn incompatible_mapping_and_recursive_cycle_fail_closed() {
        let (mut graph, protein, mut template) = composition_fixture();
        template.mappings[0].protein_field = "quantity".into();
        assert!(template.validate(&graph, &protein).is_err());
        graph
            .definitions
            .get_mut("panel")
            .unwrap()
            .children
            .push(child("cycle", "record-card", 0.0, 0.0, 1.0, 1.0, 0));
        assert!(graph.validate().is_err());
    }

    #[test]
    fn diff_is_atomic_and_rejects_unknown_version() {
        let (_, mut protein, _) = composition_fixture();
        let mut tokens = StyleLayer::default();
        let mut revision = 1;
        let diff = SemanticDiff {
            schema_version: SEMANTIC_SCHEMA_VERSION,
            base_revision: 1,
            next_revision: 2,
            operations: vec![SemanticDiffOp::SetGlobalToken {
                name: "--lynx-radius-control".into(),
                value: StyleValue::LengthPx(10.0),
            }],
        };
        assert!(diff.apply(&mut revision, &mut protein, &mut tokens).is_ok());
        assert_eq!(revision, 2);
        let invalid = SemanticDiff {
            schema_version: 99,
            base_revision: 2,
            next_revision: 3,
            operations: Vec::new(),
        };
        let previous = tokens.clone();
        assert!(
            invalid
                .apply(&mut revision, &mut protein, &mut tokens)
                .is_err()
        );
        assert_eq!(tokens, previous);
    }

    #[test]
    fn projection_selection_uses_declared_capabilities() {
        let native = RendererProjectionManifest {
            key: "native-light".into(),
            kind: ProjectionKind::NativeRetained,
            isolation: Isolation::Trusted,
            required: BTreeSet::from([RendererCapability::InstancedNodes]),
            capabilities: BTreeSet::new(),
            assets: Vec::new(),
            projected_nodes: BTreeSet::from(["root".into()]),
        };
        let html = RendererProjectionManifest {
            key: "installed-html".into(),
            kind: ProjectionKind::InstalledHtml,
            isolation: Isolation::InstalledHtml,
            required: BTreeSet::from([RendererCapability::ExternalHtml]),
            capabilities: BTreeSet::new(),
            assets: vec!["index.html".into()],
            projected_nodes: BTreeSet::from(["root".into()]),
        };
        let available = BTreeSet::from([
            RendererCapability::InstancedNodes,
            RendererCapability::Accessibility,
        ]);
        assert_eq!(
            RendererProjectionManifest::select(&[html, native], &available)
                .unwrap()
                .key,
            "native-light"
        );
    }
}
