use super::*;
use ui::{Command, button, row};

const PAGE: usize = 32;

pub(super) fn description(content: &Value) -> String {
    use nucleus::transfer::receipt::Content;
    match serde_json::from_value::<Content>(content.clone()) {
        Ok(Content::Need { requested, .. }) => format!("Receiver Need · requested {requested}"),
        Ok(Content::Statement {
            role,
            quantity,
            withdrawn,
            ..
        }) => format!(
            "{role:?} · {}",
            if withdrawn {
                "withdrawn".into()
            } else {
                format!("reported {quantity}")
            }
        ),
        Ok(Content::Settled { quantity }) => quantity
            .value()
            .map(|quantity| {
                format!("Canonical settlement · cumulative {quantity} (recorded binary64)")
            })
            .unwrap_or_else(|_| "Invalid recorded settlement quantity".into()),
        Ok(Content::SettlementStatus {
            quantity,
            remaining_after,
        }) => match (quantity.value(), remaining_after.value()) {
            (Ok(quantity), Ok(remaining)) => format!(
                "Canonical obligation · {} · cumulative {quantity} · remaining {remaining} (recorded binary64; not Need fulfillment)",
                if remaining == 0.0 {
                    "fully settled"
                } else {
                    "partially settled"
                }
            ),
            _ => "Invalid recorded settlement status".into(),
        },
        Ok(Content::Cancelled { quantity, revision }) => {
            format!(
                "Cancelled remainder · {quantity} · agreed revision {revision} (not delivered quantity)"
            )
        }
        Ok(Content::Compensated { role, .. }) => {
            format!("{role:?} · private application compensated (canonical settlement unchanged)")
        }
        Ok(Content::Dispute { role, active }) => format!(
            "{role:?} · {}",
            if active {
                "disputed"
            } else {
                "dispute cleared"
            }
        ),
        Err(_) => model::human(&text(content, "kind")),
    }
}

pub(super) fn history_form(person: &str, scope: &Value) -> Form {
    let mut form = receipts::action_form(
        person,
        "Source outcome history",
        json!({"kind":"history","origin":scope["source"]["origin"],"occurrence":scope["source"]["occurrence"]}),
    );
    form.review = json!({"Coverage":"Read the signed receipts you can currently access on this Lince. Copies describe the same outcome; conflicting statements remain visible. This view does not add quantities or establish physical delivery."});
    form
}

pub(super) fn continuation_form(person: &str, value: &Value) -> Form {
    let mut form = history_form(
        person,
        &json!({"source":{"origin":value["origin"],"occurrence":value["occurrence"]}}),
    );
    form.data["request"]["before"] = value["next_before"].clone();
    form.review["Page"] = json!(
        "Open another page with fresh permission and authority checks. Earlier pages are not combined into a total or a complete correction history."
    );
    form
}

pub(super) fn render(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    person: &str,
    document: &Value,
) -> bool {
    if document["kind"] != "source_history" {
        return false;
    }
    let value = &document["value"];
    let history = &value["history"];
    crate::native::edit_mode::label(world, parent, "Source outcome history", 15.0);
    crate::native::edit_mode::label(world, parent, &text(value, "coverage"), 12.0);
    if let Some(checked_at) = value["checked_at"]
        .as_i64()
        .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
    {
        crate::native::edit_mode::label(
            world,
            parent,
            &format!(
                "Evidence checked {}. Refresh to check current access and authority.",
                checked_at.to_rfc3339()
            ),
            12.0,
        );
    }
    if value["partial"] == true {
        crate::native::edit_mode::label(
            world,
            parent,
            "Partial history: other pages, a work limit or unavailable proof prevent a complete view.",
            13.0,
        );
    }
    if history["source_conflict"] == true {
        crate::native::edit_mode::label(
            world,
            parent,
            "Conflicting source terms. These outcomes cannot support a combined contribution total.",
            13.0,
        );
        if !array(history, "person_conflicts").is_empty() {
            forms::review(
                world,
                parent,
                &json!({"Conflicting public Person attributions":history["person_conflicts"]}),
                0,
            );
        }
    }
    for failure in array(value, "failures") {
        crate::native::edit_mode::label(
            world,
            parent,
            &receipts::verification_message(failure["state"].as_str(), &text(failure, "message")),
            12.0,
        );
    }
    let outcomes = array(history, "outcomes");
    if outcomes.is_empty() {
        crate::native::edit_mode::label(
            world,
            parent,
            "No verified outcome evidence in this permitted local view. This does not establish non-delivery.",
            13.0,
        );
    }
    let unresolved = array(value, "unresolved");
    let pages = outcomes.len().max(unresolved.len()).div_ceil(PAGE).max(1);
    let page = world
        .get::<View>(owner)
        .unwrap()
        .receipt_page
        .min(pages - 1);
    let controls = row(world, parent);
    if page > 0 {
        button(
            world,
            controls,
            owner,
            "Previous outcomes",
            Command::ReceiptPage(page - 1),
        );
    }
    if page + 1 < pages {
        button(
            world,
            controls,
            owner,
            "More outcomes",
            Command::ReceiptPage(page + 1),
        );
    }
    for outcome in outcomes.iter().skip(page * PAGE).take(PAGE) {
        let copies = array(outcome, "copies");
        let Some(first) = copies.first() else {
            continue;
        };
        let slots = array(history, "slots");
        let head = slots
            .iter()
            .any(|slot| array(slot, "heads").contains(&outcome["id"]));
        let conflict = outcome["conflicted"] == true
            || slots.iter().any(|slot| {
                array(outcome, "slots").contains(&slot["id"]) && slot["conflicted"] == true
            });
        let label = format!(
            "{} · {} · {} signed copies",
            description(&first["content"]),
            if conflict {
                "conflicting"
            } else if head {
                "current visible head"
            } else {
                "superseded"
            },
            copies.len()
        );
        detail::evidence(
            world,
            owner,
            parent,
            &format!("history-{}", text(outcome, "id")),
            &label,
            outcome,
        );
        let expanded = world
            .get::<View>(owner)
            .unwrap()
            .expanded
            .contains(&format!("history-{}", text(outcome, "id")));
        if expanded {
            for copy in copies {
                button(
                    world,
                    parent,
                    owner,
                    &format!(
                        "Open original version {} · {}",
                        text(copy, "version"),
                        text(copy, "record")
                    ),
                    Command::Open(receipts::action_form(
                        person,
                        "Inspect signed receipt",
                        json!({"kind":"inspect_version","record":copy["record"],"version":copy["version"]}),
                    )),
                );
            }
        }
    }
    if !unresolved.is_empty() {
        crate::native::edit_mode::label(
            world,
            parent,
            "Signed claims awaiting valid dependency proofs. These are excluded from accepted outcomes.",
            13.0,
        );
        for claim in unresolved.iter().skip(page * PAGE).take(PAGE) {
            detail::evidence(
                world,
                owner,
                parent,
                &format!("pending-{}", text(claim, "event")),
                &receipts::verification_message(claim["state"].as_str(), &text(claim, "reason")),
                &claim["content"],
            );
            button(
                world,
                parent,
                owner,
                "Open unresolved original version",
                Command::Open(receipts::action_form(
                    person,
                    "Inspect original signed claim",
                    json!({"kind":"inspect_version","record":claim["record"],"version":claim["version"]}),
                )),
            );
        }
    }
    if value["next_before"].is_string() {
        button(
            world,
            parent,
            owner,
            "Older evidence Records",
            Command::Open(continuation_form(person, value)),
        );
    }
    button(
        world,
        parent,
        owner,
        "Refresh source history",
        Command::Open(history_form(
            person,
            &json!({"source":{"origin":value["origin"],"occurrence":value["occurrence"]}}),
        )),
    );
    true
}
