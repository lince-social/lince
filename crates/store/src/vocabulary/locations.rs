use super::invalid;
use crate::{StoreError, write_tx};
use nucleus::{
    execution::now,
    valid_uid,
    vocabulary::{LocationBinding, SAVED_LOCATION_ROLE_UID},
};
use sqlx::{SqlitePool, query, query_as, query_scalar};
use std::collections::HashSet;

const MAX_LOCATION_CONCEPTS: usize = 128;

/// Reads the saved-location write Concept, revision, and accepted Concept set.
///
/// # Errors
///
/// Returns an error if either binding query fails.
pub async fn location_bindings(
    pool: &SqlitePool,
) -> Result<(String, i64, HashSet<String>), StoreError> {
    let (write, revision) =
        query_as("SELECT write_concept_uid,revision FROM vocabulary_role WHERE uid=?")
            .bind(SAVED_LOCATION_ROLE_UID)
            .fetch_one(pool)
            .await?;
    let concepts = query_scalar::<_, String>(
        "SELECT concept_uid FROM vocabulary_role_concept WHERE role_uid=?",
    )
    .bind(SAVED_LOCATION_ROLE_UID)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    Ok((write, revision, concepts))
}

/// Replaces saved-location bindings in a write transaction and advances their revision.
///
/// # Errors
///
/// Returns an error for invalid Concept bindings or a failed transaction.
pub async fn apply_location_binding(
    pool: &SqlitePool,
    binding: &LocationBinding,
) -> Result<(), StoreError> {
    let concepts: HashSet<_> = binding.concept_uids.iter().map(String::as_str).collect();
    if binding.revision < 1
        || concepts.is_empty()
        || concepts.len() > MAX_LOCATION_CONCEPTS
        || concepts.len() != binding.concept_uids.len()
        || !concepts.contains(binding.write_concept_uid.as_str())
        || concepts.iter().any(|uid| !valid_uid(uid, "c"))
    {
        return Err(invalid("Invalid saved location bindings"));
    }
    let mut tx = write_tx(pool).await?;
    for uid in &binding.concept_uids {
        query("INSERT OR IGNORE INTO concept(uid,canonical_name,created_at) VALUES(?,?,?)")
            .bind(uid)
            .bind(format!("concept:{uid}"))
            .bind(now().to_rfc3339())
            .execute(&mut *tx)
            .await?;
        query("INSERT INTO vocabulary_role_concept(role_uid,concept_uid) VALUES(?,?) ON CONFLICT DO NOTHING")
            .bind(SAVED_LOCATION_ROLE_UID).bind(uid).execute(&mut *tx).await?;
    }
    query("UPDATE vocabulary_role SET write_concept_uid=?,revision=MAX(revision+1,?) WHERE uid=?")
        .bind(&binding.write_concept_uid)
        .bind(binding.revision)
        .bind(SAVED_LOCATION_ROLE_UID)
        .execute(&mut *tx)
        .await?;
    let previous: Vec<String> =
        query_scalar("SELECT concept_uid FROM vocabulary_role_concept WHERE role_uid=?")
            .bind(SAVED_LOCATION_ROLE_UID)
            .fetch_all(&mut *tx)
            .await?;
    for uid in previous {
        if !concepts.contains(uid.as_str()) {
            query("DELETE FROM vocabulary_role_concept WHERE role_uid=? AND concept_uid=?")
                .bind(SAVED_LOCATION_ROLE_UID)
                .bind(uid)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await
}
