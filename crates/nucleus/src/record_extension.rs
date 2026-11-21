// non-lean-yet
// Scope: const SCHEMA_NAMESPACE, const VALUES_NAMESPACE, const MAX_BYTES, enum FieldKind.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 401, 402, 403, 405, 409, 414, 418.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 230.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 11, 302.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 95, 153, 157, 172, 196, 219, 242.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 167, 280.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   7, 11, 12, 13, 14, 15, 16, 20, 28, 38, 45, 46, 47, 48, 49, 54, 55, 56, 58, 60, 65, 66, 67, 68,
//   70, 72, 77, 78, 79, 95, 153, 157, 167, 172, 196, 219, 242, 280, 302, 303, 305, 307, 309, 310,
//   311, 313, 314, 315, 316, 318, 319, 320, 321, 322, 323, 325, 330, 336, 337, 338, 339, 340, 344,
//   345, 346, 347, 351, 352, 353, 354, 355, 356, 357, 360, 364.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites
//   selected_choices: String (219), selection: String (196), update_from: String (153, 172),
//   validate: String (95), validate_value: String (242), validated: String (157).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_NAMESPACE: &str = "lince.extension-schema";
pub const VALUES_NAMESPACE: &str = "lince.extension-values";
pub const MAX_BYTES: usize = 512 * 1024;

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
    pub const ALL: [Self; 5] = [
        Self::Text,
        Self::Number,
        Self::Boolean,
        Self::Select,
        Self::MultiSelect,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Number => "Number",
            Self::Boolean => "Checkbox",
            Self::Select => "Single choice",
            Self::MultiSelect => "Multiple choices",
        }
    }

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

fn name(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 160 && !value.chars().any(char::is_control)
}

fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

impl Schema {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_with(|_, _| {})
    }

    fn validate_with<'a>(
        &'a self,
        mut field_validated: impl FnMut(&'a Field, BTreeSet<&'a str>),
    ) -> Result<(), String> {
        let _validation = crate::performance::Span::new("extension-schema-validation");
        if !name(&self.name) || self.fields.is_empty() || self.fields.len() > 64 {
            return Err("Give the schema a name and 1–64 fields".into());
        }
        let mut fields = BTreeSet::new();
        for field in &self.fields {
            if !id(&field.id)
                || !name(&field.name)
                || !fields.insert(&field.id)
                || field.choices.len() > 512
                || (!field.kind.choices() && !field.choices.is_empty())
            {
                return Err("Fields need unique IDs, names and at most 512 choices".into());
            }
            let mut choices = BTreeSet::new();
            for choice in &field.choices {
                if !id(&choice.id)
                    || !name(&choice.name)
                    || !choices.insert(choice.id.as_str())
                    || choice.assertions.len() > 16
                {
                    return Err(
                        "Choices need unique IDs, names and at most 16 preset assertions".into(),
                    );
                }
                for preset in &choice.assertions {
                    if preset.predicate.is_empty()
                        || preset.predicate.len() > 160
                        || preset
                            .object
                            .as_ref()
                            .is_some_and(|value| value.len() > 160)
                        || preset.unit.as_ref().is_some_and(|value| value.len() > 160)
                        || preset
                            .quantity
                            .as_ref()
                            .is_some_and(|value| value.len() > 128)
                    {
                        return Err("Invalid preset assertion".into());
                    }
                }
            }
            field_validated(field, choices);
        }
        if crate::json::encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES {
            return Err("The schema is too large".into());
        }
        Ok(())
    }

    pub fn update_from(&self, before: &Self) -> Result<(), String> {
        self.validated()?.update_from(before)
    }

    pub fn validated(&self) -> Result<ValidatedSchema<'_>, String> {
        let mut fields = BTreeMap::new();
        self.validate_with(|field, choices| {
            fields.insert(field.id.as_str(), (field, choices));
        })?;
        crate::performance::add("extension-schema-indexes-prepared", 1);
        Ok(ValidatedSchema { fields })
    }
}

pub struct ValidatedSchema<'a> {
    fields: BTreeMap<&'a str, (&'a Field, BTreeSet<&'a str>)>,
}

impl ValidatedSchema<'_> {
    pub fn update_from(&self, before: &Schema) -> Result<(), String> {
        let _preservation = crate::performance::Span::new("extension-schema-preservation");
        for old in &before.fields {
            let (field, choices) = self
                .fields
                .get(old.id.as_str())
                .ok_or("Archive existing fields instead of deleting their IDs")?;
            if field.kind != old.kind
                || old
                    .choices
                    .iter()
                    .any(|old| !choices.contains(old.id.as_str()))
            {
                return Err(
                    "Keep existing field types and choice IDs; archive choices to retire them"
                        .into(),
                );
            }
        }
        Ok(())
    }
}

impl Field {
    pub fn selection<'a>(&self, value: &'a Value) -> Result<Vec<&'a str>, String> {
        if value.is_null() {
            return Ok(Vec::new());
        }
        match self.kind {
            FieldKind::Select => Ok(vec![value.as_str().ok_or("Choose one option")?]),
            FieldKind::MultiSelect => {
                let values = value.as_array().ok_or("Choose a list of options")?;
                if values.len() > 512 {
                    return Err("Too many selected options".into());
                }
                let mut selected = BTreeSet::new();
                for value in values {
                    if !selected.insert(value.as_str().ok_or("Choice IDs must be text")?) {
                        return Err("A choice can only be selected once".into());
                    }
                }
                Ok(selected.into_iter().collect())
            }
            _ => Ok(Vec::new()),
        }
    }

    pub fn selected_choices<'schema, 'value>(
        &'schema self,
        value: &'value Value,
    ) -> Result<SelectedChoices<'schema, 'value>, String> {
        let selected = self.selection(value)?;
        let by_id = if selected.len() > 1 {
            let mut by_id = BTreeMap::new();
            for choice in &self.choices {
                by_id.entry(choice.id.as_str()).or_insert(choice);
            }
            crate::performance::add("extension-choice-indexes-prepared", 1);
            crate::performance::add("extension-choices-indexed", self.choices.len() as u64);
            Some(by_id)
        } else {
            None
        };
        Ok(SelectedChoices {
            field: self,
            selected: selected.into_iter(),
            by_id,
        })
    }

    pub fn validate_value(&self, value: &Value, before: &Value) -> Result<(), String> {
        if self.archived && value != before && !value.is_null() {
            return Err("This field is archived; its existing value is kept".into());
        }
        if value.is_null() {
            return Ok(());
        }
        match self.kind {
            FieldKind::Text if value.as_str().is_some_and(|value| value.len() <= 16 * 1024) => {
                Ok(())
            }
            FieldKind::Number
                if value.as_str().is_some_and(|value| {
                    value.len() <= 128 && crate::DecimalValue::parse_inferred(value).is_ok()
                }) =>
            {
                Ok(())
            }
            FieldKind::Boolean if value.is_boolean() => Ok(()),
            FieldKind::Select | FieldKind::MultiSelect => {
                let old = self.selection(before)?;
                for choice in self.selected_choices(value)? {
                    let choice = choice.ok_or("Unknown choice; reload the schema")?;
                    if choice.archived && old.binary_search(&choice.id.as_str()).is_err() {
                        return Err("This choice is archived".into());
                    }
                }
                Ok(())
            }
            _ => Err(format!(
                "{} needs a {} value",
                self.name,
                self.kind.name().to_lowercase()
            )),
        }
    }
}

pub struct SelectedChoices<'schema, 'value> {
    field: &'schema Field,
    selected: std::vec::IntoIter<&'value str>,
    by_id: Option<BTreeMap<&'schema str, &'schema Choice>>,
}

impl<'schema> Iterator for SelectedChoices<'schema, '_> {
    type Item = Option<&'schema Choice>;

    fn next(&mut self) -> Option<Self::Item> {
        self.selected.next().map(|id| {
            if let Some(by_id) = &self.by_id {
                by_id.get(id).copied()
            } else {
                self.field.choices.iter().find(|choice| choice.id == id)
            }
        })
    }
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

pub fn column(schema: &str, field: &str) -> String {
    format!("schema:{schema}:{field}")
}

pub fn column_binding(value: &str) -> Option<(&str, &str)> {
    let (schema, field) = value.strip_prefix("schema:")?.split_once(':')?;
    (crate::valid_uid(schema, "r") && id(field)).then_some((schema, field))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn renamed_and_reordered_choices_preserve_ids_and_archived_choices_keep_values() {
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
        renamed.fields[0].choices.reverse();
        renamed.fields[0].choices[1].name = "Queued".into();
        renamed.fields[0].choices[1].archived = true;
        renamed.update_from(&schema).unwrap();
        renamed.fields[0]
            .validate_value(&json!(["todo"]), &json!(["todo"]))
            .unwrap();
        assert!(
            renamed.fields[0]
                .validate_value(&json!(["todo"]), &Value::Null)
                .is_err()
        );
        assert!(
            renamed.fields[0]
                .validate_value(&json!(["done", "done"]), &Value::Null)
                .is_err()
        );
        renamed.fields[0].choices.pop();
        assert!(renamed.update_from(&schema).is_err());
    }
}
