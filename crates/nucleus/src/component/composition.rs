//! Standalone composition documents, part settings, and event bindings.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_str, to_string};

use crate::{json::encoded_len, valid_uid};

use super::ComponentState;

/// Serialization format identifier for a standalone component composition.
pub const FORMAT: &str = "lince.custom_component";
/// Maximum serialized size of a composition or its enclosing document.
pub const MAX_BYTES: usize = 256 * 1024;

const MAX_DEPTH: usize = 8;
const MAX_PARTS: usize = 256;
const MAX_NAME_CHARACTERS: usize = 80;
const MAX_SETTING_CHARACTERS: usize = 4096;
const SIZE_ERROR: &str = "Composition exceeds 256 KiB";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Immunity {
    #[default]
    None,
    External,
    Internal,
    All,
    Containment,
    Isolation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Part {
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    pub id: String,
    #[serde(default)]
    pub position: [i32; 2],
    pub size: [u32; 2],
    pub component: ComponentState,
    #[serde(default)]
    pub events: Vec<EventBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventBinding {
    pub event: String,
    pub action: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub name: String,
    pub parts: Vec<Part>,
    #[serde(default)]
    pub origin: Option<Origin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub agent: String,
    pub thread: String,
}

impl Composition {
    /// Checks serialized size and the layout, settings, and events of every part.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid part, exceeded limit, or serialization failure.
    pub fn validate(&self) -> Result<(), String> {
        if encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES {
            return Err(SIZE_ERROR.into());
        }
        self.validate_tree(0, &mut 0)
    }

    fn validate_tree(&self, depth: usize, total: &mut usize) -> Result<(), String> {
        if self
            .origin
            .as_ref()
            .is_some_and(|origin| !valid_uid(&origin.agent, "r") || !valid_uid(&origin.thread, "r"))
        {
            return Err("Composition origin requires stable Agent and conversation UIDs".into());
        }
        if depth >= MAX_DEPTH
            || self.parts.is_empty()
            || self.name.trim().is_empty()
            || self
                .name
                .chars()
                .enumerate()
                .any(|(index, character)| index >= MAX_NAME_CHARACTERS || character.is_control())
        {
            return Err(
                "Use a name of 1–80 characters, nonempty parts and at most 8 nested compositions"
                    .into(),
            );
        }
        let mut ids = BTreeSet::new();
        for part in &self.parts {
            if part.settings.len() > 32
                || part.settings.iter().any(|(id, value)| {
                    id.is_empty()
                        || id.len() > 80
                        || !id
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                        || match value {
                            Value::Bool(_) | Value::Number(_) => false,
                            Value::String(value) => {
                                value.chars().nth(MAX_SETTING_CHARACTERS).is_some()
                            }
                            _ => true,
                        }
                })
            {
                return Err(
                    "A part supports at most 32 settings with boolean, numeric or text values"
                        .into(),
                );
            }
            if part.events.len() > 32
                || part.events.iter().any(|binding| {
                    binding.event.trim().is_empty()
                        || binding.event.len() > 128
                        || binding.event.chars().any(char::is_control)
                        || !binding.action.get("action").is_some_and(Value::is_string)
                })
            {
                return Err(
                    "A part supports at most 32 named event bindings to ordinary Actions".into(),
                );
            }
            *total += 1;
            if *total > MAX_PARTS
                || part.id.is_empty()
                || part.id.len() > 80
                || part.id.chars().any(char::is_control)
                || !ids.insert(&part.id)
                || part.position.iter().any(|v| v.unsigned_abs() > 100_000)
                || part.size.iter().any(|v| !(1..=10_000).contains(v))
            {
                return Err("Invalid or duplicate part ID, bounds, or more than 256 parts".into());
            }
            match &part.component {
                ComponentState::Composition { composition } => {
                    composition.validate_tree(depth + 1, total)?
                }
                component => component.validate()?,
            }
        }
        Ok(())
    }
}

/// A versioned composition document.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format: String,
    pub composition: Composition,
}

impl Document {
    /// Decodes a document and validates its format and composition.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON, an incompatible format, or an invalid composition.
    pub fn decode(body: &str) -> Result<Self, String> {
        if body.len() > MAX_BYTES {
            return Err(SIZE_ERROR.into());
        }
        let document: Self = from_str(body).map_err(|error| error.to_string())?;
        if document.format != FORMAT {
            return Err("Invalid composition format".into());
        }
        document.composition.validate()?;
        Ok(document)
    }

    /// Validates a composition and encodes its document.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid composition or a document that cannot fit within the limit.
    pub fn encode(composition: Composition) -> Result<String, String> {
        composition.validate()?;
        let body = to_string(&Self {
            format: FORMAT.into(),
            composition,
        })
        .map_err(|error| error.to_string())?;
        if body.len() > MAX_BYTES {
            return Err(SIZE_ERROR.into());
        }
        Ok(body)
    }
}
