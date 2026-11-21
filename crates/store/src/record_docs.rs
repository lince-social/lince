// non-lean-yet
// Scope: const MAX_SNAPSHOT_BYTES, const MAX_RECORD_UID_BYTES, const MAX_TIMESTAMP_BYTES, struct
//   RecordDocRow.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 197.
// - [Clippy: clippy::missing_panics_doc] State the panic conditions for exported functions. Sites
//   143, 302.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 182, 332.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 18.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   105, 114, 176, 177, 202, 323.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   213, 243.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 143, 302, 339, 390, 443, 454, 465, 489, 497, 518, 541, 557, 578.
// - [Clippy: clippy::needless_question_mark] enclosing `Ok` and `?` operator are unneeded. Sites
//   526.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 10.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 6, 10,
//   11, 12, 13, 14, 18, 19, 20, 21, 22, 24, 25, 26, 31, 32, 33, 34, 35, 36, 37, 38, 42, 43, 44,
//   143, 302, 339, 390, 443, 454, 465, 489, 497, 518, 541, 557, 578.

use chrono::DateTime;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

pub const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_RECORD_UID_BYTES: usize = 28;
const MAX_TIMESTAMP_BYTES: usize = 64;

pub struct RecordDocRow {
    pub record_uid: String,
    pub snapshot: Vec<u8>,
    pub through_seq: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Qualification {
    Absent,
    Unqualified {
        generation: Option<i64>,
        base_revision: Option<i64>,
    },
    Qualified {
        generation: i64,
        base_revision: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDocMetadata {
    pub record_uid: String,
    pub record_revision: i64,
    pub retained_generation: i64,
    pub qualification: Qualification,
    pub snapshot_bytes: Option<usize>,
    pub through_seq: Option<i64>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDocSnapshot {
    pub metadata: RecordDocMetadata,
    pub snapshot: Option<Vec<u8>>,
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn validate_record_uid(record_uid: &str) -> Result<(), StoreError> {
    if !nucleus::valid_uid(record_uid, "r") {
        return Err(protocol("Record document requires a canonical Record uid"));
    }
    Ok(())
}

fn validate_positive(value: i64, label: &str) -> Result<(), StoreError> {
    if value <= 0 {
        return Err(protocol(format!(
            "Record document {label} must be positive"
        )));
    }
    Ok(())
}

fn validate_through_seq(through_seq: i64) -> Result<(), StoreError> {
    if through_seq < 0 {
        return Err(protocol(
            "Record document through-sequence must not be negative",
        ));
    }
    Ok(())
}

fn validate_snapshot(snapshot: &[u8]) -> Result<(), StoreError> {
    if snapshot.len() > MAX_SNAPSHOT_BYTES {
        return Err(protocol("Record document snapshot exceeds its byte limit"));
    }
    Ok(())
}

async fn record_state_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
) -> Result<(i64, i64), StoreError> {
    validate_record_uid(record_uid)?;
    let row = sqlx::query(
        "SELECT typeof(r.uid) AS uid_type,
                length(CAST(r.uid AS BLOB)) AS uid_bytes,
                CASE
                    WHEN typeof(r.uid) = 'text' AND length(CAST(r.uid AS BLOB)) <= ?
                    THEN r.uid
                END AS safe_uid,
                r.deleted_at IS NULL AS active,
                typeof(rr.revision) AS revision_type,
                CASE WHEN typeof(rr.revision) = 'integer' THEN rr.revision END AS revision,
                typeof(rr.document_generation) AS generation_type,
                CASE WHEN typeof(rr.document_generation) = 'integer'
                     THEN rr.document_generation END AS document_generation
           FROM record AS r
           LEFT JOIN record_revision AS rr ON rr.record_uid = r.uid
          WHERE r.uid = ?",
    )
    .bind(i64::try_from(MAX_RECORD_UID_BYTES).expect("uid limit fits SQLite"))
    .bind(record_uid)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| protocol("Record document target is missing"))?;
    let uid_type: &str = row.try_get("uid_type")?;
    let uid_bytes: i64 = row.try_get("uid_bytes")?;
    let safe_uid: Option<&str> = row.try_get("safe_uid")?;
    if uid_type != "text"
        || uid_bytes != i64::try_from(record_uid.len()).expect("uid length fits SQLite")
        || safe_uid != Some(record_uid)
    {
        return Err(protocol("Record document target identity is corrupt"));
    }
    let active: i64 = row.try_get("active")?;
    if active != 1 {
        return Err(protocol("Record document target is deleted"));
    }
    let revision_type: &str = row.try_get("revision_type")?;
    let revision: Option<i64> = row.try_get("revision")?;
    if revision_type != "integer" {
        return Err(protocol("Record document revision has an invalid type"));
    }
    let revision = revision.ok_or_else(|| protocol("Record document revision is unreadable"))?;
    validate_positive(revision, "revision")?;
    let generation_type: &str = row.try_get("generation_type")?;
    let generation: Option<i64> = row.try_get("document_generation")?;
    if generation_type != "integer" {
        return Err(protocol(
            "Record document retained generation has an invalid type",
        ));
    }
    let generation =
        generation.ok_or_else(|| protocol("Record document retained generation is unreadable"))?;
    validate_positive(generation, "retained generation")?;
    Ok((revision, generation))
}

pub async fn metadata_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
) -> Result<RecordDocMetadata, StoreError> {
    let _read = nucleus::performance::Span::new("record-doc-metadata-read");
    let (record_revision, retained_generation) = record_state_on(tx, record_uid).await?;
    let rows = sqlx::query(
        "SELECT typeof(record_uid) AS uid_type,
                length(CAST(record_uid AS BLOB)) AS uid_bytes,
                CASE
                    WHEN typeof(record_uid) = 'text'
                     AND length(CAST(record_uid AS BLOB)) <= ?
                    THEN record_uid
                END AS safe_uid,
                typeof(snapshot) AS snapshot_type,
                length(CAST(snapshot AS BLOB)) AS snapshot_bytes,
                typeof(through_seq) AS through_type,
                CASE WHEN typeof(through_seq) = 'integer' THEN through_seq END AS through_seq,
                typeof(updated_at) AS updated_type,
                length(CAST(updated_at AS BLOB)) AS updated_bytes,
                CASE
                    WHEN typeof(updated_at) = 'text'
                     AND length(CAST(updated_at AS BLOB)) <= ?
                    THEN updated_at
                END AS safe_updated_at,
                typeof(generation) AS generation_type,
                CASE WHEN typeof(generation) = 'integer' THEN generation END AS generation,
                typeof(base_revision) AS base_type,
                CASE WHEN typeof(base_revision) = 'integer'
                     THEN base_revision END AS base_revision
           FROM record_doc
          WHERE record_uid = ? OR record_uid = CAST(? AS BLOB)",
    )
    .bind(i64::try_from(MAX_RECORD_UID_BYTES).expect("uid limit fits SQLite"))
    .bind(i64::try_from(MAX_TIMESTAMP_BYTES).expect("timestamp limit fits SQLite"))
    .bind(record_uid)
    .bind(record_uid)
    .fetch_all(&mut **tx)
    .await?;
    nucleus::performance::add("record-doc-metadata-matches", rows.len() as u64);
    if rows.is_empty() {
        return Ok(RecordDocMetadata {
            record_uid: record_uid.to_owned(),
            record_revision,
            retained_generation,
            qualification: Qualification::Absent,
            snapshot_bytes: None,
            through_seq: None,
            updated_at: None,
        });
    }
    if rows.len() != 1 {
        return Err(protocol("Record document identity is ambiguous"));
    }
    let row = &rows[0];
    let uid_type: &str = row.try_get("uid_type")?;
    let uid_bytes: i64 = row.try_get("uid_bytes")?;
    let safe_uid: Option<&str> = row.try_get("safe_uid")?;
    if uid_type != "text"
        || uid_bytes != i64::try_from(record_uid.len()).expect("uid length fits SQLite")
        || safe_uid != Some(record_uid)
    {
        return Err(protocol("Record document identity is corrupt"));
    }
    let snapshot_type: &str = row.try_get("snapshot_type")?;
    if snapshot_type != "blob" {
        return Err(protocol("Record document snapshot has an invalid type"));
    }
    let snapshot_bytes: i64 = row.try_get("snapshot_bytes")?;
    let snapshot_bytes = usize::try_from(snapshot_bytes)
        .map_err(|_| protocol("Record document snapshot has an invalid byte length"))?;
    if snapshot_bytes > MAX_SNAPSHOT_BYTES {
        return Err(protocol("Record document snapshot exceeds its byte limit"));
    }
    let through_type: &str = row.try_get("through_type")?;
    if through_type != "integer" {
        return Err(protocol(
            "Record document through-sequence has an invalid type",
        ));
    }
    let through_seq: Option<i64> = row.try_get("through_seq")?;
    let through_seq =
        through_seq.ok_or_else(|| protocol("Record document through-sequence is unreadable"))?;
    validate_through_seq(through_seq)?;
    let updated_type: &str = row.try_get("updated_type")?;
    let updated_bytes: i64 = row.try_get("updated_bytes")?;
    let updated_at: Option<String> = row.try_get("safe_updated_at")?;
    if updated_type != "text"
        || updated_bytes < 0
        || usize::try_from(updated_bytes)
            .ok()
            .is_none_or(|n| n > MAX_TIMESTAMP_BYTES)
    {
        return Err(protocol(
            "Record document update time has an invalid type or size",
        ));
    }
    let updated_at =
        updated_at.ok_or_else(|| protocol("Record document update time is unreadable"))?;
    DateTime::parse_from_rfc3339(&updated_at)
        .map_err(|_| protocol("Record document update time is invalid"))?;
    let generation_type: &str = row.try_get("generation_type")?;
    let base_type: &str = row.try_get("base_type")?;
    let generation = match generation_type {
        "null" => None,
        "integer" => row.try_get::<Option<i64>, _>("generation")?,
        _ => return Err(protocol("Record document generation has an invalid type")),
    };
    let base_revision = match base_type {
        "null" => None,
        "integer" => row.try_get::<Option<i64>, _>("base_revision")?,
        _ => {
            return Err(protocol(
                "Record document base revision has an invalid type",
            ));
        }
    };
    let qualification = match (generation, base_revision) {
        (None, None) => Qualification::Unqualified {
            generation: None,
            base_revision: None,
        },
        (Some(generation), Some(base_revision)) => {
            validate_positive(generation, "generation")?;
            validate_positive(base_revision, "base revision")?;
            if base_revision > record_revision {
                return Err(protocol(
                    "Record document base revision is ahead of its Record",
                ));
            }
            if generation == retained_generation {
                Qualification::Qualified {
                    generation,
                    base_revision,
                }
            } else {
                Qualification::Unqualified {
                    generation: Some(generation),
                    base_revision: Some(base_revision),
                }
            }
        }
        _ => {
            return Err(protocol(
                "Record document qualification metadata is incomplete",
            ));
        }
    };
    Ok(RecordDocMetadata {
        record_uid: record_uid.to_owned(),
        record_revision,
        retained_generation,
        qualification,
        snapshot_bytes: Some(snapshot_bytes),
        through_seq: Some(through_seq),
        updated_at: Some(updated_at),
    })
}

pub async fn snapshot_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
) -> Result<RecordDocSnapshot, StoreError> {
    let _read = nucleus::performance::Span::new("record-doc-snapshot-read");
    let metadata = metadata_on(tx, record_uid).await?;
    if metadata.qualification == Qualification::Absent {
        return Ok(RecordDocSnapshot {
            metadata,
            snapshot: None,
        });
    }
    let snapshot = sqlx::query_scalar::<_, Option<Vec<u8>>>(
        "SELECT CASE
                    WHEN typeof(snapshot) = 'blob'
                     AND length(CAST(snapshot AS BLOB)) <= ?
                    THEN snapshot
                END
           FROM record_doc
          WHERE record_uid = ? OR record_uid = CAST(? AS BLOB)",
    )
    .bind(i64::try_from(MAX_SNAPSHOT_BYTES).expect("snapshot limit fits SQLite"))
    .bind(record_uid)
    .bind(record_uid)
    .fetch_one(&mut **tx)
    .await?
    .ok_or_else(|| protocol("Record document snapshot became unreadable"))?;
    if Some(snapshot.len()) != metadata.snapshot_bytes {
        return Err(protocol("Record document snapshot changed during its read"));
    }
    nucleus::performance::add("record-doc-snapshot-bytes", snapshot.len() as u64);
    Ok(RecordDocSnapshot {
        metadata,
        snapshot: Some(snapshot),
    })
}

pub async fn get(pool: &SqlitePool, record_uid: &str) -> Result<Option<RecordDocRow>, StoreError> {
    let mut tx = pool.begin().await?;
    let stored = snapshot_on(&mut tx, record_uid).await?;
    tx.commit().await?;
    let Some(snapshot) = stored.snapshot else {
        return Ok(None);
    };
    Ok(Some(RecordDocRow {
        record_uid: stored.metadata.record_uid,
        snapshot,
        through_seq: stored
            .metadata
            .through_seq
            .ok_or_else(|| protocol("Record document through-sequence disappeared"))?,
        updated_at: stored
            .metadata
            .updated_at
            .ok_or_else(|| protocol("Record document update time disappeared"))?,
    }))
}

async fn advance_generation_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    expected_generation: Option<i64>,
) -> Result<i64, StoreError> {
    let (_, current) = record_state_on(tx, record_uid).await?;
    if expected_generation.is_some_and(|expected| expected != current) {
        return Err(protocol("Record document generation conflict"));
    }
    if current == i64::MAX {
        return Err(protocol("Record document generation is exhausted"));
    }
    let next = current + 1;
    let result = sqlx::query(
        "UPDATE record_revision SET document_generation = ?
          WHERE record_uid = ? AND document_generation = ?",
    )
    .bind(next)
    .bind(record_uid)
    .bind(current)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol(
            "Record document generation changed during its reset",
        ));
    }
    Ok(next)
}

pub async fn put_qualified_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    snapshot: &[u8],
    through_seq: i64,
    expected_generation: i64,
    final_revision: i64,
) -> Result<RecordDocMetadata, StoreError> {
    validate_snapshot(snapshot)?;
    validate_through_seq(through_seq)?;
    validate_positive(expected_generation, "expected generation")?;
    validate_positive(final_revision, "final revision")?;
    let current = metadata_on(tx, record_uid).await?;
    if current.retained_generation != expected_generation {
        return Err(protocol("Record document generation conflict"));
    }
    if current.record_revision != final_revision {
        return Err(protocol("Record document revision conflict"));
    }
    sqlx::query(
        "INSERT INTO record_doc
             (record_uid, snapshot, through_seq, updated_at, generation, base_revision)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(record_uid) DO UPDATE SET
             snapshot = excluded.snapshot,
             through_seq = excluded.through_seq,
             updated_at = excluded.updated_at,
             generation = excluded.generation,
             base_revision = excluded.base_revision",
    )
    .bind(record_uid)
    .bind(snapshot)
    .bind(through_seq)
    .bind(nucleus::execution::now().to_rfc3339())
    .bind(expected_generation)
    .bind(final_revision)
    .execute(&mut **tx)
    .await?;
    metadata_on(tx, record_uid).await
}

async fn retire_on(tx: &mut Transaction<'_, Sqlite>, record_uid: &str) -> Result<(), StoreError> {
    let _retirement = nucleus::performance::Span::new("record-doc-retirement");
    let result =
        sqlx::query("DELETE FROM record_doc WHERE record_uid = ? OR record_uid = CAST(? AS BLOB)")
            .bind(record_uid)
            .bind(record_uid)
            .execute(&mut **tx)
            .await?;
    nucleus::performance::add("record-doc-retired-rows", result.rows_affected());
    Ok(())
}

pub async fn reset_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    expected_generation: i64,
) -> Result<i64, StoreError> {
    validate_positive(expected_generation, "expected generation")?;
    let next = advance_generation_on(tx, record_uid, Some(expected_generation)).await?;
    retire_on(tx, record_uid).await?;
    Ok(next)
}

pub async fn put(
    pool: &SqlitePool,
    record_uid: &str,
    snapshot: &[u8],
    through_seq: i64,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    put_on(&mut tx, record_uid, snapshot, through_seq).await?;
    tx.commit().await
}

pub async fn put_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    snapshot: &[u8],
    through_seq: i64,
) -> Result<(), StoreError> {
    validate_snapshot(snapshot)?;
    validate_through_seq(through_seq)?;
    advance_generation_on(tx, record_uid, None).await?;
    retire_on(tx, record_uid).await?;
    sqlx::query(
        "INSERT INTO record_doc
             (record_uid, snapshot, through_seq, updated_at, generation, base_revision)
         VALUES (?, ?, ?, ?, NULL, NULL)",
    )
    .bind(record_uid)
    .bind(snapshot)
    .bind(through_seq)
    .bind(nucleus::execution::now().to_rfc3339())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    advance_generation_on(&mut tx, record_uid, None).await?;
    retire_on(&mut tx, record_uid).await?;
    tx.commit().await?;
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

pub async fn compaction_due_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    min_ops: i64,
) -> Result<bool, StoreError> {
    if min_ops <= 0 {
        return Ok(true);
    }
    Ok(sqlx::query_scalar(
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
    .await?)
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
