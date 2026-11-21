//! Published Program and Frequency definitions and their revisions.

use std::collections::HashMap;

use nucleus::karma::DefinitionStatus::Active;
use serde_json::{Value, from_str, json};
use store::{
    cells, executor,
    karma::{execution, frequencies, programs},
    sqlx::{SqlitePool, query_as},
};

use crate::ProteinError;

pub(super) async fn append(pool: &SqlitePool, rows: &mut Vec<Value>) -> Result<(), ProteinError> {
    let executing: HashMap<_, _> = execution::list(pool)
        .await?
        .into_iter()
        .map(|row| (row.program_uid, (row.executes, row.note)))
        .collect();

    let revisions = programs::list_revisions(pool).await?;
    let outward: HashMap<_, _> = revisions
        .iter()
        .map(|revision| {
            (
                &revision.revision_hash,
                revision.program.is_externally_observable(),
            )
        })
        .collect();

    let this_cell = cells::local(pool).await?.map(|cell| cell.uid);

    let designations: HashMap<_, _> = query_as::<_, (String, String)>(
        "SELECT record_uid, fds FROM record_extension WHERE namespace = ?",
    )
    .bind(executor::NAMESPACE)
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(|(uid, fds)| {
        let fds = from_str::<Value>(&fds).ok()?;
        Some((uid, fds["cell"].as_str()?.to_owned()))
    })
    .collect();

    for handle in programs::list_handles(pool).await? {
        let can_activate = handle.active_revision_hash.as_ref() != Some(&handle.head_revision_hash);
        let can_pause = handle.status == Active;
        let (executes, execution_note) = executing
            .get(&handle.record_uid)
            .map_or((true, None), |(executes, note)| {
                (*executes, note.as_deref())
            });
        rows.push(json!({
            "object_kind": "program",
            "uid": handle.record_uid,
            "slug": handle.slug,
            "handle_revision": handle.handle_revision,
            "status": handle.status.as_str(),
            "head_revision_hash": handle.head_revision_hash,
            "active_revision_hash": handle.active_revision_hash,
            "owner_person_uid": handle.owner_person_uid,
            "created_at": handle.created_at,
            "updated_at": handle.updated_at,
            "executes_here": executes,
            "execution_note": execution_note,
            "designated_cell": designations.get(&handle.record_uid),
            "this_cell": this_cell,
            "externally_observable": handle
                .active_revision_hash
                .as_ref()
                .and_then(|hash| outward.get(hash))
                .copied()
                .unwrap_or(false),
            "capabilities": {
                "revise": true,
                "activate": can_activate,
                "pause": can_pause,
                "set_execution": true,
            },
            "blocking_reasons": {
                "revise": [],
                "activate": (!can_activate).then_some("head_already_active").as_slice(),
                "pause": (!can_pause).then_some("program_not_active").as_slice(),
            },
            "action_templates": {
                "revise": {
                    "action": "revise-karma-program",
                    "request_id": Value::Null,
                    "program_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "program": Value::Null,
                },
                "activate": {
                    "action": "activate-karma-program",
                    "request_id": Value::Null,
                    "program_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "revision_hash": handle.head_revision_hash,
                },
                "pause": {
                    "action": "pause-karma-program",
                    "request_id": Value::Null,
                    "program_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                },
                "set_execution": {
                    "action": "set-karma-execution",
                    "program_uid": handle.record_uid,
                    "executes": !executes,
                    "note": Value::Null,
                },
            },
        }));
    }
    for revision in &revisions {
        rows.push(json!({
            "object_kind": "program_revision",
            "externally_observable": outward.get(&revision.revision_hash),
            "uid": revision.revision_hash,
            "program_uid": revision.program_uid,
            "revision_hash": revision.revision_hash,
            "program": revision.program,
            "canonical_dsl": revision.canonical_dsl,
            "proof": revision.proof,
            "created_at": revision.created_at,
        }));
    }
    drop(outward);
    drop(revisions);

    for handle in frequencies::list_handles(pool).await? {
        let can_activate = handle.active_revision_hash.as_ref() != Some(&handle.head_revision_hash);
        let can_pause = handle.status == Active;
        let inactive = (!can_pause).then_some("frequency_not_active");
        rows.push(json!({
            "object_kind": "frequency",
            "uid": handle.record_uid,
            "slug": handle.slug,
            "handle_revision": handle.handle_revision,
            "status": handle.status.as_str(),
            "head_revision_hash": handle.head_revision_hash,
            "active_revision_hash": handle.active_revision_hash,
            "active_activation_hash": handle.active_activation_hash,
            "latest_activation_hash": handle.latest_activation_hash,
            "owner_person_uid": handle.owner_person_uid,
            "created_at": handle.created_at,
            "updated_at": handle.updated_at,
            "capabilities": {
                "revise": true,
                "activate": can_activate,
                "pause": can_pause,
                "set_parameters": can_pause,
                "reset_parameters": can_pause,
            },
            "blocking_reasons": {
                "revise": [],
                "activate": (!can_activate).then_some("head_already_active").as_slice(),
                "pause": inactive.as_slice(),
                "set_parameters": inactive.as_slice(),
                "reset_parameters": inactive.as_slice(),
            },
            "requires_runtime_admission": true,
            "action_templates": {
                "revise": {
                    "action": "revise-karma-frequency",
                    "request_id": Value::Null,
                    "frequency_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "frequency": Value::Null,
                },
                "activate": {
                    "action": "activate-karma-frequency",
                    "request_id": Value::Null,
                    "frequency_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                    "revision_hash": handle.head_revision_hash,
                    "parameter_overrides": {},
                },
                "pause": {
                    "action": "pause-karma-frequency",
                    "request_id": Value::Null,
                    "frequency_uid": handle.record_uid,
                    "expected_handle_revision": handle.handle_revision,
                },
            },
        }));
    }
    for revision in frequencies::list_revisions(pool).await? {
        rows.push(json!({
            "object_kind": "frequency_revision",
            "uid": revision.revision_hash,
            "frequency_uid": revision.frequency_uid,
            "revision_hash": revision.revision_hash,
            "frequency": revision.frequency,
            "canonical_dsl": revision.canonical_dsl,
            "default_compiled": revision.default_compiled,
            "created_at": revision.created_at,
        }));
    }
    for activation in frequencies::list_activations(pool).await? {
        rows.push(json!({
            "object_kind": "frequency_activation",
            "uid": activation.activation_hash,
            "activation_hash": activation.activation_hash,
            "frequency_uid": activation.epoch.frequency_uid(),
            "epoch": activation.epoch,
        }));
    }
    Ok(())
}
