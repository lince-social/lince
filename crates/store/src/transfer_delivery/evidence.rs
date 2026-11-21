//! Immutable delivery receipts and remote command conflict evidence.

use chrono::{DateTime, Utc};
use nucleus::new_uid;
use serde_json::{Value, to_string};
use sqlx::{Row, SqlitePool};

use super::validation::protocol;
use crate::StoreError;

/// The receipt identity, signed payload, and optional local Fact.
pub struct NewPackageReceipt<'a> {
    pub delivery_uid: &'a str,
    pub envelope_uid: &'a str,
    pub cursor: u64,
    pub kind: &'a str,
    pub actor_organ_uid: &'a str,
    pub payload_hash: &'a str,
    pub key_id: &'a str,
    pub signature: &'a str,
    pub signed_payload: &'a Value,
    pub local_fact_uid: Option<&'a str>,
    pub request_id: &'a str,
}

/// Records a package receipt or returns the UID of the same saved proof.
///
/// # Errors
///
/// Returns an error for an invalid kind or cursor, conflicting signed proof,
/// or a failed write.
pub async fn record_package_receipt(
    pool: &SqlitePool,
    input: NewPackageReceipt<'_>,
    now: DateTime<Utc>,
) -> Result<String, StoreError> {
    if !matches!(input.kind, "received" | "seen") || input.cursor == 0 {
        return Err(protocol("invalid Transfer package receipt"));
    }
    let payload = to_string(input.signed_payload).map_err(|error| protocol(error.to_string()))?;
    let cursor = input.cursor.cast_signed();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(row) = sqlx::query(
        "SELECT * FROM transfer_delivery_receipt
         WHERE request_id = ?1
            OR (delivery_uid = ?2 AND envelope_uid = ?3 AND kind = ?4 AND actor_organ_uid = ?5)
         ORDER BY request_id = ?1 DESC LIMIT 1",
    )
    .bind(input.request_id)
    .bind(input.delivery_uid)
    .bind(input.envelope_uid)
    .bind(input.kind)
    .bind(input.actor_organ_uid)
    .fetch_optional(&mut *tx)
    .await?
    {
        if row.get::<&str, _>("delivery_uid") == input.delivery_uid
            && row.get::<&str, _>("envelope_uid") == input.envelope_uid
            && row.get::<&str, _>("kind") == input.kind
            && row.get::<&str, _>("actor_organ_uid") == input.actor_organ_uid
            && row.get::<i64, _>("cursor") == cursor
            && row.get::<&str, _>("payload_hash") == input.payload_hash
            && row.get::<&str, _>("key_id") == input.key_id
            && row.get::<&str, _>("signature") == input.signature
            && row.get::<&str, _>("signed_payload") == payload
        {
            tx.rollback().await?;
            return Ok(row.get("uid"));
        }
        return Err(protocol(
            if row.get::<&str, _>("request_id") == input.request_id {
                "package receipt request id was replayed with changed contents"
            } else {
                "package receipt identity was replayed with changed proof"
            },
        ));
    }
    let uid = new_uid("tdr");
    sqlx::query(
        "INSERT INTO transfer_delivery_receipt
         (uid, delivery_uid, envelope_uid, cursor, kind, actor_organ_uid, payload_hash,
          key_id, signature, signed_payload, local_fact_uid, request_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.delivery_uid)
    .bind(input.envelope_uid)
    .bind(cursor)
    .bind(input.kind)
    .bind(input.actor_organ_uid)
    .bind(input.payload_hash)
    .bind(input.key_id)
    .bind(input.signature)
    .bind(payload)
    .bind(input.local_fact_uid)
    .bind(input.request_id)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(uid)
}

/// Identifies a rejected attempt and the reviewed payload kept as evidence.
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
    pub reviewed_payload: &'a Value,
}

/// Records a rejected attempt or returns the UID of an exact command replay.
///
/// # Errors
///
/// Returns an error if no attempt identity is supplied, a command replay
/// differs from the saved evidence, or storage fails.
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
    let payload = to_string(input.reviewed_payload).map_err(|error| protocol(error.to_string()))?;
    let revision = input.authoritative_revision.cast_signed();
    let cursor = input.authoritative_cursor.cast_signed();
    let submitted = input.submitted_revision.map(u64::cast_signed);
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(command_uid) = input.command_uid
        && let Some(row) = sqlx::query(
            "SELECT * FROM transfer_remote_conflict WHERE origin_organ_uid = ? AND command_uid = ?",
        )
        .bind(input.origin_organ_uid)
        .bind(command_uid)
        .fetch_optional(&mut *tx)
        .await?
    {
        let exact = row.get::<&str, _>("transfer_uid") == input.transfer_uid
            && row.get::<&str, _>("recipient_person_uid") == input.recipient_person_uid
            && row.get::<Option<&str>, _>("request_id") == input.request_id
            && row.get::<Option<&str>, _>("envelope_uid") == input.envelope_uid
            && row.get::<Option<i64>, _>("submitted_revision") == submitted
            && row.get::<i64, _>("authoritative_revision") == revision
            && row.get::<i64, _>("authoritative_cursor") == cursor
            && row.get::<&str, _>("code") == input.code
            && row.get::<&str, _>("reviewed_payload") == payload;
        if exact {
            tx.rollback().await?;
            return Ok(row.get("uid"));
        }
        return Err(protocol(
            "remote conflict command was replayed with changed contents",
        ));
    }
    let uid = new_uid("trc");
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
    .bind(submitted)
    .bind(revision)
    .bind(cursor)
    .bind(input.code)
    .bind(payload)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(uid)
}
