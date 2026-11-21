use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{DecimalValue, performance};

use super::{Choice, Field, FieldKind, MAX_CHOICES, SelectedChoices};

impl Field {
    /// Borrows selected IDs in sorted order.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, oversized, or duplicate selections.
    pub fn selection<'a>(&self, value: &'a Value) -> Result<Vec<&'a str>, String> {
        if value.is_null() {
            return Ok(Vec::new());
        }
        match self.kind {
            FieldKind::Select => Ok(vec![value.as_str().ok_or("Choose one option")?]),
            FieldKind::MultiSelect => {
                let values = value.as_array().ok_or("Choose a list of options")?;
                if values.len() > MAX_CHOICES {
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

    /// Iterates through the declared choices corresponding to the selected IDs.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [Self::selection].
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
            performance::add("extension-choice-indexes-prepared", 1);
            performance::add(
                "extension-choices-indexed",
                u64::try_from(self.choices.len()).unwrap_or(u64::MAX),
            );
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
                    value.len() <= 128 && DecimalValue::parse_inferred(value).is_ok()
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

impl<'schema> Iterator for SelectedChoices<'schema, '_> {
    type Item = Option<&'schema Choice>;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.selected.next()?;
        let choice = if let Some(by_id) = &self.by_id {
            by_id.get(id).copied()
        } else {
            self.field.choices.iter().find(|choice| choice.id == id)
        };
        Some(choice)
    }
}
