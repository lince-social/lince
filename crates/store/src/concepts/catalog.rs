use std::collections::{HashMap, HashSet};

use nucleus::{execution::now, new_uid, performance::Span};
use serde_json::{Value, to_string};
use sqlx::{SqlitePool, query, query_as, query_scalar};

use crate::{
    StoreError, linguas,
    sync_ops::{OpKind, log_local_tx},
    vocabulary, write_tx,
};

/// Creates a concept and its preferred term in the local vocabulary.
///
/// # Errors
/// Returns an error for invalid terms or a failed vocabulary write.
pub async fn create(
    pool: &SqlitePool,
    canonical_name: &str,
    parents: &[&str],
) -> Result<String, StoreError> {
    let lingua_uid = linguas::ensure_local(pool).await?;
    create_in(pool, &lingua_uid, canonical_name, parents).await
}

/// Creates a concept in the specified vocabulary with the supplied parents.
///
/// # Errors
/// Returns an error for invalid terms, missing references, or a failed write.
pub async fn create_in(
    pool: &SqlitePool,
    lingua_uid: &str,
    canonical_name: &str,
    parents: &[&str],
) -> Result<String, StoreError> {
    let uid = new_uid("c");
    let canonical_name = vocabulary::term_text(canonical_name)?;
    let mut transaction = write_tx(pool).await?;
    query("INSERT INTO concept (uid, canonical_name, created_at) VALUES (?, ?, ?)")
        .bind(&uid)
        .bind(&canonical_name)
        .bind(now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
    for &parent in parents {
        query("INSERT INTO concept_parent (concept_uid, parent_uid) VALUES (?, ?)")
            .bind(&uid)
            .bind(parent)
            .execute(&mut *transaction)
            .await?;
    }
    query("INSERT INTO lingua_concept(lingua_uid,concept_uid,adopted_at) VALUES(?,?,?)")
        .bind(lingua_uid)
        .bind(&uid)
        .bind(now().to_rfc3339())
        .execute(&mut *transaction)
        .await?;
    query("INSERT INTO concept_source(concept_uid,lingua_uid,original_name) VALUES(?,?,?)")
        .bind(&uid)
        .bind(lingua_uid)
        .bind(&canonical_name)
        .execute(&mut *transaction)
        .await?;
    vocabulary::add_term_on(&mut transaction, &uid, &canonical_name, "", None, true).await?;
    log_local_tx(
        &mut transaction,
        "concept",
        &uid,
        "canonical_name",
        OpKind::Set,
        Some(Value::String(canonical_name).to_string()),
    )
    .await?;
    vocabulary::log_shared_on(&mut transaction, &uid).await?;
    transaction.commit().await?;
    Ok(uid)
}

/// Resolves an existing concept or creates one with no parents.
///
/// # Errors
/// Returns an error for ambiguous names, invalid terms, or failed storage operations.
pub async fn ensure(pool: &SqlitePool, canonical_name: &str) -> Result<String, StoreError> {
    if let Some(uid) = resolve(pool, canonical_name).await? {
        return Ok(uid);
    }
    create(pool, canonical_name, &[]).await
}

/// Adds an alternative term in the supplied language and publishes the vocabulary change.
///
/// # Errors
/// Returns an error for invalid terms or a failed vocabulary write.
pub async fn add_name(
    pool: &SqlitePool,
    concept_uid: &str,
    lang: &str,
    name: &str,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    vocabulary::add_term_on(&mut tx, concept_uid, name, lang, None, false).await?;
    vocabulary::log_shared_on(&mut tx, concept_uid).await?;
    tx.commit().await
}

/// Replaces the preferred term, returning `false` if the concept is absent.
///
/// # Errors
/// Returns an error for invalid terms or failed storage operations.
pub async fn rename(
    pool: &SqlitePool,
    concept_uid: &str,
    canonical_name: &str,
) -> Result<bool, StoreError> {
    if self::canonical_name(pool, concept_uid).await?.is_none() {
        return Ok(false);
    }
    let mut tx = write_tx(pool).await?;
    let current: Option<(String, i64)> = query_as("SELECT uid,revision FROM concept_term WHERE concept_uid=? AND language='' AND owner_uid IS NULL AND preferred=1 AND retired=0")
        .bind(concept_uid).fetch_optional(&mut *tx).await?;
    if let Some((uid, revision)) = current {
        vocabulary::edit_term_on(&mut tx, &uid, revision, canonical_name, "", true).await?;
    } else {
        vocabulary::add_term_on(&mut tx, concept_uid, canonical_name, "", None, true).await?;
    }
    vocabulary::log_shared_on(&mut tx, concept_uid).await?;
    tx.commit().await?;
    Ok(true)
}

/// Removes a concept and its links, returning whether a concept was removed.
///
/// # Errors
/// Returns an error when a publication retains the concept or storage fails.
pub async fn delete(pool: &SqlitePool, concept_uid: &str) -> Result<bool, StoreError> {
    let mut transaction = write_tx(pool).await?;
    if vocabulary::retained_by_publication_on(&mut transaction, concept_uid).await? {
        return Err(StoreError::Protocol(
            "This meaning is retained by a published snapshot; remove it from a collection instead"
                .into(),
        ));
    }
    query("DELETE FROM concept_name WHERE concept_uid = ?")
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?;
    query("DELETE FROM concept_parent WHERE concept_uid = ? OR parent_uid = ?")
        .bind(concept_uid)
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?;
    query("DELETE FROM concept_equivalence WHERE a_uid = ? OR b_uid = ?")
        .bind(concept_uid)
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?;
    query("DELETE FROM lingua_concept WHERE concept_uid = ?")
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?;
    let deleted = query("DELETE FROM concept WHERE uid = ?")
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
        > 0;
    if deleted {
        log_local_tx(
            &mut transaction,
            "concept",
            concept_uid,
            "",
            OpKind::Tombstone,
            None,
        )
        .await?;
    }
    transaction.commit().await?;
    Ok(deleted)
}

/// Finds a concept by its identity or a unique matching term.
///
/// # Errors
/// Returns an error for an ambiguous term or a failed lookup.
pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<String>, StoreError> {
    vocabulary::resolve(pool, token, None).await
}

/// Adopts an existing identity and records its parent links in local storage.
///
/// # Errors
/// Returns an error when a referenced row is missing or a storage operation fails.
pub async fn adopt(
    pool: &SqlitePool,
    uid: &str,
    canonical_name: &str,
    origin_organ: Option<&str>,
    parents: &[String],
) -> Result<(), StoreError> {
    query(
        "INSERT OR IGNORE INTO concept (uid, canonical_name, origin_organ, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(canonical_name)
    .bind(origin_organ)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    for parent in parents {
        query("INSERT OR IGNORE INTO concept_parent (concept_uid, parent_uid) VALUES (?, ?)")
            .bind(uid)
            .bind(parent)
            .execute(pool)
            .await?;
    }
    let lingua_uid = linguas::ensure_local(pool).await?;
    linguas::adopt(pool, &lingua_uid, uid).await?;
    Ok(())
}

/// Records a directed equivalence declaration when one is absent.
///
/// # Errors
/// Returns an error when a referenced concept is missing or the write fails.
pub async fn declare_equivalence(
    pool: &SqlitePool,
    a_uid: &str,
    b_uid: &str,
    declared_by: Option<&str>,
) -> Result<(), StoreError> {
    query("INSERT OR IGNORE INTO concept_equivalence (a_uid, b_uid, declared_by) VALUES (?, ?, ?)")
        .bind(a_uid)
        .bind(b_uid)
        .bind(declared_by)
        .execute(pool)
        .await?;
    Ok(())
}

/// A concept and its directly linked parents.
#[derive(Debug, Clone)]
pub struct ConceptRow {
    /// Stable concept identity.
    pub uid: String,
    /// Current canonical name.
    pub canonical_name: String,
    /// Optional associated Instinct identifier.
    pub instinct: Option<String>,
    /// Direct parent identities.
    pub parents: Vec<String>,
}

/// Lists concepts by canonical name with their direct parents.
///
/// # Errors
/// Returns an error if a concept or parent query fails.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<ConceptRow>, StoreError> {
    let mut parents: HashMap<String, Vec<String>> = HashMap::new();
    for (uid, parent) in query_as("SELECT concept_uid, parent_uid FROM concept_parent")
        .fetch_all(pool)
        .await?
    {
        parents.entry(uid).or_default().push(parent);
    }
    Ok(
        query_as("SELECT uid, canonical_name, instinct FROM concept ORDER BY canonical_name")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(
                |(uid, canonical_name, instinct): (String, _, _)| ConceptRow {
                    parents: parents.remove(&uid).unwrap_or_default(),
                    uid,
                    canonical_name,
                    instinct,
                },
            )
            .collect(),
    )
}

/// Reads the canonical name for a concept if it exists.
///
/// # Errors
/// Returns an error if the lookup fails.
pub async fn canonical_name(
    pool: &SqlitePool,
    concept_uid: &str,
) -> Result<Option<String>, StoreError> {
    query_scalar("SELECT canonical_name FROM concept WHERE uid = ?")
        .bind(concept_uid)
        .fetch_optional(pool)
        .await
}

/// Lists concept identities and names in canonical-name order.
///
/// # Errors
/// Returns an error if the list query fails.
pub async fn canonical_names(pool: &SqlitePool) -> Result<Vec<(String, String)>, StoreError> {
    let _names = Span::new("store-concept-names-await");
    query_as("SELECT uid, canonical_name FROM concept ORDER BY canonical_name")
        .fetch_all(pool)
        .await
}

/// Reads the set of concept identities without their names.
///
/// # Errors
/// Returns an error if the identity query fails.
pub async fn identities(pool: &SqlitePool) -> Result<HashSet<String>, StoreError> {
    let _identities = Span::new("store-concept-identities-await");
    Ok(query_scalar("SELECT uid FROM concept")
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect())
}

/// Checks whether any supplied concept has an exact canonical name match.
///
/// # Errors
/// Returns an error if the match set cannot be encoded or queried.
pub async fn any_named(
    pool: &SqlitePool,
    concepts: &[String],
    names: &[&str],
) -> Result<bool, StoreError> {
    let _names = Span::new("store-concept-name-match-await");
    if concepts.is_empty() || names.is_empty() {
        return Ok(false);
    }
    let concepts = to_string(concepts).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let names = to_string(names).map_err(|error| StoreError::Protocol(error.to_string()))?;
    query_scalar(
        "SELECT EXISTS(SELECT 1 FROM concept
         WHERE uid IN (SELECT value FROM json_each(?))
           AND canonical_name COLLATE BINARY IN (SELECT value FROM json_each(?)))",
    )
    .bind(concepts)
    .bind(names)
    .fetch_one(pool)
    .await
}
