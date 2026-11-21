//! Classified ledger Entries within the reader's Record scope.

use std::collections::HashSet;

use serde_json::{Value, json};
use store::{Store, entries::list_all, ledger::all_fact_concepts, records::resolve_uid};

use super::activity::concept_descendants;
use crate::{Predicate, Protein, ProteinError};

pub(crate) async fn execute_entries(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let pool = &store.pool;
    let mut record = None;
    let mut family = None;
    for predicate in &protein.filter {
        match predicate {
            Predicate::RecordEq(token) => {
                record = resolve_uid(pool, token).await?;
                if record.is_none() {
                    return Ok(Vec::new());
                }
            }
            Predicate::ClassifiedIn(name) => {
                family = Some(concept_descendants(store, name).await?);
            }
            _ => {}
        }
    }

    let classifications = all_fact_concepts(pool).await?;
    const ENTRY_SCAN_CAP: i64 = 100_000;
    let limit = protein.limit.unwrap_or(500);
    let entries = list_all(pool, ENTRY_SCAN_CAP).await?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| {
            if visible.is_some_and(|visible| !visible.contains(&entry.record_uid))
                || record
                    .as_ref()
                    .is_some_and(|record| record != &entry.record_uid)
            {
                return None;
            }
            let concept = entry
                .fact_uid
                .as_deref()
                .and_then(|uid| classifications.get(uid));
            if let Some(family) = &family
                && !concept.is_some_and(|concept| family.contains(concept))
            {
                return None;
            }
            Some(json!({
                "kind": "entry",
                "uid": entry.uid,
                "record": entry.record_uid,
                "amount": entry.amount.to_string(),
                "note": entry.note,
                "occurred_at": entry.occurred_at,
                "state": entry.state,
                "revision": entry.revision,
                "fact": entry.fact_uid,
                "concept": concept,
                "void": entry.is_void(),
            }))
        })
        .take(limit)
        .collect())
}
