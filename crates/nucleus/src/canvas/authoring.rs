//! Authored component edits, layout settings, and inspected parts.
//!
//! [Edits] describes a stored editing layer.
//! [Change] describes one operation on that layer.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::ControlFlow,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, to_value};

use crate::json::sha256_hex;

use super::{Component, Geometry, Placement};

mod changes;
mod layout;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum TokenValue {
    Color([u8; 4]),
    Number(f32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "unit",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Measure {
    Auto,
    Px(f32),
    Percent(f32),
    Vw(f32),
    Vh(f32),
    Vmin(f32),
    Vmax(f32),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Style {
    #[serde(default)]
    pub dimensions: BTreeMap<String, Measure>,
    #[serde(default)]
    pub numbers: BTreeMap<String, f32>,
    #[serde(default)]
    pub choices: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartEdit {
    pub parent: Option<String>,
    pub order: Option<usize>,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub tokens: BTreeMap<String, TokenValue>,
    #[serde(default)]
    pub token_uses: BTreeMap<String, String>,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    pub text: Option<String>,
    pub native_actions_enabled: Option<bool>,
    pub geometry: Option<Geometry>,
    pub layout: Option<Value>,
    pub layout_parent: Option<String>,
    #[serde(default)]
    pub layout_detached: bool,
    #[serde(default)]
    pub removed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddedPart {
    pub parent: String,
    pub component: Option<Component>,
    pub size: [f64; 2],
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartDefaults {
    pub style: Style,
    pub tokens: BTreeMap<String, Option<TokenValue>>,
    pub token_uses: BTreeMap<String, Option<String>>,
    pub parent: Option<String>,
    pub order: usize,
    pub text: Option<String>,
    pub geometry: Option<Geometry>,
    pub aspect_ratio: Option<Measure>,
    pub layout: Option<Value>,
    pub layout_parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Behavior {
    pub part: String,
    pub event: String,
    pub action: Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub event: String,
    pub receive: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edits {
    pub size: Option<[f64; 2]>,
    #[serde(default)]
    pub defaults: BTreeMap<String, PartDefaults>,
    #[serde(default)]
    pub parts: BTreeMap<String, PartEdit>,
    #[serde(default)]
    pub added: BTreeMap<String, AddedPart>,
    #[serde(default)]
    pub behaviors: BTreeMap<String, Behavior>,
    #[serde(default)]
    pub links: BTreeMap<String, Link>,
    #[serde(default)]
    pub groups: BTreeMap<String, Vec<String>>,
}

/// Checks the stable identifier of an authored component part.
///
/// # Errors
///
/// Returns an error for an empty, oversized, or control-character-containing identifier.
pub fn part_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        Err("Use a nonempty stable part identity of at most 512 bytes.".into())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "change", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    SetPart {
        part: String,
        edit: PartEdit,
    },
    ResetPart {
        part: String,
    },
    InsertPart {
        part: String,
        definition: AddedPart,
    },
    ConfigurePart {
        part: String,
        component: Option<Component>,
    },
    RemovePart {
        part: String,
        cascade: bool,
    },
    SetBehavior {
        id: String,
        behavior: Behavior,
    },
    RemoveBehavior {
        id: String,
    },
    SetLink {
        id: String,
        link: Link,
    },
    RemoveLink {
        id: String,
    },
    SetGroup {
        id: String,
        members: Vec<String>,
    },
    RemoveGroup {
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TokenInfo {
    pub token: String,
    pub name: String,
    pub range: Option<[f32; 2]>,
    pub authored: Option<TokenValue>,
    pub effective: TokenValue,
    pub source: String,
    pub source_part: Option<String>,
    pub uses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PartInfo {
    pub id: String,
    pub parent: Option<String>,
    pub name: String,
    pub role: String,
    pub style: Style,
    pub measured_size: [f32; 2],
    pub geometry: Option<Geometry>,
    pub group: Option<String>,
    pub layout: Option<Value>,
    pub layout_parent: Option<String>,
    pub tokens: Vec<TokenInfo>,
    pub text: Option<String>,
    pub settings: Value,
    pub removed: bool,
    pub structural_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Element {
    pub revision: u64,
    pub placement: Placement,
    pub parts: Vec<PartInfo>,
    pub total_parts: usize,
    pub next_offset: Option<usize>,
    pub capabilities: Vec<String>,
}

/// Borrows the component without its editing layer.
pub fn base(component: &Component) -> &Component {
    match component {
        Component::Authored { base, .. } => base,
        _ => component,
    }
}

/// Clones the stored editing layer, or creates an empty one.
pub fn edits(component: &Component) -> Edits {
    edits_ref(component).cloned().unwrap_or_default()
}

/// Borrows the stored editing layer, when present.
pub fn edits_ref(component: &Component) -> Option<&Edits> {
    match component {
        Component::Authored { edits, .. } => Some(edits),
        _ => None,
    }
}

/// Returns a copy with matching declared Record bindings replaced.
///
/// # Errors
///
/// Returns an error for undeclared bindings or typed native settings that embed a binding.
pub fn rebind(
    component: &Component,
    records: &BTreeMap<String, String>,
) -> Result<Component, String> {
    if !records.is_empty() {
        let mut missing: BTreeSet<_> = records.keys().map(String::as_str).collect();
        let _ = component.visit_records(&mut |record| {
            missing.remove(record);
            if missing.is_empty() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        if !missing.is_empty() {
            return Err("Rebinding keys must be declared Record bindings in this item.".into());
        }
    }
    fn replace(
        component: &mut Component,
        records: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        match component {
            Component::Builtin { state } => {
                state.visit_records_mut(&mut |binding| {
                    if let Some(record) = records.get(binding) {
                        binding.clone_from(record);
                    }
                });
            }
            Component::Native {
                bindings, settings, ..
            } => {
                if bindings.iter().any(|binding| records.contains_key(binding))
                    && settings.values().any(|value| {
                        value.as_str().is_some_and(|value| {
                            records.keys().any(|record| value.contains(record))
                        })
                    })
                {
                    return Err("This native factory embeds bindings in typed settings. Configure its state and bindings together instead of rewriting arbitrary text.".into());
                }
                for binding in bindings {
                    if let Some(record) = records.get(binding) {
                        binding.clone_from(record);
                    }
                }
            }
            Component::Composition { composition } => {
                for part in &mut composition.parts {
                    replace(&mut part.component, records)?;
                }
            }
            Component::Authored { base, edits } => {
                replace(base, records)?;
                for part in edits.added.values_mut() {
                    if let Some(component) = &mut part.component {
                        replace(component, records)?;
                    }
                }
            }
        }
        Ok(())
    }
    let mut component = component.clone();
    replace(&mut component, records)?;
    Ok(component)
}

/// Returns the lowercase SHA-256 digest of JSON with recursively sorted object keys.
///
/// # Errors
///
/// Returns an error if the value cannot be serialized.
pub fn hash(value: &impl Serialize) -> Result<String, String> {
    let mut value = to_value(value).map_err(|error| error.to_string())?;
    value.sort_all_objects();
    sha256_hex(&value).map_err(|error| error.to_string())
}
