use std::collections::HashMap;

use nucleus::performance::{Span, add};
use serde_json::from_str;
use sqlx::{
    Row, Sqlite, SqlitePool, Transaction, query, query_as, query_scalar, sqlite::SqliteRow,
};

use super::{
    History, MAX_CHECKPOINT_BYTES, MAX_UPDATES, Metadata, PendingRequest, Replay, Stamp,
    bounded_sum, identity, invalid, pending, scope,
};
use crate::StoreError;

pub(super) fn decode_metadata(
    row: &SqliteRow,
    known_pending: Option<(&str, &[String])>,
) -> Result<Metadata, StoreError> {
    let generation: String = row.try_get("generation")?;
    identity(&generation, 128)?;
    let encoded: &str = row.try_get("pending")?;
    bounded_sum([encoded.len()])?;
    let ids = match known_pending {
        Some((expected, ids)) if encoded == expected => {
            add("record-recovery-pending-reused", 1);
            ids.to_vec()
        }
        _ => {
            let ids: Vec<String> = from_str(encoded).map_err(|error| invalid(error.to_string()))?;
            pending(&ids)?;
            ids
        }
    };
    let size = |name| {
        usize::try_from(row.try_get::<i64, _>(name)?)
            .map_err(|error| invalid(format!("Invalid recovery journal size: {error}")))
    };
    let metadata = Metadata {
        stamp: Stamp {
            generation,
            sequence: row.try_get("sequence")?,
        },
        pending: ids,
        update_count: size("update_count")?,
        update_bytes: size("update_bytes")?,
        request_bytes: size("request_bytes")?,
        checkpoint_bytes: size("checkpoint_bytes")?,
        write_hash: row.try_get("write_hash")?,
    };
    if metadata.stamp.sequence < 0
        || metadata.checkpoint_bytes > MAX_CHECKPOINT_BYTES
        || metadata.update_count > MAX_UPDATES
    {
        return Err(invalid("Invalid recovery journal metadata"));
    }
    bounded_sum([
        metadata.checkpoint_bytes,
        metadata.request_bytes,
        metadata.update_bytes,
        encoded.len(),
    ])?;
    Ok(metadata)
}

pub(super) async fn metadata_on(
    tx: &mut Transaction<'_, Sqlite>,
    source: &str,
    record: &str,
) -> Result<Option<Metadata>, StoreError> {
    let _read = Span::new("record-recovery-metadata-read-await");
    query(
        "SELECT d.generation, d.sequence, d.pending, d.update_count, d.update_bytes, d.request_bytes,
                octet_length(c.payload) AS checkpoint_bytes, d.write_hash
           FROM record_edit_draft d LEFT JOIN record_edit_checkpoint c USING (source, record_uid)
          WHERE d.source = ? AND d.record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .try_map(|row| decode_metadata(&row, None))
    .fetch_optional(&mut **tx)
    .await
}

/// Reads the latest recovery metadata for a source and record.
///
/// # Errors
/// Returns an error for invalid identities, invalid metadata, or a failed read.
pub async fn metadata(
    pool: &SqlitePool,
    source: &str,
    record: &str,
) -> Result<Option<Metadata>, StoreError> {
    scope(source, record)?;
    let mut tx = pool.begin().await?;
    let metadata = metadata_on(&mut tx, source, record).await?;
    tx.commit().await?;
    Ok(metadata)
}

/// Reads the checkpoint, updates, and ordered requests when the saved stamp still matches.
///
/// # Errors
/// Returns an error for a changed stamp, inconsistent journal data, or a failed read.
pub async fn replay(
    pool: &SqlitePool,
    source: &str,
    record: &str,
    expected: &Stamp,
) -> Result<Replay, StoreError> {
    scope(source, record)?;
    let mut tx = pool.begin().await?;
    let metadata = metadata_on(&mut tx, source, record)
        .await?
        .filter(|metadata| &metadata.stamp == expected)
        .ok_or_else(|| invalid("Recovery journal changed while loading"))?;
    let History {
        checkpoint,
        updates,
    } = history_on(&mut tx, source, record, &metadata).await?;
    let mut requests: HashMap<String, String> = query_as(
        "SELECT request_uid, payload FROM record_edit_request WHERE source = ? AND record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    request_sizes(
        &metadata,
        requests.len(),
        requests.values().map(String::len),
    )?;
    let ordered = metadata
        .pending
        .iter()
        .map(|id| {
            requests
                .remove_entry(id)
                .map(|(uid, payload)| PendingRequest { uid, payload })
                .ok_or_else(|| invalid("Recovery request is missing"))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    tx.commit().await?;
    Ok(Replay {
        metadata,
        checkpoint,
        updates,
        requests: ordered,
    })
}

async fn history_on(
    tx: &mut Transaction<'_, Sqlite>,
    source: &str,
    record: &str,
    metadata: &Metadata,
) -> Result<History, StoreError> {
    let checkpoint = query_scalar(
        "SELECT payload FROM record_edit_checkpoint WHERE source = ? AND record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .fetch_one(&mut **tx)
    .await?;
    let updates = query_scalar(
        "SELECT payload FROM record_edit_update WHERE source = ? AND record_uid = ?
          ORDER BY sequence, position",
    )
    .bind(source)
    .bind(record)
    .fetch_all(&mut **tx)
    .await?;
    if updates.len() != metadata.update_count
        || bounded_sum(updates.iter().map(Vec::len))? != metadata.update_bytes
    {
        return Err(invalid("Recovery journal update metadata disagrees"));
    }
    Ok(History {
        checkpoint,
        updates,
    })
}

fn request_sizes(
    metadata: &Metadata,
    count: usize,
    sizes: impl IntoIterator<Item = usize>,
) -> Result<(), StoreError> {
    if count != metadata.pending.len() || bounded_sum(sizes)? != metadata.request_bytes {
        return Err(invalid("Recovery journal request metadata disagrees"));
    }
    Ok(())
}

/// Reads only the history needed for compaction when the expected stamp still matches.
///
/// # Errors
/// Returns an error for invalid identities, inconsistent journal data, or a failed read.
pub async fn history_if_current(
    pool: &SqlitePool,
    source: &str,
    record: &str,
    expected: &Stamp,
) -> Result<Option<History>, StoreError> {
    let _history = Span::new("record-recovery-compaction-read-await");
    scope(source, record)?;
    let mut tx = pool.begin().await?;
    let Some(metadata) = metadata_on(&mut tx, source, record)
        .await?
        .filter(|metadata| &metadata.stamp == expected)
    else {
        tx.commit().await?;
        return Ok(None);
    };
    let history = history_on(&mut tx, source, record, &metadata).await?;
    let requests: HashMap<String, i64> = query_as(
        "SELECT request_uid, octet_length(payload) FROM record_edit_request
          WHERE source = ? AND record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    request_sizes(
        &metadata,
        requests.len(),
        requests
            .values()
            .map(|size| usize::try_from(*size).unwrap_or(usize::MAX)),
    )?;
    if metadata.pending.iter().any(|id| !requests.contains_key(id)) {
        return Err(invalid("Recovery request is missing"));
    }
    tx.commit().await?;
    Ok(Some(history))
}
