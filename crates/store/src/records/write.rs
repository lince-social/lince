use nucleus::{execution::now, valid_uid};
use serde_json::json;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{log_set_on, protocol, require_active_on, validate_slug};
use crate::{StoreError, write_tx};

/// Assigns the file origin of a live Record that has none.
pub async fn set_file_origin_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    origin: &str,
) -> Result<(), StoreError> {
    let updated = sqlx::query(
        "UPDATE record SET file_origin = ? WHERE uid = ? AND deleted_at IS NULL AND file_origin IS NULL",
    )
    .bind(origin)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(protocol(
            "This Record already has a file origin or is unavailable",
        ));
    }
    Ok(())
}

/// Updates stored text locally without logging authoring sync operations.
pub async fn set_text(
    pool: &SqlitePool,
    uid: &str,
    head: Option<&str>,
    body: Option<&str>,
) -> Result<(), StoreError> {
    let now = now().to_rfc3339();
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

/// Updates supplied text fields and records the authoring sync operations.
pub async fn set_authoring_text(
    pool: &SqlitePool,
    uid: &str,
    head: Option<&str>,
    body: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    set_authoring_text_on(&mut tx, uid, head, body).await?;
    tx.commit().await
}

/// Updates supplied text fields and logs them in the caller's transaction.
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
    .bind(now().to_rfc3339())
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("Record text authoring lost its active target"));
    }
    if let Some(head) = head {
        log_set_on(tx, uid, "head", json!(head)).await?;
    }
    if let Some(body) = body {
        log_set_on(tx, uid, "body", json!(body)).await?;
    }
    Ok(())
}

/// Sets or clears a live Record's slug and logs the change.
pub async fn set_slug(pool: &SqlitePool, uid: &str, slug: Option<&str>) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    set_slug_on(&mut tx, uid, slug).await?;
    tx.commit().await
}

/// Sets or clears a live Record's slug inside the caller's transaction.
pub async fn set_slug_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    slug: Option<&str>,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    validate_slug(slug)?;
    let now = now().to_rfc3339();
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
    log_set_on(tx, uid, "slug", json!(slug)).await
}

/// Updates the owning Organ locally without logging a sync operation.
pub async fn set_organ_origin(
    pool: &SqlitePool,
    uid: &str,
    organ_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = now().to_rfc3339();
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

/// Sets or clears a live Record's unit and logs the change.
pub async fn set_unit(
    pool: &SqlitePool,
    uid: &str,
    unit_uid: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    set_unit_on(&mut tx, uid, unit_uid).await?;
    tx.commit().await
}

enum Reference {
    Unit,
    Place,
}

impl Reference {
    async fn validate(self, tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<(), StoreError> {
        let (prefix, label, sql) = match self {
            Self::Unit => ("c", "unit", "SELECT 1 FROM concept WHERE uid = ?"),
            Self::Place => ("pl", "place", "SELECT 1 FROM place WHERE uid = ?"),
        };
        if !valid_uid(uid, prefix) {
            return Err(protocol(format!(
                "Record {label} requires a canonical {label} uid"
            )));
        }
        if sqlx::query_scalar::<_, i64>(sql)
            .bind(uid)
            .fetch_optional(&mut **tx)
            .await?
            .is_none()
        {
            return Err(protocol(format!("Record {label} is missing")));
        }
        Ok(())
    }
}

/// Sets or clears a live Record's unit inside the caller's transaction.
pub async fn set_unit_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    unit_uid: Option<&str>,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    if let Some(unit_uid) = unit_uid {
        Reference::Unit.validate(tx, unit_uid).await?;
    }
    let now = now().to_rfc3339();
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
    log_set_on(tx, uid, "unit_uid", json!(unit_uid)).await
}

/// Sets or clears a live Record's place and logs the change.
pub async fn set_place(
    pool: &SqlitePool,
    uid: &str,
    place_uid: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    set_place_on(&mut tx, uid, place_uid).await?;
    tx.commit().await
}

/// Sets or clears a live Record's place inside the caller's transaction.
pub async fn set_place_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    place_uid: Option<&str>,
) -> Result<(), StoreError> {
    require_active_on(tx, uid).await?;
    if let Some(place_uid) = place_uid {
        Reference::Place.validate(tx, place_uid).await?;
    }
    let result = sqlx::query(
        "UPDATE record SET place_uid = ?, updated_at = ?
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(place_uid)
    .bind(now().to_rfc3339())
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("Record place authoring lost its active target"));
    }
    log_set_on(tx, uid, "place_uid", json!(place_uid)).await
}
