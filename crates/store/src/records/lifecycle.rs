use nucleus::{
    DecimalValue, RecordKind, execution::now, hlc, new_uid, visibility::plan::PRIVATE_NAMESPACE,
};
use serde_json::json;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{
    RecordRow, log_set_on, map_row, protocol, require_active_on, validate_record_uid, validate_slug,
};
use crate::{
    StoreError,
    exact::decimal_columns,
    organs::LOCAL_ORGAN_SLUG,
    sync_ops::{OpKind, log_local_tx},
    write_tx,
};

/// Borrowed fields used to create a Record.
pub struct NewRecord<'a> {
    pub slug: Option<&'a str>,
    pub kind: RecordKind,
    pub head: &'a str,
    pub body: &'a str,
    pub quantity: DecimalValue,
}

/// Creates a Record with a fresh UID under the local Organ.
pub async fn create(pool: &SqlitePool, new: NewRecord<'_>) -> Result<RecordRow, StoreError> {
    create_in_root(pool, new, None).await
}

/// Creates a Record under the local Organ using a preallocated UID.
pub async fn create_with_uid(
    pool: &SqlitePool,
    new: NewRecord<'_>,
    uid: &str,
) -> Result<RecordRow, StoreError> {
    create_local(pool, new, uid, None).await
}

/// Creates a Record under the local Organ with an optional replica root.
pub async fn create_in_root(
    pool: &SqlitePool,
    new: NewRecord<'_>,
    root: Option<&str>,
) -> Result<RecordRow, StoreError> {
    create_local(pool, new, &new_uid("r"), root).await
}

async fn create_local(
    pool: &SqlitePool,
    new: NewRecord<'_>,
    uid: &str,
    root: Option<&str>,
) -> Result<RecordRow, StoreError> {
    let mut tx = write_tx(pool).await?;
    let organ_uid = local_organ_uid_on(&mut tx).await?;
    let row = create_with_uid_on(&mut tx, uid, new, &organ_uid, root).await?;
    tx.commit().await?;
    Ok(row)
}

async fn local_organ_uid_on(tx: &mut Transaction<'_, Sqlite>) -> Result<String, StoreError> {
    sqlx::query_scalar::<_, String>(
        "SELECT uid FROM record
          WHERE slug = ? AND kind = 'organ' AND deleted_at IS NULL",
    )
    .bind(LOCAL_ORGAN_SLUG)
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
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM record
          WHERE uid = ? AND kind = 'organ' AND deleted_at IS NULL",
    )
    .bind(organ_uid)
    .fetch_optional(&mut **tx)
    .await?
    .is_some();
    if !exists {
        return Err(protocol(
            "Record creation requires an existing undeleted Organ",
        ));
    }
    if let Some(root) = replica_root.filter(|root| *root != uid) {
        validate_record_uid(root)?;
        if sqlx::query_scalar::<_, i64>(
            "SELECT 1 FROM record
              WHERE uid = ? AND deleted_at IS NULL
                AND (replica_root IS NULL OR replica_root = uid)",
        )
        .bind(root)
        .fetch_optional(&mut **tx)
        .await?
        .is_none()
        {
            return Err(protocol(
                "Record creation replica root is missing, deleted or nested",
            ));
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

/// Creates a Record and logs its initial fields in the supplied transaction.
///
/// # Errors
/// Rejects invalid identifiers or slugs, missing Organs, missing or nested
/// replica roots, occupied UIDs, and failed database or sync writes.
pub async fn create_with_uid_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    new: NewRecord<'_>,
    organ_uid: &str,
    replica_root: Option<&str>,
) -> Result<RecordRow, StoreError> {
    validate_slug(new.slug)?;
    validate_creation_origin_on(tx, uid, organ_uid, replica_root).await?;
    let now = now().to_rfc3339();
    let created_hlc = hlc::next();
    let (mantissa, scale) = decimal_columns(new.quantity);
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at, replica_root, created_hlc)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uid)
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
    .bind(created_hlc)
    .execute(&mut **tx)
    .await?;
    log_set_on(tx, uid, "kind", json!(new.kind.as_str())).await?;
    log_set_on(tx, uid, "head", json!(new.head)).await?;
    log_set_on(tx, uid, "body", json!(new.body)).await?;
    log_set_on(
        tx,
        uid,
        "quantity",
        json!({ "mantissa": mantissa, "scale": scale }),
    )
    .await?;
    if let Some(slug) = new.slug {
        log_set_on(tx, uid, "slug", json!(slug)).await?;
    }
    log_set_on(tx, uid, "organ_uid", json!(organ_uid)).await?;
    inserted_on(tx, uid).await
}

/// Soft-deletes a Record, returning false if it is absent or already deleted.
pub async fn mark_deleted(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let mut tx = write_tx(pool).await?;
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

/// Soft-deletes a live Record and logs a tombstone in the supplied transaction.
///
/// # Errors
/// Rejects missing or deleted targets, active audience plans, and failed writes.
pub async fn mark_deleted_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    let active_plan = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM record_extension WHERE record_uid=? AND namespace=?
         AND CASE WHEN json_valid(fds) THEN COALESCE(json_extract(fds,'$.status'),'') ELSE '' END!='stopped')",
    )
    .bind(uid)
    .bind(PRIVATE_NAMESPACE)
    .fetch_one(&mut **tx)
    .await?;
    if active_plan {
        return Err(protocol(
            "Stop the audience plan before deleting its Record",
        ));
    }
    let now = now().to_rfc3339();
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
    log_local_tx(tx, "record", uid, "", OpKind::Tombstone, None).await
}

/// Restores a soft-deleted Record and optionally assigns it a slug.
pub async fn restore(pool: &SqlitePool, uid: &str, slug: Option<&str>) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    restore_on(&mut tx, uid, slug).await?;
    tx.commit().await
}

/// Restores the original Record identity and logs the slug in the transaction.
///
/// # Errors
/// Rejects moved Records, invalid or occupied slugs, missing or live targets,
/// and failed writes.
pub async fn restore_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    slug: Option<&str>,
) -> Result<(), StoreError> {
    validate_record_uid(uid)?;
    let moved = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM record_move_member m
         JOIN record_move_offer o ON o.uid=m.offer_uid WHERE m.record_uid=?
         AND o.direction='outgoing' AND o.state='complete')",
    )
    .bind(uid)
    .fetch_one(&mut **tx)
    .await?;
    if moved {
        return Err(protocol(
            "This Record moved to another Organ and cannot be restored from its source",
        ));
    }
    validate_slug(slug)?;
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
    .bind(now().to_rfc3339())
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("Record restoration lost its deleted target"));
    }
    log_set_on(tx, uid, "slug", json!(slug)).await
}
