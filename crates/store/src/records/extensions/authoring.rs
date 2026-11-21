use std::io::{self, Write};

use nucleus::{transfer::public_history, visibility::plan};
use serde_json::{Map, Value, json, to_value};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{
    MAX_EXTENSION_BYTES, MAX_EXTENSION_KEYS, SQLITE_BYTE_LIMIT, policy, protocol,
    require_active_on, validate_extension_name,
};
use crate::{
    StoreError,
    sync_ops::{OpKind, log_local_tx},
    write_tx,
};

#[derive(Default)]
struct BoundedWriter(Vec<u8>);

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = self
            .0
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("Record extension size overflow"))?;
        if length > MAX_EXTENSION_BYTES {
            return Err(io::Error::other("Record extension exceeds its byte limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Writes extension JSON without authoring validation or sync logging.
pub async fn set_extension_raw(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    fds: &Value,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(record_uid)
    .bind(namespace)
    .bind(fds.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Authors a validated extension and commits its field-level sync changes.
pub async fn set_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    fds: &Value,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    set_extension_on(&mut tx, record_uid, namespace, fds).await?;
    tx.commit().await
}

struct StoredExtension {
    fields: Map<String, Value>,
    version: i64,
}

fn extension_json(value: &Value) -> Result<(String, &Map<String, Value>), StoreError> {
    let fields = value
        .as_object()
        .ok_or_else(|| protocol("Record extension authoring requires a JSON object"))?;
    if fields.len() > MAX_EXTENSION_KEYS {
        return Err(protocol("Record extension exceeds its key count limit"));
    }
    for key in fields.keys() {
        validate_extension_name(key, "key", false)?;
    }
    let mut writer = BoundedWriter::default();
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| protocol(format!("Record extension is not serialisable: {error}")))?;
    let encoded = String::from_utf8(writer.0)
        .map_err(|error| protocol(format!("Record extension is not UTF-8: {error}")))?;
    Ok((encoded, fields))
}

async fn stored_extension_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    namespace: &str,
) -> Result<Option<StoredExtension>, StoreError> {
    let row = sqlx::query_as::<_, (String, i64, String, Option<String>, Option<i64>)>(
        "SELECT typeof(fds), length(CAST(fds AS BLOB)), typeof(version),
                CASE
                    WHEN typeof(fds) = 'text' AND length(CAST(fds AS BLOB)) <= ?
                    THEN fds
                END,
                CASE WHEN typeof(version) = 'integer' THEN version END
           FROM record_extension
          WHERE record_uid = ? AND namespace = ?",
    )
    .bind(SQLITE_BYTE_LIMIT)
    .bind(record_uid)
    .bind(namespace)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((fds_type, bytes, version_type, fds, version)) = row else {
        return Ok(None);
    };
    if fds_type != "text" {
        return Err(protocol("Record extension has an invalid storage type"));
    }
    let bytes = usize::try_from(bytes)
        .map_err(|_| protocol("Record extension has an invalid byte length"))?;
    if bytes > MAX_EXTENSION_BYTES {
        return Err(protocol("Record extension exceeds its byte limit"));
    }
    if version_type != "integer" {
        return Err(protocol(
            "Record extension version has an invalid storage type",
        ));
    }
    let version = version.ok_or_else(|| protocol("Record extension version is unreadable"))?;
    if version <= 0 {
        return Err(protocol("Record extension version must be positive"));
    }
    let fds = fds.ok_or_else(|| protocol("Record extension is unreadable"))?;
    let value: Value = serde_json::from_str(&fds)
        .map_err(|error| protocol(format!("Record extension is not valid JSON: {error}")))?;
    let Value::Object(fields) = value else {
        return Err(protocol("Stored Record extension is not a JSON object"));
    };
    if fields.len() > MAX_EXTENSION_KEYS {
        return Err(protocol(
            "Stored Record extension exceeds its key count limit",
        ));
    }
    for key in fields.keys() {
        validate_extension_name(key, "key", false)?;
    }
    Ok(Some(StoredExtension { fields, version }))
}

async fn log_extension_diff_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    namespace: &str,
    old: &Map<String, Value>,
    new: &Map<String, Value>,
) -> Result<(), StoreError> {
    for (key, value) in new {
        if old.get(key) != Some(value) {
            log_local_tx(
                tx,
                "record_extension",
                record_uid,
                &format!("{namespace}.{key}"),
                OpKind::Set,
                Some(value.to_string()),
            )
            .await?;
        }
    }
    for key in old.keys() {
        if !new.contains_key(key) {
            log_local_tx(
                tx,
                "record_extension",
                record_uid,
                &format!("{namespace}.{key}"),
                OpKind::Tombstone,
                None,
            )
            .await?;
        }
    }
    Ok(())
}

/// Authors an extension in the caller's transaction and returns its stored
/// version if fields remain.
///
/// # Errors
/// Rejects invalid fields, inactive Records, changes that require a reviewed
/// action, invalid stored values, and failed database or sync writes.
pub async fn set_extension_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    namespace: &str,
    fds: &Value,
) -> Result<Option<i64>, StoreError> {
    policy::check_set(tx, record_uid, namespace, fds).await?;
    write_extension_on(tx, record_uid, namespace, fds).await
}

/// Writes a reviewed audience plan in the caller's transaction.
pub async fn set_audience_plan_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    value: &plan::Saved,
) -> Result<Option<i64>, StoreError> {
    require_active_on(tx, record).await?;
    write_extension_on(
        tx,
        record,
        plan::PRIVATE_NAMESPACE,
        &to_value(value).map_err(|error| protocol(error.to_string()))?,
    )
    .await
}

pub(crate) async fn set_transfer_public_history_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    terms: Option<&public_history::Terms>,
) -> Result<Option<i64>, StoreError> {
    require_active_on(tx, record).await?;
    if let Some(terms) = terms {
        terms.validate().map_err(protocol)?;
    }
    let fields = terms.map_or_else(|| json!({}), |terms| json!({"terms":terms}));
    write_extension_on(tx, record, public_history::NAMESPACE, &fields).await
}

async fn write_extension_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    namespace: &str,
    fds: &Value,
) -> Result<Option<i64>, StoreError> {
    let (encoded, new_fields) = extension_json(fds)?;
    let old = stored_extension_on(tx, record_uid, namespace).await?;
    let Some(old) = old else {
        if new_fields.is_empty() {
            return Ok(None);
        }
        sqlx::query(
            "INSERT INTO record_extension (record_uid, namespace, version, fds)
             VALUES (?, ?, 1, ?)",
        )
        .bind(record_uid)
        .bind(namespace)
        .bind(encoded)
        .execute(&mut **tx)
        .await?;
        log_extension_diff_on(tx, record_uid, namespace, &Map::new(), new_fields).await?;
        return Ok(Some(1));
    };
    if &old.fields == new_fields {
        return Ok(Some(old.version));
    }
    if new_fields.is_empty() {
        sqlx::query("DELETE FROM record_extension WHERE record_uid = ? AND namespace = ?")
            .bind(record_uid)
            .bind(namespace)
            .execute(&mut **tx)
            .await?;
        log_extension_diff_on(tx, record_uid, namespace, &old.fields, new_fields).await?;
        return Ok(None);
    }
    let next_version = old
        .version
        .checked_add(1)
        .ok_or_else(|| protocol("Record extension version overflow"))?;
    let result = sqlx::query(
        "UPDATE record_extension SET fds = ?, version = ?
          WHERE record_uid = ? AND namespace = ? AND version = ?",
    )
    .bind(encoded)
    .bind(next_version)
    .bind(record_uid)
    .bind(namespace)
    .bind(old.version)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol(
            "Record extension version changed during its write",
        ));
    }
    log_extension_diff_on(tx, record_uid, namespace, &old.fields, new_fields).await?;
    Ok(Some(next_version))
}

/// Deletes a removable extension and commits its field tombstones.
pub async fn delete_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    delete_extension_on(&mut tx, record_uid, namespace).await?;
    tx.commit().await
}

/// Removes an eligible extension and logs field tombstones. Returns false if absent.
///
/// # Errors
/// Rejects protected extensions, inactive Records, invalid stored values,
/// and failed database or sync writes.
pub async fn delete_extension_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    namespace: &str,
) -> Result<bool, StoreError> {
    policy::check_delete(tx, record_uid, namespace).await?;
    let Some(old) = stored_extension_on(tx, record_uid, namespace).await? else {
        return Ok(false);
    };
    let result = sqlx::query("DELETE FROM record_extension WHERE record_uid = ? AND namespace = ?")
        .bind(record_uid)
        .bind(namespace)
        .execute(&mut **tx)
        .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("Record extension removal lost its target"));
    }
    log_extension_diff_on(tx, record_uid, namespace, &old.fields, &Map::new()).await?;
    Ok(true)
}
