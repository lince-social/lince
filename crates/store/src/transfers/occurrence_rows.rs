use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::{TransferLocationSnapshot, location_snapshot};
use crate::StoreError;

/// The current occurrence, including the evidence that activated it and its exchange path.
#[derive(Debug, Clone)]
pub struct TransferOccurrenceRow {
    pub uid: String,
    pub transfer_uid: String,
    pub revision: u64,
    pub promise_uid: String,
    pub exchange_path_uid: String,
    pub exchange_uid: Option<String>,
    pub opposite_promise_uid: Option<String>,
    pub activation_event_uid: String,
    pub activation_fact_uid: String,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub quantity: f64,
    pub giver_person_uid: String,
    pub receiver_person_uid: String,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub delivery_claimed: bool,
    pub receipt_claimed: bool,
    pub disputed: bool,
    pub system_disputed: bool,
    pub system_dispute_fact_uid: Option<String>,
    pub system_disputed_at: Option<String>,
    pub dispute_fact_uid: Option<String>,
    pub disputed_at: Option<String>,
    pub created_at: String,
}

fn map_occurrence(row: SqliteRow) -> TransferOccurrenceRow {
    TransferOccurrenceRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        revision: row.get::<i64, _>("revision").cast_unsigned(),
        promise_uid: row.get("promise_uid"),
        exchange_path_uid: row.get("exchange_path_uid"),
        exchange_uid: row.get("public_exchange_uid"),
        opposite_promise_uid: row.get("opposite_promise_uid"),
        activation_event_uid: row.get("activation_event_uid"),
        activation_fact_uid: row.get("activation_fact_uid"),
        record_uid: row.get("record_uid"),
        concept_uid: row.get("concept_uid"),
        unit_uid: row.get("unit_uid"),
        quantity: row.get("quantity"),
        giver_person_uid: row.get("giver_person_uid"),
        receiver_person_uid: row.get("receiver_person_uid"),
        window_start: row.get("window_start"),
        window_end: row.get("window_end"),
        location: location_snapshot(
            row.get("location_lat"),
            row.get("location_lon"),
            row.get("location_address"),
        ),
        delivery_claimed: row.get("delivery_claimed"),
        receipt_claimed: row.get("receipt_claimed"),
        disputed: row.get("disputed"),
        system_disputed: row.get("system_disputed"),
        system_dispute_fact_uid: row.get("system_dispute_fact_uid"),
        system_disputed_at: row.get("system_disputed_at"),
        dispute_fact_uid: row.get("dispute_fact_uid"),
        disputed_at: row.get("disputed_at"),
        created_at: row.get("created_at"),
    }
}

const OCCURRENCE_SELECT: &str = "SELECT o.*,
            CASE WHEN path.primary_promise_uid = o.promise_uid
                 THEN path.opposite_promise_uid ELSE path.primary_promise_uid END
                 AS opposite_promise_uid,
            activation.fact_uid AS activation_fact_uid, path.public_exchange_uid
     FROM transfer_occurrence o
     JOIN transfer_exchange_path path ON path.uid = o.exchange_path_uid
     JOIN transfer_activation_event activation ON activation.uid = o.activation_event_uid";

/// Reads the current occurrence by its UID.
///
/// # Errors
///
/// Returns a storage error if the occurrence cannot be read.
pub async fn occurrence(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Option<TransferOccurrenceRow>, StoreError> {
    let sql = format!("{OCCURRENCE_SELECT} WHERE o.uid = ?");
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(occurrence_uid)
        .fetch_optional(pool)
        .await?
        .map(map_occurrence))
}

/// Reads the Transfer's occurrences in creation-time and UID order.
///
/// # Errors
///
/// Returns a storage error if the occurrences cannot be read.
pub async fn occurrences_of(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<TransferOccurrenceRow>, StoreError> {
    let sql = format!("{OCCURRENCE_SELECT} WHERE o.transfer_uid = ? ORDER BY o.created_at, o.uid");
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(transfer_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_occurrence)
        .collect())
}

/// Reads the occurrence backed by the given promise.
///
/// # Errors
///
/// Returns a storage error if the occurrence cannot be read.
pub async fn occurrence_for_promise(
    pool: &SqlitePool,
    promise_uid: &str,
) -> Result<Option<TransferOccurrenceRow>, StoreError> {
    let sql = format!("{OCCURRENCE_SELECT} WHERE o.promise_uid = ?");
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(promise_uid)
        .fetch_optional(pool)
        .await?
        .map(map_occurrence))
}

pub(super) async fn occurrences_for_activation_event(
    pool: &SqlitePool,
    event_uid: &str,
) -> Result<Vec<TransferOccurrenceRow>, StoreError> {
    let sql = format!("{OCCURRENCE_SELECT} WHERE o.activation_event_uid = ? ORDER BY o.uid");
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(event_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_occurrence)
        .collect())
}
