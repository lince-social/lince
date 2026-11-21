use serde_json::{Value, json};
use store::transfers::{TransferCorrectionLinkRow, TransferSourceGroupState};

pub(super) fn first_completes(state: &TransferSourceGroupState) -> Value {
    let result = &state.result;
    json!({
        "state": if state.satiated { "satiated" } else { "completed" },
        "satiated": state.satiated,
        "result": {
            "uid": result.uid,
            "source": result.source_uid,
            "policy": result.policy,
            "winner": result.winner_transfer_uid,
            "winner_revision": result.winner_revision,
            "settlement": result.settlement_uid,
            "fact": result.fact_uid,
            "at": result.created_at,
        },
        "loser": state.loser.as_ref().map(|loser| json!({
            "uid": loser.uid,
            "transfer": loser.transfer_uid,
            "revision": loser.transfer_revision,
            "fact": loser.fact_uid,
            "at": loser.created_at,
        })),
    })
}

pub(super) fn corrections(
    links: &[TransferCorrectionLinkRow],
    uid: &str,
    recipient_details_authorized: bool,
) -> Vec<Value> {
    links
        .iter()
        .map(|lineage| {
            let role = if lineage.source_transfer_uid == uid {
                "source"
            } else {
                "created"
            };
            if recipient_details_authorized {
                json!({
                    "uid": lineage.uid,
                    "kind": lineage.kind,
                    "role": role,
                    "source_transfer": lineage.source_transfer_uid,
                    "source_occurrence": lineage.source_occurrence_uid,
                    "created_transfer": lineage.created_transfer_uid,
                    "source_revision": lineage.source_revision,
                    "canonical_quantity": lineage.canonical_quantity,
                    "actor": lineage.actor_person_uid,
                    "fact": lineage.fact_uid,
                    "request_id": lineage.idempotency_key,
                    "at": lineage.created_at,
                })
            } else {
                json!({ "kind": lineage.kind, "role": role })
            }
        })
        .collect()
}
