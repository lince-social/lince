use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_CHECKPOINT_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_REQUESTS: usize = MAX_BYTES / 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub generation: String,
    pub sequence: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub stamp: Stamp,
    pub pending: Vec<String>,
    pub update_count: usize,
    pub update_bytes: usize,
    pub request_bytes: usize,
    pub checkpoint_bytes: usize,
    pub write_hash: String,
}

#[derive(Clone, Debug)]
pub struct PendingRequest {
    pub uid: String,
    pub payload: String,
}

#[derive(Debug)]
pub struct Replay {
    pub metadata: Metadata,
    pub checkpoint: String,
    pub updates: Vec<Vec<u8>>,
    pub requests: Vec<PendingRequest>,
}

#[derive(Debug)]
pub struct Write {
    pub expected: Option<Stamp>,
    pub generation: String,
    pub checkpoint: Option<String>,
    pub pending: Vec<String>,
    pub requests: Vec<PendingRequest>,
    pub updates: Vec<Vec<u8>>,
}

fn invalid(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.to_owned())
}

fn identity(value: &str, maximum: usize) -> Result<(), StoreError> {
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(invalid("Invalid recovery journal identity"));
    }
    Ok(())
}

fn scope(source: &str, record: &str) -> Result<(), StoreError> {
    identity(source, 8192)?;
    if !nucleus::valid_uid(record, "r") {
        return Err(invalid("Recovery journal requires a Record uid"));
    }
    Ok(())
}

fn pending(ids: &[String]) -> Result<HashSet<&str>, StoreError> {
    if ids.len() > MAX_REQUESTS {
        return Err(invalid("Too many pending recovery requests"));
    }
    let mut unique = HashSet::with_capacity(ids.len());
    for id in ids {
        identity(id, 128)?;
        if !unique.insert(id.as_str()) {
            return Err(invalid("Duplicate recovery request identity"));
        }
    }
    Ok(unique)
}

fn bounded_sum(values: impl IntoIterator<Item = usize>) -> Result<usize, StoreError> {
    values.into_iter().try_fold(0_usize, |sum, value| {
        sum.checked_add(value)
            .filter(|sum| *sum <= MAX_BYTES)
            .ok_or_else(|| invalid("Recovery journal exceeds its storage limit"))
    })
}

async fn metadata_on(
    tx: &mut Transaction<'_, Sqlite>,
    source: &str,
    record: &str,
) -> Result<Option<Metadata>, StoreError> {
    let row = sqlx::query(
        "SELECT d.generation, d.sequence, d.pending, d.update_count, d.update_bytes, d.request_bytes,
                octet_length(c.payload) AS checkpoint_bytes, d.write_hash
           FROM record_edit_draft d LEFT JOIN record_edit_checkpoint c USING (source, record_uid)
          WHERE d.source = ? AND d.record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| {
        let generation: String = row.try_get("generation")?;
        identity(&generation, 128)?;
        let encoded: &str = row.try_get("pending")?;
        bounded_sum([encoded.len()])?;
        let ids: Vec<String> = serde_json::from_str(encoded)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        pending(&ids)?;
        let size = |name| {
            usize::try_from(row.try_get::<i64, _>(name)?)
                .map_err(|_| invalid("Invalid recovery journal size"))
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
            || metadata.update_count > 4096
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
    })
    .transpose()
}

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
    let checkpoint: String = sqlx::query_scalar(
        "SELECT payload FROM record_edit_checkpoint WHERE source = ? AND record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .fetch_one(&mut *tx)
    .await?;
    let updates: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT payload FROM record_edit_update WHERE source = ? AND record_uid = ?
          ORDER BY sequence, position",
    )
    .bind(source)
    .bind(record)
    .fetch_all(&mut *tx)
    .await?;
    if updates.len() != metadata.update_count
        || bounded_sum(updates.iter().map(Vec::len))? != metadata.update_bytes
    {
        return Err(invalid("Recovery journal update metadata disagrees"));
    }
    let mut requests: HashMap<String, String> = sqlx::query_as(
        "SELECT request_uid, payload FROM record_edit_request WHERE source = ? AND record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    if requests.len() != metadata.pending.len()
        || bounded_sum(requests.values().map(String::len))? != metadata.request_bytes
    {
        return Err(invalid("Recovery journal request metadata disagrees"));
    }
    let ordered = metadata
        .pending
        .iter()
        .map(|id| {
            Ok(PendingRequest {
                uid: id.clone(),
                payload: requests
                    .remove(id)
                    .ok_or_else(|| invalid("Recovery request is missing"))?,
            })
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

fn fingerprint(source: &str, record: &str, write: &Write) -> String {
    let mut hash = Sha256::new();
    let mut field = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    field(source.as_bytes());
    field(record.as_bytes());
    if let Some(expected) = &write.expected {
        field(expected.generation.as_bytes());
        field(&expected.sequence.to_le_bytes());
    } else {
        field(&[]);
    }
    field(write.generation.as_bytes());
    field(write.checkpoint.as_deref().unwrap_or_default().as_bytes());
    field(&(write.pending.len() as u64).to_le_bytes());
    for id in &write.pending {
        field(id.as_bytes());
    }
    field(&(write.requests.len() as u64).to_le_bytes());
    for request in &write.requests {
        field(request.uid.as_bytes());
        field(request.payload.as_bytes());
    }
    field(&(write.updates.len() as u64).to_le_bytes());
    for update in &write.updates {
        field(update);
    }
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(64);
    for byte in hash.finalize() {
        write!(encoded, "{byte:02x}").expect("Writing to a String cannot fail");
    }
    encoded
}

pub async fn save(
    pool: &SqlitePool,
    source: &str,
    record: &str,
    write: &Write,
    compacted: Option<&str>,
) -> Result<Metadata, StoreError> {
    scope(source, record)?;
    identity(&write.generation, 128)?;
    let ids = pending(&write.pending)?;
    if write.requests.len() > MAX_REQUESTS || write.updates.len() > 4096 {
        return Err(invalid("Recovery journal batch is too large"));
    }
    for request in &write.requests {
        identity(&request.uid, 128)?;
        if !ids.contains(request.uid.as_str()) {
            return Err(invalid("Recovery payload is not pending"));
        }
    }
    let pending = serde_json::to_string(&write.pending)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    bounded_sum(
        write
            .updates
            .iter()
            .map(Vec::len)
            .chain(write.requests.iter().map(|request| request.payload.len()))
            .chain(write.checkpoint.as_ref().map(String::len))
            .chain([pending.len()]),
    )?;
    for checkpoint in [write.checkpoint.as_deref(), compacted]
        .into_iter()
        .flatten()
    {
        if checkpoint.len() > MAX_CHECKPOINT_BYTES {
            return Err(invalid("Recovery checkpoint exceeds its storage limit"));
        }
    }
    let hash = fingerprint(source, record, write);
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let current = metadata_on(&mut tx, source, record).await?;
    if let Some(current) = &current {
        if current.stamp.generation == write.generation && current.write_hash == hash {
            tx.rollback().await?;
            return Ok(current.clone());
        }
    }
    let rotating = write
        .expected
        .as_ref()
        .is_some_and(|expected| expected.generation != write.generation);
    match (&current, &write.expected) {
        (Some(current), Some(expected))
            if &current.stamp == expected && (!rotating || current.pending.is_empty()) => {}
        (None, None) => {}
        (None, Some(_)) if rotating && write.checkpoint.is_some() => {}
        _ => return Err(invalid("Recovery journal revision conflict")),
    }
    let sequence = if rotating {
        1
    } else {
        current
            .as_ref()
            .map_or(0, |current| current.stamp.sequence)
            .checked_add(1)
            .ok_or_else(|| invalid("Recovery journal sequence is exhausted"))?
    };
    let checkpoint = compacted.or(write.checkpoint.as_deref());
    if current.is_none() && checkpoint.is_none() {
        return Err(invalid("Recovery journal requires an initial checkpoint"));
    }
    if rotating && checkpoint.is_none() {
        return Err(invalid("A new recovery generation requires a checkpoint"));
    }
    if checkpoint.is_some() {
        sqlx::query("DELETE FROM record_edit_update WHERE source = ? AND record_uid = ?")
            .bind(source)
            .bind(record)
            .execute(&mut *tx)
            .await?;
    }
    if current.is_some() {
        sqlx::query(
            "UPDATE record_edit_draft SET generation = ?, sequence = ?,
                    pending = ?, write_hash = ?, updated_at = ?
              WHERE source = ? AND record_uid = ?",
        )
        .bind(&write.generation)
        .bind(sequence)
        .bind(&pending)
        .bind(&hash)
        .bind(nucleus::execution::now().to_rfc3339())
        .bind(source)
        .bind(record)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO record_edit_draft
                 (source, record_uid, generation, sequence, pending, write_hash, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(source)
        .bind(record)
        .bind(&write.generation)
        .bind(sequence)
        .bind(&pending)
        .bind(&hash)
        .bind(nucleus::execution::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
    }
    if let Some(checkpoint) = checkpoint {
        sqlx::query(
            "INSERT INTO record_edit_checkpoint (source, record_uid, payload) VALUES (?, ?, ?)
             ON CONFLICT(source, record_uid) DO UPDATE SET payload = excluded.payload",
        )
        .bind(source)
        .bind(record)
        .bind(checkpoint)
        .execute(&mut *tx)
        .await?;
    }
    for request in &write.requests {
        let inserted = sqlx::query(
            "INSERT INTO record_edit_request (source, record_uid, request_uid, payload)
             VALUES (?, ?, ?, ?) ON CONFLICT(source, record_uid, request_uid) DO NOTHING",
        )
        .bind(source)
        .bind(record)
        .bind(&request.uid)
        .bind(&request.payload)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            let existing: String = sqlx::query_scalar(
                "SELECT payload FROM record_edit_request
                  WHERE source = ? AND record_uid = ? AND request_uid = ?",
            )
            .bind(source)
            .bind(record)
            .bind(&request.uid)
            .fetch_one(&mut *tx)
            .await?;
            if existing != request.payload {
                return Err(invalid("Recovery request identity was reused"));
            }
        }
    }
    sqlx::query(
        "DELETE FROM record_edit_request WHERE source = ? AND record_uid = ?
           AND request_uid NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(source)
    .bind(record)
    .bind(&pending)
    .execute(&mut *tx)
    .await?;
    let (request_count, request_bytes): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(octet_length(payload)), 0)
           FROM record_edit_request WHERE source = ? AND record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .fetch_one(&mut *tx)
    .await?;
    if usize::try_from(request_count).ok() != Some(write.pending.len()) {
        return Err(invalid("Recovery request payload is missing"));
    }
    let mut update_count = if checkpoint.is_some() {
        0
    } else {
        current.as_ref().map_or(0, |current| current.update_count)
    };
    let mut update_bytes = if checkpoint.is_some() {
        0
    } else {
        current.as_ref().map_or(0, |current| current.update_bytes)
    };
    if compacted.is_none() {
        for (position, update) in write.updates.iter().enumerate() {
            if update.is_empty() {
                return Err(invalid("Recovery update must not be empty"));
            }
            update_count += 1;
            update_bytes = bounded_sum([update_bytes, update.len()])?;
            sqlx::query(
                "INSERT INTO record_edit_update (source, record_uid, sequence, position, payload)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(source)
            .bind(record)
            .bind(sequence)
            .bind(position as i64)
            .bind(update)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query(
        "UPDATE record_edit_draft SET update_count = ?, update_bytes = ?, request_bytes = ?
          WHERE source = ? AND record_uid = ?",
    )
    .bind(update_count as i64)
    .bind(update_bytes as i64)
    .bind(request_bytes)
    .bind(source)
    .bind(record)
    .execute(&mut *tx)
    .await?;
    let result = metadata_on(&mut tx, source, record)
        .await?
        .ok_or_else(|| invalid("Recovery journal disappeared"))?;
    tx.commit().await?;
    Ok(result)
}

pub async fn retire(
    pool: &SqlitePool,
    source: &str,
    record: &str,
    expected: &Stamp,
) -> Result<bool, StoreError> {
    scope(source, record)?;
    let removed = sqlx::query(
        "DELETE FROM record_edit_draft WHERE source = ? AND record_uid = ?
           AND generation = ? AND sequence = ? AND pending = '[]'",
    )
    .bind(source)
    .bind(record)
    .bind(&expected.generation)
    .bind(expected.sequence)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(removed == 1)
}
