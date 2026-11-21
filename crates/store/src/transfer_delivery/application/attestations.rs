//! Attestation evidence and delivery retries for applied settlement slices.

use chrono::{DateTime, Utc};
use nucleus::transfer_delivery::TransferApplicationAttestationV1;
use serde_json::to_string;
use sqlx::{Row, SqlitePool};

use crate::{StoreError, backoff::next_attempt_at, transfer_delivery::validation::protocol};

/// Records a received attestation or returns its UID on exact replay.
///
/// # Errors
///
/// Returns an error for invalid shape, a changed replay, or a failed write.
pub async fn store_application_attestation(
    pool: &SqlitePool,
    attestation: &TransferApplicationAttestationV1,
    now: DateTime<Utc>,
) -> Result<String, StoreError> {
    attestation.validate_shape().map_err(protocol)?;
    let payload = to_string(attestation).map_err(|error| protocol(error.to_string()))?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(existing) = sqlx::query_scalar::<_, String>(
        "SELECT payload FROM transfer_application_attestation WHERE uid = ?",
    )
    .bind(&attestation.attestation_uid)
    .fetch_optional(&mut *tx)
    .await?
    {
        if existing == payload {
            tx.rollback().await?;
            return Ok(attestation.attestation_uid.clone());
        }
        return Err(protocol(
            "application attestation uid was replayed with changed contents",
        ));
    }
    sqlx::query(
        "INSERT INTO transfer_application_attestation
         (uid, origin_organ_uid, participant_organ_uid, participant_person_uid, transfer_uid,
          occurrence_uid, settlement_slice_uid, origin_revision, canonical_slice_hash,
          formula_commitment, formula_version, application_fact_uid, applied_at,
          key_id, signature, payload, received_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&attestation.attestation_uid)
    .bind(&attestation.origin_organ_uid)
    .bind(&attestation.participant_organ_uid)
    .bind(&attestation.participant_person_uid)
    .bind(&attestation.transfer_uid)
    .bind(&attestation.occurrence_uid)
    .bind(&attestation.settlement_slice_uid)
    .bind(attestation.origin_revision.cast_signed())
    .bind(&attestation.canonical_slice_hash)
    .bind(&attestation.formula_commitment)
    .bind(&attestation.formula_version)
    .bind(&attestation.application_fact_uid)
    .bind(&attestation.applied_at)
    .bind(&attestation.key_id)
    .bind(&attestation.signature)
    .bind(payload)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(attestation.attestation_uid.clone())
}

/// A signed attestation awaiting transmission to its origin Organ.
#[derive(Debug, Clone)]
pub struct AttestationOutboxRow {
    pub attestation_uid: String,
    pub handoff_uid: String,
    pub reference_uid: String,
    pub origin_organ_uid: String,
    pub payload: String,
}

/// Queues an attestation, retaining the original delivery on duplicate UID.
///
/// # Errors
///
/// Returns an error if serialization or storage fails.
pub async fn enqueue_application_attestation(
    pool: &SqlitePool,
    handoff_uid: &str,
    reference_uid: &str,
    origin_organ_uid: &str,
    attestation: &TransferApplicationAttestationV1,
    now: DateTime<Utc>,
) -> Result<String, StoreError> {
    let payload = to_string(attestation).map_err(|error| protocol(error.to_string()))?;
    sqlx::query(
        "INSERT INTO transfer_application_attestation_outbox
         (attestation_uid, handoff_uid, reference_uid, origin_organ_uid, payload,
          next_attempt_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(attestation_uid) DO NOTHING",
    )
    .bind(&attestation.attestation_uid)
    .bind(handoff_uid)
    .bind(reference_uid)
    .bind(origin_organ_uid)
    .bind(payload)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(attestation.attestation_uid.clone())
}

/// Lists queued and failed attestations due for transmission.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn application_attestations_due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<AttestationOutboxRow>, StoreError> {
    sqlx::query(
        "SELECT attestation_uid, handoff_uid, reference_uid, origin_organ_uid, payload
         FROM transfer_application_attestation_outbox
         WHERE status IN ('queued', 'failed') AND next_attempt_at <= ?
         ORDER BY created_at, attestation_uid LIMIT ?",
    )
    .bind(now.to_rfc3339())
    .bind(i64::from(limit.max(1)))
    .map(|row| AttestationOutboxRow {
        attestation_uid: row.get("attestation_uid"),
        handoff_uid: row.get("handoff_uid"),
        reference_uid: row.get("reference_uid"),
        origin_organ_uid: row.get("origin_organ_uid"),
        payload: row.get("payload"),
    })
    .fetch_all(pool)
    .await
}

/// Marks the attestation sent and records acceptance on its remote handoff.
///
/// # Errors
///
/// Returns an error if either update or the commit fails.
pub async fn application_attestation_mark_sent(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    let at = now.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_application_attestation_outbox
         SET status = 'sent', attempts = attempts + 1, last_attempt_at = ?1, sent_at = ?1, last_error = NULL
         WHERE attestation_uid = ?2 AND status IN ('queued', 'failed')",
    )
    .bind(&at)
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE transfer_remote_application_handoff SET origin_state = 'accepted', updated_at = ?
         WHERE uid = (SELECT handoff_uid FROM transfer_application_attestation_outbox
                      WHERE attestation_uid = ?)",
    )
    .bind(at)
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Records a failed send and schedules the next attempt.
///
/// # Errors
///
/// Returns an error if the attestation is missing or storage fails.
pub async fn application_attestation_mark_failed(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
    error: &str,
) -> Result<(), StoreError> {
    let attempts: i64 = sqlx::query_scalar(
        "SELECT attempts FROM transfer_application_attestation_outbox WHERE attestation_uid = ?",
    )
    .bind(uid)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "UPDATE transfer_application_attestation_outbox
         SET status = 'failed', attempts = attempts + 1, last_attempt_at = ?,
             next_attempt_at = ?, last_error = ?
         WHERE attestation_uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(now.to_rfc3339())
    .bind(next_attempt_at(now, attempts, 5, 3600).to_rfc3339())
    .bind(error)
    .bind(uid)
    .execute(pool)
    .await?;
    Ok(())
}
