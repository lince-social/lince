//! Scheduled occurrences, cursor state, and occurrence expansion.

use serde_json::{Value, from_str, json};
use store::{
    karma::{expansions, schedules},
    sqlx::SqlitePool,
};

use super::karma_query_error;
use crate::ProteinError;

pub(super) async fn append(pool: &SqlitePool, rows: &mut Vec<Value>) -> Result<(), ProteinError> {
    for cursor in schedules::list_cursors(pool).await? {
        rows.push(json!({
            "object_kind": "schedule_cursor",
            "uid": cursor.activation_hash,
            "activation_hash": cursor.activation_hash,
            "frequency_uid": cursor.frequency_uid,
            "cursor_revision": cursor.cursor_revision,
            "lifecycle": cursor.lifecycle.as_str(),
            "cursor": cursor.cursor,
            "deadline": cursor.deadline,
            "timer": cursor.timer,
            "overload_policy": cursor.overload_policy,
            "demand": cursor.demand,
            "admitted_resolution_ms": cursor.admitted_resolution_ms.map(|value| value.get()),
            "admission_degraded": cursor.admission_degraded,
            "admitted_at": cursor.admitted_at,
            "lease_fencing_token": cursor.lease_fencing_token,
            "lease_owner": cursor.lease_owner,
            "lease_expires_at": cursor.lease_expires_at,
            "last_occurrence_sequence": cursor.last_occurrence_sequence,
            "last_error": cursor.last_error_json.as_deref()
                .map(from_str::<Value>)
                .transpose()
                .map_err(|error| karma_query_error("protein_karma_stored_json_invalid", error))?,
            "created_at": cursor.created_at,
            "updated_at": cursor.updated_at,
        }));
    }
    for occurrence in schedules::list_occurrences(pool).await? {
        let sequence = occurrence.occurrence.sequence();
        rows.push(json!({
            "object_kind": "schedule_occurrence",
            "uid": occurrence.occurrence_hash,
            "occurrence_hash": occurrence.occurrence_hash,
            "sequence": sequence,
            "occurrence": occurrence.occurrence,
            "created_at": occurrence.created_at,
        }));
    }
    for expansion in expansions::list_cursors(pool).await? {
        rows.push(json!({
            "object_kind": "schedule_occurrence_expansion",
            "uid": expansion.schedule_occurrence_hash,
            "schedule_occurrence_hash": expansion.schedule_occurrence_hash,
            "cadence": expansion.cadence.as_str(),
            "emission": expansion.emission.as_str(),
            "next_ordinal": expansion.next_ordinal,
            "total_items": expansion.total_items,
            "status": if expansion.completed { "completed" } else { "pending" },
            "created_at": expansion.created_at,
            "updated_at": expansion.updated_at,
        }));
    }
    Ok(())
}
