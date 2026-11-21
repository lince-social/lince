//! Outgoing Transfer envelopes and their retry and acknowledgement state.

use chrono::{DateTime, Utc};
use nucleus::{new_uid, transfer_delivery::TransferEnvelopeV1};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::{policy::map_policy, validation::protocol};
use crate::{StoreError, backoff::next_attempt_at};

/// An outgoing envelope and its current delivery state.
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

/// The new outbox entry or the existing entry for an exact replay.
#[derive(Debug, Clone)]
pub enum EnqueueCommit {
    Applied(DeliveryOutboxRow),
    Replayed(DeliveryOutboxRow),
}

fn map_outbox(row: SqliteRow) -> Result<DeliveryOutboxRow, StoreError> {
    Ok(DeliveryOutboxRow {
        uid: row.get("uid"),
        envelope_uid: row.get("envelope_uid"),
        delivery_uid: row.get("delivery_uid"),
        cursor: row.get::<i64, _>("cursor").cast_unsigned(),
        transfer_revision: row.get::<i64, _>("transfer_revision").cast_unsigned(),
        payload: row.get("payload"),
        payload_hash: row.get("payload_hash"),
        status: row.get("status"),
        attempts: row.try_get("attempts")?,
        next_attempt_at: row.get("next_attempt_at"),
        last_error: row.get("last_error"),
    })
}

/// Queues an envelope matching an active policy and a new cursor.
///
/// # Errors
///
/// Returns an error for invalid envelope contents, changed replay contents,
/// stale policy or cursor, or a failed storage operation.
pub async fn enqueue(
    pool: &SqlitePool,
    delivery_uid: &str,
    envelope: &TransferEnvelopeV1,
    now: DateTime<Utc>,
) -> Result<EnqueueCommit, StoreError> {
    envelope.validate_shape().map_err(protocol)?;
    let payload = serde_json::to_string(envelope).map_err(|error| protocol(error.to_string()))?;
    let mut tx = crate::write_tx(pool).await?;
    if let Some(existing) =
        sqlx::query("SELECT * FROM transfer_delivery_outbox WHERE envelope_uid = ?")
            .bind(&envelope.envelope_uid)
            .try_map(map_outbox)
            .fetch_optional(&mut *tx)
            .await?
    {
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
        .try_map(map_policy)
        .fetch_optional(&mut *tx)
        .await?
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
    if envelope.cursor <= last.cast_unsigned() {
        return Err(protocol("Transfer envelope cursor is not monotonic"));
    }
    let uid = new_uid("tdo");
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
    .bind(envelope.cursor.cast_signed())
    .bind(envelope.transfer_revision.cast_signed())
    .bind(&payload)
    .bind(&envelope.payload_hash)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    entry(pool, &uid).await.map(EnqueueCommit::Applied)
}

/// Lists due envelopes for active policies in next-attempt order.
///
/// # Errors
///
/// Returns an error if the query or row decoding fails.
pub async fn outbox_due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<DeliveryOutboxRow>, StoreError> {
    sqlx::query(
        "SELECT o.* FROM transfer_delivery_outbox o
         JOIN transfer_delivery_policy p ON p.uid = o.delivery_uid
         WHERE o.status IN ('queued', 'failed') AND o.next_attempt_at <= ? AND p.state = 'active'
         ORDER BY o.next_attempt_at, o.created_at, o.uid LIMIT ?",
    )
    .bind(now.to_rfc3339())
    .bind(i64::from(limit.max(1)))
    .try_map(map_outbox)
    .fetch_all(pool)
    .await
}

/// Records a failed attempt and returns its next retry time.
///
/// # Errors
///
/// Returns an error if there is no queued or failed entry, or storage fails.
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
    let next = next_attempt_at(now, attempts, base_delay_seconds, max_delay_seconds).to_rfc3339();
    sqlx::query(
        "UPDATE transfer_delivery_outbox
         SET status = 'failed', attempts = attempts + 1, last_attempt_at = ?,
             next_attempt_at = ?, last_error = ?
         WHERE uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(now.to_rfc3339())
    .bind(&next)
    .bind(error)
    .bind(uid)
    .execute(pool)
    .await?;
    Ok(next)
}

/// Acknowledges an envelope at or above its cursor.
///
/// # Errors
///
/// Returns an error for a changed replay, a stale entry, a cursor below the
/// envelope cursor, or a failed storage operation.
pub async fn outbox_mark_sent(
    pool: &SqlitePool,
    uid: &str,
    acknowledged_cursor: u64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let acknowledged_cursor = acknowledged_cursor.cast_signed();
    if let Some((status, cursor, acknowledged)) = sqlx::query_as::<_, (String, i64, Option<i64>)>(
        "SELECT status, cursor, acknowledged_cursor FROM transfer_delivery_outbox WHERE uid = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
        && status == "sent"
    {
        if cursor <= acknowledged_cursor && acknowledged == Some(acknowledged_cursor) {
            return Ok(());
        }
        return Err(protocol("outbox acknowledgement changed on replay"));
    }
    let at = now.to_rfc3339();
    let result = sqlx::query(
        "UPDATE transfer_delivery_outbox
         SET status = 'sent', attempts = attempts + 1, last_attempt_at = ?, sent_at = ?,
             acknowledged_cursor = ?, last_error = NULL
         WHERE uid = ? AND status IN ('queued', 'failed') AND cursor <= ?",
    )
    .bind(&at)
    .bind(&at)
    .bind(acknowledged_cursor)
    .bind(uid)
    .bind(acknowledged_cursor)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol(
            "outbox acknowledgement is stale or below its envelope cursor",
        ));
    }
    Ok(())
}

/// Cancels queued and failed envelopes and returns the number changed.
///
/// # Errors
///
/// Returns an error if the update fails.
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

/// Requeues the latest failed envelope for an active delivery.
///
/// # Errors
///
/// Returns an error if the delivery is inactive, no envelope failed, the
/// request belongs to a different delivery, or storage fails.
pub async fn retry_outbox(
    pool: &SqlitePool,
    delivery_uid: &str,
    request_id: &str,
    now: DateTime<Utc>,
) -> Result<DeliveryOutboxRow, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if let Some(existing) = sqlx::query(
        "SELECT o.* FROM transfer_delivery_retry_event e
         JOIN transfer_delivery_outbox o ON o.uid = e.outbox_uid WHERE e.request_id = ?",
    )
    .bind(request_id)
    .try_map(map_outbox)
    .fetch_optional(&mut *tx)
    .await?
    {
        if existing.delivery_uid == delivery_uid {
            tx.rollback().await?;
            return Ok(existing);
        }
        return Err(protocol(
            "Transfer delivery retry request id belongs to another delivery",
        ));
    }
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_delivery_policy WHERE uid = ? AND state = 'active')",
    )
    .bind(delivery_uid)
    .fetch_one(&mut *tx)
    .await?;
    if !active {
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
    .bind(new_uid("tdre"))
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
    entry(pool, &outbox_uid).await
}

async fn entry(pool: &SqlitePool, uid: &str) -> Result<DeliveryOutboxRow, StoreError> {
    sqlx::query("SELECT * FROM transfer_delivery_outbox WHERE uid = ?")
        .bind(uid)
        .try_map(map_outbox)
        .fetch_one(pool)
        .await
}
