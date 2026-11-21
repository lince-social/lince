//! Read canonical allocations and calculate exact local quantity deltas.

use nucleus::{DecimalValue, transfer::application};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::invalid;
use crate::{
    StoreError,
    exact::{difference, read_decimal, sum_exact},
};

/// Quantities already applied from one occurrence for one Person.
#[derive(Clone, Debug)]
pub struct Applied {
    /// Total received in the Transfer's canonical unit.
    pub canonical: DecimalValue,
    /// Sum of the signed application Fact deltas in the local unit.
    pub local: DecimalValue,
}

/// Read the already-applied canonical and local quantities.
///
/// # Errors
///
/// Returns an error if a read fails or the quantities cannot be summed exactly.
pub async fn applied(
    pool: &SqlitePool,
    occurrence: &str,
    person: &str,
) -> Result<Applied, StoreError> {
    applied_on(&mut *pool.acquire().await?, occurrence, person).await
}

pub(crate) async fn applied_on(
    connection: &mut SqliteConnection,
    occurrence: &str,
    person: &str,
) -> Result<Applied, StoreError> {
    let rows = sqlx::query(
        "SELECT s.canonical_quantity, f.delta_mantissa, f.delta_scale
         FROM transfer_occurrence_settlement_slice s JOIN fact f ON f.uid = s.application_fact_uid
         WHERE s.occurrence_uid = ?1 AND s.owner_person_uid = ?2
         UNION ALL
         SELECT h.canonical_quantity, f.delta_mantissa, f.delta_scale
         FROM transfer_local_application a
         JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid
         JOIN fact f ON f.uid = a.application_fact_uid
         WHERE h.occurrence_uid = ?1 AND h.participant_person_uid = ?2",
    )
    .bind(occurrence)
    .bind(person)
    .fetch_all(connection)
    .await?;
    let canonical = rows
        .iter()
        .map(|row| amount(row.get("canonical_quantity")))
        .collect::<Result<Vec<_>, _>>()?;
    let local = rows
        .iter()
        .map(|row| read_decimal(row, "delta"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Applied {
        canonical: sum_exact(canonical)?,
        local: sum_exact(local)?,
    })
}

/// Return the signed delta and the new local cumulative quantity.
///
/// The formula receives the canonical cumulative quantity. The previously
/// applied local quantity is subtracted from its result.
///
/// # Errors
///
/// Returns an error if formula evaluation or exact subtraction fails.
pub fn calculate(
    formula: &str,
    canonical: DecimalValue,
    applied: DecimalValue,
) -> Result<(DecimalValue, DecimalValue), StoreError> {
    let after = application::evaluate_exact(formula, canonical).map_err(invalid)?;
    Ok((difference(after, applied)?, after))
}

/// Convert a stored Transfer quantity to the canonical decimal representation.
///
/// # Errors
///
/// Returns an error for non-finite or unrepresentable quantities.
pub fn amount(value: f64) -> Result<DecimalValue, StoreError> {
    application::amount(value).map_err(invalid)
}

/// Sum settled and accepted allocations in the occurrence's canonical unit.
///
/// Set `include_pending` to also count reservations awaiting acceptance.
///
/// # Errors
///
/// Returns an error if a read fails or the quantities cannot be summed exactly.
pub async fn allocated(
    pool: &SqlitePool,
    occurrence: &str,
    include_pending: bool,
) -> Result<DecimalValue, StoreError> {
    allocated_on(&mut *pool.acquire().await?, occurrence, include_pending).await
}

pub(crate) async fn allocated_on(
    connection: &mut SqliteConnection,
    occurrence: &str,
    include_pending: bool,
) -> Result<DecimalValue, StoreError> {
    let values: Vec<f64> = sqlx::query_scalar(
        "SELECT canonical_quantity FROM transfer_occurrence_settlement_slice WHERE occurrence_uid = ?1
         UNION ALL
         SELECT d.canonical_quantity FROM transfer_application_handoff h
         JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
         JOIN promise p ON p.uid = d.source_promise_uid AND p.party_uid = h.participant_person_uid
         WHERE h.occurrence_uid = ?1 AND (h.state = 'accepted' OR (?2 AND h.state = 'pending'))",
    )
    .bind(occurrence)
    .bind(include_pending)
    .fetch_all(connection)
    .await?;
    sum_exact(
        values
            .into_iter()
            .map(amount)
            .collect::<Result<Vec<_>, _>>()?,
    )
}
