use sqlx::{SqlitePool, query_as, query_scalar};

use super::OpRow;
use crate::StoreError;

/// Reads general-feed operations for an Organ after the given sequence.
pub async fn after(
    pool: &SqlitePool,
    organ_uid: &str,
    seq: i64,
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    query_as(
        "SELECT * FROM sync_op
          WHERE seq > ? AND replica_root IS NULL AND organ_uid = ?
          ORDER BY seq LIMIT ?",
    )
    .bind(seq)
    .bind(organ_uid)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Reads operations in the given replica root after the supplied sequence.
pub async fn after_in_root(
    pool: &SqlitePool,
    root: &str,
    seq: i64,
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    query_as("SELECT * FROM sync_op WHERE seq > ? AND replica_root = ? ORDER BY seq LIMIT ?")
        .bind(seq)
        .bind(root)
        .bind(limit)
        .fetch_all(pool)
        .await
}

/// Reads a field's operation history in sequence order.
pub async fn for_field(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
) -> Result<Vec<OpRow>, StoreError> {
    query_as("SELECT * FROM sync_op WHERE tbl = ? AND uid = ? AND field = ? ORDER BY seq")
        .bind(tbl)
        .bind(uid)
        .bind(field)
        .fetch_all(pool)
        .await
}

/// Returns the highest clock seen for the specified field, if present.
pub async fn latest_hlc_for_field(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
) -> Result<Option<i64>, StoreError> {
    query_scalar("SELECT MAX(hlc) AS hlc FROM sync_op WHERE tbl = ? AND uid = ? AND field = ?")
        .bind(tbl)
        .bind(uid)
        .bind(field)
        .fetch_one(pool)
        .await
}

/// Returns the authoring Organ of the highest-clock field operation.
pub async fn latest_author_for_field(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
) -> Result<Option<String>, StoreError> {
    query_scalar(
        "SELECT organ_uid FROM sync_op
          WHERE tbl = ? AND uid = ? AND field = ?
          ORDER BY hlc DESC
          LIMIT 1",
    )
    .bind(tbl)
    .bind(uid)
    .bind(field)
    .fetch_optional(pool)
    .await
}

/// Returns the highest stored hybrid clock, if the log contains operations.
pub async fn max_hlc(pool: &SqlitePool) -> Result<Option<i64>, StoreError> {
    query_scalar("SELECT MAX(hlc) AS hlc FROM sync_op")
        .fetch_one(pool)
        .await
}

/// Returns the largest stored sequence, or zero for an empty log.
pub async fn max_seq(pool: &SqlitePool) -> Result<i64, StoreError> {
    query_scalar("SELECT COALESCE(MAX(seq), 0) AS seq FROM sync_op")
        .fetch_one(pool)
        .await
}

/// Reads the entire log in hybrid-clock order, breaking ties by sequence.
pub async fn all_by_hlc(pool: &SqlitePool) -> Result<Vec<OpRow>, StoreError> {
    query_as("SELECT * FROM sync_op ORDER BY hlc, seq")
        .fetch_all(pool)
        .await
}

/// Reads the latest set operation for each stored Record field.
pub async fn record_field_tips(pool: &SqlitePool) -> Result<Vec<OpRow>, StoreError> {
    query_as(
        "SELECT * FROM sync_op a
          WHERE a.tbl = 'record' AND a.kind = 'set'
            AND NOT EXISTS (SELECT 1 FROM sync_op b
                             WHERE b.tbl = 'record' AND b.kind = 'set'
                               AND b.uid = a.uid AND b.field = a.field
                               AND (b.hlc > a.hlc
                                    OR (b.hlc = a.hlc AND b.seq > a.seq)))",
    )
    .fetch_all(pool)
    .await
}

/// Looks up an operation by its local sequence.
pub async fn get_by_seq(pool: &SqlitePool, seq: i64) -> Result<Option<OpRow>, StoreError> {
    query_as("SELECT * FROM sync_op WHERE seq = ?")
        .bind(seq)
        .fetch_optional(pool)
        .await
}

/// Returns the highest clock among set operations for the given row.
pub async fn latest_set_hlc_for_row(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
) -> Result<Option<i64>, StoreError> {
    query_scalar("SELECT MAX(hlc) AS hlc FROM sync_op WHERE tbl = ? AND uid = ? AND kind = 'set'")
        .bind(tbl)
        .bind(uid)
        .fetch_one(pool)
        .await
}
