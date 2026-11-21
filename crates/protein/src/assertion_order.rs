//! Resolves named assertion priorities for Record ordering and Schedule allocation.

use std::collections::{BTreeSet, HashMap};

use nucleus::{
    DecimalValue,
    performance::{Span, add},
};
use store::{Store, assertions::incident_to, concepts::resolve, sqlx::Error::Protocol};

use crate::ProteinError;

const MAX_FIELDS: usize = 16;
const MAX_NAME_BYTES: usize = 128;

#[derive(Default)]
pub(super) struct Values<'a> {
    fields: HashMap<&'a str, Option<usize>>,
    priorities: Vec<HashMap<String, DecimalValue>>,
}

impl Values<'_> {
    pub(super) fn get(&self, field: &str) -> Option<&HashMap<String, DecimalValue>> {
        let index = self.fields.get(field).copied().flatten()?;
        self.priorities.get(index)
    }
}

pub(super) async fn values<'a>(
    store: &Store,
    records: &[&str],
    fields: &[&'a str],
) -> Result<Values<'a>, ProteinError> {
    let _order = Span::new("protein-assertion-order-await");
    if fields.len() > MAX_FIELDS {
        return Err(invalid_fields());
    }
    let names = fields.iter().map(|field| {
        field
            .strip_prefix('#')
            .filter(|name| !name.is_empty() && name.len() <= MAX_NAME_BYTES)
    });
    if names.clone().any(|name| name.is_none()) {
        return Err(invalid_fields());
    }
    let records: Vec<_> = records
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let assertions = incident_to(&store.pool, &records).await?;
    let mut values = Values::default();
    let mut predicates = HashMap::new();
    for (&field, name) in fields.iter().zip(names.flatten()) {
        if values.fields.contains_key(field) {
            continue;
        }
        let index = resolve(&store.pool, name).await?.map(|predicate| {
            *predicates.entry(predicate).or_insert_with(|| {
                let index = values.priorities.len();
                values.priorities.push(HashMap::new());
                index
            })
        });
        values.fields.insert(field, index);
    }
    if predicates.is_empty() {
        return Ok(values);
    }
    let _publish = Span::new("protein-assertion-order-publish");
    for assertion in assertions {
        if let Some(index) = predicates.get(&assertion.predicate_uid)
            && assertion.object_uid.is_none()
            && assertion.object_uri.is_none()
            && assertion.role == "ordinary"
            && assertion.unit_uid.is_none()
            && let Some(value) = assertion.quantity
        {
            // Each predicate index names a slot created by its first alias.
            values.priorities[*index]
                .entry(assertion.subject_uid)
                .and_modify(|current| *current = (*current).max(value))
                .or_insert(value);
        }
    }
    add(
        "protein-assertion-order-predicates",
        u64::try_from(predicates.len()).unwrap_or(u64::MAX),
    );
    Ok(values)
}

fn invalid_fields() -> ProteinError {
    Protocol("protein_sort_invalid:Use at most 16 named assertion priorities".into())
}
