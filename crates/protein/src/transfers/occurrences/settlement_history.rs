//! Settlement slices with local effects and compensation details for authorized viewers.

use crate::ProteinError;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use store::{
    Store,
    transfer_effects::recorded,
    transfers::{OccurrenceSettlementCompensationRow, OccurrenceSettlementSliceRow},
};

pub(super) struct History {
    pub rows: Vec<Value>,
    pub private_slices: usize,
    pub compensated_slices: usize,
    pub can_compensate: bool,
}

pub(super) async fn project(
    store: &Store,
    slices: &[OccurrenceSettlementSliceRow],
    compensations: &[OccurrenceSettlementCompensationRow],
    disclosure: Option<&[&'static str]>,
) -> Result<History, ProteinError> {
    let by_settlement: HashMap<_, _> = if disclosure.is_some() {
        compensations
            .iter()
            .map(|item| (item.settlement_uid.as_str(), item))
            .collect()
    } else {
        HashMap::new()
    };
    let mut history = History {
        rows: Vec::with_capacity(slices.len()),
        private_slices: 0,
        compensated_slices: 0,
        can_compensate: false,
    };
    for slice in slices {
        let mut row: Map<_, _> = [
            ("uid", json!(slice.uid)),
            ("occurrence", json!(slice.occurrence_uid)),
            ("promise", json!(slice.promise_uid)),
            ("owner_person", json!(slice.owner_person_uid)),
            ("canonical_quantity", json!(slice.canonical_quantity)),
            ("canonical_unit", json!(slice.canonical_unit_uid)),
            ("cumulative_before", json!(slice.cumulative_before)),
            ("cumulative_after", json!(slice.cumulative_after)),
            ("remaining_after", json!(slice.remaining_after)),
            ("evidence_fact", json!(slice.evidence_fact_uid)),
            (
                "application_formula_hash",
                json!(slice.application_formula_hash),
            ),
            (
                "application_formula_version",
                json!(slice.application_formula_version),
            ),
            ("remainder_policy", json!(slice.remainder_policy.as_str())),
            ("request_id", json!(slice.idempotency_key)),
            ("at", json!(slice.created_at)),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect();
        if let Some(disclosed_blockers) = disclosure {
            let effects = recorded(&store.pool, &slice.application_fact_uid).await?;
            let compensation = by_settlement.get(slice.uid.as_str()).copied();
            let mut blockers = disclosed_blockers.to_vec();
            if compensation.is_some() {
                blockers.push("settlement_already_compensated");
                history.compensated_slices += 1;
            }
            let can_compensate = blockers.is_empty();
            history.can_compensate |= can_compensate;
            history.private_slices += 1;
            let status = if compensation.is_some() {
                "compensated"
            } else {
                "applied"
            };
            row.extend(
                [
                    ("recorded_effects", json!(effects)),
                    ("application_fact", json!(slice.application_fact_uid)),
                    ("local_record", json!(slice.local_record_uid)),
                    ("local_delta", json!(slice.local_delta)),
                    (
                        "local_cumulative_before",
                        json!(slice.local_cumulative_before),
                    ),
                    (
                        "local_cumulative_after",
                        json!(slice.local_cumulative_after),
                    ),
                    ("application_formula", json!(slice.application_formula)),
                    ("compensation_status", json!(status)),
                    ("compensation", json!(compensation.map(compensation_row))),
                    ("capabilities", json!({ "compensate": can_compensate })),
                    ("blocking_reasons", json!({ "compensate": blockers })),
                ]
                .into_iter()
                .map(|(key, value)| (key.into(), value)),
            );
        }
        history.rows.push(row.into());
    }
    Ok(history)
}

fn compensation_row(compensation: &OccurrenceSettlementCompensationRow) -> Value {
    json!({
        "uid": compensation.uid,
        "fact": compensation.compensation_fact_uid,
        "original_application_fact": compensation.original_application_fact_uid,
        "local_record": compensation.local_record_uid,
        "inverse_delta": compensation.inverse_delta,
        "request_id": compensation.idempotency_key,
        "at": compensation.created_at,
    })
}
