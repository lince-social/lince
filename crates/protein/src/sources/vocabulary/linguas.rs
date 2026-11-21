use serde_json::{Value, json};
use store::{
    Store,
    linguas::{concepts_for_linguas, list},
    vocabulary::collection_details,
};

use super::row_limit;
use crate::{Predicate, Protein, ProteinError};

pub(crate) async fn execute_linguas(
    store: &Store,
    protein: &Protein,
) -> Result<Vec<Value>, ProteinError> {
    let pool = &store.pool;
    let details = collection_details(pool).await?;
    let selected: Vec<_> = list(pool)
        .await?
        .into_iter()
        .filter(|lingua| {
            protein.filter.iter().all(|predicate| match predicate {
                Predicate::UidEq(uid) => lingua.uid == *uid,
                Predicate::SlugEq(name) => lingua.name == *name,
                _ => true,
            })
        })
        .take(row_limit(protein.limit))
        .collect();
    let ids: Vec<_> = selected.iter().map(|lingua| lingua.uid.as_str()).collect();
    let mut memberships = concepts_for_linguas(pool, &ids).await?;
    Ok(selected
        .into_iter()
        .map(|lingua| {
            let detail = details.get(&lingua.uid);
            json!({
                "uid": lingua.uid,
                "name": lingua.name,
                "owner_organ": lingua.owner_organ,
                "visibility": lingua.visibility,
                "concepts": memberships.remove(&lingua.uid).unwrap_or_default(),
                "created_at": lingua.created_at,
                "revision": lingua.revision,
                "publications": detail.map(|detail| &detail["publications"]),
                "organs": detail.map(|detail| &detail["organs"]),
            })
        })
        .collect())
}
