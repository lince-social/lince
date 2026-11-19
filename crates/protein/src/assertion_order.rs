use crate::ProteinError;
use nucleus::DecimalValue;
use std::collections::{BTreeSet, HashMap};

#[derive(Default)]
pub(crate) struct Values<'a> {
    fields: HashMap<&'a str, Option<usize>>,
    priorities: Vec<HashMap<String, DecimalValue>>,
}

impl Values<'_> {
    pub(crate) fn get(&self, field: &str) -> Option<&HashMap<String, DecimalValue>> {
        self.fields
            .get(field)
            .and_then(|index| index.map(|index| &self.priorities[index]))
    }
}

pub(crate) async fn values<'a>(
    store: &store::Store,
    records: &[&str],
    fields: &[&'a str],
) -> Result<Values<'a>, ProteinError> {
    let _order = nucleus::performance::Span::new("protein-assertion-order-await");
    if fields.len() > 16
        || fields
            .iter()
            .any(|field| !field.starts_with('#') || field.len() < 2 || field.len() > 129)
    {
        return Err(store::StoreError::Protocol(
            "protein_sort_invalid:Use at most 16 named assertion priorities".into(),
        ));
    }
    let records: Vec<_> = records
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let assertions = store::assertions::incident_to(&store.pool, &records).await?;
    let mut values = Values::default();
    let mut predicates = HashMap::new();
    for &field in fields {
        if values.fields.contains_key(field) {
            continue;
        }
        let index = store::concepts::resolve(&store.pool, &field[1..])
            .await?
            .map(|predicate| {
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
    let _publish = nucleus::performance::Span::new("protein-assertion-order-publish");
    for assertion in assertions {
        if let Some(index) = predicates.get(&assertion.predicate_uid)
            && assertion.object_uid.is_none()
            && assertion.object_uri.is_none()
            && assertion.role == "ordinary"
            && assertion.unit_uid.is_none()
            && let Some(value) = assertion.quantity
        {
            let slot = values.priorities[*index]
                .entry(assertion.subject_uid)
                .or_insert(value);
            *slot = (*slot).max(value);
        }
    }
    nucleus::performance::add(
        "protein-assertion-order-predicates",
        predicates.len() as u64,
    );
    Ok(values)
}
