//! Canonical amounts and evidence bound to an origin handoff.

use std::cmp::Ordering::Greater;

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::{
    StoreError,
    exact::{difference, sum_exact},
    transfer_accounting::{allocated_on, amount},
    transfer_cancellations::effective_total_on,
    transfer_delivery::validation::protocol,
};

/// The immutable allocation and its origin evidence.
#[derive(Debug, Clone)]
pub struct ApplicationHandoffDetailRow {
    pub handoff_uid: String,
    pub source_promise_uid: String,
    pub canonical_quantity: f64,
    pub canonical_unit_uid: Option<String>,
    pub canonical_cumulative_before: f64,
    pub canonical_cumulative_after: f64,
    pub canonical_remaining_after: f64,
    pub application_direction: i8,
    pub origin_evidence_fact_uid: Option<String>,
    pub origin_acceptance_fact_uid: Option<String>,
    pub created_at: String,
}

fn map_handoff_detail(row: SqliteRow) -> Result<ApplicationHandoffDetailRow, StoreError> {
    Ok(ApplicationHandoffDetailRow {
        handoff_uid: row.get("handoff_uid"),
        source_promise_uid: row.get("source_promise_uid"),
        canonical_quantity: row.get("canonical_quantity"),
        canonical_unit_uid: row.get("canonical_unit_uid"),
        canonical_cumulative_before: row.get("canonical_cumulative_before"),
        canonical_cumulative_after: row.get("canonical_cumulative_after"),
        canonical_remaining_after: row.get("canonical_remaining_after"),
        application_direction: row.try_get("application_direction")?,
        origin_evidence_fact_uid: row.get("origin_evidence_fact_uid"),
        origin_acceptance_fact_uid: row.get("origin_acceptance_fact_uid"),
        created_at: row.get("created_at"),
    })
}

/// Finds the stored allocation for an origin handoff.
///
/// # Errors
///
/// Returns an error if storage or row decoding fails.
pub async fn application_handoff_detail(
    pool: &SqlitePool,
    handoff_uid: &str,
) -> Result<Option<ApplicationHandoffDetailRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_application_handoff_detail WHERE handoff_uid = ?")
        .bind(handoff_uid)
        .try_map(map_handoff_detail)
        .fetch_optional(pool)
        .await
}

/// Records a reviewed allocation or returns the original matching detail.
///
/// # Errors
///
/// Returns an error if the allocation changed, the detail conflicts with the
/// saved row, or storage fails.
#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the public Store API for canonical allocation details"
)]
pub async fn store_application_handoff_detail(
    pool: &SqlitePool,
    handoff_uid: &str,
    source_promise_uid: &str,
    canonical_quantity: f64,
    canonical_unit_uid: Option<&str>,
    canonical_cumulative_before: f64,
    canonical_cumulative_after: f64,
    canonical_remaining_after: f64,
    application_direction: i8,
    origin_evidence_fact_uid: Option<&str>,
    now: DateTime<Utc>,
) -> Result<ApplicationHandoffDetailRow, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if let Some(stored) =
        sqlx::query("SELECT * FROM transfer_application_handoff_detail WHERE handoff_uid = ?")
            .bind(handoff_uid)
            .try_map(map_handoff_detail)
            .fetch_optional(&mut *tx)
            .await?
    {
        if stored.source_promise_uid != source_promise_uid
            || stored.canonical_quantity != canonical_quantity
            || stored.canonical_unit_uid.as_deref() != canonical_unit_uid
            || stored.canonical_cumulative_before != canonical_cumulative_before
            || stored.canonical_cumulative_after != canonical_cumulative_after
            || stored.canonical_remaining_after != canonical_remaining_after
            || stored.application_direction != application_direction
        {
            return Err(protocol(
                "application handoff details changed during replay",
            ));
        }
        tx.rollback().await?;
        return Ok(stored);
    }
    let allocation: Option<(String, f64)> = sqlx::query_as("SELECT h.occurrence_uid, o.quantity FROM transfer_application_handoff h JOIN transfer_occurrence o ON o.uid = h.occurrence_uid JOIN promise p ON p.uid = ? AND p.party_uid = h.participant_person_uid WHERE h.uid = ?")
        .bind(source_promise_uid).bind(handoff_uid).fetch_optional(&mut *tx).await?;
    if let Some((occurrence, total)) = allocation {
        let total = effective_total_on(&mut tx, &occurrence, amount(total)?).await?;
        let committed = allocated_on(&mut tx, &occurrence, true).await?;
        let after = sum_exact([committed, amount(canonical_quantity)?])?;
        if canonical_quantity <= 0.0
            || committed.to_f64() != canonical_cumulative_before
            || after.exact_numeric_cmp(total) == Greater
            || after.to_f64() != canonical_cumulative_after
            || difference(total, after)?.to_f64() != canonical_remaining_after
        {
            return Err(protocol(
                "Transfer application allocation changed after review",
            ));
        }
    }
    let stored = sqlx::query(
        "INSERT INTO transfer_application_handoff_detail
         (handoff_uid, source_promise_uid, canonical_quantity, canonical_unit_uid,
          canonical_cumulative_before, canonical_cumulative_after,
          canonical_remaining_after, application_direction,
          origin_evidence_fact_uid, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING *",
    )
    .bind(handoff_uid)
    .bind(source_promise_uid)
    .bind(canonical_quantity)
    .bind(canonical_unit_uid)
    .bind(canonical_cumulative_before)
    .bind(canonical_cumulative_after)
    .bind(canonical_remaining_after)
    .bind(application_direction)
    .bind(origin_evidence_fact_uid)
    .bind(now.to_rfc3339())
    .try_map(map_handoff_detail)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(stored)
}
