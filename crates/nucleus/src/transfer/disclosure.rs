//! Audience rules and the public projection of a Transfer.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Value, json};

mod history;
mod item;
mod terms;
#[cfg(test)]
mod tests;

use history::{hide_private_accounting, project_history};
use item::redact_item;
pub use terms::{AudienceScope, FieldAudience, ItemAccess, ItemDisclosure, TransferItem};

fn hide(row: &mut Value, fields: &[&str]) {
    if let Some(object) = row.as_object_mut() {
        for field in fields {
            object.insert((*field).into(), Value::Null);
        }
    }
}

fn hide_blocker_details(value: &mut Value) {
    if let Some(blockers) = value.as_array_mut() {
        for blocker in blockers {
            if blocker.is_object() {
                *blocker = json!({"code":blocker.get("code")});
            }
        }
    }
}

fn object_field<'a>(row: &'a mut Value, field: &str) -> &'a mut Value {
    let value = &mut row[field];
    if !value.is_object() {
        *value = json!({});
    }
    value
}

fn entries<'a>(row: &'a Value, field: &str) -> impl Iterator<Item = &'a Value> {
    row[field].as_array().into_iter().flatten()
}

fn decode_items(row: &Value) -> Option<Vec<(usize, TransferItem)>> {
    if !row.is_object() && !row.is_null() {
        return None;
    }
    let mut items = Vec::new();
    for (index, promise) in entries(row, "promises").enumerate() {
        if !promise.is_object() && !promise.is_null() {
            return None;
        }
        if let Some(raw) = promise.get("item").filter(|item| !item.is_null()) {
            items.push((index, TransferItem::deserialize(raw).ok()?));
        }
    }
    Some(items)
}

/// Apply the viewer's disclosure permissions to a Transfer projection in place.
///
/// A malformed Promise or item yields only the Transfer identifier and disclosure error.
pub fn project_transfer(row: &mut Value, viewer: Option<&str>, privileged: bool) {
    let Some(items) = decode_items(row) else {
        *row = json!({"kind":"transfer","uid":row["uid"],"disclosure_error":"invalid_item"});
        return;
    };
    if let Some(object) = row.as_object_mut() {
        object.remove("private_applications");
    }
    let redact_accounting = !items.is_empty() && !privileged;
    let mut items = items.into_iter().peekable();
    let participant = viewer.is_some_and(|viewer| {
        entries(row, "parties").any(|party| party["actor"] == viewer)
            || entries(row, "invitations").any(|invite| invite["addressed_person"] == viewer)
    });
    let mut access_by_promise = BTreeMap::new();
    let mut visible_people: BTreeSet<_> = viewer.into_iter().map(str::to_owned).collect();
    if let Some(promises) = row["promises"].as_array_mut() {
        for (index, promise) in promises.iter_mut().enumerate() {
            let item = items
                .next_if(|(item_index, _)| *item_index == index)
                .map(|(_, item)| item);
            let access = ItemAccess::for_item(
                item.as_ref(),
                viewer,
                promise["party"].as_str(),
                participant,
                privileged,
            );
            if access.parties
                && let Some(person) = promise["party"].as_str()
            {
                visible_people.insert(person.into());
            }
            if let Some(item) = item {
                if let Some(exchange) = item.exchange {
                    promise["exchange"] = Value::String(exchange.uid);
                    promise["giver"] = json!(exchange.giver);
                    promise["receiver"] = json!(exchange.receiver);
                    if access.parties {
                        visible_people.extend([exchange.giver, exchange.receiver]);
                    }
                }
                promise["title"] = Value::String(item.title);
                promise["description"] = Value::String(item.description);
                promise["loan"] = json!(item.loan);
                promise["return_of"] = json!(item.return_of);
                promise["future_need_for"] = json!(item.future_need_for);
                promise["transport"] = json!(item.transport);
            }
            if !privileged && !participant {
                hide(promise, &["return_of", "future_need_for", "accepted_loan"]);
            }
            if let Some(uid) = promise["uid"].as_str() {
                access_by_promise.insert(uid.to_string(), access);
            }
            if !privileged && let Some(object) = promise.as_object_mut() {
                object.remove("item");
            }
            redact_item(promise, access);
        }
    }
    let access = access_by_promise
        .values()
        .fold(ItemAccess::ALL, ItemAccess::intersect);
    let summary_only = !access.complete() || redact_accounting;
    if redact_accounting {
        hide_private_accounting(row);
    }
    if let Some(occurrences) = row["occurrences"].as_array_mut() {
        for occurrence in occurrences {
            if let Some(access) = occurrence["promise"]
                .as_str()
                .and_then(|uid| access_by_promise.get(uid))
                .copied()
            {
                redact_item(occurrence, access);
            }
        }
    }
    if summary_only {
        project_history(row, &access_by_promise, &visible_people, !access.parties);
    }
    // The summary's field list already drops Journey, capacity, source, and balance data.
    if !summary_only && !privileged {
        if !participant {
            hide(row, &["journey", "transport_capacity"]);
        } else if let Some(people) = row
            .get_mut("journey")
            .and_then(|journey| journey.get_mut("participants"))
            .and_then(Value::as_array_mut)
        {
            people.retain(|progress| {
                progress["person"]
                    .as_str()
                    .is_some_and(|person| visible_people.contains(person))
            });
        }
    }
    if !access.quantity {
        hide(row, &["settlement_progress"]);
    } else if !access.source {
        object_field(row, "settlement_progress")["by_resource"] = Value::Null;
    }
    if !access.location {
        hide(row, &["default_place"]);
    }
    if !access.parties {
        hide_blocker_details(&mut object_field(row, "readiness")["blockers"]);
        if let Some(parties) = row["parties"].as_array_mut() {
            parties.retain(|party| {
                party["actor"]
                    .as_str()
                    .is_some_and(|person| visible_people.contains(person))
            });
            for party in parties {
                hide_blocker_details(&mut object_field(party, "readiness")["blockers"]);
            }
        }
        hide(row, &["invitations", "agreement", "viewer_party"]);
    }
    if summary_only {
        if let Some(object) = row.as_object_mut() {
            object.retain(|key, _| {
                [
                    "kind",
                    "uid",
                    "slug",
                    "head",
                    "revision",
                    "status",
                    "primary_status",
                    "operational_status",
                    "inbox_facets",
                    "active",
                    "agreement_type",
                    "agreement_pct",
                    "settlement",
                    "visibility",
                    "max_proximity",
                    "satiation",
                    "reserve_default",
                    "require_confirmation",
                    "default_place",
                    "viewer_party",
                    "viewer_roles",
                    "timeline",
                    "proof",
                    "history_disclosure",
                    "capabilities",
                    "blocking_reasons",
                    "agreement",
                    "readiness",
                    "outcome",
                    "progress",
                    "settlement_progress",
                    "correction_status",
                    "parties",
                    "promises",
                    "occurrences",
                    "invitations",
                    "threads",
                ]
                .contains(&key.as_str())
            });
        }
        row["visibility_projection"] = json!({"policy":row["visibility"],"scope":"item_fields","disclosure":{"field_overrides_supported":true}});
    } else if row["visibility_projection"].is_object() {
        object_field(&mut row["visibility_projection"], "disclosure")["field_overrides_supported"] =
            json!(true);
    }
}
