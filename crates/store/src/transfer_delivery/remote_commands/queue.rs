//! Dispatch eligibility, backoff, and terminal results for remote commands.

use chrono::{DateTime, Utc};
use serde_json::{Value, to_string};
use sqlx::{Row, SqlitePool};

use super::{RemoteCommandRow, map_remote_command};
use crate::{StoreError, backoff::next_attempt_at, transfer_delivery::validation::protocol};

/// Lists due outgoing commands for this Cell, excluding cancelled automation.
///
/// # Errors
///
/// Returns an error if the query or row decoding fails.
pub async fn remote_commands_due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<RemoteCommandRow>, StoreError> {
    sqlx::query(
        "SELECT c.* FROM transfer_remote_command c
         WHERE c.direction = 'outgoing' AND c.status IN ('queued', 'failed') AND c.next_attempt_at <= ?
           AND NOT EXISTS (
               SELECT 1 FROM karma_transfer_command k
               WHERE k.command_uid = c.command_uid AND k.cancelled = 1
           )
           AND NOT EXISTS (
               SELECT 1 FROM transfer_sync_owner o
               WHERE o.table_name = 'transfer_remote_command' AND o.row_key = json_array(c.command_uid)
                 AND o.cell_uid != (SELECT uid FROM record WHERE slug = 'local-cell')
           )
         ORDER BY c.next_attempt_at, c.created_at, c.command_uid LIMIT ?",
    )
    .bind(now.to_rfc3339())
    .bind(i64::from(limit.max(1)))
    .try_map(map_remote_command)
    .fetch_all(pool)
    .await
}

/// Records a failed send and returns the next attempt time.
///
/// # Errors
///
/// Returns an error if the command is missing, ineligible, or storage fails.
pub async fn remote_command_mark_failed(
    pool: &SqlitePool,
    command_uid: &str,
    now: DateTime<Utc>,
    code: &str,
    error: &str,
    base_delay_seconds: u32,
    max_delay_seconds: u32,
) -> Result<String, StoreError> {
    let attempts = sqlx::query_scalar(
        "SELECT attempts FROM transfer_remote_command
         WHERE command_uid = ? AND status IN ('queued', 'failed')",
    )
    .bind(command_uid)
    .fetch_one(pool)
    .await?;
    let next = next_attempt_at(now, attempts, base_delay_seconds, max_delay_seconds).to_rfc3339();
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

/// Marks an eligible outgoing command as sent and clears its last error.
///
/// # Errors
///
/// Returns an error if the update fails.
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

/// Saves the terminal result or recognizes an exact replay.
///
/// # Errors
///
/// Returns an error if the command is missing, the saved result differs,
/// or the result cannot be written.
#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the public Store API for a command result"
)]
pub async fn finish_remote_command(
    pool: &SqlitePool,
    command_uid: &str,
    accepted: bool,
    authoritative_revision: u64,
    result: &Value,
    error_code: Option<&str>,
    error: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let payload = to_string(result).map_err(|error| protocol(error.to_string()))?;
    let status = if accepted { "accepted" } else { "rejected" };
    let revision = authoritative_revision.cast_signed();
    let changed = sqlx::query(
        "UPDATE transfer_remote_command SET status = ?, authoritative_revision = ?,
          result_payload = ?, last_error_code = ?, last_error = ?, finished_at = ?
         WHERE command_uid = ? AND status IN ('queued', 'sent', 'failed')",
    )
    .bind(status)
    .bind(revision)
    .bind(&payload)
    .bind(error_code)
    .bind(error)
    .bind(now.to_rfc3339())
    .bind(command_uid)
    .execute(pool)
    .await?;
    if changed.rows_affected() == 1 {
        return Ok(());
    }

    let saved = sqlx::query(
        "SELECT status, authoritative_revision, result_payload, last_error_code, last_error
         FROM transfer_remote_command WHERE command_uid = ? AND status IN ('accepted', 'rejected')",
    )
    .bind(command_uid)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| protocol("remote Transfer command is already terminal or missing"))?;
    if saved.get::<&str, _>("status") == status
        && saved.get::<Option<i64>, _>("authoritative_revision") == Some(revision)
        && saved.get::<Option<&str>, _>("result_payload") == Some(payload.as_str())
        && saved.get::<Option<&str>, _>("last_error_code") == error_code
        && saved.get::<Option<&str>, _>("last_error") == error
    {
        Ok(())
    } else {
        Err(protocol("remote Transfer command result changed on replay"))
    }
}
