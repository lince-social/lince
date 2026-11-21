use chrono::{DateTime, Utc};
use nucleus::{Cause, Fact, NewFact, transfer::TransferOccurrenceSettlementCompensationEvidence};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::{Signing, reads::map_occurrence_settlement_slice};
use crate::{
    StoreError,
    transfers::{ensure_phase4_request_unused, validate_request_key},
};

/// An owner's request to reverse a settlement's private application.
#[derive(Debug, Clone)]
pub struct OccurrenceSettlementCompensationInput {
    pub settlement_uid: String,
    pub idempotency_key: String,
    pub actor_person_uid: String,
    pub authorization_intent_uid: Option<String>,
}

/// An immutable correction of one local application fact.
#[derive(Debug, Clone)]
pub struct OccurrenceSettlementCompensationRow {
    pub uid: String,
    pub settlement_uid: String,
    pub occurrence_uid: String,
    pub transfer_uid: String,
    pub owner_person_uid: String,
    pub original_application_fact_uid: String,
    pub compensation_fact_uid: String,
    pub local_record_uid: String,
    pub inverse_delta: f64,
    pub idempotency_key: String,
    pub created_at: String,
}

/// The compensation and the signed fact recording it.
#[derive(Debug, Clone)]
pub struct OccurrenceSettlementCompensationOutcome {
    pub correction: OccurrenceSettlementCompensationRow,
    pub fact: Fact,
}

/// Whether compensation was committed or replayed.
#[derive(Debug, Clone)]
pub enum OccurrenceSettlementCompensationCommit {
    Committed(OccurrenceSettlementCompensationOutcome),
    Replayed(OccurrenceSettlementCompensationOutcome),
}

fn map_occurrence_settlement_compensation(row: SqliteRow) -> OccurrenceSettlementCompensationRow {
    OccurrenceSettlementCompensationRow {
        uid: row.get("uid"),
        settlement_uid: row.get("settlement_uid"),
        occurrence_uid: row.get("occurrence_uid"),
        transfer_uid: row.get("transfer_uid"),
        owner_person_uid: row.get("owner_person_uid"),
        original_application_fact_uid: row.get("original_application_fact_uid"),
        compensation_fact_uid: row.get("compensation_fact_uid"),
        local_record_uid: row.get("local_record_uid"),
        inverse_delta: row.get("inverse_delta"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

/// Finds the compensation for a settlement slice.
///
/// # Errors
///
/// Returns a storage error if the correction cannot be read.
pub async fn occurrence_settlement_compensation_for_settlement(
    pool: &SqlitePool,
    settlement_uid: &str,
) -> Result<Option<OccurrenceSettlementCompensationRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_compensation
         WHERE settlement_uid = ?",
    )
    .bind(settlement_uid)
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_settlement_compensation))
}

/// Finds the correction written by a compensation fact.
///
/// # Errors
///
/// Returns a storage error if the correction cannot be read.
pub async fn occurrence_settlement_compensation_for_fact(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<OccurrenceSettlementCompensationRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_compensation
         WHERE compensation_fact_uid = ?",
    )
    .bind(fact_uid)
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_settlement_compensation))
}

/// Reads the occurrence's corrections in time and UID order.
///
/// # Errors
///
/// Returns a storage error if the corrections cannot be read.
pub async fn occurrence_settlement_compensations(
    pool: &SqlitePool,
    occurrence_uid: &str,
) -> Result<Vec<OccurrenceSettlementCompensationRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_compensation
         WHERE occurrence_uid = ? ORDER BY created_at, uid",
    )
    .bind(occurrence_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_occurrence_settlement_compensation)
    .collect())
}

/// Finds a correction and its fact by request key.
///
/// # Errors
///
/// Returns an error if the correction or its fact cannot be read.
pub async fn occurrence_settlement_compensation_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<OccurrenceSettlementCompensationOutcome>, StoreError> {
    let Some(correction) = sqlx::query(
        "SELECT * FROM transfer_occurrence_settlement_compensation
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    .map(map_occurrence_settlement_compensation) else {
        return Ok(None);
    };
    let fact = crate::facts::get(pool, &correction.compensation_fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(OccurrenceSettlementCompensationOutcome {
        correction,
        fact,
    }))
}

/// Reverses a private application while preserving the public settlement.
///
/// # Errors
///
/// Returns an error for an invalid or conflicting request, missing authorization,
/// a changed Record unit, a prior correction, or a storage failure.
pub async fn compensate_occurrence_settlement<F>(
    pool: &SqlitePool,
    input: OccurrenceSettlementCompensationInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OccurrenceSettlementCompensationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = occurrence_settlement_compensation_for_request(pool, request_id).await? {
        if outcome.correction.settlement_uid != input.settlement_uid
            || outcome.correction.owner_person_uid != input.actor_person_uid
        {
            return Err(sqlx::Error::Protocol(
                "compensation request id was already used with different targets".into(),
            ));
        }
        return Ok(OccurrenceSettlementCompensationCommit::Replayed(outcome));
    }
    ensure_phase4_request_unused(pool, request_id).await?;

    let mut tx = crate::write_tx(pool).await?;
    let row = sqlx::query("SELECT * FROM transfer_occurrence_settlement_slice WHERE uid = ?")
        .bind(&input.settlement_uid)
        .fetch_one(&mut *tx)
        .await?;
    let slice = map_occurrence_settlement_slice(row)?;
    if slice.owner_person_uid != input.actor_person_uid {
        return Err(sqlx::Error::Protocol(
            "only the settlement owner may compensate its private application".into(),
        ));
    }
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM transfer_occurrence_settlement_compensation
         WHERE settlement_uid = ?)",
    )
    .bind(&input.settlement_uid)
    .fetch_one(&mut *tx)
    .await?
    {
        return Err(sqlx::Error::Protocol(
            "settlement application was already compensated".into(),
        ));
    }
    let original = sqlx::query(
        "SELECT f.delta_mantissa, f.delta_scale,
                json_extract(f.payload, '$.local_unit_uid') AS original_unit,
                r.unit_uid AS current_unit
         FROM fact f JOIN record r ON r.uid = f.record_uid
         WHERE f.uid = ? AND f.record_uid = ?",
    )
    .bind(&slice.application_fact_uid)
    .bind(&slice.local_record_uid)
    .fetch_one(&mut *tx)
    .await?;
    if original.get::<Option<&str>, _>("original_unit")
        != original.get::<Option<&str>, _>("current_unit")
    {
        return Err(sqlx::Error::Protocol(
            "restore the original Record unit before correcting this application".into(),
        ));
    }
    let inverse_delta = crate::exact::negate(crate::exact::read_decimal(&original, "delta")?)?;
    let correction_uid = nucleus::new_uid("tsc");
    let evidence = TransferOccurrenceSettlementCompensationEvidence {
        action: "compensate-transfer-occurrence-settlement".into(),
        idempotency_key: request_id.into(),
        compensation_uid: correction_uid.clone(),
        settlement_uid: slice.uid.clone(),
        transfer_uid: slice.transfer_uid.clone(),
        occurrence_uid: slice.occurrence_uid.clone(),
        owner_person_uid: input.actor_person_uid.clone(),
        original_application_fact_uid: slice.application_fact_uid.clone(),
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let signing = Signing {
        now,
        intent: input.authorization_intent_uid.as_deref(),
        sign: &sign,
    };
    let fact = signing
        .append(
            &mut tx,
            NewFact {
                uid: None,
                record_uid: slice.local_record_uid.clone(),
                delta: inverse_delta,
                at: None,
                actor_uid: Some(input.actor_person_uid.clone()),
                cause: Cause {
                    kind: nucleus::CauseKind::Compensation,
                    uid: Some(slice.application_fact_uid.clone()),
                },
                payload: Some(payload),
            },
            &previous_hash,
            "settlement compensation requires a signing key or verified Action intent",
        )
        .await?;

    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_phase5_correction_request
            (idempotency_key, kind, target_uid, fact_uid, created_at)
         VALUES (?, 'compensation', ?, ?, ?)",
    )
    .bind(request_id)
    .bind(&slice.uid)
    .bind(&fact.uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_occurrence_settlement_compensation
            (uid, settlement_uid, occurrence_uid, transfer_uid,
             owner_person_uid, original_application_fact_uid,
             compensation_fact_uid, local_record_uid, inverse_delta,
             idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&correction_uid)
    .bind(&slice.uid)
    .bind(&slice.occurrence_uid)
    .bind(&slice.transfer_uid)
    .bind(&slice.owner_person_uid)
    .bind(&slice.application_fact_uid)
    .bind(&fact.uid)
    .bind(&slice.local_record_uid)
    .bind(inverse_delta.to_f64())
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    crate::records::bump_quantity(&mut tx, &slice.local_record_uid, inverse_delta, &at).await?;
    crate::transfer_effects::compensate_on(
        &mut tx,
        &slice.application_fact_uid,
        &input.actor_person_uid,
        input.authorization_intent_uid.as_deref(),
        now,
        &sign,
    )
    .await?;
    tx.commit().await?;

    let outcome = occurrence_settlement_compensation_for_request(pool, request_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(OccurrenceSettlementCompensationCommit::Committed(outcome))
}
