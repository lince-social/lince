//! Redact one Promise or occurrence and preserve its public cancellation actions.

use serde_json::{Value, json};

use super::{ItemAccess, hide, hide_blocker_details, object_field};

fn cancellation_action(mut value: Value) -> Value {
    let fields = [
        "action",
        "transfer",
        "occurrence",
        "cancellation",
        "expected_revision",
        "expected_remaining_quantity",
        "person",
        "request_id",
    ];
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, _| fields.contains(&key.as_str()));
    }
    value
}

fn take_field(row: &mut Value, key: &str) -> Value {
    row.get_mut(key).map(Value::take).unwrap_or_default()
}

fn project_cancellation(mut entry: Value, parties: bool) -> Value {
    let mut result = Value::from_iter(
        [
            "uid",
            "revision",
            "quantity",
            "status",
            "proposal_fact",
            "applied_fact",
            "all_agreed",
        ]
        .into_iter()
        .map(|key| (key, take_field(&mut entry, key))),
    );
    if parties {
        let mut capabilities = take_field(&mut entry, "capabilities");
        let mut actions = take_field(&mut entry, "action_payloads");
        result["capabilities"] = Value::from_iter([(
            "apply_cancellation",
            take_field(&mut capabilities, "apply_cancellation"),
        )]);
        result["action_payloads"] = Value::from_iter([(
            "apply_cancellation",
            cancellation_action(take_field(&mut actions, "apply_cancellation")),
        )]);
    }
    result
}

pub(super) fn redact_item(row: &mut Value, access: ItemAccess) {
    if !access.parties || !access.quantity {
        hide(
            row,
            &["loan", "accepted_loan", "return_of", "future_need_for"],
        );
    }
    let cancellation_action_payload = if access.quantity && access.parties {
        row.get_mut("action_payloads")
            .and_then(|actions| actions.get_mut("propose_cancellation"))
            .filter(|action| action.is_object())
            .map(|action| cancellation_action(action.take()))
    } else {
        None
    };
    let cancellations = if access.quantity {
        row.get_mut("cancellations")
            .and_then(Value::as_array_mut)
            .map(|rows| {
                std::mem::take(rows)
                    .into_iter()
                    .map(|entry| project_cancellation(entry, access.parties))
                    .collect()
            })
    } else {
        None
    };
    if !access.title {
        hide(row, &["title"]);
    }
    if !access.description {
        hide(row, &["description"]);
    }
    if !access.source {
        hide(
            row,
            &[
                "record",
                "record_head",
                "record_slug",
                "record_quantity",
                "concept",
                "concept_name",
            ],
        );
    }
    if !access.parties {
        hide_blocker_details(&mut row["agreement_blockers"]);
        hide(
            row,
            &[
                "party",
                "proposer",
                "proposer_head",
                "proposer_slug",
                "giver",
                "receiver",
            ],
        );
    }
    if !access.quantity {
        hide(
            row,
            &[
                "delta",
                "quantity",
                "direction",
                "unit",
                "unit_name",
                "settlement_progress",
            ],
        );
    }
    if !access.location {
        hide(row, &["place"]);
    }
    if !access.location || !access.quantity {
        hide(row, &["transport"]);
    }
    if !access.complete() {
        hide(row, &["condition"]);
        if let Some(progress) = row
            .get_mut("settlement_progress")
            .and_then(Value::as_object_mut)
        {
            progress.remove("slices");
        }
        if let Some(object) = row.as_object_mut() {
            object.retain(|key, _| {
                [
                    "uid",
                    "revision",
                    "title",
                    "description",
                    "record",
                    "record_head",
                    "record_slug",
                    "record_quantity",
                    "concept",
                    "concept_name",
                    "unit",
                    "unit_name",
                    "delta",
                    "quantity",
                    "direction",
                    "state",
                    "status",
                    "party",
                    "open",
                    "proposer",
                    "proposer_head",
                    "proposer_slug",
                    "withdrawn",
                    "window_start",
                    "window_end",
                    "place",
                    "transport",
                    "reuse_policy",
                    "condition",
                    "reserve_from",
                    "agreement_ready",
                    "agreement_eligible",
                    "agreement_blockers",
                    "capabilities",
                    "blocking_reasons",
                    "promise",
                    "opposite_promise",
                    "exchange_path",
                    "exchange",
                    "loan",
                    "accepted_loan",
                    "return_of",
                    "future_need_for",
                    "giver",
                    "receiver",
                    "delivery_claimed",
                    "receipt_claimed",
                    "confirmed_conclusion",
                    "disputed",
                    "settlement_progress",
                ]
                .contains(&key.as_str())
            });
        }
    }
    if let Some(cancellations) = cancellations {
        row["cancellations"] = Value::Array(cancellations);
    }
    if let Some(action) = cancellation_action_payload {
        object_field(row, "action_payloads")["propose_cancellation"] = action;
    }
    row["disclosed"] = json!({
        "title": access.title, "description": access.description, "source": access.source,
        "parties": access.parties, "quantity": access.quantity, "location": access.location,
    });
}
