use serde_json::{Value, json};
use store::{Store, assertions::list_active, vocabulary::display_names};

use super::{row_limit, selected_family};
use crate::{Predicate, Protein, ProteinError};

pub(crate) async fn execute_assertions(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let pool = &store.pool;
    let names = display_names(pool, subject).await?;
    let family = selected_family(pool, &protein.filter, subject).await?;
    Ok(list_active(pool)
        .await?
        .into_iter()
        .filter(|assertion| {
            family
                .as_ref()
                .is_none_or(|family| family.contains(&assertion.predicate_uid))
                && protein.filter.iter().all(|predicate| match predicate {
                    Predicate::UidEq(uid) => assertion.uid == *uid,
                    _ => true,
                })
        })
        .take(row_limit(protein.limit))
        .map(|assertion| {
            json!({
                "uid": assertion.uid,
                "subject": assertion.subject_uid,
                "predicate": assertion.predicate_uid,
                "predicate_revision": assertion.predicate_revision,
                "predicate_name": names.get(&assertion.predicate_uid),
                "object": assertion.object_uid,
                "uri": assertion.object_uri,
                "resolved_object": assertion.resolved_object_uid,
                "role": assertion.role,
                "quantity": assertion.quantity.map(|value| value.to_string()),
                "unit": assertion.unit_uid,
                "unit_revision": assertion.unit_revision,
                "asserted_by": assertion.asserted_by,
                "created_at": assertion.created_at,
            })
        })
        .collect())
}
