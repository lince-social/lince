use std::fmt;

use nucleus::{Fact, transfer::TransferRemainderPolicy};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::source_groups::facts_for_settlement;
use crate::StoreError;

/// An immutable settlement and the private application made for it.
#[derive(Clone)]
pub struct OccurrenceSettlementSliceRow {
    pub uid: String,
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub promise_uid: String,
    pub owner_person_uid: String,
    pub canonical_quantity: f64,
    pub canonical_unit_uid: Option<String>,
    pub cumulative_before: f64,
    pub cumulative_after: f64,
    pub remaining_after: f64,
    pub evidence_fact_uid: String,
    pub application_fact_uid: String,
    pub local_record_uid: String,
    pub local_delta: f64,
    pub local_cumulative_before: f64,
    pub local_cumulative_after: f64,
    pub application_formula: String,
    pub application_formula_hash: String,
    pub application_formula_version: u64,
    pub remainder_policy: TransferRemainderPolicy,
    pub idempotency_key: String,
    pub created_at: String,
}

impl fmt::Debug for OccurrenceSettlementSliceRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OccurrenceSettlementSliceRow")
            .field("uid", &self.uid)
            .field("occurrence_uid", &self.occurrence_uid)
            .field("canonical_quantity", &self.canonical_quantity)
            .finish_non_exhaustive()
    }
}

/// Public quantities and slices. Private compensation does not reverse settlement.
#[derive(Debug, Clone)]
pub struct OccurrenceSettlementProgress {
    pub occurrence_uid: String,
    pub canonical_quantity: f64,
    pub settled_quantity: f64,
    pub cancelled_quantity: f64,
    pub cancelled_exact: nucleus::DecimalValue,
    pub remaining_exact: nucleus::DecimalValue,
    pub remaining_quantity: f64,
    pub partially_settled: bool,
    pub settled: bool,
    pub slices: Vec<OccurrenceSettlementSliceRow>,
}

/// The settlement and private application facts alongside current public progress.
#[derive(Debug, Clone)]
pub struct OccurrenceSettlementOutcome {
    pub transfer_uid: String,
    pub slice: OccurrenceSettlementSliceRow,
    pub evidence_fact: Fact,
    pub application_fact: Fact,
    pub progress: OccurrenceSettlementProgress,
    pub satiation_facts: Vec<Fact>,
}

pub(super) fn map_occurrence_settlement_slice(
    row: SqliteRow,
) -> Result<OccurrenceSettlementSliceRow, StoreError> {
    let remainder_policy: &str = row.get("remainder_policy");
    Ok(OccurrenceSettlementSliceRow {
        uid: row.get("uid"),
        occurrence_uid: row.get("occurrence_uid"),
        transfer_uid: row.get("transfer_uid"),
        promise_uid: row.get("promise_uid"),
        owner_person_uid: row.get("owner_person_uid"),
        canonical_quantity: row.get("canonical_quantity"),
        canonical_unit_uid: row.get("canonical_unit_uid"),
        cumulative_before: row.get("cumulative_before"),
        cumulative_after: row.get("cumulative_after"),
        remaining_after: row.get("remaining_after"),
        evidence_fact_uid: row.get("evidence_fact_uid"),
        application_fact_uid: row.get("application_fact_uid"),
        local_record_uid: row.get("local_record_uid"),
        local_delta: row.get("local_delta"),
        local_cumulative_before: row.get("local_cumulative_before"),
        local_cumulative_after: row.get("local_cumulative_after"),
        application_formula: row.get("application_formula"),
        application_formula_hash: row.get("application_formula_hash"),
        application_formula_version: row
            .get::<i64, _>("application_formula_version")
            .cast_unsigned(),
        remainder_policy: TransferRemainderPolicy::parse(remainder_policy).ok_or_else(|| {
            sqlx::Error::Protocol(format!(
                "unknown Transfer remainder policy `{remainder_policy}`"
            ))
        })?,
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    })
}

/// Reads the occurrence's slices ordered by time and UID.
///
/// # Errors
///
/// Returns an error if a slice cannot be loaded or has an invalid policy.
pub async fn occurrence_settlement_slices(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Vec<OccurrenceSettlementSliceRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_slice
         WHERE occurrence_uid = ? ORDER BY created_at, uid",
    )
    .bind(occurrence_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_occurrence_settlement_slice)
    .collect()
}

/// Reads a settlement slice by its UID.
///
/// # Errors
///
/// Returns an error if the slice cannot be loaded or has an invalid policy.
pub async fn occurrence_settlement_slice(
    pool: &SqlitePool,
    settlement_uid: &str,
) -> Result<Option<OccurrenceSettlementSliceRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_occurrence_settlement_slice WHERE uid = ?")
        .bind(settlement_uid)
        .fetch_optional(pool)
        .await?
        .map(map_occurrence_settlement_slice)
        .transpose()
}

/// Finds a slice by its request key, ignoring surrounding whitespace.
///
/// # Errors
///
/// Returns an error if the slice cannot be loaded or has an invalid policy.
pub async fn occurrence_settlement_slice_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceSettlementSliceRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_occurrence_settlement_slice WHERE idempotency_key = ?")
        .bind(request_id.trim())
        .fetch_optional(pool)
        .await?
        .map(map_occurrence_settlement_slice)
        .transpose()
}

/// Reads public progress, including cancellations and allocated quantities.
///
/// # Errors
///
/// Returns an error if storage fails or a quantity cannot be represented.
pub async fn occurrence_settlement_progress(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Option<OccurrenceSettlementProgress>, StoreError> {
    let Some(canonical_quantity) =
        sqlx::query_scalar::<_, f64>("SELECT quantity FROM transfer_occurrence WHERE uid = ?")
            .bind(occurrence_uid)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let slices = occurrence_settlement_slices(pool, occurrence_uid).await?;
    let settled = crate::transfer_accounting::allocated(pool, occurrence_uid, false).await?;
    let cancelled = crate::transfer_cancellations::cancelled(pool, occurrence_uid).await?;
    let effective_total = crate::exact::difference(
        crate::transfer_accounting::amount(canonical_quantity)?,
        cancelled,
    )?;
    let remaining = crate::exact::difference(effective_total, settled)?;
    let settled_quantity = settled.to_f64();
    let remaining_quantity = remaining.to_f64().max(0.0);
    Ok(Some(OccurrenceSettlementProgress {
        occurrence_uid: occurrence_uid.into(),
        canonical_quantity,
        settled_quantity,
        cancelled_quantity: cancelled.to_f64(),
        cancelled_exact: cancelled,
        remaining_exact: remaining,
        remaining_quantity,
        partially_settled: settled_quantity > 0.0 && remaining_quantity > 0.0,
        settled: settled_quantity > 0.0 && remaining_quantity == 0.0 && cancelled.is_zero(),
        slices,
    }))
}

/// Reads the request's facts alongside the current progress and source-group result.
///
/// # Errors
///
/// Returns an error if the slice or any of its related evidence cannot be read.
pub async fn occurrence_settlement_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceSettlementOutcome>, StoreError> {
    let Some(slice) = occurrence_settlement_slice_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    let evidence_fact = crate::facts::get(pool, &slice.evidence_fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let application_fact = crate::facts::get(pool, &slice.application_fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let progress = occurrence_settlement_progress(pool, &slice.occurrence_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let satiation_facts = facts_for_settlement(pool, &slice.uid).await?;
    Ok(Some(OccurrenceSettlementOutcome {
        transfer_uid: slice.transfer_uid.clone(),
        slice,
        evidence_fact,
        application_fact,
        progress,
        satiation_facts,
    }))
}

/// Finds the slice that produced a local application fact.
///
/// # Errors
///
/// Returns an error if the slice cannot be loaded or has an invalid policy.
pub async fn occurrence_settlement_for_application_fact(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<OccurrenceSettlementSliceRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_slice
         WHERE application_fact_uid = ?",
    )
    .bind(fact_uid)
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_settlement_slice)
    .transpose()
}
