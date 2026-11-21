//! Record extension schemas, field choices, and column bindings.

use std::{
    collections::{BTreeMap, BTreeSet},
    vec::IntoIter,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::valid_uid;

mod schema;
mod selection;

/// The namespace for Record extension schema definitions.
pub const SCHEMA_NAMESPACE: &str = "lince.extension-schema";
/// The namespace for Record extension values.
pub const VALUES_NAMESPACE: &str = "lince.extension-values";
/// The maximum serialized size of an extension schema.
pub const MAX_BYTES: usize = 512 * 1024;

const MAX_CHOICES: usize = 512;
const MAX_ID_BYTES: usize = 64;
const COLUMN_PREFIX: &str = "schema:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Number,
    Boolean,
    Select,
    MultiSelect,
}

impl FieldKind {
    /// Field kinds in their editor display order.
    pub const ALL: [Self; 5] = [
        Self::Text,
        Self::Number,
        Self::Boolean,
        Self::Select,
        Self::MultiSelect,
    ];

    /// Returns the display label for this field kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Number => "Number",
            Self::Boolean => "Checkbox",
            Self::Select => "Single choice",
            Self::MultiSelect => "Multiple choices",
        }
    }

    /// Reports whether this kind can declare choices.
    pub fn choices(self) -> bool {
        matches!(self, Self::Select | Self::MultiSelect)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub predicate: String,
    pub object: Option<String>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub assertions: Vec<Preset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Schema {
    pub name: String,
    pub fields: Vec<Field>,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// A checked schema with borrowed indexes for comparing existing fields and choices.
#[derive(Debug)]
pub struct ValidatedSchema<'a> {
    fields: BTreeMap<&'a str, (&'a Field, BTreeSet<&'a str>)>,
}

/// Resolved choices in sorted selection order, with `None` for unknown choice IDs.
#[derive(Debug)]
pub struct SelectedChoices<'schema, 'value> {
    field: &'schema Field,
    selected: IntoIter<&'value str>,
    by_id: Option<BTreeMap<&'schema str, &'schema Choice>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Inspect {
        #[serde(default)]
        catalog: bool,
        #[serde(default)]
        schemas: Vec<String>,
    },
    Create {
        id: String,
        schema: Schema,
    },
    Save {
        id: String,
        expected_revision: u64,
        schema: Schema,
    },
    Apply {
        id: String,
        schema: String,
        expected_revision: u64,
        expected_schema_revision: u64,
        values: BTreeMap<String, Value>,
        #[serde(default)]
        remove: bool,
    },
}

impl Request {
    /// Reports whether this request can modify a Record or its schema.
    pub fn is_mutation(&self) -> bool {
        !matches!(self, Self::Inspect { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Definition {
    pub uid: String,
    pub revision: u64,
    pub schema: Schema,
    pub editable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Values {
    pub revision: u64,
    pub attached: bool,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Inspection {
    pub record: Option<String>,
    pub schemas: Vec<Definition>,
    pub values: BTreeMap<String, Values>,
    pub writable: Vec<String>,
    pub labels: BTreeMap<String, String>,
    pub more: bool,
}

/// Formats the column binding of an extension field.
pub fn column(schema: &str, field: &str) -> String {
    format!("{COLUMN_PREFIX}{schema}:{field}")
}

/// Borrows the schema Record UID and field ID from a valid column binding.
pub fn column_binding(value: &str) -> Option<(&str, &str)> {
    let (schema, field) = value.strip_prefix(COLUMN_PREFIX)?.split_once(':')?;
    (valid_uid(schema, "r") && valid_id(field)).then_some((schema, field))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn only_field(schema: &mut Schema) -> Result<&mut Field, String> {
        let [field] = schema.fields.as_mut_slice() else {
            return Err("Expected one field in the fixture".into());
        };
        Ok(field)
    }

    #[test]
    fn renamed_and_reordered_choices_preserve_ids_and_archived_choices_keep_values()
    -> Result<(), String> {
        let field = Field {
            id: "state".into(),
            name: "State".into(),
            kind: FieldKind::MultiSelect,
            archived: false,
            choices: vec![
                Choice {
                    id: "todo".into(),
                    name: "Todo".into(),
                    archived: false,
                    assertions: vec![],
                },
                Choice {
                    id: "done".into(),
                    name: "Done".into(),
                    archived: false,
                    assertions: vec![],
                },
            ],
        };
        let schema = Schema {
            name: "Workflow".into(),
            fields: vec![field],
        };
        let mut renamed = schema.clone();
        let field = only_field(&mut renamed)?;
        field.choices.reverse();
        let choice = field
            .choices
            .iter_mut()
            .find(|choice| choice.id == "todo")
            .ok_or("Expected the todo choice")?;
        choice.name = "Queued".into();
        choice.archived = true;

        renamed.update_from(&schema)?;
        let field = only_field(&mut renamed)?;
        let selected = json!(["todo", "done"]);
        assert_eq!(field.selection(&selected)?, ["done", "todo"]);
        field.validate_value(&json!(["todo"]), &json!(["todo"]))?;
        assert!(
            field
                .validate_value(&json!(["todo"]), &Value::Null)
                .is_err()
        );
        assert!(
            field
                .validate_value(&json!(["done", "done"]), &Value::Null)
                .is_err()
        );
        field.choices.pop();
        assert!(renamed.update_from(&schema).is_err());
        Ok(())
    }
}
