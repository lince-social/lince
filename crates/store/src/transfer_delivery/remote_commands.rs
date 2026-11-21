//! Signed remote command identity, replay, and Fact linkage.

use chrono::{DateTime, Utc};
use nucleus::{Fact, transfer_delivery::TransferRemoteCommandV1};
use serde_json::{from_str, to_string};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, sqlite::SqliteRow};

use super::validation::protocol;
use crate::{
    StoreError,
    karma_commands::{Origin, save_tx},
};

mod queue;

pub use queue::{
    finish_remote_command, remote_command_mark_failed, remote_command_mark_sent,
    remote_commands_due,
};

/// A remote command, its original signed payload, and current delivery state.
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

/// A newly stored command or the current row after an exact replay.
#[derive(Debug, Clone)]
pub enum RemoteCommandCommit {
    Applied(RemoteCommandRow),
    Replayed(RemoteCommandRow),
}

fn map_remote_command(row: SqliteRow) -> Result<RemoteCommandRow, StoreError> {
    Ok(RemoteCommandRow {
        command_uid: row.get("command_uid"),
        request_id: row.get("request_id"),
        direction: row.get("direction"),
        origin_organ_uid: row.get("origin_organ_uid"),
        sender_organ_uid: row.get("sender_organ_uid"),
        transfer_uid: row.get("transfer_uid"),
        actor_person_uid: row.get("actor_person_uid"),
        expected_revision: row
            .get::<Option<i64>, _>("expected_revision")
            .map(i64::cast_unsigned),
        payload: row.get("payload"),
        payload_hash: row.get("payload_hash"),
        status: row.get("status"),
        attempts: row.try_get("attempts")?,
        next_attempt_at: row.get("next_attempt_at"),
        last_error_code: row.get("last_error_code"),
        last_error: row.get("last_error"),
        result_payload: row.get("result_payload"),
        authoritative_revision: row
            .get::<Option<i64>, _>("authoritative_revision")
            .map(i64::cast_unsigned),
    })
}

/// Links a Fact to an incoming pending command with the same actor.
///
/// # Errors
///
/// Returns an error if the command is missing or ineligible, the actor differs,
/// or the Fact link cannot be recorded.
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
    .fetch_one(&mut **tx)
    .await?;
    if row.get::<&str, _>("direction") != "incoming"
        || !matches!(row.get::<&str, _>("status"), "queued" | "sent")
        || fact.actor_uid.as_deref() != Some(row.get("actor_person_uid"))
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

/// Reads a stored command linked to the given Fact.
///
/// # Errors
///
/// Returns an error if the query or row decoding fails.
pub async fn remote_command_for_fact(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<RemoteCommandRow>, StoreError> {
    sqlx::query(
        "SELECT c.* FROM fact_remote_command l
         JOIN transfer_remote_command c ON c.command_uid = l.command_uid WHERE l.fact_uid = ?",
    )
    .bind(fact_uid)
    .try_map(map_remote_command)
    .fetch_optional(pool)
    .await
}

/// Stores a signed command or returns its exact replay.
///
/// # Errors
///
/// Returns an error for an invalid direction or command shape, conflicting
/// replay, or a failed write.
pub async fn persist_remote_command(
    pool: &SqlitePool,
    direction: &str,
    command: &TransferRemoteCommandV1,
    now: DateTime<Utc>,
) -> Result<RemoteCommandCommit, StoreError> {
    persist_remote_command_with_karma(pool, direction, command, now, None).await
}

/// Stores a signed command with its optional local Rule origin.
///
/// # Errors
///
/// Returns an error for invalid inputs, an ineligible Rule, a conflicting
/// command or origin on replay, or a failed write.
pub async fn persist_remote_command_with_karma(
    pool: &SqlitePool,
    direction: &str,
    command: &TransferRemoteCommandV1,
    now: DateTime<Utc>,
    origin: Option<&Origin>,
) -> Result<RemoteCommandCommit, StoreError> {
    if !matches!(direction, "outgoing" | "incoming") {
        return Err(protocol("remote Transfer command direction is invalid"));
    }
    command.validate_shape().map_err(protocol)?;
    if origin.is_some() && direction != "outgoing" {
        return Err(protocol(
            "Only outgoing commands can have a local Rule origin",
        ));
    }
    let payload = to_string(command).map_err(|error| protocol(error.to_string()))?;
    let payload_hash = command.payload_hash();
    let mut tx = crate::write_tx(pool).await?;
    if let Some(existing) =
        sqlx::query("SELECT * FROM transfer_remote_command WHERE command_uid = ? OR request_id = ?")
            .bind(&command.command_uid)
            .bind(&command.request_id)
            .try_map(map_remote_command)
            .fetch_optional(&mut *tx)
            .await?
    {
        if existing.command_uid != command.command_uid
            || existing.request_id != command.request_id
            || existing.direction != direction
            || existing.payload_hash != payload_hash
            || existing.payload != payload
        {
            return Err(protocol(
                "remote Transfer command uid or request id changed on replay",
            ));
        }
        if let Some(origin) = origin {
            let original: Option<String> = sqlx::query_scalar(
                "SELECT origin FROM karma_transfer_command WHERE command_uid = ?",
            )
            .bind(&command.command_uid)
            .fetch_optional(&mut *tx)
            .await?;
            let original: Option<Origin> = original
                .as_deref()
                .map(from_str)
                .transpose()
                .map_err(|error| protocol(error.to_string()))?;
            if original.as_ref() != Some(origin) {
                return Err(protocol("Remote automation origin changed on replay"));
            }
        }
        tx.rollback().await?;
        return Ok(RemoteCommandCommit::Replayed(existing));
    }
    let row = sqlx::query(
        "INSERT INTO transfer_remote_command
         (command_uid, request_id, direction, origin_organ_uid, sender_organ_uid, transfer_uid,
          actor_person_uid, expected_revision, payload, payload_hash, next_attempt_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         RETURNING *",
    )
    .bind(&command.command_uid)
    .bind(&command.request_id)
    .bind(direction)
    .bind(&command.origin_organ_uid)
    .bind(&command.sender_organ_uid)
    .bind(&command.transfer_uid)
    .bind(&command.actor_person_uid)
    .bind(command.expected_revision.map(u64::cast_signed))
    .bind(payload)
    .bind(payload_hash)
    .bind(now.to_rfc3339())
    .try_map(map_remote_command)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(origin) = origin {
        save_tx(&mut tx, &command.command_uid, origin).await?;
    }
    tx.commit().await?;
    Ok(RemoteCommandCommit::Applied(row))
}

/// Looks up a stored remote command by UID.
///
/// # Errors
///
/// Returns an error if the query or row decoding fails.
pub async fn remote_command(
    pool: &SqlitePool,
    command_uid: &str,
) -> Result<Option<RemoteCommandRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_remote_command WHERE command_uid = ?")
        .bind(command_uid)
        .try_map(map_remote_command)
        .fetch_optional(pool)
        .await
}
