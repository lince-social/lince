//! Candidate state and review actions.

use nucleus::karma::CandidateStatus;
use serde_json::{Value, json};
use store::{karma::candidates, sqlx::SqlitePool};

use super::karma_query_error;
use crate::ProteinError;

pub(super) async fn append(pool: &SqlitePool, rows: &mut Vec<Value>) -> Result<(), ProteinError> {
    for candidate in candidates::list(pool).await? {
        let state = candidates::get_state(pool, &candidate.candidate_hash)
            .await?
            .ok_or_else(|| {
                karma_query_error(
                    "protein_karma_candidate_state_missing",
                    candidate.candidate_hash.as_str(),
                )
            })?;
        let review = |response: Value| {
            json!({
                "action": "respond-karma-candidate",
                "request_id": Value::Null,
                "candidate_hash": candidate.candidate_hash,
                "expected_state_revision": state.state_revision,
                "response": response,
            })
        };
        rows.push(json!({
            "object_kind": "candidate",
            "uid": candidate.candidate_hash,
            "candidate_hash": candidate.candidate_hash,
            "source_run_hash": candidate.proposal.source_run_hash,
            "occurrence_hash": candidate.proposal.occurrence_hash,
            "program_uid": candidate.proposal.program_uid,
            "program_revision_hash": candidate.proposal.program_revision_hash,
            "node_id": candidate.proposal.node_id,
            "output": candidate.proposal.output,
            "route": candidate.proposal.route,
            "template": candidate.proposal.template,
            "fields": candidate.proposal.fields,
            "status": match state.status {
                CandidateStatus::Proposed => "proposed",
                CandidateStatus::Accepted => "accepted",
                CandidateStatus::Dismissed => "dismissed",
                CandidateStatus::Snoozed => "snoozed",
                _ => "unsupported",
            },
            "state_revision": state.state_revision,
            "snoozed_until": state.snoozed_until,
            "current_event_hash": state.current_event_hash,
            "actor_person_uid": state.actor_person_uid,
            "updated_at": state.updated_at,
            "created_at": candidate.created_at,
            "capabilities": {
                "accept": true,
                "dismiss": true,
                "snooze": true,
            },
            "action_templates": {
                "accept": review(json!({"action": "accept"})),
                "dismiss": review(json!({"action": "dismiss"})),
                "snooze": review(json!({"action": "snooze", "until": Value::Null})),
            },
        }));
    }
    Ok(())
}
