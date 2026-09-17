use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Clone, Copy)]
pub struct Stamp<'a> {
    pub tbl: &'a str,
    pub uid: &'a str,
    pub field: &'a str,
    pub hlc: i64,
}

const NOT_SUPERSEDED: &str =
    " AND NOT EXISTS (SELECT 1 FROM sync_op WHERE tbl = ? AND uid = ? AND field = ? AND hlc > ?)";

type SqliteQuery<'q> = sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>;

fn bind_stamp<'q>(query: SqliteQuery<'q>, stamp: &Stamp<'q>) -> SqliteQuery<'q> {
    query
        .bind(stamp.tbl)
        .bind(stamp.uid)
        .bind(stamp.field)
        .bind(stamp.hlc)
}

async fn superseded(
    conn: &mut sqlx::SqliteConnection,
    stamp: &Stamp<'_>,
) -> Result<bool, StoreError> {
    Ok(sqlx::query(
        "SELECT 1 FROM sync_op
          WHERE tbl = ? AND uid = ? AND field = ? AND hlc > ? LIMIT 1",
    )
    .bind(stamp.tbl)
    .bind(stamp.uid)
    .bind(stamp.field)
    .bind(stamp.hlc)
    .fetch_optional(&mut *conn)
    .await?
    .is_some())
}

pub async fn ensure_record_stub(
    pool: &SqlitePool,
    uid: &str,
    kind: &str,
    organ_uid: &str,
    replica_root: Option<&str>,
    created_hlc: Option<i64>,
) -> Result<(), StoreError> {
    let exists = sqlx::query("SELECT 1 FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .is_some();
    if exists {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at, replica_root, created_hlc)
         VALUES (?, NULL, ?, '', '', '0', 0, ?, ?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(if kind.is_empty() { "plain" } else { kind })
    .bind(organ_uid)
    .bind(&now)
    .bind(&now)
    .bind(replica_root)
    .bind(created_hlc)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_record_field(
    pool: &SqlitePool,
    uid: &str,
    field: &str,
    value: &serde_json::Value,
    undelete: bool,
    stamp: Stamp<'_>,
) -> Result<bool, StoreError> {
    let now = Utc::now().to_rfc3339();
    let text = value.as_str().map(str::to_string);
    let applied = match field {
        "head" | "body" | "kind" => {
            let sql = format!(
                "UPDATE record SET {field} = ?, updated_at = ? WHERE uid = ?{NOT_SUPERSEDED}"
            );
            bind_stamp(
                sqlx::query(&sql)
                    .bind(text.unwrap_or_default())
                    .bind(&now)
                    .bind(uid),
                &stamp,
            )
            .execute(pool)
            .await?
            .rows_affected()
                > 0
        }
        "slug" => {
            let taken = match text.as_deref() {
                Some(slug) => sqlx::query("SELECT 1 FROM record WHERE slug = ? AND uid != ?")
                    .bind(slug)
                    .bind(uid)
                    .fetch_optional(pool)
                    .await?
                    .is_some(),
                None => false,
            };
            let sql =
                format!("UPDATE record SET slug = ?, updated_at = ? WHERE uid = ?{NOT_SUPERSEDED}");
            bind_stamp(
                sqlx::query(&sql)
                    .bind(if taken { None } else { text })
                    .bind(&now)
                    .bind(uid),
                &stamp,
            )
            .execute(pool)
            .await?
            .rows_affected()
                > 0
        }
        "organ_uid" if text.as_deref().unwrap_or_default().is_empty() => false,
        "unit_uid" | "organ_uid" => {
            let sql = format!(
                "UPDATE record SET {field} = ?, updated_at = ? WHERE uid = ?{NOT_SUPERSEDED}"
            );
            bind_stamp(sqlx::query(&sql).bind(text).bind(&now).bind(uid), &stamp)
                .execute(pool)
                .await?
                .rows_affected()
                > 0
        }
        "quantity" => {
            let mantissa = value
                .get("mantissa")
                .and_then(|item| item.as_str())
                .unwrap_or("0");
            let scale = value
                .get("scale")
                .and_then(|item| item.as_i64())
                .unwrap_or(0);
            let opening = crate::exact::parse_decimal(mantissa, scale)?;
            if opening.is_zero() {
                sqlx::query("SELECT 1 FROM record WHERE uid = ?")
                    .bind(uid)
                    .fetch_optional(pool)
                    .await?
                    .is_some()
            } else {
                let mut tx = crate::write_tx(pool).await?;
                if superseded(&mut tx, &stamp).await? {
                    return Ok(false);
                }
                let row = sqlx::query(
                    "SELECT quantity_mantissa, quantity_scale FROM record WHERE uid = ?",
                )
                .bind(uid)
                .fetch_optional(&mut *tx)
                .await?;
                let Some(row) = row else {
                    return Ok(false);
                };
                let updated = crate::exact::read_decimal(&row, "quantity")?
                    .aligned_add(opening)
                    .ok_or_else(|| {
                        StoreError::Decode(
                            format!("quantity of {uid} overflows i128 exact range").into(),
                        )
                    })?;
                let (mantissa, scale) = crate::exact::decimal_columns(updated);
                let hit = sqlx::query(
                    "UPDATE record
                        SET quantity_mantissa = ?, quantity_scale = ?, updated_at = ?
                      WHERE uid = ?",
                )
                .bind(mantissa)
                .bind(scale)
                .bind(&now)
                .bind(uid)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                    > 0;
                tx.commit().await?;
                hit
            }
        }
        _ => false,
    };
    if applied && undelete {
        let sql = format!("UPDATE record SET deleted_at = NULL WHERE uid = ?{NOT_SUPERSEDED}");
        bind_stamp(sqlx::query(&sql).bind(uid), &stamp)
            .execute(pool)
            .await?;
    }
    Ok(applied)
}

pub async fn undelete_record(
    pool: &SqlitePool,
    uid: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let sql = format!(
        "UPDATE record SET deleted_at = NULL, updated_at = ? WHERE uid = ?{NOT_SUPERSEDED}"
    );
    bind_stamp(
        sqlx::query(&sql).bind(Utc::now().to_rfc3339()).bind(uid),
        &stamp,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_deleted(pool: &SqlitePool, uid: &str) -> Result<Option<bool>, StoreError> {
    Ok(
        sqlx::query("SELECT deleted_at IS NOT NULL AS gone FROM record WHERE uid = ?")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .map(|row| row.get::<bool, _>("gone")),
    )
}

pub async fn set_record_text_raw(
    pool: &SqlitePool,
    uid: &str,
    head: &str,
    body: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET head = ?, body = ?, updated_at = ? WHERE uid = ?")
        .bind(head)
        .bind(body)
        .bind(Utc::now().to_rfc3339())
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn tombstone_record(
    pool: &SqlitePool,
    uid: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let sql = format!(
        "UPDATE record SET deleted_at = ?, slug = NULL, updated_at = ? WHERE uid = ?{NOT_SUPERSEDED}"
    );
    bind_stamp(sqlx::query(&sql).bind(&now).bind(&now).bind(uid), &stamp)
        .execute(pool)
        .await?;
    Ok(())
}

async fn extension_fds(
    conn: &mut sqlx::SqliteConnection,
    record_uid: &str,
    namespace: &str,
) -> Result<serde_json::Value, StoreError> {
    Ok(
        sqlx::query("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?")
            .bind(record_uid)
            .bind(namespace)
            .fetch_optional(&mut *conn)
            .await?
            .and_then(|r| serde_json::from_str(&r.get::<String, _>("fds")).ok())
            .unwrap_or_else(|| serde_json::json!({})),
    )
}

async fn write_extension_fds(
    conn: &mut sqlx::SqliteConnection,
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
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn set_extension_key(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    key: &str,
    value: serde_json::Value,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if superseded(&mut tx, &stamp).await? {
        return Ok(());
    }
    let mut fds = extension_fds(&mut tx, record_uid, namespace).await?;
    if !fds.is_object() {
        fds = serde_json::json!({});
    }
    fds.as_object_mut()
        .expect("just ensured object")
        .insert(key.to_string(), value);
    write_extension_fds(&mut tx, record_uid, namespace, &fds).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn tombstone_extension_key(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    key: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if superseded(&mut tx, &stamp).await? {
        return Ok(());
    }
    let mut fds = extension_fds(&mut tx, record_uid, namespace).await?;
    if let Some(map) = fds.as_object_mut() {
        map.remove(key);
    }
    write_extension_fds(&mut tx, record_uid, namespace, &fds).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_extension_whole(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    value: &serde_json::Value,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if superseded(&mut tx, &stamp).await? {
        return Ok(());
    }
    write_extension_fds(&mut tx, record_uid, namespace, value).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn upsert_assertion(
    pool: &SqlitePool,
    uid: &str,
    value: &serde_json::Value,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if superseded(&mut tx, &stamp).await? {
        return Ok(());
    }
    let s = |k: &str| value.get(k).and_then(|v| v.as_str()).map(str::to_string);
    if let Some(predicate_uid) = s("predicate_uid") {
        let placeholder = format!("concept:{predicate_uid}");
        let predicate_name = s("predicate_name")
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| placeholder.clone());
        sqlx::query(
            "INSERT OR IGNORE INTO concept (uid, canonical_name, created_at)
             VALUES (?, ?, ?)",
        )
        .bind(&predicate_uid)
        .bind(&predicate_name)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
        if predicate_name != placeholder {
            sqlx::query(
                "UPDATE concept SET canonical_name = ?
                  WHERE uid = ? AND canonical_name = ?",
            )
            .bind(&predicate_name)
            .bind(&predicate_uid)
            .bind(&placeholder)
            .execute(&mut *tx)
            .await?;
        }
    }
    if s("role").as_deref().unwrap_or("ordinary") == "ordinary" {
        let subject = s("subject_uid").unwrap_or_default();
        let predicate = s("predicate_uid").unwrap_or_default();
        let object = s("object_uid");
        sqlx::query("INSERT INTO record_assertion (uid, subject_uid, predicate_uid, object_uid, role, quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at, retracted_at) VALUES (?, ?, ?, ?, 'ordinary', ?, ?, ?, ?, ?, ?) ON CONFLICT(uid) DO UPDATE SET quantity_mantissa = excluded.quantity_mantissa, quantity_scale = excluded.quantity_scale, unit_uid = excluded.unit_uid")
            .bind(uid).bind(&subject).bind(&predicate).bind(&object).bind(s("quantity_mantissa")).bind(value["quantity_scale"].as_i64()).bind(s("unit_uid")).bind(s("asserted_by")).bind(s("created_at").unwrap_or_else(|| Utc::now().to_rfc3339())).bind(Utc::now().to_rfc3339()).execute(&mut *tx).await?;
        project_ordinary_assertion(&mut tx, &subject, &predicate, object.as_deref()).await?;
        tx.commit().await?;
        return Ok(());
    }
    let res = sqlx::query(
        "INSERT OR IGNORE INTO record_assertion
           (uid, subject_uid, predicate_uid, object_uid, role,
            quantity_mantissa, quantity_scale, unit_uid, asserted_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(s("subject_uid").unwrap_or_default())
    .bind(s("predicate_uid").unwrap_or_default())
    .bind(s("object_uid"))
    .bind(s("role").unwrap_or_else(|| "ordinary".into()))
    .bind(s("quantity_mantissa"))
    .bind(value.get("quantity_scale").and_then(|v| v.as_i64()))
    .bind(s("unit_uid"))
    .bind(s("asserted_by"))
    .bind(s("created_at").unwrap_or_else(|| Utc::now().to_rfc3339()))
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        sqlx::query(
            "UPDATE record_assertion SET retracted_at = NULL, retracted_by = NULL WHERE uid = ?",
        )
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn retract_assertion(
    pool: &SqlitePool,
    uid: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let sql = format!(
        "UPDATE record_assertion SET retracted_at = ?
          WHERE uid = ? AND retracted_at IS NULL{NOT_SUPERSEDED}"
    );
    bind_stamp(
        sqlx::query(&sql).bind(Utc::now().to_rfc3339()).bind(uid),
        &stamp,
    )
    .execute(pool)
    .await?;
    let tuple: Option<(String, String, Option<String>)> = sqlx::query_as("SELECT subject_uid, predicate_uid, object_uid FROM record_assertion WHERE uid = ? AND role = 'ordinary'").bind(uid).fetch_optional(pool).await?;
    if let Some((subject, predicate, object)) = tuple {
        let mut tx = crate::write_tx(pool).await?;
        project_ordinary_assertion(&mut tx, &subject, &predicate, object.as_deref()).await?;
        tx.commit().await?;
    }
    Ok(())
}

pub(crate) async fn project_ordinary_assertion(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    subject: &str,
    predicate: &str,
    object: Option<&str>,
) -> Result<(), StoreError> {
    let winner: Option<String> = sqlx::query_scalar("SELECT a.uid FROM record_assertion a WHERE a.subject_uid = ? AND a.predicate_uid = ? AND a.object_uid IS ? AND a.role = 'ordinary' AND NOT EXISTS (SELECT 1 FROM sync_op o WHERE o.tbl = 'record_assertion' AND o.uid = a.uid AND o.kind = 'tombstone') ORDER BY a.uid DESC LIMIT 1")
        .bind(subject).bind(predicate).bind(object).fetch_optional(&mut **tx).await?;
    sqlx::query("UPDATE record_assertion SET retracted_at = COALESCE(retracted_at, ?) WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ? AND role = 'ordinary'")
        .bind(Utc::now().to_rfc3339()).bind(subject).bind(predicate).bind(object).execute(&mut **tx).await?;
    if let Some(winner) = winner {
        let identity: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_assertion WHERE subject_uid = ? AND predicate_uid = ? AND object_uid IS ? AND role = 'identity' AND retracted_at IS NULL)")
            .bind(subject).bind(predicate).bind(object).fetch_one(&mut **tx).await?;
        if !identity {
            sqlx::query("UPDATE record_assertion SET retracted_at = NULL, retracted_by = NULL WHERE uid = ?").bind(winner).execute(&mut **tx).await?;
        }
    }
    Ok(())
}

pub async fn upsert_concept(
    pool: &SqlitePool,
    uid: &str,
    canonical_name: &str,
    origin_organ: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if superseded(&mut tx, &stamp).await? {
        return Ok(());
    }
    let name_holder: Option<String> =
        sqlx::query("SELECT uid FROM concept WHERE canonical_name = ?")
            .bind(canonical_name)
            .fetch_optional(&mut *tx)
            .await?
            .map(|r| r.get("uid"));
    if let Some(holder) = name_holder {
        if holder != uid {
            return Ok(());
        }
    }
    let res = sqlx::query(
        "INSERT OR IGNORE INTO concept (uid, canonical_name, origin_organ, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(canonical_name)
    .bind(origin_organ)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        sqlx::query("UPDATE concept SET canonical_name = ? WHERE uid = ?")
            .bind(canonical_name)
            .bind(uid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn delete_concept(
    pool: &SqlitePool,
    uid: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if superseded(&mut tx, &stamp).await? {
        return Ok(());
    }
    for sql in [
        "DELETE FROM concept_name WHERE concept_uid = ?",
        "DELETE FROM lingua_concept WHERE concept_uid = ?",
    ] {
        sqlx::query(sql).bind(uid).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM concept_parent WHERE concept_uid = ? OR parent_uid = ?")
        .bind(uid)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM concept_equivalence WHERE a_uid = ? OR b_uid = ?")
        .bind(uid)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM concept WHERE uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
