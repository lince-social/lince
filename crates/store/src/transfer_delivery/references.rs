//! Recipient-side delivery references and signed policy revisions.

use chrono::{DateTime, Utc};
use nucleus::{fact::sha256_hex, new_uid, transfer_delivery::TransferDeliveryMode};
use serde_json::{Value, to_string, to_vec};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteExecutor, SqliteRow},
};

use super::validation::protocol;
use crate::StoreError;

/// The delivery identity and original signed policy received by a recipient.
pub struct NewRemoteReference<'a> {
    pub origin_organ_uid: &'a str,
    pub transfer_uid: &'a str,
    pub delivery_policy_uid: &'a str,
    pub recipient_person_uid: &'a str,
    pub recipient_organ_uid: &'a str,
    pub mode: TransferDeliveryMode,
    pub policy_revision: u64,
    pub hosted_url: Option<&'a str>,
    pub policy_payload_hash: &'a str,
    pub signed_policy_payload: &'a Value,
    pub envelope_uid: Option<&'a str>,
}

/// The recipient's current policy and latest materialized snapshot.
#[derive(Debug, Clone)]
pub struct RemoteReferenceRow {
    pub uid: String,
    pub origin_organ_uid: String,
    pub transfer_uid: String,
    pub delivery_policy_uid: String,
    pub recipient_person_uid: String,
    pub recipient_organ_uid: String,
    pub mode: TransferDeliveryMode,
    pub state: String,
    pub policy_revision: u64,
    pub last_cursor: u64,
    pub last_transfer_revision: u64,
    pub last_envelope_uid: Option<String>,
    pub last_payload_hash: Option<String>,
    pub projection: Option<String>,
    pub disclosure: Option<String>,
}

fn map_reference(row: SqliteRow) -> Result<RemoteReferenceRow, StoreError> {
    Ok(RemoteReferenceRow {
        uid: row.get("uid"),
        origin_organ_uid: row.get("origin_organ_uid"),
        transfer_uid: row.get("transfer_uid"),
        delivery_policy_uid: row.get("delivery_policy_uid"),
        recipient_person_uid: row.get("recipient_person_uid"),
        recipient_organ_uid: row.get("recipient_organ_uid"),
        mode: TransferDeliveryMode::parse(row.get("mode"))
            .ok_or_else(|| protocol("stored remote delivery mode is invalid"))?,
        state: row.get("state"),
        policy_revision: row.get::<i64, _>("policy_revision").cast_unsigned(),
        last_cursor: row.get::<i64, _>("last_cursor").cast_unsigned(),
        last_transfer_revision: row.get::<i64, _>("last_transfer_revision").cast_unsigned(),
        last_envelope_uid: row.get("last_envelope_uid"),
        last_payload_hash: row.get("last_payload_hash"),
        projection: row.get("projection"),
        disclosure: row.get("disclosure"),
    })
}

/// Stores the original signed policy and returns the current reference.
///
/// # Errors
///
/// Returns an error if this is not the writing Cell, an existing event
/// differs from the submitted proof, or storage fails.
pub async fn create_remote_reference(
    pool: &SqlitePool,
    input: NewRemoteReference<'_>,
    now: DateTime<Utc>,
) -> Result<RemoteReferenceRow, StoreError> {
    let identity = to_vec(&(
        input.origin_organ_uid,
        input.transfer_uid,
        input.recipient_person_uid,
        input.recipient_organ_uid,
    ))
    .map_err(|error| protocol(error.to_string()))?;
    let uid = format!("trr:{}", sha256_hex(&identity));
    let revision = input.policy_revision.cast_signed();
    let at = now.to_rfc3339();
    let signed_payload =
        to_string(input.signed_policy_payload).map_err(|error| protocol(error.to_string()))?;
    let mut tx = crate::write_tx(pool).await?;
    crate::transfer_replication::require_private_writer(
        &mut tx,
        input.transfer_uid,
        input.recipient_person_uid,
    )
    .await?;
    let reference_uid: String = sqlx::query_scalar(
        "INSERT INTO transfer_remote_reference
         (uid, origin_organ_uid, transfer_uid, delivery_policy_uid, recipient_person_uid, recipient_organ_uid,
          mode, state, policy_revision, hosted_url, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'active', ?, ?, ?, ?)
         ON CONFLICT(origin_organ_uid, transfer_uid, recipient_person_uid, recipient_organ_uid)
         DO UPDATE SET hosted_url = COALESCE(excluded.hosted_url, hosted_url), updated_at = excluded.updated_at
         RETURNING uid",
    )
    .bind(&uid)
    .bind(input.origin_organ_uid)
    .bind(input.transfer_uid)
    .bind(input.delivery_policy_uid)
    .bind(input.recipient_person_uid)
    .bind(input.recipient_organ_uid)
    .bind(input.mode.as_str())
    .bind(revision)
    .bind(input.hosted_url)
    .bind(&at)
    .bind(&at)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(row) = sqlx::query(
        "SELECT envelope_uid, payload_hash, signed_payload FROM transfer_remote_policy_event
         WHERE reference_uid = ? AND policy_revision = ?",
    )
    .bind(&reference_uid)
    .bind(revision)
    .fetch_optional(&mut *tx)
    .await?
    {
        if row.get::<Option<&str>, _>("envelope_uid") != input.envelope_uid
            || row.get::<&str, _>("payload_hash") != input.policy_payload_hash
            || row.get::<&str, _>("signed_payload") != signed_payload
        {
            return Err(protocol(
                "hosted reference policy was replayed with changed contents",
            ));
        }
        tx.rollback().await?;
    } else {
        sqlx::query(
            "INSERT INTO transfer_remote_policy_event
             (uid, reference_uid, policy_revision, kind, mode, state, envelope_uid,
              payload_hash, signed_payload, received_at)
             VALUES (?, ?, ?, 'reference', ?, 'active', ?, ?, ?, ?)
             ON CONFLICT(reference_uid, policy_revision) DO NOTHING",
        )
        .bind(new_uid("trpe"))
        .bind(&reference_uid)
        .bind(revision)
        .bind(input.mode.as_str())
        .bind(input.envelope_uid)
        .bind(input.policy_payload_hash)
        .bind(signed_payload)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    reference_from(pool, &reference_uid).await
}

/// Looks up the reference using its origin, Transfer, and recipient identity.
///
/// # Errors
///
/// Returns an error if the lookup or row decoding fails.
pub async fn remote_reference_by_identity(
    pool: &SqlitePool,
    origin_organ_uid: &str,
    transfer_uid: &str,
    recipient_person_uid: &str,
    recipient_organ_uid: &str,
) -> Result<Option<RemoteReferenceRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_remote_reference
         WHERE origin_organ_uid = ? AND transfer_uid = ?
           AND recipient_person_uid = ? AND recipient_organ_uid = ?",
    )
    .bind(origin_organ_uid)
    .bind(transfer_uid)
    .bind(recipient_person_uid)
    .bind(recipient_organ_uid)
    .try_map(map_reference)
    .fetch_optional(pool)
    .await
}

/// Looks up a reference by its local UID.
///
/// # Errors
///
/// Returns an error if the lookup or row decoding fails.
pub async fn remote_reference_by_uid(
    pool: &SqlitePool,
    reference_uid: &str,
) -> Result<Option<RemoteReferenceRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
        .bind(reference_uid)
        .try_map(map_reference)
        .fetch_optional(pool)
        .await
}

pub(super) async fn reference_from<'e>(
    executor: impl SqliteExecutor<'e>,
    uid: &str,
) -> Result<RemoteReferenceRow, StoreError> {
    sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
        .bind(uid)
        .try_map(map_reference)
        .fetch_one(executor)
        .await
}

/// Applies the next signed recipient policy revision or returns its exact replay.
///
/// # Errors
///
/// Returns an error if the revision overflows, admission is stale or revoked,
/// an existing event has different contents, or storage fails.
#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the public Store API for a signed policy event"
)]
pub async fn apply_remote_policy(
    pool: &SqlitePool,
    reference_uid: &str,
    expected_revision: u64,
    mode: TransferDeliveryMode,
    revoked: bool,
    envelope_uid: Option<&str>,
    payload_hash: &str,
    signed_payload: &Value,
    now: DateTime<Utc>,
) -> Result<RemoteReferenceRow, StoreError> {
    let next = expected_revision
        .checked_add(1)
        .ok_or_else(|| protocol("remote delivery policy revision overflow"))?
        .cast_signed();
    let (kind, state) = if revoked {
        ("revoked", "revoked")
    } else {
        ("mode_changed", "active")
    };
    let payload = to_string(signed_payload).map_err(|error| protocol(error.to_string()))?;
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query(
        "SELECT mode, state, envelope_uid, payload_hash, signed_payload FROM transfer_remote_policy_event
         WHERE reference_uid = ? AND policy_revision = ?",
    )
    .bind(reference_uid)
    .bind(next)
    .fetch_optional(&mut *tx)
    .await?
    {
        if row.get::<&str, _>("mode") != mode.as_str()
            || row.get::<&str, _>("state") != state
            || row.get::<Option<&str>, _>("envelope_uid") != envelope_uid
            || row.get::<&str, _>("payload_hash") != payload_hash
            || row.get::<&str, _>("signed_payload") != payload
        {
            return Err(protocol("remote policy revision was replayed with changed contents"));
        }
        tx.rollback().await?;
        return reference_from(pool, reference_uid).await;
    }
    let at = now.to_rfc3339();
    let result = sqlx::query(
        "UPDATE transfer_remote_reference SET mode = ?, state = ?, policy_revision = ?, updated_at = ?
         WHERE uid = ? AND policy_revision = ? AND state = 'active'",
    )
    .bind(mode.as_str())
    .bind(state)
    .bind(next)
    .bind(&at)
    .bind(reference_uid)
    .bind(expected_revision.cast_signed())
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol(
            "remote delivery policy revision is stale or revoked",
        ));
    }
    sqlx::query(
        "INSERT INTO transfer_remote_policy_event
         (uid, reference_uid, policy_revision, kind, mode, state, envelope_uid,
          payload_hash, signed_payload, received_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_uid("trpe"))
    .bind(reference_uid)
    .bind(next)
    .bind(kind)
    .bind(mode.as_str())
    .bind(state)
    .bind(envelope_uid)
    .bind(payload_hash)
    .bind(payload)
    .bind(at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    reference_from(pool, reference_uid).await
}
