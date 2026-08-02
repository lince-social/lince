//! Persisted Loro record-doc snapshots (Ontology §11 "Merge"). The store
//! treats snapshots as opaque blobs — every Loro call lives in
//! `engine::collab`, so the CRDT engine stays replaceable.

use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub struct RecordDocRow {
    pub record_uid: String,
    pub snapshot: Vec<u8>,
    /// Ops at or below this seq are folded into the snapshot; a doc load
    /// imports the snapshot plus every `crdt` op past it.
    pub through_seq: i64,
    pub updated_at: String,
}

pub async fn get(pool: &SqlitePool, record_uid: &str) -> Result<Option<RecordDocRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM record_doc WHERE record_uid = ?")
            .bind(record_uid)
            .fetch_optional(pool)
            .await?
            .map(|row| RecordDocRow {
                record_uid: row.get("record_uid"),
                snapshot: row.get("snapshot"),
                through_seq: row.get("through_seq"),
                updated_at: row.get("updated_at"),
            }),
    )
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

/// Crdt ops for one record past the snapshot's seq — the doc's load tail.
pub async fn crdt_tail(
    pool: &SqlitePool,
    record_uid: &str,
    through_seq: i64,
) -> Result<Vec<(i64, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT seq, value FROM sync_op
          WHERE tbl = 'record' AND kind = 'crdt' AND uid = ? AND seq > ?
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

/// How many crdt ops one record has accumulated past a seq (compaction gauge).
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

/// Whether any crdt history exists for this record (snapshot or logged op) —
/// the "has collab started" test that decides seeding and set-op precedence.
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
        "SELECT 1 FROM sync_op WHERE tbl = 'record' AND kind = 'crdt' AND uid = ? LIMIT 1",
    )
    .bind(record_uid)
    .fetch_optional(pool)
    .await?
    .is_some())
}
