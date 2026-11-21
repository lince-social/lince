use std::fmt;

use nucleus::{
    execution::now,
    performance::{Span, add},
};
use sqlx::{Sqlite, SqlitePool, Transaction, query, query_scalar};

use super::{
    MAX_SNAPSHOT_BYTES, Qualification, RecordDocMetadata,
    metadata::{record_state_on, validate_positive, validate_through_seq},
    metadata_on, protocol,
};
use crate::{StoreError, write_tx};

/// Legacy snapshot read result.
pub struct RecordDocRow {
    /// Canonical record identity.
    pub record_uid: String,
    /// Stored document bytes.
    pub snapshot: Vec<u8>,
    /// Highest journal sequence included in the snapshot.
    pub through_seq: i64,
    /// Snapshot update time in RFC 3339 form.
    pub updated_at: String,
}

impl fmt::Debug for RecordDocRow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordDocRow")
            .field("record_uid", &self.record_uid)
            .field("snapshot_bytes", &self.snapshot.len())
            .field("through_seq", &self.through_seq)
            .finish_non_exhaustive()
    }
}

/// Validated metadata and the corresponding bounded snapshot bytes, when present.
#[derive(Clone, PartialEq, Eq)]
pub struct RecordDocSnapshot {
    /// Validated record and snapshot metadata.
    pub metadata: RecordDocMetadata,
    /// Stored snapshot bytes, when present.
    pub snapshot: Option<Vec<u8>>,
}

impl fmt::Debug for RecordDocSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordDocSnapshot")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

fn validate_snapshot(snapshot: &[u8]) -> Result<(), StoreError> {
    if snapshot.len() > MAX_SNAPSHOT_BYTES {
        return Err(protocol("Record document snapshot exceeds its byte limit"));
    }
    Ok(())
}

/// Reads a validated snapshot in the caller's transaction, bounded by the byte limit.
///
/// # Errors
/// Returns an error for invalid record metadata, malformed bytes, or a failed read.
pub async fn snapshot_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
) -> Result<RecordDocSnapshot, StoreError> {
    let _read = Span::new("record-doc-snapshot-read");
    let metadata = metadata_on(tx, record_uid).await?;
    if metadata.qualification == Qualification::Absent {
        return Ok(RecordDocSnapshot {
            metadata,
            snapshot: None,
        });
    }
    let snapshot = query_scalar::<_, Option<Vec<u8>>>(
        "SELECT CASE
                    WHEN typeof(snapshot) = 'blob'
                     AND length(CAST(snapshot AS BLOB)) <= ?
                    THEN snapshot
                END
           FROM record_doc
          WHERE record_uid = ? OR record_uid = CAST(? AS BLOB)",
    )
    .bind(i64::try_from(MAX_SNAPSHOT_BYTES).map_err(|error| protocol(error.to_string()))?)
    .bind(record_uid)
    .bind(record_uid)
    .fetch_one(&mut **tx)
    .await?
    .ok_or_else(|| protocol("Record document snapshot became unreadable"))?;
    if Some(snapshot.len()) != metadata.snapshot_bytes {
        return Err(protocol("Record document snapshot changed during its read"));
    }
    add(
        "record-doc-snapshot-bytes",
        u64::try_from(snapshot.len()).unwrap_or(u64::MAX),
    );
    Ok(RecordDocSnapshot {
        metadata,
        snapshot: Some(snapshot),
    })
}

/// Reads a stored snapshot and its journal position through the legacy API.
///
/// # Errors
/// Returns an error if the record or snapshot is invalid or the read fails.
pub async fn get(pool: &SqlitePool, record_uid: &str) -> Result<Option<RecordDocRow>, StoreError> {
    let mut tx = pool.begin().await?;
    let RecordDocSnapshot { metadata, snapshot } = snapshot_on(&mut tx, record_uid).await?;
    tx.commit().await?;
    let Some(snapshot) = snapshot else {
        return Ok(None);
    };
    Ok(Some(RecordDocRow {
        record_uid: metadata.record_uid,
        snapshot,
        through_seq: metadata
            .through_seq
            .ok_or_else(|| protocol("Record document through-sequence disappeared"))?,
        updated_at: metadata
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
    let next = current
        .checked_add(1)
        .ok_or_else(|| protocol("Record document generation is exhausted"))?;
    let result = query(
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

/// Writes a snapshot if both the retained generation and record revision match.
///
/// # Errors
/// Returns an error for invalid data, a generation or revision conflict, or a failed write.
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
    query(
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
    .bind(now().to_rfc3339())
    .bind(expected_generation)
    .bind(final_revision)
    .execute(&mut **tx)
    .await?;
    metadata_on(tx, record_uid).await
}

async fn retire_on(tx: &mut Transaction<'_, Sqlite>, record_uid: &str) -> Result<(), StoreError> {
    let _retirement = Span::new("record-doc-retirement");
    let result =
        query("DELETE FROM record_doc WHERE record_uid = ? OR record_uid = CAST(? AS BLOB)")
            .bind(record_uid)
            .bind(record_uid)
            .execute(&mut **tx)
            .await?;
    add("record-doc-retired-rows", result.rows_affected());
    Ok(())
}

/// Advances the retained generation and removes the prior snapshot in the caller's transaction.
///
/// # Errors
/// Returns an error for an invalid or mismatched generation, or a failed update or delete.
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

/// Writes an unqualified legacy snapshot in a new transaction.
///
/// # Errors
/// Returns an error for invalid data, an exhausted generation, or a failed write.
pub async fn put(
    pool: &SqlitePool,
    record_uid: &str,
    snapshot: &[u8],
    through_seq: i64,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    put_on(&mut tx, record_uid, snapshot, through_seq).await?;
    tx.commit().await
}

/// Advances the retained generation and writes an unqualified legacy snapshot.
///
/// # Errors
/// Returns an error for invalid data, an exhausted generation, or a failed write.
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
    query(
        "INSERT INTO record_doc
             (record_uid, snapshot, through_seq, updated_at, generation, base_revision)
         VALUES (?, ?, ?, ?, NULL, NULL)",
    )
    .bind(record_uid)
    .bind(snapshot)
    .bind(through_seq)
    .bind(now().to_rfc3339())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Advances the retained generation and removes the snapshot.
///
/// # Errors
/// Returns an error for an invalid record, an exhausted generation, or a failed write.
pub async fn delete(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    advance_generation_on(&mut tx, record_uid, None).await?;
    retire_on(&mut tx, record_uid).await?;
    tx.commit().await
}
