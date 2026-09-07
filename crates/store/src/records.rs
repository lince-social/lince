use std::io::{self, Write};

use chrono::Utc;
use nucleus::{DecimalValue, RecordKind};
use serde_json::{Map, Value};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;
use crate::exact::{decimal_columns, read_decimal};
use crate::sync_ops::OpKind;

pub const MAX_EXTENSION_BYTES: usize = 1024 * 1024;
pub const MAX_EXTENSION_KEYS: usize = 4096;
pub const MAX_EXTENSION_NAME_BYTES: usize = 200;

struct BoundedWriter {
    bytes: Vec<u8>,
}

impl BoundedWriter {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("Record extension size overflow"))?;
        if length > MAX_EXTENSION_BYTES {
            return Err(io::Error::other("Record extension exceeds its byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

async fn log_set_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    field: &str,
    value: serde_json::Value,
) -> Result<(), StoreError> {
    crate::sync_ops::log_local_tx(
        tx,
        "record",
        uid,
        field,
        OpKind::Set,
        Some(value.to_string()),
    )
    .await
}

fn validate_record_uid(uid: &str) -> Result<(), StoreError> {
    if !nucleus::valid_uid(uid, "r") {
        return Err(protocol("Record authoring requires a canonical Record uid"));
    }
    Ok(())
}

async fn require_active_on(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<(), StoreError> {
    validate_record_uid(uid)?;
    let active =
        sqlx::query_scalar::<_, i64>("SELECT deleted_at IS NULL FROM record WHERE uid = ?")
            .bind(uid)
            .fetch_optional(&mut **tx)
            .await?;
    match active {
        Some(1) => Ok(()),
        Some(_) => Err(protocol("Record authoring target is deleted")),
        None => Err(protocol("Record authoring target is missing")),
    }
}

#[derive(Debug, Clone)]
pub struct RecordRow {
    pub uid: String,
    pub slug: Option<String>,
    pub kind: String,
    pub head: String,
    pub body: String,
    pub quantity: DecimalValue,
    pub identity_predicate_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub place_uid: Option<String>,
    pub organ_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub created_hlc: Option<i64>,
}

impl RecordRow {
    pub fn quantity_f64(&self) -> f64 {
        self.quantity.to_f64()
    }

    pub fn is_active(&self) -> bool {
        !self.quantity.is_zero()
    }
}

fn map_row(r: sqlx::sqlite::SqliteRow) -> Result<RecordRow, StoreError> {
    Ok(RecordRow {
        uid: r.get("uid"),
        slug: r.get("slug"),
        kind: r.get("kind"),
        head: r.get("head"),
        body: r.get("body"),
        quantity: read_decimal(&r, "quantity")?,
        identity_predicate_uid: r.get("identity_predicate_uid"),
        unit_uid: r.get("unit_uid"),
        place_uid: r.get("place_uid"),
        organ_uid: r.get("organ_uid"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        created_hlc: r.try_get("created_hlc").ok().flatten(),
    })
}

fn map_rows(rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Vec<RecordRow>, StoreError> {
    rows.into_iter().map(map_row).collect()
}

pub struct NewRecord<'a> {
    pub slug: Option<&'a str>,
    pub kind: RecordKind,
    pub head: &'a str,
    pub body: &'a str,
    pub quantity: DecimalValue,
}

pub async fn create(pool: &SqlitePool, new: NewRecord<'_>) -> Result<RecordRow, StoreError> {
    create_in_root(pool, new, None).await
}

pub async fn create_with_uid(
    pool: &SqlitePool,
    new: NewRecord<'_>,
    uid: &str,
) -> Result<RecordRow, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let organ_uid = local_organ_uid_on(&mut tx).await?;
    let row = create_with_uid_on(&mut tx, uid, new, &organ_uid, None).await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn create_in_root(
    pool: &SqlitePool,
    new: NewRecord<'_>,
    root: Option<&str>,
) -> Result<RecordRow, StoreError> {
    let uid = nucleus::new_uid("r");
    let mut tx = crate::write_tx(pool).await?;
    let organ_uid = local_organ_uid_on(&mut tx).await?;
    let row = create_with_uid_on(&mut tx, &uid, new, &organ_uid, root).await?;
    tx.commit().await?;
    Ok(row)
}

async fn local_organ_uid_on(tx: &mut Transaction<'_, Sqlite>) -> Result<String, StoreError> {
    sqlx::query_scalar::<_, String>(
        "SELECT uid FROM record
          WHERE slug = ? AND kind = 'organ' AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| protocol("cannot create a Record before this Cell has an Organ"))
}

async fn validate_creation_origin_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    organ_uid: &str,
    replica_root: Option<&str>,
) -> Result<(), StoreError> {
    validate_record_uid(uid)?;
    validate_record_uid(organ_uid)?;
    let organ = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM record
          WHERE uid = ? AND kind = 'organ' AND deleted_at IS NULL",
    )
    .bind(organ_uid)
    .fetch_optional(&mut **tx)
    .await?
    .is_some();
    if !organ {
        return Err(protocol(
            "Record creation requires an existing undeleted Organ",
        ));
    }
    if let Some(root) = replica_root {
        validate_record_uid(root)?;
        if root != uid {
            let valid = sqlx::query_scalar::<_, i64>(
                "SELECT 1 FROM record
                  WHERE uid = ? AND deleted_at IS NULL
                    AND (replica_root IS NULL OR replica_root = uid)",
            )
            .bind(root)
            .fetch_optional(&mut **tx)
            .await?
            .is_some();
            if !valid {
                return Err(protocol(
                    "Record creation replica root is missing, deleted or nested",
                ));
            }
        }
    }
    let available = sqlx::query_scalar::<_, i64>("SELECT 1 FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .is_none();
    if !available {
        return Err(protocol("Record uid is already in use"));
    }
    Ok(())
}

async fn inserted_on(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<RecordRow, StoreError> {
    sqlx::query(
        "SELECT r.*,
                (SELECT predicate_uid FROM record_assertion a
                  WHERE a.subject_uid = r.uid AND a.role = 'identity'
                    AND a.retracted_at IS NULL) AS identity_predicate_uid
           FROM record r WHERE r.uid = ?",
    )
    .bind(uid)
    .fetch_optional(&mut **tx)
    .await?
    .map(map_row)
    .transpose()?
    .ok_or_else(|| protocol("new Record disappeared inside its transaction"))
}

pub async fn create_with_uid_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    new: NewRecord<'_>,
    organ_uid: &str,
    replica_root: Option<&str>,
) -> Result<RecordRow, StoreError> {
    if let Some(slug) = new.slug {
        if !nucleus::valid_slug(slug) {
            return Err(protocol(format!("invalid slug `{slug}`")));
        }
    }
    validate_creation_origin_on(tx, uid, organ_uid, replica_root).await?;
    let now = Utc::now().to_rfc3339();
    let created_hlc = nucleus::hlc::next();
    let (mantissa, scale) = decimal_columns(new.quantity);
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at, replica_root, created_hlc)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(new.slug)
    .bind(new.kind.as_str())
    .bind(new.head)
    .bind(new.body)
    .bind(&mantissa)
    .bind(scale)
    .bind(organ_uid)
    .bind(&now)
    .bind(&now)
    .bind(replica_root)
    .bind(&created_hlc)
    .execute(&mut **tx)
    .await?;
    log_set_on(tx, uid, "kind", serde_json::json!(new.kind.as_str())).await?;
    log_set_on(tx, uid, "head", serde_json::json!(new.head)).await?;
    log_set_on(tx, uid, "body", serde_json::json!(new.body)).await?;
    log_set_on(
        tx,
        uid,
        "quantity",
        serde_json::json!({ "mantissa": mantissa, "scale": scale }),
    )
    .await?;
    if let Some(slug) = new.slug {
        log_set_on(tx, uid, "slug", serde_json::json!(slug)).await?;
    }
    log_set_on(tx, uid, "organ_uid", serde_json::json!(organ_uid)).await?;
    inserted_on(tx, uid).await
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<RecordRow>, StoreError> {
    sqlx::query(
        "SELECT r.*,
                (SELECT predicate_uid FROM record_assertion a
                  WHERE a.subject_uid = r.uid AND a.role = 'identity'
                    AND a.retracted_at IS NULL) AS identity_predicate_uid
           FROM record r WHERE r.uid = ? AND r.deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(map_row)
    .transpose()
}

pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<RecordRow>, StoreError> {
    sqlx::query(
        "SELECT r.*,
                (SELECT predicate_uid FROM record_assertion a
                  WHERE a.subject_uid = r.uid AND a.role = 'identity'
                    AND a.retracted_at IS NULL) AS identity_predicate_uid
           FROM record r
          WHERE (r.slug = ? OR r.uid = ?) AND r.deleted_at IS NULL LIMIT 1",
    )
    .bind(token)
    .bind(token)
    .fetch_optional(pool)
    .await?
    .map(map_row)
    .transpose()
}

pub async fn quantity(pool: &SqlitePool, uid: &str) -> Result<Option<DecimalValue>, StoreError> {
    sqlx::query(
        "SELECT quantity_mantissa, quantity_scale FROM record
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(|r| read_decimal(&r, "quantity"))
    .transpose()
}

pub async fn quantity_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<DecimalValue>, StoreError> {
    sqlx::query(
        "SELECT quantity_mantissa, quantity_scale FROM record
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_optional(&mut **tx)
    .await?
    .map(|row| read_decimal(&row, "quantity"))
    .transpose()
}

pub async fn created_at(pool: &SqlitePool, uid: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT created_at FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get::<String, _>("created_at")),
    )
}

pub async fn mark_deleted(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let active =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
    if !active {
        tx.commit().await?;
        return Ok(false);
    }
    mark_deleted_on(&mut tx, uid).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn mark_deleted_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record SET deleted_at = ?, slug = NULL, updated_at = ?
         WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() != 1 {
        return Err(protocol("Record deletion lost its active target"));
    }
    crate::sync_ops::log_local_tx(tx, "record", uid, "", OpKind::Tombstone, None).await
}

pub async fn restore(pool: &SqlitePool, uid: &str, slug: Option<&str>) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    restore_on(&mut tx, uid, slug).await?;
    tx.commit().await
}

pub async fn restore_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    slug: Option<&str>,
) -> Result<(), StoreError> {
    validate_record_uid(uid)?;
    if let Some(slug) = slug {
        if !nucleus::valid_slug(slug) {
            return Err(protocol(format!("invalid slug `{slug}`")));
        }
    }
    let deleted =
        sqlx::query_scalar::<_, i64>("SELECT deleted_at IS NOT NULL FROM record WHERE uid = ?")
            .bind(uid)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| protocol("Record restoration target is missing"))?;
    if deleted == 0 {
        return Err(protocol("Record restoration target is not deleted"));
    }
    let result = sqlx::query(
        "UPDATE record
            SET deleted_at = NULL, slug = ?, updated_at = ?
          WHERE uid = ? AND deleted_at IS NOT NULL",
    )
    .bind(slug)
    .bind(Utc::now().to_rfc3339())
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("Record restoration lost its deleted target"));
    }
    log_set_on(tx, uid, "slug", serde_json::json!(slug)).await
}

pub async fn list_all(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    map_rows(
        sqlx::query(
            "SELECT r.*,
                    (SELECT predicate_uid FROM record_assertion a
                      WHERE a.subject_uid = r.uid AND a.role = 'identity'
                        AND a.retracted_at IS NULL) AS identity_predicate_uid
               FROM record r WHERE r.deleted_at IS NULL ORDER BY r.created_at, r.uid",
        )
        .fetch_all(pool)
        .await?,
    )
}

pub async fn active_needs(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    map_rows(
        sqlx::query(
            "SELECT r.*,
                    (SELECT predicate_uid FROM record_assertion a
                      WHERE a.subject_uid = r.uid AND a.role = 'identity'
                        AND a.retracted_at IS NULL) AS identity_predicate_uid
               FROM record r
              WHERE r.quantity_mantissa LIKE '-%' AND r.kind = 'plain'
                AND r.deleted_at IS NULL
              ORDER BY r.created_at, r.uid",
        )
        .fetch_all(pool)
        .await?,
    )
}

pub async fn all_levels(pool: &SqlitePool) -> Result<Vec<(String, DecimalValue)>, StoreError> {
    sqlx::query(
        "SELECT uid, quantity_mantissa, quantity_scale FROM record WHERE deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| Ok((r.get("uid"), read_decimal(&r, "quantity")?)))
    .collect()
}

pub async fn set_extension_raw(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    fds: &serde_json::Value,
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

pub async fn set_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    fds: &serde_json::Value,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    set_extension_on(&mut tx, record_uid, namespace, fds).await?;
    tx.commit().await?;
    Ok(())
}

struct StoredExtension {
    fields: Map<String, Value>,
    version: i64,
}

fn validate_extension_name(value: &str, label: &str, allow_dot: bool) -> Result<(), StoreError> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > MAX_EXTENSION_NAME_BYTES
        || value.chars().any(char::is_control)
        || (!allow_dot && value.contains('.'))
    {
        return Err(protocol(format!("Record extension {label} is invalid")));
    }
    Ok(())
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
    let mut writer = BoundedWriter::new();
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| protocol(format!("Record extension is not serialisable: {error}")))?;
    let encoded = String::from_utf8(writer.bytes)
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
    .bind(i64::try_from(MAX_EXTENSION_BYTES).expect("extension limit fits SQLite"))
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
    let fields = value
        .as_object()
        .ok_or_else(|| protocol("Stored Record extension is not a JSON object"))?;
    if fields.len() > MAX_EXTENSION_KEYS {
        return Err(protocol(
            "Stored Record extension exceeds its key count limit",
        ));
    }
    for key in fields.keys() {
        validate_extension_name(key, "key", false)?;
    }
    Ok(Some(StoredExtension {
        fields: fields.clone(),
        version,
    }))
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
            crate::sync_ops::log_local_tx(
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
            crate::sync_ops::log_local_tx(
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

pub async fn set_extension_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    namespace: &str,
    fds: &Value,
) -> Result<Option<i64>, StoreError> {
    require_active_on(tx, record_uid).await?;
    validate_extension_name(namespace, "namespace", true)?;
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
    if old.version == i64::MAX {
        return Err(protocol("Record extension version overflow"));
    }
    let next_version = old.version + 1;
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

pub async fn delete_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    delete_extension_on(&mut tx, record_uid, namespace).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_extension_on(
    tx: &mut Transaction<'_, Sqlite>,
    record_uid: &str,
    namespace: &str,
) -> Result<bool, StoreError> {
    require_active_on(tx, record_uid).await?;
    validate_extension_name(namespace, "namespace", true)?;
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

pub async fn get_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<Option<serde_json::Value>, StoreError> {
    Ok(
        sqlx::query("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?")
            .bind(record_uid)
            .bind(namespace)
            .fetch_optional(pool)
            .await?
            .and_then(|r| serde_json::from_str(&r.get::<String, _>("fds")).ok()),
    )
}

pub async fn all_extensions(
    pool: &SqlitePool,
    namespace: &str,
) -> Result<std::collections::HashMap<String, serde_json::Value>, StoreError> {
    let rows = sqlx::query(
        "SELECT e.record_uid, e.fds
           FROM record_extension e
           JOIN record r ON r.uid = e.record_uid
          WHERE e.namespace = ? AND r.deleted_at IS NULL",
    )
    .bind(namespace)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let uid = row.get::<String, _>("record_uid");
            serde_json::from_str(&row.get::<String, _>("fds"))
                .ok()
                .map(|value| (uid, value))
        })
        .collect())
}

pub async fn set_text(
    pool: &SqlitePool,
    uid: &str,
    head: Option<&str>,
    body: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record
           SET head = COALESCE(?, head),
               body = COALESCE(?, body),
               updated_at = ?
         WHERE uid = ?",
    )
    .bind(head)
    .bind(body)
    .bind(&now)
    .bind(uid)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

pub async fn set_authoring_text(
    pool: &SqlitePool,
    uid: &str,
    head: Option<&str>,
    body: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    set_authoring_text_on(&mut tx, uid, head, body).await?;
    tx.commit().await
}

pub async fn set_authoring_text_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    head: Option<&str>,
    body: Option<&str>,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    if head.is_none() && body.is_none() {
        return Ok(());
    }
    let result = sqlx::query(
        "UPDATE record
            SET head = COALESCE(?, head), body = COALESCE(?, body), updated_at = ?
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(head)
    .bind(body)
    .bind(Utc::now().to_rfc3339())
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("Record text authoring lost its active target"));
    }
    if let Some(head) = head {
        log_set_on(tx, uid, "head", serde_json::json!(head)).await?;
    }
    if let Some(body) = body {
        log_set_on(tx, uid, "body", serde_json::json!(body)).await?;
    }
    Ok(())
}

pub async fn set_slug(pool: &SqlitePool, uid: &str, slug: Option<&str>) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    set_slug_on(&mut tx, uid, slug).await?;
    tx.commit().await
}

pub async fn set_slug_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    slug: Option<&str>,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    if let Some(slug) = slug {
        if !nucleus::valid_slug(slug) {
            return Err(protocol(format!("invalid slug `{slug}`")));
        }
    }
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record SET slug = ?, updated_at = ?
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(slug)
    .bind(&now)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() != 1 {
        return Err(protocol("Record slug authoring lost its active target"));
    }
    log_set_on(tx, uid, "slug", serde_json::json!(slug)).await
}

pub async fn set_organ_origin(
    pool: &SqlitePool,
    uid: &str,
    organ_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET organ_uid = ?, updated_at = ? WHERE uid = ?")
        .bind(organ_uid)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

pub async fn set_unit(
    pool: &SqlitePool,
    uid: &str,
    unit_uid: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    set_unit_on(&mut tx, uid, unit_uid).await?;
    tx.commit().await
}

async fn validate_reference_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    prefix: &str,
    table: &str,
    label: &str,
) -> Result<(), StoreError> {
    if !nucleus::valid_uid(uid, prefix) {
        return Err(protocol(format!(
            "Record {label} requires a canonical {label} uid"
        )));
    }
    let sql = format!("SELECT 1 FROM {table} WHERE uid = ?");
    let exists = sqlx::query_scalar::<_, i64>(&sql)
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .is_some();
    if !exists {
        return Err(protocol(format!("Record {label} is missing")));
    }
    Ok(())
}

pub async fn set_unit_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    unit_uid: Option<&str>,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    if let Some(unit_uid) = unit_uid {
        validate_reference_on(tx, unit_uid, "c", "concept", "unit").await?;
    }
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record SET unit_uid = ?, updated_at = ?
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(unit_uid)
    .bind(&now)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() != 1 {
        return Err(protocol("Record unit authoring lost its active target"));
    }
    log_set_on(tx, uid, "unit_uid", serde_json::json!(unit_uid)).await
}

pub async fn set_place(
    pool: &SqlitePool,
    uid: &str,
    place_uid: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    set_place_on(&mut tx, uid, place_uid).await?;
    tx.commit().await
}

pub async fn set_place_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    place_uid: Option<&str>,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    if let Some(place_uid) = place_uid {
        validate_reference_on(tx, place_uid, "pl", "place", "place").await?;
    }
    let result = sqlx::query(
        "UPDATE record SET place_uid = ?, updated_at = ?
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(place_uid)
    .bind(Utc::now().to_rfc3339())
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("Record place authoring lost its active target"));
    }
    log_set_on(tx, uid, "place_uid", serde_json::json!(place_uid)).await
}

pub async fn bump_quantity(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    delta: DecimalValue,
    now_rfc3339: &str,
) -> Result<(), StoreError> {
    let row = sqlx::query("SELECT quantity_mantissa, quantity_scale FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let updated = read_decimal(&row, "quantity")?
        .aligned_add(delta)
        .ok_or_else(|| {
            StoreError::Decode(format!("quantity of {uid} overflows i128 exact range").into())
        })?;
    let (mantissa, scale) = decimal_columns(updated);
    let res = sqlx::query(
        "UPDATE record SET quantity_mantissa = ?, quantity_scale = ?, updated_at = ?
          WHERE uid = ?",
    )
    .bind(mantissa)
    .bind(scale)
    .bind(now_rfc3339)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

pub async fn heads_for(
    pool: &SqlitePool,
    uids: &[String],
) -> Result<std::collections::HashMap<String, String>, StoreError> {
    if uids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let holes = std::iter::repeat_n("?", uids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("SELECT uid, head FROM record WHERE uid IN ({holes})");
    let mut query = sqlx::query(&sql);
    for uid in uids {
        query = query.bind(uid);
    }
    Ok(query
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.get("uid"), row.get("head")))
        .collect())
}
