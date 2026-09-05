use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub struct RecordDocRow {
    pub record_uid: String,
    pub snapshot: Vec<u8>,
    pub through_seq: i64,
    pub updated_at: String,
}

pub async fn get(pool: &SqlitePool, record_uid: &str) -> Result<Option<RecordDocRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM record_doc WHERE record_uid = ?")
        .bind(record_uid)
        .fetch_optional(pool)
        .await?
        .map(|row| RecordDocRow {
            record_uid: row.get("record_uid"),
            snapshot: row.get("snapshot"),
            through_seq: row.get("through_seq"),
            updated_at: row.get("updated_at"),
        }))
}

pub async fn put(
    pool: &SqlitePool,
    record_uid: &str,
    snapshot: &[u8],
    through_seq: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO record_doc (record_uid, snapshot, through_seq, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(record_uid) DO UPDATE SET
             snapshot = excluded.snapshot,
             through_seq = excluded.through_seq,
             updated_at = excluded.updated_at",
    )
    .bind(record_uid)
    .bind(snapshot)
    .bind(through_seq)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM record_doc WHERE record_uid = ?")
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn doc_tail(
    pool: &SqlitePool,
    record_uid: &str,
    through_seq: i64,
) -> Result<Vec<(i64, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT seq, value FROM sync_op
          WHERE tbl = 'record' AND kind IN ('crdt', 'snapshot')
            AND uid = ? AND seq > ?
            AND value IS NOT NULL
          ORDER BY seq",
    )
    .bind(record_uid)
    .bind(through_seq)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("seq"), row.get("value")))
    .collect())
}

pub async fn crdt_ops_since(
    pool: &SqlitePool,
    record_uid: &str,
    through_seq: i64,
) -> Result<i64, StoreError> {
    Ok(sqlx::query(
        "SELECT COUNT(1) AS n FROM sync_op
          WHERE tbl = 'record' AND kind = 'crdt' AND uid = ? AND seq > ?",
    )
    .bind(record_uid)
    .bind(through_seq)
    .fetch_one(pool)
    .await?
    .get("n"))
}

pub async fn records_needing_compaction(
    pool: &SqlitePool,
    min_ops: i64,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query(
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
    .await?
    .into_iter()
    .map(|row| row.get("uid"))
    .collect())
}

pub async fn has_crdt_history(pool: &SqlitePool, record_uid: &str) -> Result<bool, StoreError> {
    if sqlx::query("SELECT 1 FROM record_doc WHERE record_uid = ?")
        .bind(record_uid)
        .fetch_optional(pool)
        .await?
        .is_some()
    {
        return Ok(true);
    }
    Ok(sqlx::query(
        "SELECT 1 FROM sync_op
          WHERE tbl = 'record' AND kind IN ('crdt', 'snapshot') AND uid = ? LIMIT 1",
    )
    .bind(record_uid)
    .fetch_optional(pool)
    .await?
    .is_some())
}
