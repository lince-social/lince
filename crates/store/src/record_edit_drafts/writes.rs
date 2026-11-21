use nucleus::{
    execution::now,
    performance::{Span, add},
};
use serde_json::to_string;
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqlitePool, Transaction, query, query_as, query_scalar};

use super::{
    MAX_CHECKPOINT_BYTES, MAX_REQUESTS, MAX_UPDATES, Metadata, Stamp, Write, bounded_sum, identity,
    invalid, pending,
    reads::{decode_metadata, metadata_on},
    scope,
};
use crate::StoreError;

fn len_bytes(len: usize) -> Result<[u8; 8], StoreError> {
    u64::try_from(len)
        .map(u64::to_le_bytes)
        .map_err(|error| invalid(format!("Recovery journal length is out of range: {error}")))
}

fn hex_digit(nibble: u8) -> char {
    char::from(if nibble < 10 {
        b'0' + nibble
    } else {
        b'a' + nibble - 10
    })
}

fn fingerprint(source: &str, record: &str, write: &Write) -> Result<String, StoreError> {
    let mut hash = Sha256::new();
    let mut field = |bytes: &[u8]| -> Result<(), StoreError> {
        hash.update(len_bytes(bytes.len())?);
        hash.update(bytes);
        Ok(())
    };
    field(source.as_bytes())?;
    field(record.as_bytes())?;
    if let Some(expected) = &write.expected {
        field(expected.generation.as_bytes())?;
        field(&expected.sequence.to_le_bytes())?;
    } else {
        field(&[])?;
    }
    field(write.generation.as_bytes())?;
    field(write.checkpoint.as_deref().unwrap_or_default().as_bytes())?;
    field(&len_bytes(write.pending.len())?)?;
    for id in &write.pending {
        field(id.as_bytes())?;
    }
    field(&len_bytes(write.requests.len())?)?;
    for request in &write.requests {
        field(request.uid.as_bytes())?;
        field(request.payload.as_bytes())?;
    }
    field(&len_bytes(write.updates.len())?)?;
    for update in &write.updates {
        field(update)?;
    }
    let mut encoded = String::with_capacity(64);
    for byte in hash.finalize() {
        encoded.push(hex_digit(byte >> 4));
        encoded.push(hex_digit(byte & 0x0f));
    }
    Ok(encoded)
}

fn sqlite_size(size: usize) -> Result<i64, StoreError> {
    i64::try_from(size)
        .map_err(|error| invalid(format!("Recovery journal size is out of range: {error}")))
}

fn prepare(
    source: &str,
    record: &str,
    write: &Write,
    compacted: Option<&str>,
) -> Result<(String, String), StoreError> {
    scope(source, record)?;
    identity(&write.generation, 128)?;
    let ids = pending(&write.pending)?;
    if write.requests.len() > MAX_REQUESTS || write.updates.len() > MAX_UPDATES {
        return Err(invalid("Recovery journal batch is too large"));
    }
    for request in &write.requests {
        identity(&request.uid, 128)?;
        if !ids.contains(request.uid.as_str()) {
            return Err(invalid("Recovery payload is not pending"));
        }
    }
    drop(ids);
    let pending = to_string(&write.pending).map_err(|error| invalid(error.to_string()))?;
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
    Ok((pending, fingerprint(source, record, write)?))
}

/// Saves a bounded edit batch atomically or returns the previous result for a matching retry.
///
/// # Errors
/// Returns an error for invalid data, a stale stamp, exhausted limits, or a failed write.
pub async fn save(
    pool: &SqlitePool,
    source: &str,
    record: &str,
    write: &Write,
    compacted: Option<&str>,
) -> Result<Metadata, StoreError> {
    let (pending, hash) = {
        let _preparation = Span::new("record-recovery-save-prepare");
        prepare(source, record, write, compacted)?
    };
    let _transaction = Span::new("record-recovery-save-transaction-await");
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let current = match metadata_on(&mut tx, source, record).await? {
        Some(current)
            if current.stamp.generation == write.generation && current.write_hash == hash =>
        {
            tx.rollback().await?;
            return Ok(current);
        }
        current => current,
    };
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
        query("DELETE FROM record_edit_update WHERE source = ? AND record_uid = ?")
            .bind(source)
            .bind(record)
            .execute(&mut *tx)
            .await?;
    }
    let updated_at = now().to_rfc3339();
    if current.is_none() {
        query(
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
        .bind(&updated_at)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(checkpoint) = checkpoint {
        query(
            "INSERT INTO record_edit_checkpoint (source, record_uid, payload) VALUES (?, ?, ?)
             ON CONFLICT(source, record_uid) DO UPDATE SET payload = excluded.payload",
        )
        .bind(source)
        .bind(record)
        .bind(checkpoint)
        .execute(&mut *tx)
        .await?;
    }
    let request_bytes = update_requests(&mut tx, source, record, write, &pending).await?;
    let (mut update_count, mut update_bytes) = current
        .as_ref()
        .filter(|_| checkpoint.is_none())
        .map_or((0, 0), |current| {
            (current.update_count, current.update_bytes)
        });
    if compacted.is_none() {
        for (position, update) in write.updates.iter().enumerate() {
            if update.is_empty() {
                return Err(invalid("Recovery update must not be empty"));
            }
            update_count += 1;
            update_bytes = bounded_sum([update_bytes, update.len()])?;
            query(
                "INSERT INTO record_edit_update (source, record_uid, sequence, position, payload)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(source)
            .bind(record)
            .bind(sequence)
            .bind(sqlite_size(position)?)
            .bind(update)
            .execute(&mut *tx)
            .await?;
        }
    }
    add("record-recovery-metadata-updates", 1);
    let row = query(
        "UPDATE record_edit_draft
            SET generation = ?, sequence = ?, pending = ?, write_hash = ?, updated_at = ?,
                update_count = ?, update_bytes = ?, request_bytes = ?
          WHERE source = ? AND record_uid = ?
          RETURNING generation, sequence, pending, update_count, update_bytes, request_bytes,
                    write_hash,
                    (SELECT octet_length(payload) FROM record_edit_checkpoint
                      WHERE source = ? AND record_uid = ?) AS checkpoint_bytes",
    )
    .bind(&write.generation)
    .bind(sequence)
    .bind(&pending)
    .bind(&hash)
    .bind(&updated_at)
    .bind(sqlite_size(update_count)?)
    .bind(sqlite_size(update_bytes)?)
    .bind(request_bytes)
    .bind(source)
    .bind(record)
    .bind(source)
    .bind(record)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| invalid("Recovery journal disappeared"))?;
    let result = decode_metadata(&row, Some((&pending, &write.pending)))?;
    tx.commit().await?;
    Ok(result)
}

async fn update_requests(
    tx: &mut Transaction<'_, Sqlite>,
    source: &str,
    record: &str,
    write: &Write,
    pending: &str,
) -> Result<i64, StoreError> {
    for request in &write.requests {
        let inserted = query(
            "INSERT INTO record_edit_request (source, record_uid, request_uid, payload)
             VALUES (?, ?, ?, ?) ON CONFLICT(source, record_uid, request_uid) DO NOTHING",
        )
        .bind(source)
        .bind(record)
        .bind(&request.uid)
        .bind(&request.payload)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            add("record-recovery-payload-compares", 1);
            let same: bool = query_scalar(
                "SELECT payload = ? COLLATE BINARY FROM record_edit_request
                  WHERE source = ? AND record_uid = ? AND request_uid = ?",
            )
            .bind(&request.payload)
            .bind(source)
            .bind(record)
            .bind(&request.uid)
            .fetch_one(&mut **tx)
            .await?;
            if !same {
                return Err(invalid("Recovery request identity was reused"));
            }
        }
    }
    query(
        "DELETE FROM record_edit_request WHERE source = ? AND record_uid = ?
           AND request_uid NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(source)
    .bind(record)
    .bind(pending)
    .execute(&mut **tx)
    .await?;
    let (request_count, request_bytes): (i64, i64) = query_as(
        "SELECT COUNT(*), COALESCE(SUM(octet_length(payload)), 0)
           FROM record_edit_request WHERE source = ? AND record_uid = ?",
    )
    .bind(source)
    .bind(record)
    .fetch_one(&mut **tx)
    .await?;
    if usize::try_from(request_count).ok() != Some(write.pending.len()) {
        return Err(invalid("Recovery request payload is missing"));
    }
    Ok(request_bytes)
}

/// Removes a matching journal once it has no pending requests.
///
/// # Errors
/// Returns an error for invalid identities or a failed delete.
pub async fn retire(
    pool: &SqlitePool,
    source: &str,
    record: &str,
    expected: &Stamp,
) -> Result<bool, StoreError> {
    scope(source, record)?;
    let removed = query(
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

#[cfg(test)]
mod tests {
    use super::{Stamp, Write, fingerprint};
    use crate::record_edit_drafts::PendingRequest;

    #[test]
    fn fingerprints_preserve_previous_stamps_and_binary_input() {
        let mut write = Write {
            expected: Some(Stamp {
                generation: "previous".into(),
                sequence: 7,
            }),
            generation: "current".into(),
            checkpoint: Some("café".into()),
            pending: vec!["α".into(), "b".into()],
            requests: vec![PendingRequest {
                uid: "α".into(),
                payload: "✓".into(),
            }],
            updates: vec![vec![0, 255], vec![1]],
        };
        assert_eq!(
            fingerprint("session-á", "r_fixture", &write).unwrap(),
            "af251d64efb2338d047b295ac71b91f046961c073e213d38aa6ce565e36e41b3",
        );
        write.expected = None;
        assert_eq!(
            fingerprint("session-á", "r_fixture", &write).unwrap(),
            "d3541ab8cbc11d5a622a28ba49e260904bddf5609b117f31d43f7fd9d7ef25c9",
        );
    }
}
