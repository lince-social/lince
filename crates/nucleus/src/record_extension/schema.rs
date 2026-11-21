use std::collections::{BTreeMap, BTreeSet};

use crate::{
    json::encoded_len,
    performance::{self, Span},
};

use super::{Field, MAX_BYTES, MAX_CHOICES, Schema, ValidatedSchema, valid_id};

const MAX_FIELDS: usize = 64;
const MAX_NAME_CHARACTERS: usize = 160;
const MAX_PRESET_ASSERTIONS: usize = 16;

fn valid_name(value: &str) -> bool {
    !value.trim().is_empty()
        && !value
            .chars()
            .enumerate()
            .any(|(index, character)| index >= MAX_NAME_CHARACTERS || character.is_control())
}

impl Schema {
    /// Checks schema fields, choices, assertions, and the serialized size bound.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid definitions, duplicate IDs, or an exceeded limit.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_with(|_, _| {})
    }

    fn validate_with<'a>(
        &'a self,
        mut field_validated: impl FnMut(&'a Field, BTreeSet<&'a str>),
    ) -> Result<(), String> {
        let _validation = Span::new("extension-schema-validation");
        if !valid_name(&self.name) || self.fields.is_empty() || self.fields.len() > MAX_FIELDS {
            return Err("Give the schema a name and 1–64 fields".into());
        }
        let mut fields = BTreeSet::new();
        for field in &self.fields {
            if !valid_id(&field.id)
                || !valid_name(&field.name)
                || !fields.insert(&field.id)
                || field.choices.len() > MAX_CHOICES
                || (!field.kind.choices() && !field.choices.is_empty())
            {
                return Err("Fields need unique IDs, names and at most 512 choices".into());
            }
            let mut choices = BTreeSet::new();
            for choice in &field.choices {
                if !valid_id(&choice.id)
                    || !valid_name(&choice.name)
                    || !choices.insert(choice.id.as_str())
                    || choice.assertions.len() > MAX_PRESET_ASSERTIONS
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
        if encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES {
            return Err("The schema is too large".into());
        }
        Ok(())
    }

    pub fn update_from(&self, before: &Self) -> Result<(), String> {
        self.validated()?.update_from(before)
    }

    /// Checks this schema and prepares indexes for a preservation check.
    ///
    /// # Errors
    ///
    /// Returns the same validation errors as [Self::validate].
    pub fn validated(&self) -> Result<ValidatedSchema<'_>, String> {
        let mut fields = BTreeMap::new();
        self.validate_with(|field, choices| {
            fields.insert(field.id.as_str(), (field, choices));
        })?;
        performance::add("extension-schema-indexes-prepared", 1);
        Ok(ValidatedSchema { fields })
    }
}

impl ValidatedSchema<'_> {
    pub fn update_from(&self, before: &Schema) -> Result<(), String> {
        let _preservation = Span::new("extension-schema-preservation");
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
