//! Read-only history for one remote Transfer reference.

use super::Reference;
use crate::ProteinError;
use serde_json::{Value, from_str, json};
use store::sqlx::{SqlitePool, query_as};

pub(super) async fn envelopes(
    pool: &SqlitePool,
    reference: &str,
) -> Result<Vec<Value>, ProteinError> {
    Ok(query_as(
        "SELECT envelope_uid, cursor, transfer_revision, received_at
         FROM transfer_replica_envelope WHERE reference_uid = ? ORDER BY cursor DESC",
    )
    .bind(reference)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(envelope, cursor, revision, at): (String, i64, i64, String)| {
        json!({ "envelope": envelope, "cursor": cursor, "revision": revision, "received_at": at })
    })
    .collect())
}

pub(super) async fn outgoing_commands(
    pool: &SqlitePool,
    reference: &Reference,
) -> Result<Vec<Value>, ProteinError> {
    Ok(query_as(
        "SELECT command_uid, request_id, status, last_error_code, last_error, created_at
         FROM transfer_remote_command
         WHERE direction = 'outgoing' AND origin_organ_uid = ? AND transfer_uid = ?
           AND actor_person_uid = ? ORDER BY created_at, command_uid",
    )
    .bind(&reference.origin_organ_uid)
    .bind(&reference.transfer_uid)
    .bind(&reference.recipient_person_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(uid, request, status, code, message, at): (String, String, String, Option<String>, Option<String>, String)| {
        json!({ "uid": uid, "request_id": request, "status": status, "code": code, "message": message, "at": at })
    })
    .collect())
}

pub(super) async fn conflicts(
    pool: &SqlitePool,
    reference: &Reference,
    refresh: &Value,
    active: bool,
) -> Result<Vec<Value>, ProteinError> {
    Ok(query_as(
        "SELECT uid, request_id, submitted_revision, authoritative_revision,
                code, reviewed_payload, created_at
         FROM transfer_remote_conflict
         WHERE origin_organ_uid = ? AND transfer_uid = ? AND recipient_person_uid = ?
         ORDER BY created_at, uid",
    )
    .bind(&reference.origin_organ_uid)
    .bind(&reference.transfer_uid)
    .bind(&reference.recipient_person_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(
        |(uid, request, submitted, authoritative, code, payload, at): (
            String,
            Option<String>,
            Option<i64>,
            i64,
            String,
            String,
            String,
        )| {
            json!({
                "uid": uid,
                "request_id": request,
                "submitted_revision": submitted,
                "authoritative_revision": authoritative,
                "code": code,
                "retained_input": from_str::<Value>(&payload).unwrap_or(Value::Null),
                "at": at,
                "capabilities": { "refresh": active },
                "action_payloads": { "refresh": refresh },
            })
        },
    )
    .collect())
}
