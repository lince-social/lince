//! Recipient refresh requests, retry scheduling, and cursor acknowledgement.

use chrono::{DateTime, Utc};
use nucleus::new_uid;
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::validation::protocol;
use crate::{StoreError, backoff::next_attempt_at};

/// A queued refresh and its original cursor and current retry state.
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

fn map_pull(row: SqliteRow) -> Result<PullRequestRow, StoreError> {
    Ok(PullRequestRow {
        uid: row.get("uid"),
        reference_uid: row.get("reference_uid"),
        request_id: row.get("request_id"),
        after_cursor: row.get::<i64, _>("after_cursor").cast_unsigned(),
        status: row.get("status"),
        attempts: row.try_get("attempts")?,
        next_attempt_at: row.get("next_attempt_at"),
        last_error: row.get("last_error"),
    })
}

/// Queues a refresh at the reference's current cursor or returns the original request.
///
/// # Errors
///
/// Returns an error for a conflicting request, a missing or revoked reference,
/// another writing Cell, or a failed storage operation.
pub async fn enqueue_pull(
    pool: &SqlitePool,
    reference_uid: &str,
    request_id: &str,
    now: DateTime<Utc>,
) -> Result<PullRequestRow, StoreError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(existing) =
        sqlx::query("SELECT * FROM transfer_delivery_pull_request WHERE request_id = ?")
            .bind(request_id)
            .try_map(map_pull)
            .fetch_optional(&mut *tx)
            .await?
    {
        if existing.reference_uid == reference_uid {
            tx.rollback().await?;
            return Ok(existing);
        }
        return Err(protocol(
            "Transfer pull request id belongs to another reference",
        ));
    }
    let owner: Option<String> = sqlx::query_scalar(
        "SELECT cell_uid FROM transfer_sync_owner
         WHERE table_name = 'transfer_remote_reference' AND row_key = json_array(?)
           AND cell_uid != (SELECT uid FROM record WHERE slug = 'local-cell')",
    )
    .bind(reference_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(owner) = owner {
        return Err(protocol(format!(
            "Refresh this Transfer from its writing Cell {owner}"
        )));
    }
    let (state, cursor): (String, i64) =
        sqlx::query_as("SELECT state, last_cursor FROM transfer_remote_reference WHERE uid = ?")
            .bind(reference_uid)
            .fetch_one(&mut *tx)
            .await?;
    if state != "active" {
        return Err(protocol("revoked Transfer reference cannot be refreshed"));
    }
    let request = sqlx::query(
        "INSERT INTO transfer_delivery_pull_request
         (uid, reference_uid, request_id, after_cursor, next_attempt_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5) RETURNING *",
    )
    .bind(new_uid("tdpr"))
    .bind(reference_uid)
    .bind(request_id)
    .bind(cursor)
    .bind(now.to_rfc3339())
    .try_map(map_pull)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(request)
}

/// Lists due requests on active references in next-attempt order.
///
/// # Errors
///
/// Returns an error if the query or row decoding fails.
pub async fn pulls_due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<PullRequestRow>, StoreError> {
    sqlx::query(
        "SELECT q.* FROM transfer_delivery_pull_request q
         JOIN transfer_remote_reference r ON r.uid = q.reference_uid
         WHERE q.status IN ('queued', 'failed') AND q.next_attempt_at <= ? AND r.state = 'active'
         ORDER BY q.next_attempt_at, q.created_at, q.uid LIMIT ?",
    )
    .bind(now.to_rfc3339())
    .bind(i64::from(limit.max(1)))
    .try_map(map_pull)
    .fetch_all(pool)
    .await
}

/// Records an unsuccessful refresh and returns the next attempt time.
///
/// # Errors
///
/// Returns an error for a missing or terminal request or a failed update.
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
    let next = next_attempt_at(now, attempts, base_delay_seconds, max_delay_seconds).to_rfc3339();
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

/// Completes a refresh at or beyond its starting cursor.
///
/// # Errors
///
/// Returns an error for a stale request, a lower cursor, or a failed update.
pub async fn pull_mark_completed(
    pool: &SqlitePool,
    uid: &str,
    completed_cursor: u64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let result = sqlx::query(
        "UPDATE transfer_delivery_pull_request SET status = 'completed', attempts = attempts + 1,
          last_attempt_at = ?1, completed_cursor = ?2, completed_at = ?1, last_error = NULL
         WHERE uid = ?3 AND status IN ('queued', 'failed') AND after_cursor <= ?2",
    )
    .bind(now.to_rfc3339())
    .bind(completed_cursor.cast_signed())
    .bind(uid)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol(
            "Transfer pull completion is stale or below its starting cursor",
        ));
    }
    Ok(())
}
