use std::collections::HashSet;

use nucleus::performance::{Span, add};
use serde_json::to_string;
use sqlx::{SqliteConnection, SqlitePool, query, query_scalar};

use crate::StoreError;

const BATCH_SIZE: usize = 512;

/// Adds a parent edge if it does not form a cycle.
///
/// # Errors
/// Returns an error for cycles, missing concepts, or failed storage operations.
pub async fn add_parent(
    pool: &SqlitePool,
    concept_uid: &str,
    parent_uid: &str,
) -> Result<(), StoreError> {
    if concept_uid == parent_uid
        || descendants_including(pool, concept_uid)
            .await?
            .iter()
            .any(|uid| uid == parent_uid)
    {
        return Err(StoreError::Protocol(
            "concept hierarchy must remain acyclic".into(),
        ));
    }
    query(
        "INSERT INTO concept_parent (concept_uid, parent_uid) VALUES (?, ?)
         ON CONFLICT(concept_uid, parent_uid) DO NOTHING",
    )
    .bind(concept_uid)
    .bind(parent_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Removes a direct parent edge, returning whether it existed.
///
/// # Errors
/// Returns an error if the write fails.
pub async fn remove_parent(
    pool: &SqlitePool,
    concept_uid: &str,
    parent_uid: &str,
) -> Result<bool, StoreError> {
    Ok(
        query("DELETE FROM concept_parent WHERE concept_uid = ? AND parent_uid = ?")
            .bind(concept_uid)
            .bind(parent_uid)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}

/// Lists the supplied identity and its descendants in breadth-first order.
///
/// # Errors
/// Returns an error if the hierarchy cannot be read.
pub async fn descendants_including(
    pool: &SqlitePool,
    root_uid: &str,
) -> Result<Vec<String>, StoreError> {
    hierarchy_on(
        &mut *pool.acquire().await?,
        root_uid,
        HierarchyDirection::Descendants,
    )
    .await
}

/// Lists the supplied identity and its ancestors in breadth-first order.
///
/// # Errors
/// Returns an error if the hierarchy cannot be read.
pub async fn ancestors_including(
    pool: &SqlitePool,
    concept_uid: &str,
) -> Result<Vec<String>, StoreError> {
    ancestors_including_on(&mut *pool.acquire().await?, concept_uid).await
}

pub(super) async fn ancestors_including_on(
    connection: &mut SqliteConnection,
    concept_uid: &str,
) -> Result<Vec<String>, StoreError> {
    hierarchy_on(connection, concept_uid, HierarchyDirection::Ancestors).await
}

enum HierarchyDirection {
    Descendants,
    Ancestors,
}

async fn hierarchy_on(
    connection: &mut SqliteConnection,
    root_uid: &str,
    direction: HierarchyDirection,
) -> Result<Vec<String>, StoreError> {
    let _hierarchy = Span::new("store-concept-hierarchy-await");
    let sql = match direction {
        HierarchyDirection::Descendants => {
            "SELECT p.concept_uid
             FROM json_each(?) AS frontier
             JOIN concept_parent p ON p.parent_uid = frontier.value
             ORDER BY frontier.key, p.concept_uid"
        }
        HierarchyDirection::Ancestors => {
            "SELECT p.parent_uid
             FROM json_each(?) AS frontier
             JOIN concept_parent p ON p.concept_uid = frontier.value
             ORDER BY frontier.key, p.parent_uid"
        }
    };
    let mut seen = HashSet::from([root_uid.to_owned()]);
    let mut out = vec![root_uid.to_owned()];
    let mut start = 0;
    while start < out.len() {
        // Finish the current level before processing any newly discovered nodes.
        let end = out.len();
        while start < end {
            let batch_end = start.saturating_add(BATCH_SIZE).min(end);
            let batch = out.get(start..batch_end).ok_or_else(|| {
                StoreError::Protocol("Concept hierarchy frontier is out of bounds".into())
            })?;
            let frontier =
                to_string(batch).map_err(|error| StoreError::Protocol(error.to_string()))?;
            let neighbours: Vec<String> = query_scalar(sql)
                .bind(frontier)
                .fetch_all(&mut *connection)
                .await?;
            add("store-concept-hierarchy-batches", 1);
            for uid in neighbours {
                if !seen.contains(uid.as_str()) {
                    seen.insert(uid.clone());
                    out.push(uid);
                }
            }
            start = batch_end;
        }
    }
    if let Ok(count) = u64::try_from(out.len()) {
        add("store-concept-hierarchy-nodes", count);
    }
    Ok(out)
}

/// Finds the first known ancestor in breadth-first order, including the concept itself.
///
/// # Errors
/// Returns an error if the hierarchy cannot be read.
pub async fn nearest_ancestor_in(
    pool: &SqlitePool,
    concept_uid: &str,
    known: &[String],
) -> Result<Option<String>, StoreError> {
    Ok(ancestors_including(pool, concept_uid)
        .await?
        .into_iter()
        .find(|candidate| known.contains(candidate)))
}
