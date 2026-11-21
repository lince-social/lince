//! Signed hard stock limits and their enforcement for local and imported writes.

use std::fmt::Display;

use chrono::{DateTime, Utc};
use nucleus::{DecimalValue, fact::sha256_hex, new_uid};
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use sqlx::{Row, SqliteConnection, SqlitePool, sqlite::SqliteRow};

use crate::{
    StoreError,
    exact::{decimal_columns, read_decimal},
    write_tx,
};

mod authority;
mod validation;

pub use authority::require_current_writer_on;
use authority::{local_cell, require_only_writer};
pub use validation::{validate_all_on, validate_record_on};

fn invalid(message: impl Display) -> StoreError {
    StoreError::Protocol(message.to_string())
}

/// A signed request to set or remove a Record's hard stock limit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Input {
    /// Resource Record whose available surplus is protected.
    pub record: String,
    /// Local Person signing the change.
    pub person: String,
    /// Minimum required surplus, or no minimum to remove the limit.
    pub minimum: Option<DecimalValue>,
    /// Number of signed changes already reviewed for the Record.
    pub expected_version: u64,
    /// Idempotency key for this request and its serialized payload.
    pub request_id: String,
}

/// The active hard stock limit for a Record.
#[derive(Clone, Debug, Serialize)]
pub struct Limit {
    /// Resource Record protected by the limit.
    pub record: String,
    /// Cell with exclusive authority over the protected stock.
    pub writer: String,
    /// Minimum surplus after current commitments.
    pub minimum: DecimalValue,
    /// Revision at which this limit was set.
    pub version: u64,
}

async fn limit_on(
    connection: &mut SqliteConnection,
    record: &str,
) -> Result<Option<SqliteRow>, StoreError> {
    sqlx::query(
        "SELECT record_uid, writer_cell_uid, minimum_mantissa, minimum_scale, version
         FROM transfer_stock_limit WHERE record_uid = ?",
    )
    .bind(record)
    .fetch_optional(connection)
    .await
}

/// Read the active limit, if the Record has one.
///
/// # Errors
///
/// Returns an error if the limit cannot be read or its minimum cannot be decoded.
pub async fn get(pool: &SqlitePool, record: &str) -> Result<Option<Limit>, StoreError> {
    limit_on(&mut *pool.acquire().await?, record)
        .await?
        .map(|row| {
            Ok(Limit {
                record: row.get("record_uid"),
                writer: row.get("writer_cell_uid"),
                minimum: read_decimal(&row, "minimum")?,
                version: row.get::<i64, _>("version").cast_unsigned(),
            })
        })
        .transpose()
}

async fn version_on(connection: &mut SqliteConnection, record: &str) -> Result<i64, StoreError> {
    sqlx::query_scalar("SELECT COUNT(*) FROM transfer_stock_limit_event WHERE record_uid = ?")
        .bind(record)
        .fetch_one(connection)
        .await
}

/// Count the Record's signed limit changes, including removals.
///
/// # Errors
///
/// Returns an error if the event count cannot be read.
pub async fn version(pool: &SqlitePool, record: &str) -> Result<u64, StoreError> {
    Ok(version_on(&mut *pool.acquire().await?, record)
        .await?
        .cast_unsigned())
}

/// Commit a signed limit change or return the event ID for an exact replay.
///
/// # Errors
///
/// Returns an error for invalid input, a conflicting request or stale revision,
/// missing ownership, write authority or signature, a stock shortfall, or failed storage.
pub async fn set<F>(
    pool: &SqlitePool,
    input: Input,
    now: DateTime<Utc>,
    key_id: &str,
    public_key: &str,
    sign: F,
) -> Result<String, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let invalid_input =
        || invalid("a stock limit needs a nonnegative minimum and a request identifier");
    if input.request_id.trim().is_empty()
        || input.request_id.len() > 200
        || input.minimum.is_some_and(DecimalValue::is_negative)
    {
        return Err(invalid_input());
    }
    let version = i64::try_from(input.expected_version)
        .ok()
        .and_then(|version| version.checked_add(1))
        .ok_or_else(invalid_input)?;
    let payload = to_string(&input).map_err(invalid)?;
    let mut tx = write_tx(pool).await?;
    if let Some(row) =
        sqlx::query("SELECT uid, payload FROM transfer_stock_limit_event WHERE request_id = ?")
            .bind(&input.request_id)
            .fetch_optional(&mut *tx)
            .await?
    {
        if row.get::<&str, _>("payload") != payload {
            return Err(invalid("stock limit request reused with different values"));
        }
        tx.rollback().await?;
        return Ok(row.get("uid"));
    }
    let own: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM record r JOIN record p ON p.uid = ?
            JOIN record o ON o.slug = 'local-organ' AND o.kind = 'organ'
            WHERE r.uid = ? AND r.organ_uid = o.uid AND p.organ_uid = o.uid
              AND p.kind = 'person' AND p.deleted_at IS NULL
              AND r.deleted_at IS NULL AND r.kind = 'plain')",
    )
    .bind(&input.person)
    .bind(&input.record)
    .fetch_one(&mut *tx)
    .await?;
    if !own {
        return Err(invalid(
            "stock limits belong to your own live resource Records",
        ));
    }
    if version_on(&mut tx, &input.record).await? != version - 1 {
        return Err(invalid("the stock limit changed; review it again"));
    }
    let cell = local_cell(&mut tx).await?.ok_or(StoreError::RowNotFound)?;
    if limit_on(&mut tx, &input.record)
        .await?
        .is_some_and(|row| row.get::<&str, _>("writer_cell_uid") != cell)
    {
        return Err(invalid("change the stock limit on its writing Cell"));
    }
    if input.minimum.is_some() {
        require_only_writer(&mut tx, &cell).await?;
    }
    let uid = new_uid("tsl");
    let signature = sign(&sha256_hex(payload.as_bytes()))
        .ok_or_else(|| invalid("stock limits require the owner's signature"))?;
    sqlx::query(
        "INSERT INTO transfer_stock_limit_event
            (uid, record_uid, person_uid, payload, signature, key_id, public_key, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(&input.record)
    .bind(&input.person)
    .bind(payload)
    .bind(signature)
    .bind(key_id)
    .bind(public_key)
    .bind(input.request_id)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    if let Some(minimum) = input.minimum {
        let (mantissa, scale) = decimal_columns(minimum);
        sqlx::query(
            "INSERT INTO transfer_stock_limit
                (record_uid, writer_cell_uid, person_uid, minimum_mantissa, minimum_scale, version, event_uid)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(record_uid) DO UPDATE SET minimum_mantissa = excluded.minimum_mantissa,
                 minimum_scale = excluded.minimum_scale, version = excluded.version,
                 person_uid = excluded.person_uid, event_uid = excluded.event_uid",
        )
        .bind(&input.record)
        .bind(cell)
        .bind(&input.person)
        .bind(mantissa)
        .bind(scale)
        .bind(version)
        .bind(&uid)
        .execute(&mut *tx)
        .await?;
        validate_record_on(&mut tx, &input.record, None).await?;
    } else {
        sqlx::query("DELETE FROM transfer_stock_limit WHERE record_uid = ?")
            .bind(&input.record)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(uid)
}
