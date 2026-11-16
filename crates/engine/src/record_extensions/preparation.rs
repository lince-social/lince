use super::{EngineError, Preset, Schema, StoredValues, invalid};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

/// Decode once from the existing snapshot, copy untouched fields and move the
/// submitted values into their replacements.
/// Managed assertion receipts belong to the old snapshot; the next receipts
/// are produced by the transactional assertion path, never copied and cleared.
pub(super) fn values(
    before: &Value,
    schema_uid: &str,
    schema: &Schema,
    values: BTreeMap<String, Value>,
) -> Result<(StoredValues, StoredValues), EngineError> {
    let _preparation = nucleus::performance::Span::new("extension-values-preparation");
    let old: StoredValues = before
        .get(schema_uid)
        .map(StoredValues::deserialize)
        .transpose()
        .map_err(EngineError::Json)?
        .unwrap_or_default();
    let mut next = StoredValues {
        revision: old.revision,
        detached: old.detached,
        fields: if values.is_empty() {
            old.fields.clone()
        } else {
            old.fields
                .iter()
                .filter(|(id, _)| !values.contains_key(*id))
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect()
        },
        assertions: Vec::new(),
    };
    let by_id = if values.len() > 1 {
        let mut fields = BTreeMap::new();
        for field in &schema.fields {
            fields.entry(field.id.as_str()).or_insert(field);
        }
        Some(fields)
    } else {
        None
    };
    for (id, value) in values {
        let field = if let Some(by_id) = &by_id {
            by_id.get(id.as_str()).copied()
        } else {
            schema.fields.iter().find(|field| field.id == id)
        }
        .ok_or_else(|| invalid("Unknown field; reload the schema"))?;
        field
            .validate_value(&value, old.fields.get(&id).unwrap_or(&Value::Null))
            .map_err(invalid)?;
        if !value.is_null() {
            next.fields.insert(id, value);
        }
    }
    Ok((old, next))
}

/// Preserve the original encounter order and resolved-value membership rule.
/// Different unresolved spellings may still append equal resolved presets;
/// deduplicating those would change the existing save behavior. Arc shares the
/// prepared value between order and membership without duplicating its strings.
#[derive(Default)]
pub(super) struct Presets {
    ordered: Vec<Arc<Preset>>,
    membership: HashSet<Arc<Preset>>,
}

impl Presets {
    pub(super) fn contains(&self, preset: &Preset) -> bool {
        self.membership.contains(preset)
    }

    pub(super) fn push(&mut self, preset: Preset) {
        let preset = Arc::new(preset);
        self.membership.insert(preset.clone());
        self.ordered.push(preset);
        nucleus::performance::add("extension-presets-resolved", 1);
    }

    pub(super) fn len(&self) -> usize {
        self.ordered.len()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &Preset> {
        self.ordered.iter().map(|preset| preset.as_ref())
    }

    pub(super) fn into_values(self) -> impl Iterator<Item = Preset> {
        drop(self.membership);
        self.ordered.into_iter().map(Arc::unwrap_or_clone)
    }
}

/// Borrow assertion identities from the already-read values snapshot. The
/// transaction still compares that entire snapshot before using the result.
/// Other schemas retain their tolerant JSON handling, including detached ones.
pub(super) struct AssertionOwnership<'a> {
    elsewhere: HashSet<&'a str>,
    owned: HashSet<&'a str>,
}

impl<'a> AssertionOwnership<'a> {
    pub(super) fn new(values: &'a Map<String, Value>, schema_uid: &str) -> Self {
        let _preparation = nucleus::performance::Span::new("extension-assertion-ownership");
        let mut ownership = Self {
            elsewhere: HashSet::new(),
            owned: HashSet::new(),
        };
        let mut visited = 0;
        for (schema, value) in values {
            let Some(assertions) = value["assertions"].as_array() else {
                continue;
            };
            for entry in assertions {
                visited += 1;
                let Some(assertion) = entry["assertion"].as_str() else {
                    continue;
                };
                if schema != schema_uid {
                    ownership.elsewhere.insert(assertion);
                }
                if entry["owned"] == true {
                    ownership.owned.insert(assertion);
                }
            }
        }
        nucleus::performance::add("extension-assertion-receipts-visited", visited);
        ownership
    }

    pub(super) fn referenced_elsewhere(&self, assertion: &str) -> bool {
        self.elsewhere.contains(assertion)
    }

    pub(super) fn owned(&self, assertion: &str) -> bool {
        self.owned.contains(assertion)
    }
}
