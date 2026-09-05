use chrono::Utc;
use nucleus::{DecimalValue, RecordKind};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;
use crate::exact::{decimal_columns, read_decimal};
use crate::sync_ops::OpKind;

async fn log_set(
    pool: &SqlitePool,
    uid: &str,
    field: &str,
    value: serde_json::Value,
) -> Result<(), StoreError> {
    crate::sync_ops::log_local(
        pool,
        "record",
        uid,
        field,
        OpKind::Set,
        Some(value.to_string()),
    )
    .await
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
    if !nucleus::valid_uid(uid, "r") {
        return Err(sqlx::Error::Protocol(format!(
            "`{uid}` is not a record uid (expected `r_` and 26 characters)"
        )));
    }
    if get(pool, uid).await?.is_some() {
        return Err(sqlx::Error::Protocol(format!(
            "`{uid}` already names a Record here"
        )));
    }
    create_inner(pool, new, None, Some(uid)).await
}

pub async fn create_in_root(
    pool: &SqlitePool,
    new: NewRecord<'_>,
    root: Option<&str>,
) -> Result<RecordRow, StoreError> {
    create_inner(pool, new, root, None).await
}

async fn create_inner(
    pool: &SqlitePool,
    new: NewRecord<'_>,
    root: Option<&str>,
    given_uid: Option<&str>,
) -> Result<RecordRow, StoreError> {
    if let Some(slug) = new.slug {
        if !nucleus::valid_slug(slug) {
            return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
        }
    }
    let uid = match given_uid {
        Some(uid) => uid.to_string(),
        None => nucleus::new_uid("r"),
    };
    let now = Utc::now().to_rfc3339();
    let created_hlc = nucleus::hlc::next();
    let (mantissa, scale) = decimal_columns(new.quantity);
    let origin = crate::organs::local(pool)
        .await?
        .map(|organ| organ.uid)
        .ok_or_else(|| {
            sqlx::Error::Protocol("cannot create a Record before this Cell has an Organ".into())
        })?;
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
    .bind(&origin)
    .bind(&now)
    .bind(&now)
    .bind(root)
    .bind(&created_hlc)
    .execute(pool)
    .await?;
    log_set(pool, &uid, "kind", serde_json::json!(new.kind.as_str())).await?;
    log_set(pool, &uid, "head", serde_json::json!(new.head)).await?;
    log_set(pool, &uid, "body", serde_json::json!(new.body)).await?;
    log_set(
        pool,
        &uid,
        "quantity",
        serde_json::json!({ "mantissa": mantissa, "scale": scale }),
    )
    .await?;
    if let Some(slug) = new.slug {
        log_set(pool, &uid, "slug", serde_json::json!(slug)).await?;
    }
    log_set(pool, &uid, "organ_uid", serde_json::json!(&origin)).await?;
    get(pool, &uid).await.map(|r| r.expect("just inserted"))
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
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record SET deleted_at = ?, slug = NULL, updated_at = ?
         WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(uid)
    .execute(pool)
    .await?;
    let deleted = res.rows_affected() > 0;
    if deleted {
        crate::sync_ops::log_local(pool, "record", uid, "", OpKind::Tombstone, None).await?;
    }
    Ok(deleted)
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
    let old = get_extension(pool, record_uid, namespace).await?;
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
    let empty = serde_json::Map::new();
    match (old.as_ref().and_then(|v| v.as_object()), fds.as_object()) {
        (old_map, Some(new_map)) => {
            let old_map = old_map.unwrap_or(&empty);
            for (key, value) in new_map {
                if old_map.get(key) != Some(value) {
                    crate::sync_ops::log_local(
                        pool,
                        "record_extension",
                        record_uid,
                        &format!("{namespace}.{key}"),
                        OpKind::Set,
                        Some(value.to_string()),
                    )
                    .await?;
                }
            }
            for key in old_map.keys() {
                if !new_map.contains_key(key) {
                    crate::sync_ops::log_local(
                        pool,
                        "record_extension",
                        record_uid,
                        &format!("{namespace}.{key}"),
                        OpKind::Tombstone,
                        None,
                    )
                    .await?;
                }
            }
        }
        (_, None) => {
            crate::sync_ops::log_local(
                pool,
                "record_extension",
                record_uid,
                namespace,
                OpKind::Set,
                Some(fds.to_string()),
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn delete_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM record_extension WHERE record_uid = ? AND namespace = ?")
        .bind(record_uid)
        .bind(namespace)
        .execute(pool)
        .await?;
    crate::sync_ops::log_local(
        pool,
        "record_extension",
        record_uid,
        namespace,
        OpKind::Tombstone,
        None,
    )
    .await?;
    Ok(())
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

pub async fn set_slug(pool: &SqlitePool, uid: &str, slug: Option<&str>) -> Result<(), StoreError> {
    if let Some(slug) = slug {
        if !nucleus::valid_slug(slug) {
            return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
        }
    }
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET slug = ?, updated_at = ? WHERE uid = ?")
        .bind(slug)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    log_set(pool, uid, "slug", serde_json::json!(slug)).await?;
    Ok(())
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
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET unit_uid = ?, updated_at = ? WHERE uid = ?")
        .bind(unit_uid)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    log_set(pool, uid, "unit_uid", serde_json::json!(unit_uid)).await?;
    Ok(())
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
