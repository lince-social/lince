use crate::ProteinError;
use nucleus::DecimalValue;
use std::collections::{BTreeSet, HashMap};

pub(crate) async fn values(
    store: &store::Store,
    records: &[String],
    fields: &[String],
) -> Result<HashMap<(String, String), DecimalValue>, ProteinError> {
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
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let assertions = store::assertions::incident_to(&store.pool, &records).await?;
    let mut values = HashMap::new();
    for field in fields {
        if let Some(predicate) = store::concepts::resolve(&store.pool, &field[1..]).await? {
            for assertion in &assertions {
                if assertion.predicate_uid == predicate
                    && assertion.object_uid.is_none()
                    && assertion.object_uri.is_none()
                    && assertion.role == "ordinary"
                    && assertion.unit_uid.is_none()
                    && let Some(value) = assertion.quantity
                {
                    let slot = values
                        .entry((assertion.subject_uid.clone(), field.clone()))
                        .or_insert(value);
                    *slot = (*slot).max(value);
                }
            }
        }
    }
    Ok(values)
}
