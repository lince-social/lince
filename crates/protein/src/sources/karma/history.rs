//! Karma occurrences, program runs, and control state.

use serde_json::{Value, json};
use store::{
    karma::{occurrences, runs, states},
    sqlx::SqlitePool,
};

use crate::ProteinError;

pub(super) async fn append(pool: &SqlitePool, rows: &mut Vec<Value>) -> Result<(), ProteinError> {
    for occurrence in occurrences::list(pool).await? {
        rows.push(json!({
            "object_kind": "occurrence",
            "uid": occurrence.occurrence_hash,
            "occurrence_hash": occurrence.occurrence_hash,
            "cell_sequence": occurrence.cell_sequence,
            "source_kind": occurrence.source_kind,
            "source_identity": occurrence.source_identity,
            "logical_at": occurrence.logical_at,
            "parent_occurrence_hash": occurrence.parent_occurrence_hash,
            "envelope": occurrence.envelope,
            "received_at": occurrence.received_at,
        }));
    }
    for epoch in runs::list_epochs(pool).await? {
        rows.push(json!({
            "object_kind": "program_epoch",
            "uid": epoch.epoch_hash,
            "epoch_hash": epoch.epoch_hash,
            "occurrence_hash": epoch.epoch.occurrence_hash,
            "cell_sequence": epoch.epoch.cell_sequence,
            "members": epoch.epoch.members,
            "next_member_ordinal": epoch.next_member_ordinal,
            "status": if epoch.completed { "completed" } else { "pending" },
            "created_at": epoch.created_at,
            "updated_at": epoch.updated_at,
            "completed_at": epoch.completed_at,
        }));
    }
    for run in runs::list_runs(pool).await? {
        rows.push(json!({
            "object_kind": "run",
            "uid": run.run_hash,
            "run_hash": run.run_hash,
            "occurrence_hash": run.run.occurrence_hash,
            "cell_sequence": run.run.cell_sequence,
            "logical_at": run.run.logical_at,
            "program_epoch_hash": run.run.program_epoch_hash,
            "member_ordinal": run.run.member_ordinal,
            "program_uid": run.run.program_uid,
            "program_revision_hash": run.run.program_revision_hash,
            "status": run.run.outcome.status_name(),
            "fuel_used": run.run.outcome.fuel_used(),
            "outcome": run.run.outcome,
            "created_at": run.created_at,
        }));
    }
    for state in states::list_node_states(pool).await? {
        rows.push(json!({
            "object_kind": "program_state",
            "uid": state.current_event_hash,
            "program_uid": state.program_uid,
            "node_id": state.node_id,
            "state_revision": state.state_revision,
            "current_event_hash": state.current_event_hash,
            "definition_revision_hash": state.definition_revision_hash,
            "activation_handle_revision": state.activation_handle_revision,
            "status": if state.state.is_some() { "value" } else { "reset" },
            "state": state.state,
            "updated_at": state.updated_at,
        }));
    }
    for event in states::list_events(pool).await? {
        rows.push(json!({
            "object_kind": "program_state_event",
            "uid": event.event_hash,
            "event_hash": event.event_hash,
            "program_uid": event.event.program_uid,
            "node_id": event.event.node_id,
            "state_revision": event.event.state_revision,
            "previous_event_hash": event.event.previous_event_hash,
            "source_run_hash": event.event.source_run_hash,
            "definition_revision_hash": event.event.definition_revision_hash,
            "activation_handle_revision": event.event.activation_handle_revision,
            "reset_reason": event.event.reset_reason,
            "status": if event.event.state.is_some() { "value" } else { "reset" },
            "state": event.event.state,
            "created_at": event.created_at,
        }));
    }
    Ok(())
}
