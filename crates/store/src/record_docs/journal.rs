use sqlx::{Sqlite, SqlitePool, Transaction, query_as, query_scalar};

use crate::StoreError;

/// Reads document update and snapshot payloads present after a journal sequence.
///
/// # Errors
/// Returns an error if the query or journal decoding fails.
pub async fn doc_tail(
    pool: &SqlitePool,
    record_uid: &str,
    through_seq: i64,
) -> Result<Vec<(i64, String)>, StoreError> {
    query_as(
        "SELECT seq, value FROM sync_op
          WHERE tbl = 'record' AND kind IN ('crdt', 'snapshot')
            AND uid = ? AND seq > ?
            AND value IS NOT NULL
          ORDER BY seq",
    )
    .bind(record_uid)
    .bind(through_seq)
    .fetch_all(pool)
    .await
}

/// Checks whether enough CRDT operations follow the stored snapshot to compact.
///
/// # Errors
/// Returns an error if the journal query fails.
pub async fn compaction_due_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    min_ops: i64,
) -> Result<bool, StoreError> {
    if min_ops <= 0 {
        return Ok(true);
    }
    query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM sync_op
            WHERE tbl = 'record' AND kind = 'crdt' AND uid = ?
              AND seq > COALESCE((SELECT through_seq FROM record_doc WHERE record_uid = ?), 0)
            LIMIT 1 OFFSET ?
        )",
    )
    .bind(record_uid)
    .bind(record_uid)
    .bind(min_ops - 1)
    .fetch_one(&mut **tx)
    .await
}

/// Counts CRDT update operations after a journal sequence.
///
/// # Errors
/// Returns an error if the count query fails.
pub async fn crdt_ops_since(
    pool: &SqlitePool,
    record_uid: &str,
    through_seq: i64,
) -> Result<i64, StoreError> {
    query_scalar(
        "SELECT COUNT(1) FROM sync_op
          WHERE tbl = 'record' AND kind = 'crdt' AND uid = ? AND seq > ?",
    )
    .bind(record_uid)
    .bind(through_seq)
    .fetch_one(pool)
    .await
}

/// Finds records with enough post-snapshot CRDT operations to compact.
///
/// # Errors
/// Returns an error if the journal query fails.
pub async fn records_needing_compaction(
    pool: &SqlitePool,
    min_ops: i64,
) -> Result<Vec<String>, StoreError> {
    query_scalar(
        "SELECT o.uid AS uid, COUNT(1) AS n
           FROM sync_op o
           LEFT JOIN record_doc d ON d.record_uid = o.uid
          WHERE o.tbl = 'record' AND o.kind = 'crdt'
            AND o.seq > COALESCE(d.through_seq, 0)
          GROUP BY o.uid
         HAVING n >= ?",
    )
    .bind(min_ops)
    .fetch_all(pool)
    .await
}

/// Checks for a stored document snapshot or a document operation in the journal.
///
/// # Errors
/// Returns an error if the history query fails.
pub async fn has_crdt_history(pool: &SqlitePool, record_uid: &str) -> Result<bool, StoreError> {
    query_scalar(
        "SELECT EXISTS(SELECT 1 FROM record_doc WHERE record_uid = ?1)
             OR EXISTS(SELECT 1 FROM sync_op
                        WHERE tbl = 'record' AND kind IN ('crdt', 'snapshot') AND uid = ?1)",
    )
    .bind(record_uid)
    .fetch_one(pool)
    .await
}
