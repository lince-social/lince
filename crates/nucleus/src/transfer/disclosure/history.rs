//! Strip private accounting and project proofs and shared simulations.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{ItemAccess, entries, hide};
use crate::simulation::sharing::Shared;

pub(super) fn hide_private_accounting(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for key in [
                "record_quantity",
                "availability",
                "application",
                "settlement_preview",
                "private_applications",
                "recorded_effects",
                "private_compensated_slices",
                "private_settlement_slices",
                "compensation",
                "formula",
            ] {
                object.remove(key);
            }
            for child in object.values_mut() {
                hide_private_accounting(child);
            }
        }
        Value::Array(values) => {
            for value in values {
                hide_private_accounting(value);
            }
        }
        _ => {}
    }
}

pub(super) fn project_history(
    row: &mut Value,
    access_by_promise: &BTreeMap<String, ItemAccess>,
    visible_people: &BTreeSet<String>,
    hidden_people: bool,
) {
    let visible = |value: &Value| {
        !hidden_people
            || value
                .as_str()
                .is_some_and(|person| visible_people.contains(person))
    };
    let shared_threads = entries(row, "threads")
        .filter_map(|thread| {
            let transfer = row["uid"].as_str()?;
            let revision = row["revision"].as_u64()?;
            let messages: Vec<_> = entries(thread, "messages")
                .filter_map(|message| {
                    let shared = Shared::parse(message["body"].as_str()?)?;
                    if !shared.disclosed(transfer, revision, access_by_promise) {
                        return None;
                    }
                    Some(json!({
                        "uid": message["uid"],
                        "created_at": message["created_at"],
                        "body": serde_json::to_string(&shared).ok()?,
                        "sender": message.get("sender").filter(|sender| visible(sender)),
                    }))
                })
                .collect();
            (!messages.is_empty()).then(|| {
                json!({
                    "uid": thread["uid"], "head": "Shared simulations", "messages": messages,
                })
            })
        })
        .collect();
    let authorship = |proof: &Value| {
        json!({
            "fact": proof.get("fact"),
            "actor": proof.get("actor").filter(|actor| visible(actor)),
            "at": proof.get("at"),
            "state": proof.get("state"),
            "authoritative": proof.get("authoritative"),
            "terms_disclosed": false,
        })
    };
    let history = entries(row, "timeline")
        .filter(|event| event.is_object())
        .map(|event| {
            json!({
                "uid":event.get("uid"), "kind":event.get("kind"), "at":event.get("at"),
                "fact":event.get("fact"), "proof":authorship(&event["proof"]),
            })
        })
        .collect::<Vec<_>>();
    let proof = row
        .get("proof")
        .filter(|proof| proof.is_object())
        .map(authorship);
    hide(row, &["proof", "timeline", "threads"]);
    row["timeline"] = Value::Array(history);
    row["threads"] = Value::Array(shared_threads);
    row["proof"] = json!(proof);
    row["history_disclosure"] =
        json!("Full signed history contains fields unavailable to this viewer");
}
