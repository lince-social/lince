//! Admission and storage of hosted and replicated recipient snapshots.

use chrono::{DateTime, Utc};
use nucleus::transfer_delivery::{
    TransferDeliveryMode::{self, Hosted, Replicated},
    TransferEnvelopeV1,
};
use serde_json::to_string;
use sqlx::SqlitePool;

use super::{
    references::{RemoteReferenceRow, reference_from},
    validation::protocol,
};
use crate::{StoreError, transfer_outcomes::remember_on};

/// A newly materialized snapshot or the current reference after a replay.
#[derive(Debug, Clone)]
pub enum ReplicaCommit {
    Applied(RemoteReferenceRow),
    Replayed(RemoteReferenceRow),
}

/// Stores a replicated envelope and applies it when its cursor advances.
///
/// # Errors
///
/// Returns an error for an invalid envelope, a policy mismatch, conflicting
/// envelope or cursor history, or a failed write.
pub async fn accept_replica_envelope(
    pool: &SqlitePool,
    reference_uid: &str,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
) -> Result<ReplicaCommit, StoreError> {
    accept_snapshot(pool, reference_uid, envelope, now, Replicated).await
}

/// Applies a hosted snapshot when its cursor advances.
///
/// # Errors
///
/// Returns an error for an invalid envelope, a policy mismatch, changed
/// contents at the current cursor, or a failed write.
pub async fn accept_hosted_snapshot(
    pool: &SqlitePool,
    reference_uid: &str,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
) -> Result<ReplicaCommit, StoreError> {
    accept_snapshot(pool, reference_uid, envelope, now, Hosted).await
}

async fn accept_snapshot(
    pool: &SqlitePool,
    reference_uid: &str,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
    mode: TransferDeliveryMode,
) -> Result<ReplicaCommit, StoreError> {
    envelope.validate_shape().map_err(protocol)?;
    let mut tx = crate::write_tx(pool).await?;
    let reference = reference_from(&mut *tx, reference_uid).await?;
    if reference.mode != mode
        || reference.state != "active"
        || envelope.mode != mode
        || envelope.delivery_policy_uid != reference.delivery_policy_uid
        || envelope.delivery_policy_revision != reference.policy_revision
        || envelope.delivery_policy_state != reference.state
        || envelope.origin_organ_uid != reference.origin_organ_uid
        || envelope.transfer_uid != reference.transfer_uid
        || envelope.recipient_person_uid != reference.recipient_person_uid
        || envelope.recipient_organ_uid != reference.recipient_organ_uid
    {
        return Err(protocol(match mode {
            Hosted => "hosted snapshot does not match its active reference",
            Replicated => "replica envelope does not match an active replicated reference",
        }));
    }

    let order = envelope.cursor.cmp(&reference.last_cursor);
    if mode == Replicated {
        if let Some((hash, existing_reference)) = sqlx::query_as::<_, (String, String)>(
            "SELECT payload_hash, reference_uid FROM transfer_replica_envelope WHERE envelope_uid = ?",
        )
        .bind(&envelope.envelope_uid)
        .fetch_optional(&mut *tx)
        .await?
        {
            if hash != envelope.payload_hash || existing_reference != reference_uid {
                return Err(protocol("replica envelope uid was replayed with changed contents"));
            }
            tx.rollback().await?;
            return Ok(ReplicaCommit::Replayed(reference));
        }
        if order.is_eq() {
            return Err(protocol(
                "replica envelope cursor conflicts with accepted history",
            ));
        }
    } else if order.is_le() {
        if order.is_eq()
            && (reference.last_envelope_uid.as_deref() != Some(&envelope.envelope_uid)
                || reference.last_payload_hash.as_deref() != Some(&envelope.payload_hash))
        {
            return Err(protocol(
                "hosted snapshot cursor was replayed with changed contents",
            ));
        }
        tx.rollback().await?;
        return Ok(ReplicaCommit::Replayed(reference));
    }

    let payload = to_string(envelope).map_err(|error| protocol(error.to_string()))?;
    let at = now.to_rfc3339();
    if mode == Replicated {
        sqlx::query(
            "INSERT INTO transfer_replica_envelope
             (envelope_uid, reference_uid, origin_organ_uid, transfer_uid,
              recipient_person_uid, recipient_organ_uid, cursor, transfer_revision,
              payload, payload_hash, received_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&envelope.envelope_uid)
        .bind(reference_uid)
        .bind(&envelope.origin_organ_uid)
        .bind(&envelope.transfer_uid)
        .bind(&envelope.recipient_person_uid)
        .bind(&envelope.recipient_organ_uid)
        .bind(envelope.cursor.cast_signed())
        .bind(envelope.transfer_revision.cast_signed())
        .bind(&payload)
        .bind(&envelope.payload_hash)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        if order.is_lt() {
            tx.commit().await?;
            return Ok(ReplicaCommit::Replayed(reference));
        }
    }

    remember_on(&mut tx, reference_uid, envelope, &payload, &at).await?;
    let projection =
        to_string(&envelope.projection).map_err(|error| protocol(error.to_string()))?;
    let disclosure =
        to_string(&envelope.disclosure).map_err(|error| protocol(error.to_string()))?;
    sqlx::query(
        "UPDATE transfer_remote_reference
         SET last_cursor = ?, last_transfer_revision = ?, last_envelope_uid = ?,
             last_payload_hash = ?, projection = ?, disclosure = ?, last_fetched_at = ?,
             last_error = NULL, updated_at = ? WHERE uid = ?",
    )
    .bind(envelope.cursor.cast_signed())
    .bind(envelope.transfer_revision.cast_signed())
    .bind(&envelope.envelope_uid)
    .bind(&envelope.payload_hash)
    .bind(projection)
    .bind(disclosure)
    .bind(&at)
    .bind(&at)
    .bind(reference_uid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    reference_from(pool, reference_uid)
        .await
        .map(ReplicaCommit::Applied)
}
