use chrono::DateTime;
use nucleus::{
    performance::{Span, add},
    valid_uid,
};
use sqlx::{Row, Sqlite, Transaction, query, sqlite::SqliteRow};

use super::{MAX_SNAPSHOT_BYTES, protocol};
use crate::StoreError;

const MAX_RECORD_UID_BYTES: i64 = 28;
const MAX_TIMESTAMP_BYTES: i64 = 64;

/// Whether a stored snapshot belongs to the retained document generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Qualification {
    /// No snapshot is stored for this record.
    Absent,
    /// A legacy snapshot or one for another generation.
    Unqualified {
        /// Its recorded generation, when present.
        generation: Option<i64>,
        /// Its recorded base revision, when present.
        base_revision: Option<i64>,
    },
    /// A snapshot for the retained generation and a known base revision.
    Qualified {
        /// Retained document generation.
        generation: i64,
        /// Record revision on which the snapshot was based.
        base_revision: i64,
    },
}

/// Validated document metadata without loading the snapshot bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDocMetadata {
    /// Canonical record identity.
    pub record_uid: String,
    /// Current revision of the record.
    pub record_revision: i64,
    /// Generation retained by the current record.
    pub retained_generation: i64,
    /// Relationship between the snapshot and the retained generation.
    pub qualification: Qualification,
    /// Size of a stored snapshot, when present.
    pub snapshot_bytes: Option<usize>,
    /// Highest journal sequence included in that snapshot.
    pub through_seq: Option<i64>,
    /// Snapshot update time in RFC 3339 form, when present.
    pub updated_at: Option<String>,
}

fn validate_record_uid(record_uid: &str) -> Result<(), StoreError> {
    if !valid_uid(record_uid, "r") {
        return Err(protocol("Record document requires a canonical Record uid"));
    }
    Ok(())
}

pub(super) fn validate_positive(value: i64, label: &str) -> Result<(), StoreError> {
    if value <= 0 {
        return Err(protocol(format!(
            "Record document {label} must be positive"
        )));
    }
    Ok(())
}

pub(super) fn validate_through_seq(through_seq: i64) -> Result<(), StoreError> {
    if through_seq < 0 {
        return Err(protocol(
            "Record document through-sequence must not be negative",
        ));
    }
    Ok(())
}

fn matches_record_uid(row: &SqliteRow, expected: &str) -> Result<bool, StoreError> {
    let kind: &str = row.try_get("uid_type")?;
    let bytes: i64 = row.try_get("uid_bytes")?;
    let text: Option<&str> = row.try_get("safe_uid")?;
    Ok(kind == "text"
        && usize::try_from(bytes).ok() == Some(expected.len())
        && text == Some(expected))
}

pub(super) async fn record_state_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
) -> Result<(i64, i64), StoreError> {
    validate_record_uid(record_uid)?;
    let row = query(
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
    .bind(MAX_RECORD_UID_BYTES)
    .bind(record_uid)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| protocol("Record document target is missing"))?;
    if !matches_record_uid(&row, record_uid)? {
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

/// Reads validated record revision, retained generation, and bounded snapshot metadata.
///
/// # Errors
/// Returns an error for a missing, deleted, or invalid record or snapshot, or a failed read.
pub async fn metadata_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
) -> Result<RecordDocMetadata, StoreError> {
    let _read = Span::new("record-doc-metadata-read");
    let (record_revision, retained_generation) = record_state_on(tx, record_uid).await?;
    let rows = query(
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
    .bind(MAX_RECORD_UID_BYTES)
    .bind(MAX_TIMESTAMP_BYTES)
    .bind(record_uid)
    .bind(record_uid)
    .fetch_all(&mut **tx)
    .await?;
    add(
        "record-doc-metadata-matches",
        u64::try_from(rows.len()).unwrap_or(u64::MAX),
    );
    let row = match rows.as_slice() {
        [row] => row,
        [] => {
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
        _ => return Err(protocol("Record document identity is ambiguous")),
    };
    if !matches_record_uid(row, record_uid)? {
        return Err(protocol("Record document identity is corrupt"));
    }
    let snapshot_type: &str = row.try_get("snapshot_type")?;
    if snapshot_type != "blob" {
        return Err(protocol("Record document snapshot has an invalid type"));
    }
    let snapshot_bytes: i64 = row.try_get("snapshot_bytes")?;
    let snapshot_bytes = usize::try_from(snapshot_bytes).map_err(|error| {
        protocol(format!(
            "Record document snapshot has an invalid byte length: {error}"
        ))
    })?;
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
    if updated_type != "text" || !(0..=MAX_TIMESTAMP_BYTES).contains(&updated_bytes) {
        return Err(protocol(
            "Record document update time has an invalid type or size",
        ));
    }
    let updated_at =
        updated_at.ok_or_else(|| protocol("Record document update time is unreadable"))?;
    DateTime::parse_from_rfc3339(&updated_at)
        .map_err(|error| protocol(format!("Record document update time is invalid: {error}")))?;
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
