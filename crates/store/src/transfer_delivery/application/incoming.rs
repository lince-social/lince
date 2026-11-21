//! Admission of settlement handoffs from verified recipient snapshots.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::{
    StoreError,
    exact::sum_exact,
    transfer_accounting::amount,
    transfer_delivery::validation::{protocol, required},
};

/// The saved handoff, canonical amounts, and current application state.
#[derive(Debug, Clone)]
pub struct RemoteApplicationHandoffRow {
    pub uid: String,
    pub reference_uid: String,
    pub origin_organ_uid: String,
    pub participant_organ_uid: String,
    pub participant_person_uid: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub source_promise_uid: String,
    pub settlement_slice_uid: String,
    pub origin_revision: u64,
    pub canonical_quantity: f64,
    pub canonical_unit_uid: Option<String>,
    pub canonical_cumulative_before: f64,
    pub canonical_cumulative_after: f64,
    pub canonical_remaining_after: f64,
    pub application_direction: i8,
    pub canonical_slice_hash: String,
    pub envelope_uid: String,
    pub envelope_payload_hash: String,
    pub origin_created_at: String,
    pub state: String,
    pub origin_state: String,
    pub local_application_uid: Option<String>,
}

fn map_remote_application_handoff(
    row: SqliteRow,
) -> Result<RemoteApplicationHandoffRow, StoreError> {
    Ok(RemoteApplicationHandoffRow {
        uid: row.get("uid"),
        reference_uid: row.get("reference_uid"),
        origin_organ_uid: row.get("origin_organ_uid"),
        participant_organ_uid: row.get("participant_organ_uid"),
        participant_person_uid: row.get("participant_person_uid"),
        transfer_uid: row.get("transfer_uid"),
        occurrence_uid: row.get("occurrence_uid"),
        source_promise_uid: row.get("source_promise_uid"),
        settlement_slice_uid: row.get("settlement_slice_uid"),
        origin_revision: row.get::<i64, _>("origin_revision").cast_unsigned(),
        canonical_quantity: row.get("canonical_quantity"),
        canonical_unit_uid: row.get("canonical_unit_uid"),
        canonical_cumulative_before: row.get("canonical_cumulative_before"),
        canonical_cumulative_after: row.get("canonical_cumulative_after"),
        canonical_remaining_after: row.get("canonical_remaining_after"),
        application_direction: row.try_get("application_direction")?,
        canonical_slice_hash: row.get("canonical_slice_hash"),
        envelope_uid: row.get("envelope_uid"),
        envelope_payload_hash: row.get("envelope_payload_hash"),
        origin_created_at: row.get("origin_created_at"),
        state: row.get("state"),
        origin_state: row.get("origin_state"),
        local_application_uid: row.get("local_application_uid"),
    })
}

/// A received handoff carrying canonical quantities and its snapshot identity.
#[derive(Debug)]
pub struct NewRemoteApplicationHandoff<'a> {
    pub uid: &'a str,
    pub reference_uid: &'a str,
    pub origin_organ_uid: &'a str,
    pub participant_organ_uid: &'a str,
    pub participant_person_uid: &'a str,
    pub transfer_uid: &'a str,
    pub occurrence_uid: &'a str,
    pub source_promise_uid: &'a str,
    pub settlement_slice_uid: &'a str,
    pub origin_revision: u64,
    pub canonical_quantity: f64,
    pub canonical_unit_uid: Option<&'a str>,
    pub canonical_cumulative_before: f64,
    pub canonical_cumulative_after: f64,
    pub canonical_remaining_after: f64,
    pub application_direction: i8,
    pub canonical_slice_hash: &'a str,
    pub envelope_uid: &'a str,
    pub envelope_payload_hash: &'a str,
    pub origin_created_at: &'a str,
}

/// Reads a saved incoming handoff by UID.
///
/// # Errors
///
/// Returns an error if storage or row decoding fails.
pub async fn remote_application_handoff(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<RemoteApplicationHandoffRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_remote_application_handoff WHERE uid = ?")
        .bind(uid)
        .try_map(map_remote_application_handoff)
        .fetch_optional(pool)
        .await
}

/// Reads the application view for either a local or an incoming handoff.
///
/// # Errors
///
/// Returns an error if storage or row decoding fails.
pub async fn application_effect_handoff(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<RemoteApplicationHandoffRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_application_effect_handoff WHERE uid = ?")
        .bind(uid)
        .try_map(map_remote_application_handoff)
        .fetch_optional(pool)
        .await
}

/// Stores a handoff from the active verified snapshot or returns its exact replay.
///
/// # Errors
///
/// Returns an error for invalid quantities or identity, stale snapshot proof,
/// a conflicting replay, or a failed write.
pub async fn accept_remote_application_handoff(
    pool: &SqlitePool,
    input: NewRemoteApplicationHandoff<'_>,
    now: DateTime<Utc>,
) -> Result<RemoteApplicationHandoffRow, StoreError> {
    for (label, value) in [
        ("handoff uid", input.uid),
        ("reference uid", input.reference_uid),
        ("origin Organ uid", input.origin_organ_uid),
        ("participant Organ uid", input.participant_organ_uid),
        ("participant Person uid", input.participant_person_uid),
        ("Transfer uid", input.transfer_uid),
        ("occurrence uid", input.occurrence_uid),
        ("source promise uid", input.source_promise_uid),
        ("settlement slice uid", input.settlement_slice_uid),
        ("canonical slice hash", input.canonical_slice_hash),
        ("envelope uid", input.envelope_uid),
        ("envelope payload hash", input.envelope_payload_hash),
        ("origin creation time", input.origin_created_at),
    ] {
        required(label, value)?;
    }
    if input.origin_revision == 0
        || !input.canonical_quantity.is_finite()
        || input.canonical_quantity <= 0.0
        || !input.canonical_cumulative_before.is_finite()
        || input.canonical_cumulative_before < 0.0
        || !input.canonical_cumulative_after.is_finite()
        || input.canonical_cumulative_after <= input.canonical_cumulative_before
        || !input.canonical_remaining_after.is_finite()
        || input.canonical_remaining_after < 0.0
        || !matches!(input.application_direction, -1 | 1)
        || sum_exact([
            amount(input.canonical_cumulative_before)?,
            amount(input.canonical_quantity)?,
        ])?
        .to_f64()
            != input.canonical_cumulative_after
    {
        return Err(protocol(
            "remote application handoff quantities are invalid",
        ));
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let matches_reference: bool = sqlx::query_scalar(
        "SELECT state = 'active' AND origin_organ_uid = ? AND transfer_uid = ?
                AND recipient_person_uid = ? AND recipient_organ_uid = ?
                AND last_envelope_uid IS ? AND last_payload_hash IS ?
         FROM transfer_remote_reference WHERE uid = ?",
    )
    .bind(input.origin_organ_uid)
    .bind(input.transfer_uid)
    .bind(input.participant_person_uid)
    .bind(input.participant_organ_uid)
    .bind(input.envelope_uid)
    .bind(input.envelope_payload_hash)
    .bind(input.reference_uid)
    .fetch_one(&mut *tx)
    .await?;
    if !matches_reference {
        return Err(protocol(
            "remote application handoff does not match the active verified reference",
        ));
    }
    if let Some(existing) =
        sqlx::query("SELECT * FROM transfer_remote_application_handoff WHERE uid = ?")
            .bind(input.uid)
            .try_map(map_remote_application_handoff)
            .fetch_optional(&mut *tx)
            .await?
    {
        let exact = existing.reference_uid == input.reference_uid
            && existing.origin_organ_uid == input.origin_organ_uid
            && existing.participant_organ_uid == input.participant_organ_uid
            && existing.participant_person_uid == input.participant_person_uid
            && existing.transfer_uid == input.transfer_uid
            && existing.occurrence_uid == input.occurrence_uid
            && existing.source_promise_uid == input.source_promise_uid
            && existing.settlement_slice_uid == input.settlement_slice_uid
            && existing.origin_revision == input.origin_revision
            && existing.canonical_quantity == input.canonical_quantity
            && existing.canonical_unit_uid.as_deref() == input.canonical_unit_uid
            && existing.canonical_cumulative_before == input.canonical_cumulative_before
            && existing.canonical_cumulative_after == input.canonical_cumulative_after
            && existing.canonical_remaining_after == input.canonical_remaining_after
            && existing.application_direction == input.application_direction
            && existing.canonical_slice_hash == input.canonical_slice_hash
            && existing.origin_created_at == input.origin_created_at;
        if exact {
            tx.rollback().await?;
            return Ok(existing);
        }
        return Err(protocol(
            "remote application handoff was replayed with changed contents",
        ));
    }
    let at = now.to_rfc3339();
    let stored = sqlx::query(
        "INSERT INTO transfer_remote_application_handoff
         (uid, reference_uid, origin_organ_uid, participant_organ_uid,
          participant_person_uid, transfer_uid, occurrence_uid, source_promise_uid,
          settlement_slice_uid, origin_revision, canonical_quantity,
          canonical_unit_uid, canonical_cumulative_before, canonical_cumulative_after,
          canonical_remaining_after, application_direction, canonical_slice_hash, envelope_uid,
          envelope_payload_hash, origin_created_at, received_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING *",
    )
    .bind(input.uid)
    .bind(input.reference_uid)
    .bind(input.origin_organ_uid)
    .bind(input.participant_organ_uid)
    .bind(input.participant_person_uid)
    .bind(input.transfer_uid)
    .bind(input.occurrence_uid)
    .bind(input.source_promise_uid)
    .bind(input.settlement_slice_uid)
    .bind(input.origin_revision.cast_signed())
    .bind(input.canonical_quantity)
    .bind(input.canonical_unit_uid)
    .bind(input.canonical_cumulative_before)
    .bind(input.canonical_cumulative_after)
    .bind(input.canonical_remaining_after)
    .bind(input.application_direction)
    .bind(input.canonical_slice_hash)
    .bind(input.envelope_uid)
    .bind(input.envelope_payload_hash)
    .bind(input.origin_created_at)
    .bind(&at)
    .bind(&at)
    .try_map(map_remote_application_handoff)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(stored)
}
