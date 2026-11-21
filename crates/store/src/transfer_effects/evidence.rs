//! Reads recorded private effects and their correction evidence.

use nucleus::Fact;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool, sqlite::SqliteExecutor};

use crate::{StoreError, exact::read_decimal, facts::map_fact_with_origin};

/// Reads the review hash saved for an applied effect group.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn reviewed_hash(pool: &SqlitePool, primary: &str) -> Result<Option<String>, StoreError> {
    reviewed_hash_on(pool, primary).await
}

pub(crate) async fn reviewed_hash_on<'e>(
    executor: impl SqliteExecutor<'e>,
    primary: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT review_hash FROM transfer_private_effect WHERE fact_uid = ?1 AND primary_fact_uid = ?1",
    )
    .bind(primary)
    .fetch_optional(executor)
    .await
}

/// Lists the secondary Facts in ledger order, retaining their origin representation.
///
/// # Errors
///
/// Returns an error if the query or Fact decoding fails.
pub async fn facts(pool: &SqlitePool, primary: &str) -> Result<Vec<Fact>, StoreError> {
    sqlx::query(
        "SELECT f.*, origin.payload AS origin_payload FROM transfer_private_effect e
         JOIN fact f ON f.uid = e.fact_uid LEFT JOIN fact_origin origin ON origin.fact_uid = f.uid
         WHERE e.primary_fact_uid = ?1 AND e.fact_uid != ?1 ORDER BY f.rowid",
    )
    .bind(primary)
    .try_map(map_fact_with_origin)
    .fetch_all(pool)
    .await
}

/// Lists the Records touched by the primary Fact and saved private effects.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn records(pool: &SqlitePool, primary: &str) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT record_uid FROM fact WHERE uid = ?1
         UNION SELECT record_uid FROM transfer_private_effect WHERE primary_fact_uid = ?1",
    )
    .bind(primary)
    .fetch_all(pool)
    .await
}

/// Lists the saved effect projections in ledger order.
///
/// # Errors
///
/// Returns an error if storage fails or a saved quantity cannot be decoded.
pub async fn recorded(pool: &SqlitePool, primary: &str) -> Result<Vec<Value>, StoreError> {
    sqlx::query(
        "SELECT e.fact_uid, e.record_uid, e.mode, e.unit_uid, e.formula,
                f.delta_mantissa, f.delta_scale
         FROM transfer_private_effect e JOIN fact f ON f.uid = e.fact_uid
         WHERE e.primary_fact_uid = ? ORDER BY f.rowid",
    )
    .bind(primary)
    .try_map(|row| {
        Ok(json!({
            "fact": row.get::<&str, _>("fact_uid"),
            "record": row.get::<&str, _>("record_uid"),
            "mode": row.get::<&str, _>("mode"),
            "unit": row.get::<Option<&str>, _>("unit_uid"),
            "formula": row.get::<&str, _>("formula"),
            "delta": read_decimal(&row, "delta")?,
        }))
    })
    .fetch_all(pool)
    .await
}

/// Lists the compensation Facts for secondary effects in ledger order.
///
/// # Errors
///
/// Returns an error if the query or Fact decoding fails.
pub async fn correction_facts(pool: &SqlitePool, primary: &str) -> Result<Vec<Fact>, StoreError> {
    sqlx::query(
        "SELECT f.*, origin.payload AS origin_payload FROM transfer_private_effect e
         JOIN fact f ON f.cause_uid = e.fact_uid AND f.cause_kind = 'compensation'
         LEFT JOIN fact_origin origin ON origin.fact_uid = f.uid
         WHERE e.primary_fact_uid = ?1 AND e.fact_uid != ?1 ORDER BY f.rowid",
    )
    .bind(primary)
    .try_map(map_fact_with_origin)
    .fetch_all(pool)
    .await
}

/// Checks whether a Fact belongs to an application or its signed correction.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn protected(pool: &SqlitePool, fact: &str) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM transfer_private_effect WHERE fact_uid = ?1
           UNION ALL SELECT 1 FROM fact f JOIN transfer_private_effect e ON e.fact_uid = f.cause_uid
                     WHERE f.uid = ?1 AND f.cause_kind = 'compensation'
           UNION ALL SELECT 1 FROM transfer_local_application WHERE application_fact_uid = ?1
           UNION ALL SELECT 1 FROM transfer_private_application_correction WHERE fact_uid = ?1
         )",
    )
    .bind(fact)
    .fetch_one(pool)
    .await
}
