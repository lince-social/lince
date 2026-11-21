use nucleus::execution::now;
use sqlx::{SqlitePool, query};

use super::Stamp;
use crate::{StoreError, vocabulary::retained_by_publication_on, write_tx};

/// Stores a replicated Concept name while preserving any retained source name.
pub async fn upsert_concept(
    pool: &SqlitePool,
    uid: &str,
    canonical_name: &str,
    origin_organ: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    if stamp.superseded(&mut tx).await? {
        return Ok(());
    }
    let res = query(
        "INSERT OR IGNORE INTO concept (uid, canonical_name, origin_organ, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(canonical_name)
    .bind(origin_organ)
    .bind(now().to_rfc3339())
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        query("UPDATE concept SET canonical_name = ? WHERE uid = ? AND NOT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=concept.uid)")
            .bind(canonical_name)
            .bind(uid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

/// Removes an unretained Concept and its links unless the deletion is stale.
pub async fn delete_concept(
    pool: &SqlitePool,
    uid: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    if stamp.superseded(&mut tx).await? {
        return Ok(());
    }
    if retained_by_publication_on(&mut tx, uid).await? {
        return Ok(());
    }
    for sql in [
        "DELETE FROM concept_name WHERE concept_uid = ?",
        "DELETE FROM lingua_concept WHERE concept_uid = ?",
    ] {
        query(sql).bind(uid).execute(&mut *tx).await?;
    }
    query("DELETE FROM concept_parent WHERE concept_uid = ? OR parent_uid = ?")
        .bind(uid)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM concept_equivalence WHERE a_uid = ? OR b_uid = ?")
        .bind(uid)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM concept WHERE uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}
