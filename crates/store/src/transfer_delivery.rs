//! Persistent cross-Cell Transfer delivery state.
//!
//! Origin policies/outbox and recipient references/replicas are deliberately
//! separate. Replica payloads never enter the canonical Transfer tables.

use chrono::{DateTime, Duration, Utc};
use nucleus::{Cause, Fact, NewFact};
use nucleus::transfer_delivery::{
    SignedOrganRequestV1, TransferApplicationAttestationV1, TransferApplicationHandoffState,
    TransferDeliveryMode, TransferEnvelopeV1, TransferRemoteCommandV1,
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn required(label: &str, value: &str) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.len() > 1_024 {
        return Err(protocol(format!("{label} has an invalid length")));
    }
    Ok(())
}

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

pub struct DeliveryPolicyTransition<'a> {
    pub delivery_uid: &'a str,
    pub expected_revision: u64,
    pub actor_person_uid: &'a str,
    pub fact_uid: &'a str,
    pub request_id: &'a str,
}

#[derive(Debug, Clone)]
pub enum PolicyCommit {
    Applied(DeliveryPolicyRow),
    Replayed(DeliveryPolicyRow),
}

fn map_policy(row: sqlx::sqlite::SqliteRow) -> DeliveryPolicyRow {
    DeliveryPolicyRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        origin_organ_uid: row.get("origin_organ_uid"),
        recipient_person_uid: row.get("recipient_person_uid"),
        recipient_organ_uid: row.get("recipient_organ_uid"),
        mode: TransferDeliveryMode::parse(row.get::<String, _>("mode").as_str())
            .expect("database mode constraint"),
        state: row.get("state"),
        revision: row.get::<i64, _>("revision") as u64,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn policy(
    pool: &SqlitePool,
    delivery_uid: &str,
) -> Result<Option<DeliveryPolicyRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM transfer_delivery_policy WHERE uid = ?")
            .bind(delivery_uid)
            .fetch_optional(pool)
            .await?
            .map(map_policy),
    )
}

pub async fn policies_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<DeliveryPolicyRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_delivery_policy WHERE transfer_uid = ? ORDER BY created_at, uid",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_policy)
    .collect())
}

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
    let mut tx = pool.begin().await?;
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
        let event_kind = row.get::<String, _>("event_kind");
        let event_actor = row.get::<String, _>("event_actor");
        let event_fact = row.get::<String, _>("event_fact");
        let existing = map_policy(row);
        if event_kind == "created"
            && event_actor == input.actor_person_uid
            && event_fact == input.fact_uid
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
    ensure_existing_transfer_request_unused(pool, input.request_id).await?;
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
    let uid = nucleus::new_uid("tdp");
    let event_uid = nucleus::new_uid("tdpe");
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
    .bind(event_uid)
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

pub async fn change_policy_mode(
    pool: &SqlitePool,
    input: DeliveryPolicyTransition<'_>,
    mode: TransferDeliveryMode,
    now: DateTime<Utc>,
) -> Result<PolicyCommit, StoreError> {
    transition_policy(pool, input, Some(mode), false, now).await
}

pub async fn revoke_policy(
    pool: &SqlitePool,
    input: DeliveryPolicyTransition<'_>,
    now: DateTime<Utc>,
) -> Result<PolicyCommit, StoreError> {
    transition_policy(pool, input, None, true, now).await
}

async fn transition_policy(
    pool: &SqlitePool,
    input: DeliveryPolicyTransition<'_>,
    requested_mode: Option<TransferDeliveryMode>,
    revoke: bool,
    now: DateTime<Utc>,
) -> Result<PolicyCommit, StoreError> {
    let mut tx = pool.begin().await?;
    if let Some(row) = sqlx::query(
        "SELECT p.*, e.delivery_uid AS event_delivery_uid, e.revision AS event_revision,
                e.kind AS event_kind, e.from_mode AS event_from_mode, e.to_mode AS event_to_mode,
                e.actor_person_uid AS event_actor, e.fact_uid AS event_fact
         FROM transfer_delivery_policy_event e
         JOIN transfer_delivery_policy p ON p.uid = e.delivery_uid WHERE e.request_id = ?",
    )
    .bind(input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let expected_kind = if revoke { "revoked" } else { "mode_changed" };
        let expected_mode = requested_mode.map(TransferDeliveryMode::as_str);
        let exact = row.get::<String, _>("event_delivery_uid") == input.delivery_uid
            && row.get::<i64, _>("event_revision") == input.expected_revision as i64 + 1
            && row.get::<String, _>("event_kind") == expected_kind
            && row.get::<String, _>("event_actor") == input.actor_person_uid
            && row.get::<String, _>("event_fact") == input.fact_uid
            && (revoke
                || row.get::<Option<String>, _>("event_to_mode").as_deref() == expected_mode);
        if !exact {
            return Err(protocol(
                "delivery policy request id was replayed as another transition",
            ));
        }
        let existing = map_policy(row);
        tx.rollback().await?;
        return Ok(PolicyCommit::Replayed(existing));
    }
    ensure_existing_transfer_request_unused(pool, input.request_id).await?;
    let current = sqlx::query("SELECT * FROM transfer_delivery_policy WHERE uid = ?")
        .bind(input.delivery_uid)
        .fetch_optional(&mut *tx)
        .await?
        .map(map_policy)
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
    let next_revision = current.revision + 1;
    let next_mode = requested_mode.unwrap_or(current.mode);
    let next_state = if revoke { "revoked" } else { "active" };
    let kind = if revoke { "revoked" } else { "mode_changed" };
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_delivery_policy_event
         (uid, delivery_uid, revision, kind, from_mode, to_mode, from_state, to_state,
          actor_person_uid, fact_uid, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("tdpe"))
    .bind(input.delivery_uid)
    .bind(next_revision as i64)
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
    .bind(next_revision as i64)
    .bind(&at)
    .bind(input.delivery_uid)
    .execute(&mut *tx)
    .await?;
    if revoke {
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

#[derive(Debug, Clone)]
pub struct DeliveryOutboxRow {
    pub uid: String,
    pub envelope_uid: String,
    pub delivery_uid: String,
    pub cursor: u64,
    pub transfer_revision: u64,
    pub payload: String,
    pub payload_hash: String,
    pub status: String,
    pub attempts: u32,
    pub next_attempt_at: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum EnqueueCommit {
    Applied(DeliveryOutboxRow),
    Replayed(DeliveryOutboxRow),
}

fn map_outbox(row: sqlx::sqlite::SqliteRow) -> DeliveryOutboxRow {
    DeliveryOutboxRow {
        uid: row.get("uid"),
        envelope_uid: row.get("envelope_uid"),
        delivery_uid: row.get("delivery_uid"),
        cursor: row.get::<i64, _>("cursor") as u64,
        transfer_revision: row.get::<i64, _>("transfer_revision") as u64,
        payload: row.get("payload"),
        payload_hash: row.get("payload_hash"),
        status: row.get("status"),
        attempts: row.get::<i64, _>("attempts") as u32,
        next_attempt_at: row.get("next_attempt_at"),
        last_error: row.get("last_error"),
    }
}

pub async fn enqueue(
    pool: &SqlitePool,
    delivery_uid: &str,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
) -> Result<EnqueueCommit, StoreError> {
    envelope.validate_shape().map_err(protocol)?;
    let payload = serde_json::to_string(envelope).map_err(|error| protocol(error.to_string()))?;
    let mut tx = pool.begin().await?;
    if let Some(row) = sqlx::query("SELECT * FROM transfer_delivery_outbox WHERE envelope_uid = ?")
        .bind(&envelope.envelope_uid)
        .fetch_optional(&mut *tx)
        .await?
    {
        let existing = map_outbox(row);
        if existing.delivery_uid == delivery_uid
            && existing.payload_hash == envelope.payload_hash
            && existing.payload == payload
        {
            tx.rollback().await?;
            return Ok(EnqueueCommit::Replayed(existing));
        }
        return Err(protocol(
            "Transfer envelope uid was replayed with changed contents",
        ));
    }
    let delivery = sqlx::query("SELECT * FROM transfer_delivery_policy WHERE uid = ?")
        .bind(delivery_uid)
        .fetch_optional(&mut *tx)
        .await?
        .map(map_policy)
        .ok_or(sqlx::Error::RowNotFound)?;
    if delivery.state != "active"
        || delivery.mode != envelope.mode
        || envelope.delivery_policy_uid != delivery.uid
        || envelope.delivery_policy_revision != delivery.revision
        || envelope.delivery_policy_state != delivery.state
        || delivery.origin_organ_uid != envelope.origin_organ_uid
        || delivery.transfer_uid != envelope.transfer_uid
        || delivery.recipient_person_uid != envelope.recipient_person_uid
        || delivery.recipient_organ_uid != envelope.recipient_organ_uid
    {
        return Err(protocol(
            "Transfer envelope does not match active delivery policy",
        ));
    }
    let last: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(cursor), 0) FROM transfer_delivery_outbox WHERE delivery_uid = ?",
    )
    .bind(delivery_uid)
    .fetch_one(&mut *tx)
    .await?;
    if envelope.cursor <= last as u64 {
        return Err(protocol("Transfer envelope cursor is not monotonic"));
    }
    let uid = nucleus::new_uid("tdo");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_delivery_outbox
         (uid, envelope_uid, delivery_uid, cursor, transfer_revision, payload, payload_hash,
          next_attempt_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(&envelope.envelope_uid)
    .bind(delivery_uid)
    .bind(envelope.cursor as i64)
    .bind(envelope.transfer_revision as i64)
    .bind(&payload)
    .bind(&envelope.payload_hash)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row = sqlx::query("SELECT * FROM transfer_delivery_outbox WHERE uid = ?")
        .bind(&uid)
        .fetch_one(pool)
        .await?;
    Ok(EnqueueCommit::Applied(map_outbox(row)))
}

pub async fn outbox_due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<DeliveryOutboxRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT o.* FROM transfer_delivery_outbox o
         JOIN transfer_delivery_policy p ON p.uid = o.delivery_uid
         WHERE o.status IN ('queued', 'failed') AND o.next_attempt_at <= ? AND p.state = 'active'
         ORDER BY o.next_attempt_at, o.created_at, o.uid LIMIT ?",
    )
    .bind(now.to_rfc3339())
    .bind(i64::from(limit.max(1)))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_outbox)
    .collect())
}

pub async fn outbox_mark_failed(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
    error: &str,
    base_delay_seconds: u32,
    max_delay_seconds: u32,
) -> Result<String, StoreError> {
    let attempts: i64 = sqlx::query_scalar(
        "SELECT attempts FROM transfer_delivery_outbox WHERE uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(uid)
    .fetch_one(pool)
    .await?;
    let exponent = u32::try_from(attempts).unwrap_or(u32::MAX).min(20);
    let delay = u64::from(base_delay_seconds.max(1))
        .saturating_mul(1_u64 << exponent)
        .min(u64::from(max_delay_seconds.max(base_delay_seconds.max(1))));
    let next = now + Duration::seconds(i64::try_from(delay).unwrap_or(i64::MAX));
    let next_string = next.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_delivery_outbox
         SET status = 'failed', attempts = attempts + 1, last_attempt_at = ?,
             next_attempt_at = ?, last_error = ?
         WHERE uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(now.to_rfc3339())
    .bind(&next_string)
    .bind(error)
    .bind(uid)
    .execute(pool)
    .await?;
    Ok(next_string)
}

pub async fn outbox_mark_sent(
    pool: &SqlitePool,
    uid: &str,
    acknowledged_cursor: u64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    if let Some((status, cursor, acknowledged)) = sqlx::query_as::<_, (String, i64, Option<i64>)>(
        "SELECT status, cursor, acknowledged_cursor FROM transfer_delivery_outbox WHERE uid = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
        && status == "sent"
    {
        if cursor <= acknowledged_cursor as i64 && acknowledged == Some(acknowledged_cursor as i64)
        {
            return Ok(());
        }
        return Err(protocol("outbox acknowledgement changed on replay"));
    }
    let result = sqlx::query(
        "UPDATE transfer_delivery_outbox
         SET status = 'sent', attempts = attempts + 1, last_attempt_at = ?, sent_at = ?,
             acknowledged_cursor = ?, last_error = NULL
         WHERE uid = ? AND status IN ('queued', 'failed') AND cursor <= ?",
    )
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(acknowledged_cursor as i64)
    .bind(uid)
    .bind(acknowledged_cursor as i64)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol(
            "outbox acknowledgement is stale or below its envelope cursor",
        ));
    }
    Ok(())
}

pub async fn cancel_outbox(pool: &SqlitePool, delivery_uid: &str) -> Result<u64, StoreError> {
    Ok(sqlx::query(
        "UPDATE transfer_delivery_outbox SET status = 'cancelled', last_error = 'cancelled'
         WHERE delivery_uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(delivery_uid)
    .execute(pool)
    .await?
    .rows_affected())
}

pub async fn retry_outbox(
    pool: &SqlitePool,
    delivery_uid: &str,
    request_id: &str,
    now: DateTime<Utc>,
) -> Result<DeliveryOutboxRow, StoreError> {
    let mut tx = pool.begin().await?;
    if let Some(row) = sqlx::query(
        "SELECT o.* FROM transfer_delivery_retry_event e
         JOIN transfer_delivery_outbox o ON o.uid = e.outbox_uid WHERE e.request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let existing = map_outbox(row);
        if existing.delivery_uid == delivery_uid {
            tx.rollback().await?;
            return Ok(existing);
        }
        return Err(protocol(
            "Transfer delivery retry request id belongs to another delivery",
        ));
    }
    let active: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_delivery_policy WHERE uid = ? AND state = 'active')",
    )
    .bind(delivery_uid)
    .fetch_one(&mut *tx)
    .await?;
    if active == 0 {
        return Err(protocol(
            "revoked or missing Transfer delivery cannot be retried",
        ));
    }
    let outbox_uid: String = sqlx::query_scalar(
        "SELECT uid FROM transfer_delivery_outbox
         WHERE delivery_uid = ? AND status = 'failed'
         ORDER BY created_at DESC, uid DESC LIMIT 1",
    )
    .bind(delivery_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| protocol("Transfer delivery has no failed envelope to retry"))?;
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_delivery_retry_event (uid, delivery_uid, outbox_uid, request_id, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("tdre"))
    .bind(delivery_uid)
    .bind(&outbox_uid)
    .bind(request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE transfer_delivery_outbox SET status = 'queued', next_attempt_at = ?, last_error = NULL
         WHERE uid = ? AND status = 'failed'",
    )
    .bind(&at)
    .bind(&outbox_uid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(
        sqlx::query("SELECT * FROM transfer_delivery_outbox WHERE uid = ?")
            .bind(outbox_uid)
            .fetch_one(pool)
            .await
            .map(map_outbox)?,
    )
}

pub struct NewRemoteReference<'a> {
    pub origin_organ_uid: &'a str,
    pub transfer_uid: &'a str,
    pub delivery_policy_uid: &'a str,
    pub recipient_person_uid: &'a str,
    pub recipient_organ_uid: &'a str,
    /// A replicated initial policy is accepted only after the caller verifies
    /// the origin's explicit signed policy event.
    pub mode: TransferDeliveryMode,
    pub policy_revision: u64,
    pub hosted_url: Option<&'a str>,
    pub policy_payload_hash: &'a str,
    pub signed_policy_payload: &'a serde_json::Value,
    pub envelope_uid: Option<&'a str>,
}

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

fn map_reference(row: sqlx::sqlite::SqliteRow) -> RemoteReferenceRow {
    RemoteReferenceRow {
        uid: row.get("uid"),
        origin_organ_uid: row.get("origin_organ_uid"),
        transfer_uid: row.get("transfer_uid"),
        delivery_policy_uid: row.get("delivery_policy_uid"),
        recipient_person_uid: row.get("recipient_person_uid"),
        recipient_organ_uid: row.get("recipient_organ_uid"),
        mode: TransferDeliveryMode::parse(row.get::<String, _>("mode").as_str())
            .expect("database mode constraint"),
        state: row.get("state"),
        policy_revision: row.get::<i64, _>("policy_revision") as u64,
        last_cursor: row.get::<i64, _>("last_cursor") as u64,
        last_transfer_revision: row.get::<i64, _>("last_transfer_revision") as u64,
        last_envelope_uid: row.get("last_envelope_uid"),
        last_payload_hash: row.get("last_payload_hash"),
        projection: row.get("projection"),
        disclosure: row.get("disclosure"),
    }
}

pub async fn create_remote_reference(
    pool: &SqlitePool,
    input: NewRemoteReference<'_>,
    now: DateTime<Utc>,
) -> Result<RemoteReferenceRow, StoreError> {
    let uid = nucleus::new_uid("trr");
    let at = now.to_rfc3339();
    let signed_payload = serde_json::to_string(input.signed_policy_payload)
        .map_err(|error| protocol(error.to_string()))?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO transfer_remote_reference
         (uid, origin_organ_uid, transfer_uid, delivery_policy_uid, recipient_person_uid, recipient_organ_uid,
          mode, state, policy_revision, hosted_url, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'active', ?, ?, ?, ?)
         ON CONFLICT(origin_organ_uid, transfer_uid, recipient_person_uid, recipient_organ_uid)
         DO UPDATE SET hosted_url = COALESCE(excluded.hosted_url, hosted_url), updated_at = excluded.updated_at",
    )
    .bind(&uid)
    .bind(input.origin_organ_uid)
    .bind(input.transfer_uid)
    .bind(input.delivery_policy_uid)
    .bind(input.recipient_person_uid)
    .bind(input.recipient_organ_uid)
    .bind(input.mode.as_str())
    .bind(input.policy_revision as i64)
    .bind(input.hosted_url)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    let reference_uid: String = sqlx::query_scalar(
        "SELECT uid FROM transfer_remote_reference WHERE origin_organ_uid = ? AND transfer_uid = ?
         AND recipient_person_uid = ? AND recipient_organ_uid = ?",
    )
    .bind(input.origin_organ_uid)
    .bind(input.transfer_uid)
    .bind(input.recipient_person_uid)
    .bind(input.recipient_organ_uid)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(row) = sqlx::query(
        "SELECT envelope_uid, payload_hash, signed_payload FROM transfer_remote_policy_event
         WHERE reference_uid = ? AND policy_revision = ?",
    )
    .bind(&reference_uid)
    .bind(input.policy_revision as i64)
    .fetch_optional(&mut *tx)
    .await?
    {
        if row.get::<Option<String>, _>("envelope_uid").as_deref() != input.envelope_uid
            || row.get::<String, _>("payload_hash") != input.policy_payload_hash
            || row.get::<String, _>("signed_payload") != signed_payload
        {
            return Err(protocol(
                "hosted reference policy was replayed with changed contents",
            ));
        }
        tx.rollback().await?;
        return remote_reference_by_identity(
            pool,
            input.origin_organ_uid,
            input.transfer_uid,
            input.recipient_person_uid,
            input.recipient_organ_uid,
        )
        .await?
        .ok_or(sqlx::Error::RowNotFound);
    }
    sqlx::query(
        "INSERT INTO transfer_remote_policy_event
         (uid, reference_uid, policy_revision, kind, mode, state, envelope_uid,
          payload_hash, signed_payload, received_at)
         VALUES (?, ?, ?, 'reference', ?, 'active', ?, ?, ?, ?)
         ON CONFLICT(reference_uid, policy_revision) DO NOTHING",
    )
    .bind(nucleus::new_uid("trpe"))
    .bind(&reference_uid)
    .bind(input.policy_revision as i64)
    .bind(input.mode.as_str())
    .bind(input.envelope_uid)
    .bind(input.policy_payload_hash)
    .bind(signed_payload)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    remote_reference_by_identity(
        pool,
        input.origin_organ_uid,
        input.transfer_uid,
        input.recipient_person_uid,
        input.recipient_organ_uid,
    )
    .await?
    .ok_or(sqlx::Error::RowNotFound)
}

pub async fn remote_reference_by_identity(
    pool: &SqlitePool,
    origin_organ_uid: &str,
    transfer_uid: &str,
    recipient_person_uid: &str,
    recipient_organ_uid: &str,
) -> Result<Option<RemoteReferenceRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_remote_reference
         WHERE origin_organ_uid = ? AND transfer_uid = ?
           AND recipient_person_uid = ? AND recipient_organ_uid = ?",
    )
    .bind(origin_organ_uid)
    .bind(transfer_uid)
    .bind(recipient_person_uid)
    .bind(recipient_organ_uid)
    .fetch_optional(pool)
    .await?
    .map(map_reference))
}

pub async fn remote_reference_by_uid(
    pool: &SqlitePool,
    reference_uid: &str,
) -> Result<Option<RemoteReferenceRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
        .bind(reference_uid)
        .fetch_optional(pool)
        .await?
        .map(map_reference))
}

#[derive(Debug, Clone)]
pub struct PullRequestRow {
    pub uid: String,
    pub reference_uid: String,
    pub request_id: String,
    pub after_cursor: u64,
    pub status: String,
    pub attempts: u32,
    pub next_attempt_at: String,
    pub last_error: Option<String>,
}

fn map_pull(row: sqlx::sqlite::SqliteRow) -> PullRequestRow {
    PullRequestRow {
        uid: row.get("uid"),
        reference_uid: row.get("reference_uid"),
        request_id: row.get("request_id"),
        after_cursor: row.get::<i64, _>("after_cursor") as u64,
        status: row.get("status"),
        attempts: row.get::<i64, _>("attempts") as u32,
        next_attempt_at: row.get("next_attempt_at"),
        last_error: row.get("last_error"),
    }
}

pub async fn enqueue_pull(
    pool: &SqlitePool,
    reference_uid: &str,
    request_id: &str,
    now: DateTime<Utc>,
) -> Result<PullRequestRow, StoreError> {
    if let Some(row) =
        sqlx::query("SELECT * FROM transfer_delivery_pull_request WHERE request_id = ?")
            .bind(request_id)
            .fetch_optional(pool)
            .await?
    {
        let existing = map_pull(row);
        if existing.reference_uid == reference_uid {
            return Ok(existing);
        }
        return Err(protocol(
            "Transfer pull request id belongs to another reference",
        ));
    }
    let reference = sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
        .bind(reference_uid)
        .fetch_optional(pool)
        .await?
        .map(map_reference)
        .ok_or(sqlx::Error::RowNotFound)?;
    if reference.state != "active" {
        return Err(protocol("revoked Transfer reference cannot be refreshed"));
    }
    let uid = nucleus::new_uid("tdpr");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_delivery_pull_request
         (uid, reference_uid, request_id, after_cursor, next_attempt_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(reference_uid)
    .bind(request_id)
    .bind(reference.last_cursor as i64)
    .bind(&at)
    .bind(&at)
    .execute(pool)
    .await?;
    Ok(
        sqlx::query("SELECT * FROM transfer_delivery_pull_request WHERE uid = ?")
            .bind(uid)
            .fetch_one(pool)
            .await
            .map(map_pull)?,
    )
}

pub async fn pulls_due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<PullRequestRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT q.* FROM transfer_delivery_pull_request q
         JOIN transfer_remote_reference r ON r.uid = q.reference_uid
         WHERE q.status IN ('queued', 'failed') AND q.next_attempt_at <= ? AND r.state = 'active'
         ORDER BY q.next_attempt_at, q.created_at, q.uid LIMIT ?",
    )
    .bind(now.to_rfc3339())
    .bind(i64::from(limit.max(1)))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_pull)
    .collect())
}

pub async fn pull_mark_failed(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
    error: &str,
    base_delay_seconds: u32,
    max_delay_seconds: u32,
) -> Result<String, StoreError> {
    let attempts: i64 = sqlx::query_scalar(
        "SELECT attempts FROM transfer_delivery_pull_request
         WHERE uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(uid)
    .fetch_one(pool)
    .await?;
    let exponent = u32::try_from(attempts).unwrap_or(u32::MAX).min(20);
    let delay = u64::from(base_delay_seconds.max(1))
        .saturating_mul(1_u64 << exponent)
        .min(u64::from(max_delay_seconds.max(base_delay_seconds.max(1))));
    let next = (now + Duration::seconds(i64::try_from(delay).unwrap_or(i64::MAX))).to_rfc3339();
    sqlx::query(
        "UPDATE transfer_delivery_pull_request SET status = 'failed', attempts = attempts + 1,
          next_attempt_at = ?, last_attempt_at = ?, last_error = ?
         WHERE uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(&next)
    .bind(now.to_rfc3339())
    .bind(error)
    .bind(uid)
    .execute(pool)
    .await?;
    Ok(next)
}

pub async fn pull_mark_completed(
    pool: &SqlitePool,
    uid: &str,
    completed_cursor: u64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let result = sqlx::query(
        "UPDATE transfer_delivery_pull_request SET status = 'completed', attempts = attempts + 1,
          last_attempt_at = ?, completed_cursor = ?, completed_at = ?, last_error = NULL
         WHERE uid = ? AND status IN ('queued', 'failed') AND after_cursor <= ?",
    )
    .bind(now.to_rfc3339())
    .bind(completed_cursor as i64)
    .bind(now.to_rfc3339())
    .bind(uid)
    .bind(completed_cursor as i64)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol(
            "Transfer pull completion is stale or below its starting cursor",
        ));
    }
    Ok(())
}

/// Applies a verified origin policy event. Replication therefore cannot be
/// enabled by an unverified first reference: callers create hosted first, then
/// apply the explicit next revision.
pub async fn apply_remote_policy(
    pool: &SqlitePool,
    reference_uid: &str,
    expected_revision: u64,
    mode: TransferDeliveryMode,
    revoked: bool,
    envelope_uid: Option<&str>,
    payload_hash: &str,
    signed_payload: &serde_json::Value,
    now: DateTime<Utc>,
) -> Result<RemoteReferenceRow, StoreError> {
    let next = expected_revision + 1;
    let payload =
        serde_json::to_string(signed_payload).map_err(|error| protocol(error.to_string()))?;
    let mut tx = pool.begin().await?;
    if let Some(row) = sqlx::query(
        "SELECT mode, state, envelope_uid, payload_hash, signed_payload FROM transfer_remote_policy_event
         WHERE reference_uid = ? AND policy_revision = ?",
    )
    .bind(reference_uid)
    .bind(next as i64)
    .fetch_optional(&mut *tx)
    .await?
    {
        if row.get::<String, _>("mode") != mode.as_str()
            || row.get::<String, _>("state") != if revoked { "revoked" } else { "active" }
            || row.get::<Option<String>, _>("envelope_uid").as_deref() != envelope_uid
            || row.get::<String, _>("payload_hash") != payload_hash
            || row.get::<String, _>("signed_payload") != payload
        {
            return Err(protocol("remote policy revision was replayed with changed contents"));
        }
        tx.rollback().await?;
        return Ok(sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
            .bind(reference_uid)
            .fetch_one(pool)
            .await
            .map(map_reference)?);
    }
    let result = sqlx::query(
        "UPDATE transfer_remote_reference SET mode = ?, state = ?, policy_revision = ?, updated_at = ?
         WHERE uid = ? AND policy_revision = ? AND state = 'active'",
    )
    .bind(mode.as_str())
    .bind(if revoked { "revoked" } else { "active" })
    .bind(next as i64)
    .bind(now.to_rfc3339())
    .bind(reference_uid)
    .bind(expected_revision as i64)
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
    .bind(nucleus::new_uid("trpe"))
    .bind(reference_uid)
    .bind(next as i64)
    .bind(if revoked { "revoked" } else { "mode_changed" })
    .bind(mode.as_str())
    .bind(if revoked { "revoked" } else { "active" })
    .bind(envelope_uid)
    .bind(payload_hash)
    .bind(payload)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(
        sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
            .bind(reference_uid)
            .fetch_one(pool)
            .await
            .map(map_reference)?,
    )
}

#[derive(Debug, Clone)]
pub enum ReplicaCommit {
    Applied(RemoteReferenceRow),
    Replayed(RemoteReferenceRow),
}

pub async fn accept_replica_envelope(
    pool: &SqlitePool,
    reference_uid: &str,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
) -> Result<ReplicaCommit, StoreError> {
    envelope.validate_shape().map_err(protocol)?;
    let payload = serde_json::to_string(envelope).map_err(|error| protocol(error.to_string()))?;
    let projection =
        serde_json::to_string(&envelope.projection).map_err(|error| protocol(error.to_string()))?;
    let disclosure =
        serde_json::to_string(&envelope.disclosure).map_err(|error| protocol(error.to_string()))?;
    let mut tx = pool.begin().await?;
    let reference = sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
        .bind(reference_uid)
        .fetch_optional(&mut *tx)
        .await?
        .map(map_reference)
        .ok_or(sqlx::Error::RowNotFound)?;
    if reference.mode != TransferDeliveryMode::Replicated
        || reference.state != "active"
        || envelope.delivery_policy_uid != reference.delivery_policy_uid
        || envelope.delivery_policy_revision != reference.policy_revision
        || envelope.delivery_policy_state != reference.state
        || reference.origin_organ_uid != envelope.origin_organ_uid
        || reference.transfer_uid != envelope.transfer_uid
        || reference.recipient_person_uid != envelope.recipient_person_uid
        || reference.recipient_organ_uid != envelope.recipient_organ_uid
        || envelope.mode != TransferDeliveryMode::Replicated
    {
        return Err(protocol(
            "replica envelope does not match an active replicated reference",
        ));
    }
    if let Some((hash, existing_reference)) = sqlx::query_as::<_, (String, String)>(
        "SELECT payload_hash, reference_uid FROM transfer_replica_envelope WHERE envelope_uid = ?",
    )
    .bind(&envelope.envelope_uid)
    .fetch_optional(&mut *tx)
    .await?
    {
        if hash != envelope.payload_hash || existing_reference != reference_uid {
            return Err(protocol(
                "replica envelope uid was replayed with changed contents",
            ));
        }
        tx.rollback().await?;
        return Ok(ReplicaCommit::Replayed(reference));
    }
    if envelope.cursor <= reference.last_cursor {
        return Err(protocol(
            "replica envelope cursor conflicts with accepted history",
        ));
    }
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
    .bind(envelope.cursor as i64)
    .bind(envelope.transfer_revision as i64)
    .bind(payload)
    .bind(&envelope.payload_hash)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE transfer_remote_reference
         SET last_cursor = ?, last_transfer_revision = ?, last_envelope_uid = ?,
             last_payload_hash = ?, projection = ?, disclosure = ?, last_fetched_at = ?,
             last_error = NULL, updated_at = ?
         WHERE uid = ?",
    )
    .bind(envelope.cursor as i64)
    .bind(envelope.transfer_revision as i64)
    .bind(&envelope.envelope_uid)
    .bind(&envelope.payload_hash)
    .bind(projection)
    .bind(disclosure)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(reference_uid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let updated = sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
        .bind(reference_uid)
        .fetch_one(pool)
        .await
        .map(map_reference)?;
    Ok(ReplicaCommit::Applied(updated))
}

/// Advances a verified hosted reference without retaining replica history.
pub async fn accept_hosted_snapshot(
    pool: &SqlitePool,
    reference_uid: &str,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
) -> Result<ReplicaCommit, StoreError> {
    envelope.validate_shape().map_err(protocol)?;
    let projection =
        serde_json::to_string(&envelope.projection).map_err(|error| protocol(error.to_string()))?;
    let disclosure =
        serde_json::to_string(&envelope.disclosure).map_err(|error| protocol(error.to_string()))?;
    let mut tx = pool.begin().await?;
    let reference = sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
        .bind(reference_uid)
        .fetch_optional(&mut *tx)
        .await?
        .map(map_reference)
        .ok_or(sqlx::Error::RowNotFound)?;
    if reference.mode != TransferDeliveryMode::Hosted
        || reference.state != "active"
        || envelope.delivery_policy_uid != reference.delivery_policy_uid
        || envelope.delivery_policy_revision != reference.policy_revision
        || envelope.delivery_policy_state != reference.state
        || envelope.mode != TransferDeliveryMode::Hosted
        || reference.origin_organ_uid != envelope.origin_organ_uid
        || reference.transfer_uid != envelope.transfer_uid
        || reference.recipient_person_uid != envelope.recipient_person_uid
        || reference.recipient_organ_uid != envelope.recipient_organ_uid
    {
        return Err(protocol(
            "hosted snapshot does not match its active reference",
        ));
    }
    if envelope.cursor == reference.last_cursor {
        if reference.last_envelope_uid.as_deref() == Some(envelope.envelope_uid.as_str())
            && reference.last_payload_hash.as_deref() == Some(envelope.payload_hash.as_str())
        {
            tx.rollback().await?;
            return Ok(ReplicaCommit::Replayed(reference));
        }
        return Err(protocol(
            "hosted snapshot cursor was replayed with changed contents",
        ));
    }
    if envelope.cursor < reference.last_cursor {
        return Err(protocol(
            "hosted snapshot cursor conflicts with accepted history",
        ));
    }
    sqlx::query(
        "UPDATE transfer_remote_reference
         SET last_cursor = ?, last_transfer_revision = ?, last_envelope_uid = ?,
             last_payload_hash = ?, projection = ?, disclosure = ?, last_fetched_at = ?,
             last_error = NULL, updated_at = ? WHERE uid = ?",
    )
    .bind(envelope.cursor as i64)
    .bind(envelope.transfer_revision as i64)
    .bind(&envelope.envelope_uid)
    .bind(&envelope.payload_hash)
    .bind(projection)
    .bind(disclosure)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(reference_uid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let updated = sqlx::query("SELECT * FROM transfer_remote_reference WHERE uid = ?")
        .bind(reference_uid)
        .fetch_one(pool)
        .await
        .map(map_reference)?;
    Ok(ReplicaCommit::Applied(updated))
}

pub struct NewPackageReceipt<'a> {
    pub delivery_uid: &'a str,
    pub envelope_uid: &'a str,
    pub cursor: u64,
    pub kind: &'a str,
    pub actor_organ_uid: &'a str,
    pub payload_hash: &'a str,
    pub key_id: &'a str,
    pub signature: &'a str,
    pub signed_payload: &'a serde_json::Value,
    pub local_fact_uid: Option<&'a str>,
    pub request_id: &'a str,
}

pub async fn record_package_receipt(
    pool: &SqlitePool,
    input: NewPackageReceipt<'_>,
    now: DateTime<Utc>,
) -> Result<String, StoreError> {
    if !matches!(input.kind, "received" | "seen") || input.cursor == 0 {
        return Err(protocol("invalid Transfer package receipt"));
    }
    if let Some(row) = sqlx::query("SELECT * FROM transfer_delivery_receipt WHERE request_id = ?")
        .bind(input.request_id)
        .fetch_optional(pool)
        .await?
    {
        let payload = serde_json::to_string(input.signed_payload)
            .map_err(|error| protocol(error.to_string()))?;
        if row.get::<String, _>("delivery_uid") == input.delivery_uid
            && row.get::<String, _>("envelope_uid") == input.envelope_uid
            && row.get::<i64, _>("cursor") == input.cursor as i64
            && row.get::<String, _>("kind") == input.kind
            && row.get::<String, _>("actor_organ_uid") == input.actor_organ_uid
            && row.get::<String, _>("payload_hash") == input.payload_hash
            && row.get::<String, _>("key_id") == input.key_id
            && row.get::<String, _>("signature") == input.signature
            && row.get::<String, _>("signed_payload") == payload
        {
            return Ok(row.get("uid"));
        }
        return Err(protocol(
            "package receipt request id was replayed with changed contents",
        ));
    }
    let payload =
        serde_json::to_string(input.signed_payload).map_err(|error| protocol(error.to_string()))?;
    if let Some(row) = sqlx::query(
        "SELECT * FROM transfer_delivery_receipt
         WHERE delivery_uid = ? AND envelope_uid = ? AND kind = ? AND actor_organ_uid = ?",
    )
    .bind(input.delivery_uid)
    .bind(input.envelope_uid)
    .bind(input.kind)
    .bind(input.actor_organ_uid)
    .fetch_optional(pool)
    .await?
    {
        if row.get::<i64, _>("cursor") == input.cursor as i64
            && row.get::<String, _>("payload_hash") == input.payload_hash
            && row.get::<String, _>("key_id") == input.key_id
            && row.get::<String, _>("signature") == input.signature
            && row.get::<String, _>("signed_payload") == payload
        {
            return Ok(row.get("uid"));
        }
        return Err(protocol(
            "package receipt identity was replayed with changed proof",
        ));
    }
    let uid = nucleus::new_uid("tdr");
    sqlx::query(
        "INSERT INTO transfer_delivery_receipt
         (uid, delivery_uid, envelope_uid, cursor, kind, actor_organ_uid, payload_hash,
          key_id, signature, signed_payload, local_fact_uid, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.delivery_uid)
    .bind(input.envelope_uid)
    .bind(input.cursor as i64)
    .bind(input.kind)
    .bind(input.actor_organ_uid)
    .bind(input.payload_hash)
    .bind(input.key_id)
    .bind(input.signature)
    .bind(payload)
    .bind(input.local_fact_uid)
    .bind(input.request_id)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(uid)
}

pub struct NewRemoteConflict<'a> {
    pub origin_organ_uid: &'a str,
    pub transfer_uid: &'a str,
    pub recipient_person_uid: &'a str,
    pub command_uid: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub envelope_uid: Option<&'a str>,
    pub submitted_revision: Option<u64>,
    pub authoritative_revision: u64,
    pub authoritative_cursor: u64,
    pub code: &'a str,
    pub reviewed_payload: &'a serde_json::Value,
}

pub async fn record_remote_conflict(
    pool: &SqlitePool,
    input: NewRemoteConflict<'_>,
    now: DateTime<Utc>,
) -> Result<String, StoreError> {
    if input.command_uid.is_none() && input.request_id.is_none() && input.envelope_uid.is_none() {
        return Err(protocol(
            "remote conflict requires a command, request, or envelope uid",
        ));
    }
    let payload = serde_json::to_string(input.reviewed_payload)
        .map_err(|error| protocol(error.to_string()))?;
    if let Some(command_uid) = input.command_uid
        && let Some(row) = sqlx::query(
            "SELECT * FROM transfer_remote_conflict WHERE origin_organ_uid = ? AND command_uid = ?",
        )
        .bind(input.origin_organ_uid)
        .bind(command_uid)
        .fetch_optional(pool)
        .await?
    {
        let exact = row.get::<String, _>("transfer_uid") == input.transfer_uid
            && row.get::<String, _>("recipient_person_uid") == input.recipient_person_uid
            && row.get::<Option<String>, _>("request_id").as_deref() == input.request_id
            && row.get::<Option<String>, _>("envelope_uid").as_deref() == input.envelope_uid
            && row.get::<Option<i64>, _>("submitted_revision")
                == input.submitted_revision.map(|value| value as i64)
            && row.get::<i64, _>("authoritative_revision") == input.authoritative_revision as i64
            && row.get::<i64, _>("authoritative_cursor") == input.authoritative_cursor as i64
            && row.get::<String, _>("code") == input.code
            && row.get::<String, _>("reviewed_payload") == payload;
        if exact {
            return Ok(row.get("uid"));
        }
        return Err(protocol(
            "remote conflict command was replayed with changed contents",
        ));
    }
    let uid = nucleus::new_uid("trc");
    sqlx::query(
        "INSERT INTO transfer_remote_conflict
         (uid, origin_organ_uid, transfer_uid, recipient_person_uid, command_uid, request_id,
          envelope_uid, submitted_revision, authoritative_revision, authoritative_cursor,
          code, reviewed_payload, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.origin_organ_uid)
    .bind(input.transfer_uid)
    .bind(input.recipient_person_uid)
    .bind(input.command_uid)
    .bind(input.request_id)
    .bind(input.envelope_uid)
    .bind(input.submitted_revision.map(|value| value as i64))
    .bind(input.authoritative_revision as i64)
    .bind(input.authoritative_cursor as i64)
    .bind(input.code)
    .bind(payload)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(uid)
}

#[derive(Debug, Clone)]
pub struct RemoteCommandRow {
    pub command_uid: String,
    pub request_id: String,
    pub direction: String,
    pub origin_organ_uid: String,
    pub sender_organ_uid: String,
    pub transfer_uid: String,
    pub actor_person_uid: String,
    pub expected_revision: Option<u64>,
    pub payload: String,
    pub payload_hash: String,
    pub status: String,
    pub attempts: u32,
    pub next_attempt_at: String,
    pub last_error_code: Option<String>,
    pub last_error: Option<String>,
    pub result_payload: Option<String>,
    pub authoritative_revision: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum RemoteCommandCommit {
    Applied(RemoteCommandRow),
    Replayed(RemoteCommandRow),
}

fn map_remote_command(row: sqlx::sqlite::SqliteRow) -> RemoteCommandRow {
    RemoteCommandRow {
        command_uid: row.get("command_uid"),
        request_id: row.get("request_id"),
        direction: row.get("direction"),
        origin_organ_uid: row.get("origin_organ_uid"),
        sender_organ_uid: row.get("sender_organ_uid"),
        transfer_uid: row.get("transfer_uid"),
        actor_person_uid: row.get("actor_person_uid"),
        expected_revision: row
            .get::<Option<i64>, _>("expected_revision")
            .map(|value| value as u64),
        payload: row.get("payload"),
        payload_hash: row.get("payload_hash"),
        status: row.get("status"),
        attempts: row.get::<i64, _>("attempts") as u32,
        next_attempt_at: row.get("next_attempt_at"),
        last_error_code: row.get("last_error_code"),
        last_error: row.get("last_error"),
        result_payload: row.get("result_payload"),
        authoritative_revision: row
            .get::<Option<i64>, _>("authoritative_revision")
            .map(|value| value as u64),
    }
}

pub async fn link_pending_remote_command_fact(
    tx: &mut Transaction<'_, Sqlite>,
    command_uid: &str,
    fact: &Fact,
) -> Result<(), StoreError> {
    let row = sqlx::query(
        "SELECT actor_person_uid, direction, status FROM transfer_remote_command
         WHERE command_uid = ?",
    )
    .bind(command_uid)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let direction = row.get::<String, _>("direction");
    let status = row.get::<String, _>("status");
    let actor = row.get::<String, _>("actor_person_uid");
    if direction != "incoming"
        || !matches!(status.as_str(), "queued" | "sent")
        || fact.actor_uid.as_deref() != Some(actor.as_str())
    {
        return Err(protocol(
            "remote command cannot authorize this Fact actor or state",
        ));
    }
    sqlx::query("INSERT INTO fact_remote_command (fact_uid, command_uid) VALUES (?, ?)")
        .bind(&fact.uid)
        .bind(command_uid)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn remote_command_for_fact(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<RemoteCommandRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.* FROM fact_remote_command l
         JOIN transfer_remote_command c ON c.command_uid = l.command_uid WHERE l.fact_uid = ?",
    )
    .bind(fact_uid)
    .fetch_optional(pool)
    .await?
    .map(map_remote_command))
}

pub async fn persist_remote_command(
    pool: &SqlitePool,
    direction: &str,
    command: &TransferRemoteCommandV1,
    now: DateTime<Utc>,
) -> Result<RemoteCommandCommit, StoreError> {
    if !matches!(direction, "outgoing" | "incoming") {
        return Err(protocol("remote Transfer command direction is invalid"));
    }
    command.validate_shape().map_err(protocol)?;
    let payload = serde_json::to_string(command).map_err(|error| protocol(error.to_string()))?;
    let payload_hash = command.payload_hash();
    if let Some(row) =
        sqlx::query("SELECT * FROM transfer_remote_command WHERE command_uid = ? OR request_id = ?")
            .bind(&command.command_uid)
            .bind(&command.request_id)
            .fetch_optional(pool)
            .await?
    {
        let existing = map_remote_command(row);
        if existing.command_uid == command.command_uid
            && existing.request_id == command.request_id
            && existing.direction == direction
            && existing.payload_hash == payload_hash
            && existing.payload == payload
        {
            return Ok(RemoteCommandCommit::Replayed(existing));
        }
        return Err(protocol(
            "remote Transfer command uid or request id changed on replay",
        ));
    }
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_remote_command
         (command_uid, request_id, direction, origin_organ_uid, sender_organ_uid, transfer_uid,
          actor_person_uid, expected_revision, payload, payload_hash, next_attempt_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&command.command_uid)
    .bind(&command.request_id)
    .bind(direction)
    .bind(&command.origin_organ_uid)
    .bind(&command.sender_organ_uid)
    .bind(&command.transfer_uid)
    .bind(&command.actor_person_uid)
    .bind(command.expected_revision.map(|value| value as i64))
    .bind(payload)
    .bind(payload_hash)
    .bind(&at)
    .bind(&at)
    .execute(pool)
    .await?;
    let row = sqlx::query("SELECT * FROM transfer_remote_command WHERE command_uid = ?")
        .bind(&command.command_uid)
        .fetch_one(pool)
        .await?;
    Ok(RemoteCommandCommit::Applied(map_remote_command(row)))
}

pub async fn remote_commands_due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<RemoteCommandRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_remote_command
         WHERE direction = 'outgoing' AND status IN ('queued', 'failed') AND next_attempt_at <= ?
         ORDER BY next_attempt_at, created_at, command_uid LIMIT ?",
    )
    .bind(now.to_rfc3339())
    .bind(i64::from(limit.max(1)))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_remote_command)
    .collect())
}

pub async fn remote_command_mark_failed(
    pool: &SqlitePool,
    command_uid: &str,
    now: DateTime<Utc>,
    code: &str,
    error: &str,
    base_delay_seconds: u32,
    max_delay_seconds: u32,
) -> Result<String, StoreError> {
    let attempts: i64 = sqlx::query_scalar(
        "SELECT attempts FROM transfer_remote_command
         WHERE command_uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(command_uid)
    .fetch_one(pool)
    .await?;
    let exponent = u32::try_from(attempts).unwrap_or(u32::MAX).min(20);
    let delay = u64::from(base_delay_seconds.max(1))
        .saturating_mul(1_u64 << exponent)
        .min(u64::from(max_delay_seconds.max(base_delay_seconds.max(1))));
    let next = now + Duration::seconds(i64::try_from(delay).unwrap_or(i64::MAX));
    let next = next.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_remote_command SET status = 'failed', attempts = attempts + 1,
          next_attempt_at = ?, last_attempt_at = ?, last_error_code = ?, last_error = ?
         WHERE command_uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(&next)
    .bind(now.to_rfc3339())
    .bind(code)
    .bind(error)
    .bind(command_uid)
    .execute(pool)
    .await?;
    Ok(next)
}

pub async fn remote_command_mark_sent(
    pool: &SqlitePool,
    command_uid: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE transfer_remote_command SET status = 'sent', attempts = attempts + 1,
          last_attempt_at = ?, last_error_code = NULL, last_error = NULL
         WHERE command_uid = ? AND direction = 'outgoing' AND status IN ('queued', 'failed')",
    )
    .bind(now.to_rfc3339())
    .bind(command_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_remote_command(
    pool: &SqlitePool,
    command_uid: &str,
    accepted: bool,
    authoritative_revision: u64,
    result: &serde_json::Value,
    error_code: Option<&str>,
    error: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let payload = serde_json::to_string(result).map_err(|value| protocol(value.to_string()))?;
    let status = if accepted { "accepted" } else { "rejected" };
    if let Some(row) = sqlx::query("SELECT * FROM transfer_remote_command WHERE command_uid = ?")
        .bind(command_uid)
        .fetch_optional(pool)
        .await?
        && matches!(
            row.get::<String, _>("status").as_str(),
            "accepted" | "rejected"
        )
    {
        if row.get::<String, _>("status") == status
            && row.get::<Option<i64>, _>("authoritative_revision")
                == Some(authoritative_revision as i64)
            && row.get::<Option<String>, _>("result_payload").as_deref() == Some(payload.as_str())
            && row.get::<Option<String>, _>("last_error_code").as_deref() == error_code
            && row.get::<Option<String>, _>("last_error").as_deref() == error
        {
            return Ok(());
        }
        return Err(protocol("remote Transfer command result changed on replay"));
    }
    let result = sqlx::query(
        "UPDATE transfer_remote_command SET status = ?, authoritative_revision = ?,
          result_payload = ?, last_error_code = ?, last_error = ?, finished_at = ?
         WHERE command_uid = ? AND status IN ('queued', 'sent', 'failed')",
    )
    .bind(status)
    .bind(authoritative_revision as i64)
    .bind(payload)
    .bind(error_code)
    .bind(error)
    .bind(now.to_rfc3339())
    .bind(command_uid)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol(
            "remote Transfer command is already terminal or missing",
        ));
    }
    Ok(())
}

/// Consumes one authenticated Organ request nonce. Every reuse is rejected;
/// command/envelope ids provide semantic idempotency above this transport gate.
pub async fn consume_organ_request_nonce(
    pool: &SqlitePool,
    request: &SignedOrganRequestV1,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    request.validate_shape().map_err(protocol)?;
    let result = sqlx::query(
        "INSERT OR IGNORE INTO organ_transfer_request_nonce
         (sender_organ_uid, nonce, request_hash, method, path, request_timestamp, received_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&request.sender_organ_uid)
    .bind(&request.nonce)
    .bind(request.request_hash())
    .bind(&request.method)
    .bind(&request.path)
    .bind(&request.timestamp)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("signed Organ request nonce was already consumed"));
    }
    Ok(())
}

pub async fn prune_organ_request_nonces(
    pool: &SqlitePool,
    received_before: DateTime<Utc>,
) -> Result<u64, StoreError> {
    Ok(
        sqlx::query("DELETE FROM organ_transfer_request_nonce WHERE received_at < ?")
            .bind(received_before.to_rfc3339())
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

async fn ensure_existing_transfer_request_unused(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<(), StoreError> {
    let used: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = ?)
          OR EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = ?)
          OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = ?)
          OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = ?)
          OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = ?)
          OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = ?)
          OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = ?)",
    )
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .bind(request_id)
    .fetch_one(pool)
    .await?;
    if used != 0 {
        return Err(protocol("Transfer request id belongs to another workflow"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ApplicationHandoffRow {
    pub uid: String,
    pub state: TransferApplicationHandoffState,
    pub origin_organ_uid: String,
    pub participant_organ_uid: String,
    pub participant_person_uid: String,
    pub transfer_uid: String,
    pub occurrence_uid: String,
    pub settlement_slice_uid: String,
    pub origin_revision: u64,
    pub canonical_slice_hash: String,
    pub attestation_uid: Option<String>,
}

fn map_handoff(row: sqlx::sqlite::SqliteRow) -> ApplicationHandoffRow {
    let state = match row.get::<String, _>("state").as_str() {
        "pending" => TransferApplicationHandoffState::Pending,
        "accepted" => TransferApplicationHandoffState::Accepted,
        "rejected" => TransferApplicationHandoffState::Rejected,
        "compensated" => TransferApplicationHandoffState::Compensated,
        _ => unreachable!("database state constraint"),
    };
    ApplicationHandoffRow {
        uid: row.get("uid"),
        state,
        origin_organ_uid: row.get("origin_organ_uid"),
        participant_organ_uid: row.get("participant_organ_uid"),
        participant_person_uid: row.get("participant_person_uid"),
        transfer_uid: row.get("transfer_uid"),
        occurrence_uid: row.get("occurrence_uid"),
        settlement_slice_uid: row.get("settlement_slice_uid"),
        origin_revision: row.get::<i64, _>("origin_revision") as u64,
        canonical_slice_hash: row.get("canonical_slice_hash"),
        attestation_uid: row.get("attestation_uid"),
    }
}

pub struct NewApplicationHandoff<'a> {
    pub origin_organ_uid: &'a str,
    pub participant_organ_uid: &'a str,
    pub participant_person_uid: &'a str,
    pub transfer_uid: &'a str,
    pub occurrence_uid: &'a str,
    pub settlement_slice_uid: &'a str,
    pub origin_revision: u64,
    pub canonical_slice_hash: &'a str,
    pub request_id: &'a str,
    pub fact_uid: Option<&'a str>,
}

pub async fn create_application_handoff(
    pool: &SqlitePool,
    input: NewApplicationHandoff<'_>,
    now: DateTime<Utc>,
) -> Result<ApplicationHandoffRow, StoreError> {
    if let Some(row) = sqlx::query(
        "SELECT h.*, e.kind AS event_kind, e.fact_uid AS event_fact FROM transfer_application_handoff_event e
         JOIN transfer_application_handoff h ON h.uid = e.handoff_uid WHERE e.request_id = ?",
    )
    .bind(input.request_id)
    .fetch_optional(pool)
    .await?
    {
        let event_kind = row.get::<String, _>("event_kind");
        let event_fact = row.get::<Option<String>, _>("event_fact");
        let existing = map_handoff(row);
        if event_kind == "pending"
            && event_fact.as_deref() == input.fact_uid
            && existing.origin_organ_uid == input.origin_organ_uid
            && existing.participant_organ_uid == input.participant_organ_uid
            && existing.participant_person_uid == input.participant_person_uid
            && existing.transfer_uid == input.transfer_uid
            && existing.occurrence_uid == input.occurrence_uid
            && existing.settlement_slice_uid == input.settlement_slice_uid
            && existing.origin_revision == input.origin_revision
            && existing.canonical_slice_hash == input.canonical_slice_hash
        {
            return Ok(existing);
        }
        return Err(protocol("application handoff request id was replayed with changed contents"));
    }
    ensure_existing_transfer_request_unused(pool, input.request_id).await?;
    let uid = nucleus::new_uid("tah");
    let at = now.to_rfc3339();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO transfer_application_handoff
         (uid, origin_organ_uid, participant_organ_uid, participant_person_uid, transfer_uid,
          occurrence_uid, settlement_slice_uid, origin_revision, canonical_slice_hash,
          state, request_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.origin_organ_uid)
    .bind(input.participant_organ_uid)
    .bind(input.participant_person_uid)
    .bind(input.transfer_uid)
    .bind(input.occurrence_uid)
    .bind(input.settlement_slice_uid)
    .bind(input.origin_revision as i64)
    .bind(input.canonical_slice_hash)
    .bind(input.request_id)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_application_handoff_event
         (uid, handoff_uid, kind, from_state, to_state, fact_uid, request_id, created_at)
         VALUES (?, ?, 'pending', NULL, 'pending', ?, ?, ?)",
    )
    .bind(nucleus::new_uid("tahe"))
    .bind(&uid)
    .bind(input.fact_uid)
    .bind(input.request_id)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(
        sqlx::query("SELECT * FROM transfer_application_handoff WHERE uid = ?")
            .bind(&uid)
            .fetch_one(pool)
            .await
            .map(map_handoff)?,
    )
}

pub async fn store_application_attestation(
    pool: &SqlitePool,
    attestation: &TransferApplicationAttestationV1,
    now: DateTime<Utc>,
) -> Result<String, StoreError> {
    attestation.validate_shape().map_err(protocol)?;
    let payload =
        serde_json::to_string(attestation).map_err(|error| protocol(error.to_string()))?;
    if let Some(existing) = sqlx::query_scalar::<_, String>(
        "SELECT payload FROM transfer_application_attestation WHERE uid = ?",
    )
    .bind(&attestation.attestation_uid)
    .fetch_optional(pool)
    .await?
    {
        if existing == payload {
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
    .bind(attestation.origin_revision as i64)
    .bind(&attestation.canonical_slice_hash)
    .bind(&attestation.formula_commitment)
    .bind(&attestation.formula_version)
    .bind(&attestation.application_fact_uid)
    .bind(&attestation.applied_at)
    .bind(&attestation.key_id)
    .bind(&attestation.signature)
    .bind(payload)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(attestation.attestation_uid.clone())
}

pub struct ApplicationHandoffTransition<'a> {
    pub handoff_uid: &'a str,
    pub expected_state: TransferApplicationHandoffState,
    pub to_state: TransferApplicationHandoffState,
    pub attestation_uid: Option<&'a str>,
    pub fact_uid: Option<&'a str>,
    pub reason_code: Option<&'a str>,
    pub request_id: &'a str,
}

pub async fn transition_application_handoff(
    pool: &SqlitePool,
    input: ApplicationHandoffTransition<'_>,
    now: DateTime<Utc>,
) -> Result<ApplicationHandoffRow, StoreError> {
    let valid = matches!(
        (input.expected_state, input.to_state),
        (
            TransferApplicationHandoffState::Pending,
            TransferApplicationHandoffState::Accepted | TransferApplicationHandoffState::Rejected
        ) | (
            TransferApplicationHandoffState::Accepted,
            TransferApplicationHandoffState::Compensated
        )
    );
    if !valid
        || (input.to_state == TransferApplicationHandoffState::Accepted
            && input.attestation_uid.is_none())
    {
        return Err(protocol("invalid application handoff transition"));
    }
    let mut tx = pool.begin().await?;
    if let Some(row) = sqlx::query(
        "SELECT h.*, e.kind AS event_kind, e.from_state AS event_from_state,
                e.to_state AS event_to_state, e.attestation_uid AS event_attestation_uid,
                e.fact_uid AS event_fact_uid, e.reason_code AS event_reason_code
         FROM transfer_application_handoff_event e
         JOIN transfer_application_handoff h ON h.uid = e.handoff_uid WHERE e.request_id = ?",
    )
    .bind(input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let exact = row.get::<String, _>("uid") == input.handoff_uid
            && row.get::<String, _>("event_kind") == input.to_state.as_str()
            && row.get::<Option<String>, _>("event_from_state").as_deref()
                == Some(input.expected_state.as_str())
            && row.get::<String, _>("event_to_state") == input.to_state.as_str()
            && row
                .get::<Option<String>, _>("event_attestation_uid")
                .as_deref()
                == input.attestation_uid
            && row.get::<Option<String>, _>("event_fact_uid").as_deref() == input.fact_uid
            && row.get::<Option<String>, _>("event_reason_code").as_deref() == input.reason_code;
        if !exact {
            return Err(protocol(
                "application handoff request id was replayed as another transition",
            ));
        }
        let existing = map_handoff(row);
        tx.rollback().await?;
        return Ok(existing);
    }
    ensure_existing_transfer_request_unused(pool, input.request_id).await?;
    let result = sqlx::query(
        "UPDATE transfer_application_handoff SET state = ?, attestation_uid = COALESCE(?, attestation_uid),
          updated_at = ? WHERE uid = ? AND state = ?",
    )
    .bind(input.to_state.as_str())
    .bind(input.attestation_uid)
    .bind(now.to_rfc3339())
    .bind(input.handoff_uid)
    .bind(input.expected_state.as_str())
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("application handoff state is stale"));
    }
    sqlx::query(
        "INSERT INTO transfer_application_handoff_event
         (uid, handoff_uid, kind, from_state, to_state, attestation_uid, fact_uid,
          reason_code, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("tahe"))
    .bind(input.handoff_uid)
    .bind(input.to_state.as_str())
    .bind(input.expected_state.as_str())
    .bind(input.to_state.as_str())
    .bind(input.attestation_uid)
    .bind(input.fact_uid)
    .bind(input.reason_code)
    .bind(input.request_id)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(
        sqlx::query("SELECT * FROM transfer_application_handoff WHERE uid = ?")
            .bind(input.handoff_uid)
            .fetch_one(pool)
            .await
            .map(map_handoff)?,
    )
}

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
    pub canonical_slice_hash: String,
    pub envelope_uid: String,
    pub envelope_payload_hash: String,
    pub origin_created_at: String,
    pub state: String,
    pub local_application_uid: Option<String>,
}

fn map_remote_application_handoff(row: sqlx::sqlite::SqliteRow) -> RemoteApplicationHandoffRow {
    RemoteApplicationHandoffRow {
        uid: row.get("uid"),
        reference_uid: row.get("reference_uid"),
        origin_organ_uid: row.get("origin_organ_uid"),
        participant_organ_uid: row.get("participant_organ_uid"),
        participant_person_uid: row.get("participant_person_uid"),
        transfer_uid: row.get("transfer_uid"),
        occurrence_uid: row.get("occurrence_uid"),
        source_promise_uid: row.get("source_promise_uid"),
        settlement_slice_uid: row.get("settlement_slice_uid"),
        origin_revision: row.get::<i64, _>("origin_revision") as u64,
        canonical_quantity: row.get("canonical_quantity"),
        canonical_unit_uid: row.get("canonical_unit_uid"),
        canonical_cumulative_before: row.get("canonical_cumulative_before"),
        canonical_cumulative_after: row.get("canonical_cumulative_after"),
        canonical_remaining_after: row.get("canonical_remaining_after"),
        canonical_slice_hash: row.get("canonical_slice_hash"),
        envelope_uid: row.get("envelope_uid"),
        envelope_payload_hash: row.get("envelope_payload_hash"),
        origin_created_at: row.get("origin_created_at"),
        state: row.get("state"),
        local_application_uid: row.get("local_application_uid"),
    }
}

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
    pub canonical_slice_hash: &'a str,
    pub envelope_uid: &'a str,
    pub envelope_payload_hash: &'a str,
    pub origin_created_at: &'a str,
}

pub async fn remote_application_handoff(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<RemoteApplicationHandoffRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_remote_application_handoff WHERE uid = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(map_remote_application_handoff))
}

/// Retain an origin-authenticated public proposal beside the isolated remote
/// projection. Exact replay is a no-op; changed bytes under the same handoff
/// identity are rejected.
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
        || (input.canonical_cumulative_after - input.canonical_cumulative_before
            - input.canonical_quantity)
            .abs()
            > 1e-9
    {
        return Err(protocol("remote application handoff quantities are invalid"));
    }
    let reference = remote_reference_by_uid(pool, input.reference_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if reference.state != "active"
        || reference.origin_organ_uid != input.origin_organ_uid
        || reference.transfer_uid != input.transfer_uid
        || reference.recipient_person_uid != input.participant_person_uid
        || reference.recipient_organ_uid != input.participant_organ_uid
        || reference.last_envelope_uid.as_deref() != Some(input.envelope_uid)
        || reference.last_payload_hash.as_deref() != Some(input.envelope_payload_hash)
    {
        return Err(protocol(
            "remote application handoff does not match the active verified reference",
        ));
    }
    if let Some(existing) = remote_application_handoff(pool, input.uid).await? {
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
            && existing.canonical_slice_hash == input.canonical_slice_hash
            && existing.envelope_uid == input.envelope_uid
            && existing.envelope_payload_hash == input.envelope_payload_hash
            && existing.origin_created_at == input.origin_created_at;
        if exact {
            return Ok(existing);
        }
        return Err(protocol(
            "remote application handoff was replayed with changed contents",
        ));
    }
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_remote_application_handoff
         (uid, reference_uid, origin_organ_uid, participant_organ_uid,
          participant_person_uid, transfer_uid, occurrence_uid, source_promise_uid,
          settlement_slice_uid, origin_revision, canonical_quantity,
          canonical_unit_uid, canonical_cumulative_before, canonical_cumulative_after,
          canonical_remaining_after, canonical_slice_hash, envelope_uid,
          envelope_payload_hash, origin_created_at, received_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(input.origin_revision as i64)
    .bind(input.canonical_quantity)
    .bind(input.canonical_unit_uid)
    .bind(input.canonical_cumulative_before)
    .bind(input.canonical_cumulative_after)
    .bind(input.canonical_remaining_after)
    .bind(input.canonical_slice_hash)
    .bind(input.envelope_uid)
    .bind(input.envelope_payload_hash)
    .bind(input.origin_created_at)
    .bind(&at)
    .bind(&at)
    .execute(pool)
    .await?;
    remote_application_handoff(pool, input.uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

#[derive(Debug, Clone)]
pub struct LocalTransferApplicationRow {
    pub uid: String,
    pub handoff_uid: String,
    pub participant_person_uid: String,
    pub local_record_uid: String,
    pub application_fact_uid: String,
    pub local_delta: f64,
    pub local_cumulative_before: f64,
    pub local_cumulative_after: f64,
    pub application_formula: String,
    pub application_formula_hash: String,
    pub application_formula_version: u64,
    pub authorization_intent_uid: Option<String>,
    pub request_id: String,
    pub created_at: String,
}

fn map_local_application(row: sqlx::sqlite::SqliteRow) -> LocalTransferApplicationRow {
    LocalTransferApplicationRow {
        uid: row.get("uid"),
        handoff_uid: row.get("handoff_uid"),
        participant_person_uid: row.get("participant_person_uid"),
        local_record_uid: row.get("local_record_uid"),
        application_fact_uid: row.get("application_fact_uid"),
        local_delta: row.get("local_delta"),
        local_cumulative_before: row.get("local_cumulative_before"),
        local_cumulative_after: row.get("local_cumulative_after"),
        application_formula: row.get("application_formula"),
        application_formula_hash: row.get("application_formula_hash"),
        application_formula_version: row.get::<i64, _>("application_formula_version") as u64,
        authorization_intent_uid: row.get("authorization_intent_uid"),
        request_id: row.get("request_id"),
        created_at: row.get("created_at"),
    }
}

pub struct NewLocalTransferApplication {
    pub handoff_uid: String,
    pub participant_person_uid: String,
    pub local_record_uid: String,
    pub local_delta: f64,
    pub local_cumulative_before: f64,
    pub local_cumulative_after: f64,
    pub application_formula: String,
    pub application_formula_hash: String,
    pub application_formula_version: u64,
    pub authorization_intent_uid: Option<String>,
    pub request_id: String,
}

pub struct LocalTransferApplicationCommit {
    pub application: LocalTransferApplicationRow,
    pub fact: Fact,
    pub replayed: bool,
}

/// Append the participant's private quantity effect and its private audit row
/// atomically. The caller supplies either a matching Person signer or a
/// previously verified local Action intent.
pub async fn apply_remote_transfer_locally<F>(
    pool: &SqlitePool,
    input: NewLocalTransferApplication,
    now: DateTime<Utc>,
    sign: F,
) -> Result<LocalTransferApplicationCommit, StoreError>
where
    F: FnOnce(&str) -> Option<String>,
{
    if !input.local_delta.is_finite()
        || !input.local_cumulative_before.is_finite()
        || !input.local_cumulative_after.is_finite()
        || (input.local_cumulative_before + input.local_delta - input.local_cumulative_after)
            .abs()
            > 1e-9
    {
        return Err(protocol("local Transfer application quantities are invalid"));
    }
    if let Some(row) = sqlx::query("SELECT * FROM transfer_local_application WHERE request_id = ?")
        .bind(&input.request_id)
        .fetch_optional(pool)
        .await?
    {
        let application = map_local_application(row);
        let exact = application.handoff_uid == input.handoff_uid
            && application.participant_person_uid == input.participant_person_uid
            && application.local_record_uid == input.local_record_uid
            && application.local_delta == input.local_delta
            && application.local_cumulative_before == input.local_cumulative_before
            && application.local_cumulative_after == input.local_cumulative_after
            && application.application_formula == input.application_formula
            && application.application_formula_hash == input.application_formula_hash
            && application.application_formula_version == input.application_formula_version
            && application.authorization_intent_uid == input.authorization_intent_uid;
        if !exact {
            return Err(protocol(
                "local Transfer application request was replayed with changed contents",
            ));
        }
        let fact = crate::facts::get(pool, &application.application_fact_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        return Ok(LocalTransferApplicationCommit {
            application,
            fact,
            replayed: true,
        });
    }

    let mut tx = pool.begin().await?;
    let handoff = sqlx::query("SELECT * FROM transfer_remote_application_handoff WHERE uid = ?")
        .bind(&input.handoff_uid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if handoff.get::<String, _>("state") != "pending"
        || handoff.get::<String, _>("participant_person_uid") != input.participant_person_uid
    {
        return Err(protocol("remote Transfer application handoff is not pending for this Person"));
    }
    let record = sqlx::query(
        "SELECT record.organ_uid, record.deleted_at, record.quantity, local_organ.uid AS local_organ_uid
         FROM record LEFT JOIN organ local_organ ON local_organ.local = 1
         WHERE record.uid = ?",
    )
    .bind(&input.local_record_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let record_organ_uid = record.get::<Option<String>, _>("organ_uid");
    let local_organ_uid = record.get::<Option<String>, _>("local_organ_uid");
    if record.get::<Option<String>, _>("deleted_at").is_some()
        || record_organ_uid.is_none()
        || record_organ_uid != local_organ_uid
    {
        return Err(protocol(
            "remote Transfer application may mutate only a live Record originating in this Cell",
        ));
    }
    let application_uid = nucleus::new_uid("tla");
    let payload = serde_json::json!({
        "action": "apply-remote-transfer-settlement",
        "application_uid": application_uid,
        "handoff_uid": input.handoff_uid,
        "origin_organ_uid": handoff.get::<String, _>("origin_organ_uid"),
        "transfer_uid": handoff.get::<String, _>("transfer_uid"),
        "occurrence_uid": handoff.get::<String, _>("occurrence_uid"),
        "settlement_slice_uid": handoff.get::<String, _>("settlement_slice_uid"),
        "canonical_slice_hash": handoff.get::<String, _>("canonical_slice_hash"),
        "participant_person_uid": input.participant_person_uid,
        "application_formula_hash": input.application_formula_hash,
        "application_formula_version": input.application_formula_version,
        "local_cumulative_before": input.local_cumulative_before,
        "local_cumulative_after": input.local_cumulative_after,
    });
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: input.local_record_uid.clone(),
            delta: input.local_delta,
            at: None,
            actor_uid: Some(input.participant_person_uid.clone()),
            cause: Cause::settlement(handoff.get::<String, _>("settlement_slice_uid")),
            payload: Some(payload.to_string()),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(protocol(
            "local Transfer application requires a Person signer or verified Action intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent_uid) = input.authorization_intent_uid.as_deref() {
        crate::action_intents::link_pending_fact(&mut tx, intent_uid, &fact).await?;
    }
    crate::records::bump_quantity(
        &mut tx,
        &input.local_record_uid,
        input.local_delta,
        &now.to_rfc3339(),
    )
    .await?;
    sqlx::query(
        "INSERT INTO transfer_local_application
         (uid, handoff_uid, participant_person_uid, local_record_uid,
          application_fact_uid, local_delta, local_cumulative_before,
          local_cumulative_after, application_formula, application_formula_hash,
          application_formula_version, authorization_intent_uid, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&application_uid)
    .bind(&input.handoff_uid)
    .bind(&input.participant_person_uid)
    .bind(&input.local_record_uid)
    .bind(&fact.uid)
    .bind(input.local_delta)
    .bind(input.local_cumulative_before)
    .bind(input.local_cumulative_after)
    .bind(&input.application_formula)
    .bind(&input.application_formula_hash)
    .bind(input.application_formula_version as i64)
    .bind(&input.authorization_intent_uid)
    .bind(&input.request_id)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    let changed = sqlx::query(
        "UPDATE transfer_remote_application_handoff
         SET state = 'applied', local_application_uid = ?, updated_at = ?
         WHERE uid = ? AND state = 'pending'",
    )
    .bind(&application_uid)
    .bind(now.to_rfc3339())
    .bind(&input.handoff_uid)
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(protocol("remote Transfer application handoff state is stale"));
    }
    tx.commit().await?;
    let application = sqlx::query("SELECT * FROM transfer_local_application WHERE uid = ?")
        .bind(&application_uid)
        .fetch_one(pool)
        .await
        .map(map_local_application)?;
    Ok(LocalTransferApplicationCommit {
        application,
        fact,
        replayed: false,
    })
}
