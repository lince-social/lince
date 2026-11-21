//! Vocabulary identities and their concept memberships.

use std::collections::HashMap;

use nucleus::{execution::now, new_uid};
use serde_json::to_string;
use sqlx::{Row, SqlitePool, query, query_as, query_scalar, sqlite::SqliteRow};

use crate::{StoreError, vocabulary::term_text, write_tx};

/// Stable identity of the private local vocabulary.
pub const LOCAL_UID: &str = "g_local";

/// Creates the local vocabulary if needed and returns its identity.
///
/// # Errors
/// Returns an error if the write fails.
pub async fn ensure_local(pool: &SqlitePool) -> Result<String, StoreError> {
    query(
        "INSERT INTO lingua (uid, name, visibility, created_at)
         VALUES (?, 'Local', 'private', ?)
         ON CONFLICT(uid) DO NOTHING",
    )
    .bind(LOCAL_UID)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(LOCAL_UID.to_owned())
}

/// Looks up a vocabulary by identity or name.
///
/// # Errors
/// Returns an error if the lookup fails.
pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<String>, StoreError> {
    query_scalar("SELECT uid FROM lingua WHERE uid = ?1 OR name = ?1 LIMIT 1")
        .bind(token)
        .fetch_optional(pool)
        .await
}

/// Stored vocabulary identity, ownership, and revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinguaRow {
    /// Stable vocabulary identity.
    pub uid: String,
    /// Vocabulary name.
    pub name: String,
    /// Owning Organ identity, when assigned.
    pub owner_organ: Option<String>,
    /// Stored visibility policy name.
    pub visibility: String,
    /// Creation timestamp in RFC 3339 form.
    pub created_at: String,
    /// Current vocabulary revision.
    pub revision: i64,
}

fn read(row: &SqliteRow) -> Result<LinguaRow, StoreError> {
    Ok(LinguaRow {
        uid: row.try_get("uid")?,
        name: row.try_get("name")?,
        owner_organ: row.try_get("owner_organ")?,
        visibility: row.try_get("visibility")?,
        created_at: row.try_get("created_at")?,
        revision: row.try_get("revision")?,
    })
}

/// Creates a vocabulary and adopts it for its owner when that owner is a stored Organ.
///
/// # Errors
/// Returns an error for an invalid visibility or name, or a failed write.
pub async fn create(
    pool: &SqlitePool,
    name: &str,
    owner_organ: Option<&str>,
    visibility: &str,
) -> Result<String, StoreError> {
    if !matches!(visibility, "private" | "shared" | "public") {
        return Err(StoreError::Protocol(format!(
            "invalid Lingua visibility `{visibility}`"
        )));
    }
    let name = term_text(name)?;
    let uid = new_uid("g");
    query(
        "INSERT INTO lingua (uid, name, owner_organ, visibility, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(name)
    .bind(owner_organ)
    .bind(visibility)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    if let Some(organ) = owner_organ {
        query(
            "INSERT INTO organ_vocabulary (organ_uid, vocabulary_uid, adopted_at)
             SELECT uid, ?, ? FROM record WHERE uid = ? AND kind = 'organ'
             ON CONFLICT DO NOTHING",
        )
        .bind(&uid)
        .bind(now().to_rfc3339())
        .bind(organ)
        .execute(pool)
        .await?;
    }
    Ok(uid)
}

/// Lists vocabularies ordered by name and then identity.
///
/// # Errors
/// Returns an error if the query or vocabulary decoding fails.
pub async fn list(pool: &SqlitePool) -> Result<Vec<LinguaRow>, StoreError> {
    query(
        "SELECT uid, name, owner_organ, visibility, created_at, revision
           FROM lingua ORDER BY name, uid",
    )
    .try_map(|row| read(&row))
    .fetch_all(pool)
    .await
}

/// Reads a vocabulary by its identity.
///
/// # Errors
/// Returns an error if the query or vocabulary decoding fails.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<LinguaRow>, StoreError> {
    query(
        "SELECT uid, name, owner_organ, visibility, created_at, revision FROM lingua WHERE uid = ?",
    )
    .bind(uid)
    .try_map(|row| read(&row))
    .fetch_optional(pool)
    .await
}

/// Updates the vocabulary name, returning whether its row existed.
///
/// # Errors
/// Returns an error if the update fails.
pub async fn rename(pool: &SqlitePool, uid: &str, name: &str) -> Result<bool, StoreError> {
    Ok(query("UPDATE lingua SET name = ? WHERE uid = ?")
        .bind(name)
        .bind(uid)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

/// Deletes the vocabulary and its concept memberships in one transaction.
///
/// # Errors
/// Returns an error if the vocabulary is still referenced or the transaction fails.
pub async fn delete(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let mut transaction = write_tx(pool).await?;
    query("DELETE FROM lingua_concept WHERE lingua_uid = ?")
        .bind(uid)
        .execute(&mut *transaction)
        .await?;
    let deleted = query("DELETE FROM lingua WHERE uid = ?")
        .bind(uid)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
        > 0;
    transaction.commit().await?;
    Ok(deleted)
}

/// Adds a concept to a vocabulary, retaining any existing membership and adoption time.
///
/// # Errors
/// Returns an error if a reference is missing or the insert fails.
pub async fn adopt(
    pool: &SqlitePool,
    lingua_uid: &str,
    concept_uid: &str,
) -> Result<(), StoreError> {
    query(
        "INSERT INTO lingua_concept (lingua_uid, concept_uid, adopted_at)
         VALUES (?, ?, ?) ON CONFLICT(lingua_uid, concept_uid) DO NOTHING",
    )
    .bind(lingua_uid)
    .bind(concept_uid)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Removes a concept membership, returning whether it existed.
///
/// # Errors
/// Returns an error if the delete fails.
pub async fn remove_concept(
    pool: &SqlitePool,
    lingua_uid: &str,
    concept_uid: &str,
) -> Result<bool, StoreError> {
    Ok(
        query("DELETE FROM lingua_concept WHERE lingua_uid = ? AND concept_uid = ?")
            .bind(lingua_uid)
            .bind(concept_uid)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}

/// Lists concepts in a vocabulary by adoption time, then by identity.
///
/// # Errors
/// Returns an error if the membership read fails.
pub async fn concept_uids(pool: &SqlitePool, lingua_uid: &str) -> Result<Vec<String>, StoreError> {
    query_scalar("SELECT concept_uid FROM lingua_concept WHERE lingua_uid = ? ORDER BY adopted_at, concept_uid")
        .bind(lingua_uid)
        .fetch_all(pool)
        .await
}

/// Lists ordered concept memberships for selected vocabularies with any concepts.
///
/// # Errors
/// Returns an error if encoding the selection or reading memberships fails.
pub async fn concepts_for_linguas(
    pool: &SqlitePool,
    linguas: &[&str],
) -> Result<HashMap<String, Vec<String>>, StoreError> {
    memberships(
        pool,
        linguas,
        "SELECT lingua_uid, concept_uid FROM lingua_concept
         WHERE lingua_uid IN (SELECT value FROM json_each(?)) ORDER BY adopted_at, concept_uid",
    )
    .await
}

/// Lists vocabularies for a concept by adoption time, then by identity.
///
/// # Errors
/// Returns an error if the membership read fails.
pub async fn lingua_uids_for_concept(
    pool: &SqlitePool,
    concept_uid: &str,
) -> Result<Vec<String>, StoreError> {
    query_scalar("SELECT lingua_uid FROM lingua_concept WHERE concept_uid = ? ORDER BY adopted_at, lingua_uid")
        .bind(concept_uid)
        .fetch_all(pool)
        .await
}

/// Lists ordered vocabulary memberships for selected concepts in any vocabulary.
///
/// # Errors
/// Returns an error if encoding the selection or reading memberships fails.
pub async fn linguas_for_concepts(
    pool: &SqlitePool,
    concepts: &[&str],
) -> Result<HashMap<String, Vec<String>>, StoreError> {
    memberships(
        pool,
        concepts,
        "SELECT concept_uid, lingua_uid FROM lingua_concept
         WHERE concept_uid IN (SELECT value FROM json_each(?)) ORDER BY adopted_at, lingua_uid",
    )
    .await
}

async fn memberships(
    pool: &SqlitePool,
    selected: &[&str],
    sql: &'static str,
) -> Result<HashMap<String, Vec<String>>, StoreError> {
    let mut memberships = HashMap::<String, Vec<String>>::new();
    if selected.is_empty() {
        return Ok(memberships);
    }
    let ids = to_string(selected).map_err(|error| StoreError::Protocol(error.to_string()))?;
    for (source, target) in query_as(sql).bind(ids).fetch_all(pool).await? {
        memberships.entry(source).or_default().push(target);
    }
    Ok(memberships)
}
