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

/// Store this Cell's own compacted snapshot.
///
/// Written ONLY by local compaction. A `snapshot` op arriving from a peer is
/// imported into the open doc and left in the log for `doc_tail` to replay,
/// not written here — because `through_seq` moves in lockstep with the open
/// doc's `snapshot_vv`, and advancing one without the other is how updates get
/// dropped with no way to notice. Replaying a peer's snapshot on every load
/// costs a little; the alternative risks losing text.
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

/// One record's doc history past the stored snapshot's seq — the load tail.
///
/// Includes `snapshot` ops as well as `crdt` ops, in seq order. Loro imports
/// both blob kinds identically, and a snapshot op that arrived from a peer is
/// deliberately left in the log rather than written into `record_doc` (see
/// `put`), so the loader has to replay it like any other entry. Filtering to
/// `crdt` here would silently drop a peer's entire document base.
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

/// Records whose doc has accumulated at least `min_ops` `crdt` ops past its
/// stored snapshot — the compaction sweep's worklist.
///
/// A doc edited heavily and then ABANDONED is the case this exists for.
/// Compaction used to happen only on the next write, which never comes for an
/// abandoned doc, so its tail grew forever and none of it was ever prunable —
/// the exact unboundedness snapshots-in-the-log exist to end.
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

/// Whether any collaborative history exists for this record — the "has collab
/// started" test that decides seeding and set-op precedence.
///
/// Counts `snapshot` ops as well as `crdt` ops. Once crdt ops below a snapshot
/// become prunable, a long-lived document can end up represented by a snapshot
/// ALONE; answering `false` there would let a stale scalar `set` on
/// `head`/`body` win against the record-doc and clobber the text.
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
