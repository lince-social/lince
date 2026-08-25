use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Display, Formatter},
};

pub const SEMANTIC_SCHEMA_VERSION: u32 = 1;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Text,
    Number,
    Boolean,
    Record,
    Json,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum FieldValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Record(String),
    Json(serde_json::Value),
}

impl FieldValue {
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Text(_) => ValueType::Text,
            Self::Number(_) => ValueType::Number,
            Self::Boolean(_) => ValueType::Boolean,
            Self::Record(_) => ValueType::Record,
            Self::Json(_) => ValueType::Json,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPort {
    pub name: String,
    pub value_type: ValueType,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputPort {
    pub name: String,
    pub value_type: ValueType,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform2d {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_radians: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionChild {
    pub uid: String,
    pub definition_uid: String,
    pub transform: Transform2d,
    pub sibling_order: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Behavior {
    EmitEvent {
        source_output: String,
        event: String,
    },
    SetLocalState {
        source_output: String,
        key: String,
        value: FieldValue,
    },
    RequestAction {
        source_output: String,
        action: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandDefinition {
    pub uid: String,
    pub revision: u64,
    pub display_name: String,
    pub inputs: Vec<InputPort>,
    pub outputs: Vec<OutputPort>,
    pub children: Vec<DefinitionChild>,
    pub behaviors: Vec<Behavior>,
    pub token_overrides: BTreeMap<String, TokenValue>,
}

impl SandDefinition {
    fn validate(&self) -> Result<(), SemanticError> {
        validate_uid("Sand definition", &self.uid)?;
        if self.revision == 0 {
            return Err(SemanticError::new(
                "Sand definition revision must be positive",
            ));
        }
        if self.display_name.trim().is_empty() {
            return Err(SemanticError::new("Sand display name must not be empty"));
        }
        unique_names(
            "input port",
            self.inputs.iter().map(|port| port.name.as_str()),
        )?;
        unique_names(
            "output port",
            self.outputs.iter().map(|port| port.name.as_str()),
        )?;
        unique_names(
            "child",
            self.children.iter().map(|child| child.uid.as_str()),
        )?;
        for port in &self.inputs {
            validate_uid("input port", &port.name)?;
        }
        for port in &self.outputs {
            validate_uid("output port", &port.name)?;
        }
        for child in &self.children {
            validate_uid("child", &child.uid)?;
            validate_uid("child definition", &child.definition_uid)?;
            validate_transform(child.transform)?;
        }
        for token in self.token_overrides.values() {
            token.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionGraph {
    pub schema_version: u32,
    pub definitions: BTreeMap<String, SandDefinition>,
}

impl DefinitionGraph {
    pub fn validate(&self) -> Result<(), SemanticError> {
        if self.schema_version != SEMANTIC_SCHEMA_VERSION {
            return Err(SemanticError::new(format!(
                "unsupported semantic schema version {}",
                self.schema_version
            )));
        }
        for (uid, definition) in &self.definitions {
            if uid != &definition.uid {
                return Err(SemanticError::new(format!(
                    "definition map key {uid} does not match {}",
                    definition.uid
                )));
            }
            definition.validate()?;
            for child in &definition.children {
                if !self.definitions.contains_key(&child.definition_uid) {
                    return Err(SemanticError::new(format!(
                        "definition {} references missing child definition {}",
                        definition.uid, child.definition_uid
                    )));
                }
            }
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
    ) -> Result<(), SemanticError> {
        if complete.contains(uid) {
            return Ok(());
        }
        if !visiting.insert(uid.to_string()) {
            return Err(SemanticError::new(format!(
                "recursive Sand definition cycle reaches {uid}"
            )));
        }
        let definition = self
            .definitions
            .get(uid)
            .ok_or_else(|| SemanticError::new(format!("missing definition {uid}")))?;
        for child in &definition.children {
            self.validate_acyclic(&child.definition_uid, visiting, complete)?;
        }
        visiting.remove(uid);
        complete.insert(uid.to_string());
        Ok(())
    }
}

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
            .map(|child| (child.uid.as_str(), child))
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
                .get(&child.definition_uid)
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
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TokenValue {
    Number(f64),
    Color(String),
    Text(String),
    Boolean(bool),
}

impl TokenValue {
    fn validate(&self) -> Result<(), SemanticError> {
        match self {
            Self::Number(value) if !value.is_finite() => {
                Err(SemanticError::new("token number must be finite"))
            }
            Self::Color(value) if !valid_color(value) => {
                Err(SemanticError::new(format!("invalid token color {value}")))
            }
            Self::Text(value) if value.len() > 4096 => {
                Err(SemanticError::new("token text exceeds 4096 bytes"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenLayer {
    pub values: BTreeMap<String, TokenValue>,
}

impl TokenLayer {
    pub fn validate(&self) -> Result<(), SemanticError> {
        for (name, value) in &self.values {
            validate_token_name(name)?;
            value.validate()?;
        }
        Ok(())
    }
}

pub fn resolve_tokens(layers: &[&TokenLayer]) -> Result<TokenLayer, SemanticError> {
    let mut values = BTreeMap::new();
    for layer in layers {
        layer.validate()?;
        values.extend(layer.values.clone());
    }
    Ok(TokenLayer { values })
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererCapability {
    RetainedControls,
    InstancedNodes,
    ExternalHtml,
    ThreeDimensional,
    Accessibility,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendererProjectionManifest {
    pub key: String,
    pub required: BTreeSet<RendererCapability>,
    pub assets: Vec<String>,
}

impl RendererProjectionManifest {
    pub fn select<'a>(
        manifests: &'a [Self],
        available: &BTreeSet<RendererCapability>,
    ) -> Result<&'a Self, SemanticError> {
        manifests
            .iter()
            .find(|manifest| manifest.required.is_subset(available))
            .ok_or_else(|| SemanticError::new("no renderer projection satisfies capabilities"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticDiffOp {
    UpsertProteinRow { row: ProteinRow },
    RemoveProteinRow { stable_key: String },
    SetGlobalToken { name: String, value: TokenValue },
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
        tokens: &mut TokenLayer,
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
        next_tokens.validate()?;
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
        inputs: vec![
            InputPort {
                name: "label".into(),
                value_type: ValueType::Text,
                required: true,
            },
            InputPort {
                name: "record".into(),
                value_type: ValueType::Record,
                required: true,
            },
        ],
        outputs: vec![OutputPort {
            name: "clicked".into(),
            value_type: ValueType::Record,
        }],
        children: Vec::new(),
        behaviors: vec![Behavior::EmitEvent {
            source_output: "clicked".into(),
            event: "record_clicked".into(),
        }],
        token_overrides: BTreeMap::new(),
    };
    let castle = SandDefinition {
        uid: "record-card".into(),
        revision: 1,
        display_name: "Record card".into(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        children: vec![
            child("background", "panel", 0.0, 0.0, 320.0, 190.0, 0),
            child("title", "text", 18.0, 18.0, 284.0, 30.0, 1),
            child("description", "text", 18.0, 58.0, 284.0, 70.0, 2),
            child("status", "dropdown", 18.0, 140.0, 130.0, 32.0, 3),
            child("open", "button", 164.0, 140.0, 138.0, 32.0, 4),
        ],
        behaviors: Vec::new(),
        token_overrides: BTreeMap::new(),
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
    SandDefinition {
        uid: uid.into(),
        revision: 1,
        display_name: display_name.into(),
        inputs,
        outputs,
        children: Vec::new(),
        behaviors: Vec::new(),
        token_overrides: BTreeMap::new(),
    }
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
        uid: uid.into(),
        definition_uid: definition_uid.into(),
        transform: Transform2d {
            x,
            y,
            width,
            height,
            rotation_radians: 0.0,
        },
        sibling_order,
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

fn validate_transform(transform: Transform2d) -> Result<(), SemanticError> {
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
        return Err(SemanticError::new("child transform is invalid"));
    }
    Ok(())
}

fn validate_token_name(value: &str) -> Result<(), SemanticError> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._".contains(character))
    {
        return Err(SemanticError::new("invalid token name"));
    }
    Ok(())
}

fn valid_color(value: &str) -> bool {
    matches!(value.len(), 4 | 7 | 9)
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
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
    fn token_cascade_retains_instance_override() {
        let global = TokenLayer {
            values: BTreeMap::from([
                ("density".into(), TokenValue::Number(1.0)),
                ("radius".into(), TokenValue::Number(8.0)),
            ]),
        };
        let definition = TokenLayer {
            values: BTreeMap::from([("radius".into(), TokenValue::Number(12.0))]),
        };
        let instance = TokenLayer {
            values: BTreeMap::from([("radius".into(), TokenValue::Number(3.0))]),
        };
        let resolved = resolve_tokens(&[&global, &definition, &instance]).unwrap();
        assert_eq!(resolved.values["density"], TokenValue::Number(1.0));
        assert_eq!(resolved.values["radius"], TokenValue::Number(3.0));
    }

    #[test]
    fn diff_is_atomic_and_rejects_unknown_version() {
        let (_, mut protein, _) = composition_fixture();
        let mut tokens = TokenLayer::default();
        let mut revision = 1;
        let diff = SemanticDiff {
            schema_version: SEMANTIC_SCHEMA_VERSION,
            base_revision: 1,
            next_revision: 2,
            operations: vec![SemanticDiffOp::SetGlobalToken {
                name: "radius".into(),
                value: TokenValue::Number(10.0),
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
            required: BTreeSet::from([RendererCapability::InstancedNodes]),
            assets: Vec::new(),
        };
        let html = RendererProjectionManifest {
            key: "installed-html".into(),
            required: BTreeSet::from([RendererCapability::ExternalHtml]),
            assets: vec!["index.html".into()],
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
