use nucleus::execution::now;
use serde_json::Value;
use sqlx::{SqlitePool, query, query_scalar};

use super::{Stamp, stamped};
use crate::{
    StoreError,
    exact::{decimal_columns, parse_decimal, read_decimal},
    write_tx,
};

// Each column is a fixed SQL literal. Incoming field names select a statement in Rust.
macro_rules! record_field {
    ($field:literal) => {
        stamped!(
            "UPDATE record SET ",
            $field,
            " = ?, updated_at = ? WHERE uid = ?"
        )
    };
}

/// Ensures a referenced Record exists with its original creation metadata.
pub async fn ensure_record_stub(
    pool: &SqlitePool,
    uid: &str,
    kind: &str,
    organ_uid: &str,
    replica_root: Option<&str>,
    created_hlc: Option<i64>,
) -> Result<(), StoreError> {
    let exists = query("SELECT 1 FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .is_some();
    if exists {
        return Ok(());
    }
    let now = now().to_rfc3339();
    query(
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

/// Applies a supported Record field unless a newer operation has superseded it.
/// For a requested undelete, the same clock guard also applies to the deletion flag.
/// A zero opening quantity only checks that the Record exists.
///
/// # Errors
/// Returns an error on an invalid or overflowing quantity, an opening quantity
/// on a guarded Record, or a failed database operation.
pub async fn set_record_field(
    pool: &SqlitePool,
    uid: &str,
    field: &str,
    value: &Value,
    undelete: bool,
    stamp: Stamp<'_>,
) -> Result<bool, StoreError> {
    let now = now().to_rfc3339();
    let mut text = value.as_str();
    let applied = if field == "quantity" {
        apply_opening(pool, uid, value, &now, stamp).await?
    } else {
        let sql = match field {
            "head" => record_field!("head"),
            "body" => record_field!("body"),
            "kind" => record_field!("kind"),
            "slug" => {
                if let Some(slug) = text
                    && query("SELECT 1 FROM record WHERE slug = ? AND uid != ?")
                        .bind(slug)
                        .bind(uid)
                        .fetch_optional(pool)
                        .await?
                        .is_some()
                {
                    text = None;
                }
                record_field!("slug")
            }
            "unit_uid" => record_field!("unit_uid"),
            "organ_uid" if text.is_some_and(|org| !org.is_empty()) => record_field!("organ_uid"),
            _ => return Ok(false),
        };
        if matches!(field, "head" | "body" | "kind") {
            text = Some(text.unwrap_or_default());
        }
        stamp
            .bind(query(sql).bind(text).bind(&now).bind(uid))
            .execute(pool)
            .await?
            .rows_affected()
            > 0
    };
    if applied && undelete {
        stamp
            .bind(
                query(stamped!(
                    "UPDATE record SET deleted_at = NULL WHERE uid = ?"
                ))
                .bind(uid),
            )
            .execute(pool)
            .await?;
    }
    Ok(applied)
}

async fn apply_opening(
    pool: &SqlitePool,
    uid: &str,
    value: &Value,
    now: &str,
    stamp: Stamp<'_>,
) -> Result<bool, StoreError> {
    let mantissa = value.get("mantissa").and_then(Value::as_str).unwrap_or("0");
    let scale = value.get("scale").and_then(Value::as_i64).unwrap_or(0);
    let opening = parse_decimal(mantissa, scale)?;
    if opening.is_zero() {
        return Ok(query("SELECT 1 FROM record WHERE uid = ?")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .is_some());
    }

    let mut tx = write_tx(pool).await?;
    if stamp.superseded(&mut tx).await? {
        return Ok(false);
    }
    let guarded =
        query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_stock_limit WHERE record_uid = ?)")
            .bind(uid)
            .fetch_one(&mut *tx)
            .await?;
    if guarded {
        return Err(StoreError::Protocol(
            "a guarded Record cannot accept a new opening quantity".into(),
        ));
    }
    let row = query("SELECT quantity_mantissa, quantity_scale FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let updated = read_decimal(&row, "quantity")?
        .aligned_add(opening)
        .ok_or_else(|| {
            StoreError::Decode(format!("quantity of {uid} overflows i128 exact range").into())
        })?;
    let (mantissa, scale) = decimal_columns(updated);
    let hit = query(
        "UPDATE record SET quantity_mantissa = ?, quantity_scale = ?, updated_at = ? WHERE uid = ?",
    )
    .bind(mantissa)
    .bind(scale)
    .bind(now)
    .bind(uid)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    tx.commit().await?;
    Ok(hit)
}

/// Clears a Record's deletion flag unless superseded by a newer operation.
pub async fn undelete_record(
    pool: &SqlitePool,
    uid: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    stamp
        .bind(
            query(stamped!(
                "UPDATE record SET deleted_at = NULL, updated_at = ? WHERE uid = ?"
            ))
            .bind(now().to_rfc3339())
            .bind(uid),
        )
        .execute(pool)
        .await?;
    Ok(())
}

/// Reports whether the stored Record is deleted, if it exists.
pub async fn record_deleted(pool: &SqlitePool, uid: &str) -> Result<Option<bool>, StoreError> {
    query_scalar("SELECT deleted_at IS NOT NULL AS gone FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await
}

/// Stores CRDT-projected text without emitting a new operation.
pub async fn set_record_text_raw(
    pool: &SqlitePool,
    uid: &str,
    head: &str,
    body: &str,
) -> Result<(), StoreError> {
    query("UPDATE record SET head = ?, body = ?, updated_at = ? WHERE uid = ?")
        .bind(head)
        .bind(body)
        .bind(now().to_rfc3339())
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Marks a Record deleted and clears its slug unless superseded.
pub async fn tombstone_record(
    pool: &SqlitePool,
    uid: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let now = now().to_rfc3339();
    stamp
        .bind(
            query(stamped!(
                "UPDATE record SET deleted_at = ?, slug = NULL, updated_at = ? WHERE uid = ?"
            ))
            .bind(&now)
            .bind(&now)
            .bind(uid),
        )
        .execute(pool)
        .await?;
    Ok(())
}
