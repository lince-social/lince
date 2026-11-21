//! Origin-side delivery policy and its revision history.

use chrono::{DateTime, Utc};
use nucleus::{new_uid, transfer_delivery::TransferDeliveryMode};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::validation::{ensure_request_unused, protocol, required};
use crate::StoreError;

/// The current delivery mode, status, and recipient of a Transfer.
#[derive(Debug, Clone)]
pub struct DeliveryPolicyRow {
    pub uid: String,
    pub transfer_uid: String,
    pub origin_organ_uid: String,
    pub recipient_person_uid: String,
    pub recipient_organ_uid: String,
    pub mode: TransferDeliveryMode,
    pub state: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// The recipient and evidence for a new delivery policy.
#[derive(Debug)]
pub struct NewDeliveryPolicy<'a> {
    pub transfer_uid: &'a str,
    pub origin_organ_uid: &'a str,
    pub recipient_person_uid: &'a str,
    pub recipient_organ_uid: &'a str,
    pub mode: TransferDeliveryMode,
    pub actor_person_uid: &'a str,
    pub fact_uid: &'a str,
    pub request_id: &'a str,
}

/// Authorization and revision expected by a policy change.
#[derive(Debug)]
pub struct DeliveryPolicyTransition<'a> {
    pub delivery_uid: &'a str,
    pub expected_revision: u64,
    pub actor_person_uid: &'a str,
    pub fact_uid: &'a str,
    pub request_id: &'a str,
}

/// The current policy after applying a change or replaying its request.
#[derive(Debug, Clone)]
pub enum PolicyCommit {
    Applied(DeliveryPolicyRow),
    Replayed(DeliveryPolicyRow),
}

pub(super) fn map_policy(row: SqliteRow) -> Result<DeliveryPolicyRow, StoreError> {
    Ok(DeliveryPolicyRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        origin_organ_uid: row.get("origin_organ_uid"),
        recipient_person_uid: row.get("recipient_person_uid"),
        recipient_organ_uid: row.get("recipient_organ_uid"),
        mode: TransferDeliveryMode::parse(row.get("mode"))
            .ok_or_else(|| protocol("stored Transfer delivery mode is invalid"))?,
        state: row.get("state"),
        revision: row.get::<i64, _>("revision").cast_unsigned(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

/// Looks up the current policy by delivery UID.
///
/// # Errors
///
/// Returns an error if storage fails or the stored mode is invalid.
pub async fn policy(
    pool: &SqlitePool,
    delivery_uid: &str,
) -> Result<Option<DeliveryPolicyRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_delivery_policy WHERE uid = ?")
        .bind(delivery_uid)
        .try_map(map_policy)
        .fetch_optional(pool)
        .await
}

/// Lists a Transfer's policies in creation order.
///
/// # Errors
///
/// Returns an error if storage fails or a stored mode is invalid.
pub async fn policies_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<DeliveryPolicyRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_delivery_policy WHERE transfer_uid = ? ORDER BY created_at, uid",
    )
    .bind(transfer_uid)
    .try_map(map_policy)
    .fetch_all(pool)
    .await
}

/// Establishes a recipient's delivery policy under the Transfer's origin Organ.
///
/// # Errors
///
/// Returns an error for invalid origin or actor evidence, a conflicting request,
/// or a failed storage operation.
pub async fn create_policy(
    pool: &SqlitePool,
    input: NewDeliveryPolicy<'_>,
    now: DateTime<Utc>,
) -> Result<PolicyCommit, StoreError> {
    for (label, value) in [
        ("Transfer uid", input.transfer_uid),
        ("origin Organ uid", input.origin_organ_uid),
        ("recipient Person uid", input.recipient_person_uid),
        ("recipient Organ uid", input.recipient_organ_uid),
        ("actor Person uid", input.actor_person_uid),
        ("Fact uid", input.fact_uid),
        ("request id", input.request_id),
    ] {
        required(label, value)?;
    }
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query(
        "SELECT p.*, e.kind AS event_kind, e.actor_person_uid AS event_actor,
                e.fact_uid AS event_fact
         FROM transfer_delivery_policy_event e
         JOIN transfer_delivery_policy p ON p.uid = e.delivery_uid
         WHERE e.request_id = ?",
    )
    .bind(input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let matching_event = row.get::<&str, _>("event_kind") == "created"
            && row.get::<&str, _>("event_actor") == input.actor_person_uid
            && row.get::<&str, _>("event_fact") == input.fact_uid;
        let existing = map_policy(row)?;
        if matching_event
            && existing.mode == input.mode
            && existing.transfer_uid == input.transfer_uid
            && existing.origin_organ_uid == input.origin_organ_uid
            && existing.recipient_person_uid == input.recipient_person_uid
            && existing.recipient_organ_uid == input.recipient_organ_uid
        {
            tx.rollback().await?;
            return Ok(PolicyCommit::Replayed(existing));
        }
        return Err(protocol(
            "delivery policy request id belongs to another recipient",
        ));
    }
    ensure_request_unused(&mut tx, input.request_id).await?;
    let authority: Option<String> = sqlx::query_scalar(
        "SELECT r.organ_uid FROM transfer t JOIN record r ON r.uid = t.record_uid
         WHERE t.record_uid = ?",
    )
    .bind(input.transfer_uid)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if authority.as_deref() != Some(input.origin_organ_uid) {
        return Err(protocol(
            "only the Transfer origin Organ may own delivery policy",
        ));
    }
    let fact: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT record_uid, actor_uid FROM fact WHERE uid = ?")
            .bind(input.fact_uid)
            .fetch_optional(&mut *tx)
            .await?;
    if fact.as_ref().map(|row| row.0.as_str()) != Some(input.transfer_uid)
        || fact.as_ref().and_then(|row| row.1.as_deref()) != Some(input.actor_person_uid)
    {
        return Err(protocol(
            "delivery policy evidence does not belong to its Transfer actor",
        ));
    }
    let uid = new_uid("tdp");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_delivery_policy
         (uid, transfer_uid, origin_organ_uid, recipient_person_uid, recipient_organ_uid,
          mode, state, revision, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 'active', 1, ?, ?)",
    )
    .bind(&uid)
    .bind(input.transfer_uid)
    .bind(input.origin_organ_uid)
    .bind(input.recipient_person_uid)
    .bind(input.recipient_organ_uid)
    .bind(input.mode.as_str())
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_delivery_policy_event
         (uid, delivery_uid, revision, kind, from_mode, to_mode, from_state, to_state,
          actor_person_uid, fact_uid, request_id, created_at)
         VALUES (?, ?, 1, 'created', NULL, ?, NULL, 'active', ?, ?, ?, ?)",
    )
    .bind(new_uid("tdpe"))
    .bind(&uid)
    .bind(input.mode.as_str())
    .bind(input.actor_person_uid)
    .bind(input.fact_uid)
    .bind(input.request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let created = policy(pool, &uid).await?.ok_or(sqlx::Error::RowNotFound)?;
    Ok(PolicyCommit::Applied(created))
}

/// Changes an active delivery policy's mode and records the transition.
///
/// # Errors
///
/// Returns an error if the policy is missing, stale, or revoked, the evidence
/// or request conflicts, or storage fails.
pub async fn change_policy_mode(
    pool: &SqlitePool,
    input: DeliveryPolicyTransition<'_>,
    mode: TransferDeliveryMode,
    now: DateTime<Utc>,
) -> Result<PolicyCommit, StoreError> {
    transition_policy(pool, input, Change::Mode(mode), now).await
}

/// Revokes a policy and cancels its queued or failed envelopes.
///
/// # Errors
///
/// Returns an error if the policy is missing, stale, or revoked, the evidence
/// or request conflicts, or storage fails.
pub async fn revoke_policy(
    pool: &SqlitePool,
    input: DeliveryPolicyTransition<'_>,
    now: DateTime<Utc>,
) -> Result<PolicyCommit, StoreError> {
    transition_policy(pool, input, Change::Revoke, now).await
}

enum Change {
    Mode(TransferDeliveryMode),
    Revoke,
}

async fn transition_policy(
    pool: &SqlitePool,
    input: DeliveryPolicyTransition<'_>,
    change: Change,
    now: DateTime<Utc>,
) -> Result<PolicyCommit, StoreError> {
    let (requested_mode, next_state, kind) = match change {
        Change::Mode(mode) => (Some(mode), "active", "mode_changed"),
        Change::Revoke => (None, "revoked", "revoked"),
    };
    let next_revision = i64::try_from(input.expected_revision)
        .ok()
        .and_then(|revision| revision.checked_add(1));
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query(
        "SELECT p.*, e.delivery_uid AS event_delivery_uid, e.revision AS event_revision,
                e.kind AS event_kind, e.to_mode AS event_to_mode,
                e.actor_person_uid AS event_actor, e.fact_uid AS event_fact
         FROM transfer_delivery_policy_event e
         JOIN transfer_delivery_policy p ON p.uid = e.delivery_uid WHERE e.request_id = ?",
    )
    .bind(input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let exact = row.get::<&str, _>("event_delivery_uid") == input.delivery_uid
            && next_revision == Some(row.get("event_revision"))
            && row.get::<&str, _>("event_kind") == kind
            && row.get::<&str, _>("event_actor") == input.actor_person_uid
            && row.get::<&str, _>("event_fact") == input.fact_uid
            && requested_mode
                .is_none_or(|mode| mode.as_str() == row.get::<&str, _>("event_to_mode"));
        if !exact {
            return Err(protocol(
                "delivery policy request id was replayed as another transition",
            ));
        }
        let existing = map_policy(row)?;
        tx.rollback().await?;
        return Ok(PolicyCommit::Replayed(existing));
    }
    ensure_request_unused(&mut tx, input.request_id).await?;
    let current = sqlx::query("SELECT * FROM transfer_delivery_policy WHERE uid = ?")
        .bind(input.delivery_uid)
        .try_map(map_policy)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.revision != input.expected_revision || current.state != "active" {
        return Err(protocol("stale or revoked Transfer delivery policy"));
    }
    let fact_actor: Option<String> =
        sqlx::query_scalar("SELECT actor_uid FROM fact WHERE uid = ? AND record_uid = ?")
            .bind(input.fact_uid)
            .bind(&current.transfer_uid)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    if fact_actor.as_deref() != Some(input.actor_person_uid) {
        return Err(protocol(
            "delivery policy transition Fact has the wrong actor",
        ));
    }
    let next_revision =
        next_revision.ok_or_else(|| protocol("delivery policy revision overflow"))?;
    let next_mode = requested_mode.unwrap_or(current.mode);
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_delivery_policy_event
         (uid, delivery_uid, revision, kind, from_mode, to_mode, from_state, to_state,
          actor_person_uid, fact_uid, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, ?)",
    )
    .bind(new_uid("tdpe"))
    .bind(input.delivery_uid)
    .bind(next_revision)
    .bind(kind)
    .bind(current.mode.as_str())
    .bind(next_mode.as_str())
    .bind(next_state)
    .bind(input.actor_person_uid)
    .bind(input.fact_uid)
    .bind(input.request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE transfer_delivery_policy SET mode = ?, state = ?, revision = ?, updated_at = ?
         WHERE uid = ?",
    )
    .bind(next_mode.as_str())
    .bind(next_state)
    .bind(next_revision)
    .bind(&at)
    .bind(input.delivery_uid)
    .execute(&mut *tx)
    .await?;
    if matches!(change, Change::Revoke) {
        sqlx::query(
            "UPDATE transfer_delivery_outbox SET status = 'cancelled', last_error = 'delivery_revoked'
             WHERE delivery_uid = ? AND status IN ('queued', 'failed')",
        )
        .bind(input.delivery_uid)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let updated = policy(pool, input.delivery_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(PolicyCommit::Applied(updated))
}
