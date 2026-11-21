// non-lean-yet

use chrono::{DateTime, Utc};
use nucleus::Fact;
use sqlx::{Row, SqlitePool, query, query_as};

use super::{instant, rows::map_fact};
use crate::{StoreError, write_tx};

/// Sets the fact retention horizon for a record kind.
///
/// # Errors
/// Returns an error if the policy write fails.
pub async fn set_retention(
    pool: &SqlitePool,
    kind: &str,
    horizon_seconds: i64,
) -> Result<(), StoreError> {
    query(
        "INSERT INTO retention_policy (kind, horizon_seconds) VALUES (?, ?)
         ON CONFLICT(kind) DO UPDATE SET horizon_seconds = excluded.horizon_seconds",
    )
    .bind(kind)
    .bind(horizon_seconds)
    .execute(pool)
    .await?;
    Ok(())
}

/// Lists configured record kinds and their retention horizons in seconds.
///
/// # Errors
/// Returns an error if the policy query fails.
pub async fn retention_policies(pool: &SqlitePool) -> Result<Vec<(String, i64)>, StoreError> {
    query_as("SELECT kind, horizon_seconds FROM retention_policy")
        .fetch_all(pool)
        .await
}

/// Reads the record's latest checkpoint and its local database position.
///
/// # Errors
/// Returns an error if the query or checkpoint decoding fails.
pub async fn last_checkpoint(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<(i64, Fact)>, StoreError> {
    query(
        "SELECT rowid, * FROM fact
          WHERE record_uid = ? AND cause_kind = 'checkpoint'
          ORDER BY rowid DESC LIMIT 1",
    )
    .bind(record_uid)
    .try_map(|row| Ok((row.try_get("rowid")?, map_fact(row)?)))
    .fetch_optional(pool)
    .await
}

/// Lists facts strictly older than both the checkpoint position and cutoff time.
///
/// # Errors
/// Returns an error if the query or fact decoding fails.
pub async fn archivable_before(
    pool: &SqlitePool,
    record_uid: &str,
    checkpoint_rowid: i64,
    cutoff: DateTime<Utc>,
) -> Result<Vec<Fact>, StoreError> {
    query(
        "SELECT * FROM fact
          WHERE record_uid = ? AND rowid < ? AND at < ?
          ORDER BY rowid",
    )
    .bind(record_uid)
    .bind(checkpoint_rowid)
    .bind(instant(cutoff))
    .try_map(map_fact)
    .fetch_all(pool)
    .await
}

/// Deletes the specified local facts in a single write transaction.
///
/// # Errors
/// Returns an error if a delete or transaction operation fails.
pub async fn delete_by_uids(pool: &SqlitePool, uids: &[String]) -> Result<u64, StoreError> {
    let mut tx = write_tx(pool).await?;
    let mut deleted = 0;
    for uid in uids {
        deleted += query("DELETE FROM fact WHERE uid = ?")
            .bind(uid)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    }
    tx.commit().await?;
    Ok(deleted)
}
