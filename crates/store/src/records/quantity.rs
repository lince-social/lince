use nucleus::{
    DecimalValue,
    performance::{Span, add},
};
use sqlx::{Executor, Row, Sqlite, SqlitePool, Transaction};

use crate::{
    StoreError,
    exact::{decimal_columns, read_decimal},
    transfer_stock::{validate_all_on, validate_record_on},
};

/// Reads the exact quantity of a live Record.
pub async fn quantity(pool: &SqlitePool, uid: &str) -> Result<Option<DecimalValue>, StoreError> {
    live_quantity(pool, uid).await
}

/// Reads a live Record quantity within the caller's transaction.
pub async fn quantity_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<DecimalValue>, StoreError> {
    live_quantity(&mut **tx, uid).await
}

async fn live_quantity<'e>(
    executor: impl Executor<'e, Database = Sqlite>,
    uid: &str,
) -> Result<Option<DecimalValue>, StoreError> {
    sqlx::query(
        "SELECT quantity_mantissa, quantity_scale FROM record
          WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(uid)
    .fetch_optional(executor)
    .await?
    .map(|row| read_decimal(&row, "quantity"))
    .transpose()
}

/// Reads exact quantities of live Records without loading their bodies.
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

/// The identity, kind, and exact quantity of a live Record.
pub struct NamedLevel {
    pub uid: String,
    pub slug: Option<String>,
    pub kind: String,
    pub quantity: DecimalValue,
}

/// Reads Record quantities and identifiers in creation order.
pub async fn named_levels(pool: &SqlitePool) -> Result<Vec<NamedLevel>, StoreError> {
    let _read = Span::new("record-named-levels-read-await");
    let rows = sqlx::query(
        "SELECT uid, slug, kind, quantity_mantissa, quantity_scale
         FROM record WHERE deleted_at IS NULL ORDER BY created_at, uid",
    )
    .fetch_all(pool)
    .await?;
    add("record-named-level-rows", rows.len() as u64);
    rows.into_iter()
        .map(|row| {
            Ok(NamedLevel {
                uid: row.get("uid"),
                slug: row.get("slug"),
                kind: row.get("kind"),
                quantity: read_decimal(&row, "quantity")?,
            })
        })
        .collect()
}

/// Applies a local exact delta and validates the resulting stock constraints.
///
/// # Errors
/// Returns an error for a missing Record, arithmetic overflow, a stock
/// violation, or a database failure.
pub async fn bump_quantity(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    delta: DecimalValue,
    now_rfc3339: &str,
) -> Result<(), StoreError> {
    bump_quantity_on(tx, uid, delta, now_rfc3339, None).await
}

/// Applies an imported exact delta and validates stock for its writer.
///
/// # Errors
/// Returns an error for a missing Record, arithmetic overflow, a stock
/// violation, or a database failure.
pub async fn bump_imported_quantity(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    delta: DecimalValue,
    now_rfc3339: &str,
    writer: &str,
) -> Result<(), StoreError> {
    bump_quantity_on(tx, uid, delta, now_rfc3339, Some(writer)).await
}

async fn bump_quantity_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    delta: DecimalValue,
    now_rfc3339: &str,
    imported_writer: Option<&str>,
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
    if let Some(writer) = imported_writer {
        return validate_record_on(tx, uid, Some(writer)).await;
    }
    let is_transfer =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer WHERE record_uid = ?)")
            .bind(uid)
            .fetch_one(&mut **tx)
            .await?;
    if is_transfer {
        validate_all_on(tx).await
    } else {
        validate_record_on(tx, uid, None).await
    }
}
