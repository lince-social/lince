use super::*;
use model::FieldKind;
use nucleus::transfer::outcome::{Measure, Request, Role};
use ui::{Command, button, row};

fn action_form(
    transfer: &Value,
    occurrence: &Value,
    person: &str,
    title: &str,
    request: Value,
) -> Form {
    let mut form = Form::action(
        title,
        json!({
            "action":"record-transfer-outcome",
            "occurrence":occurrence["uid"],
            "expected_revision":transfer["revision"],
            "person":person,
            "request":request,
        }),
    );
    form.transfer = transfer["uid"].as_str().map(str::to_owned);
    form.revision = transfer["revision"].as_u64();
    form.review = json!({
        "scope":"Private outcome Record, with access initially granted only to its author.",
        "effect":"This report does not confirm completion, settle the Transfer or change private Record quantities.",
        "history":"Corrections append a signed version. Previous statements remain available.",
    });
    form
}

pub(super) fn statement_form(
    transfer: &Value,
    occurrence: &Value,
    person: &str,
    title: &str,
    request: Request,
) -> Form {
    let mut form = action_form(transfer, occurrence, person, title, json!(request));
    form.field(
        "/request/amount",
        "Actual amount (exact decimal)",
        FieldKind::Text,
    );
    form
}

fn previous_versions<'a>(
    entries: &'a [Value],
    person: &str,
    kind: &str,
    measure: &Value,
) -> Vec<Option<&'a Value>> {
    let matches: Vec<_> = entries
        .iter()
        .filter(|entry| {
            let content = &entry["evidence"]["content"];
            let previous_measure = if kind == "need_binding" {
                &content["terms"]["measure"]
            } else {
                &content["measure"]
            };
            entry["superseded"] != true
                && entry["evidence"]["author"].as_str() == Some(person)
                && content["kind"] == kind
                && previous_measure == measure
        })
        .map(Some)
        .collect();
    if matches.is_empty() {
        vec![None]
    } else {
        matches
    }
}

pub(super) fn render(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    transfer: &Value,
    occurrence: &Value,
) {
    let history = &occurrence["outcomes"];
    let entries = array(history, "entries");
    crate::native::edit_mode::label(world, parent, "Outcome reports", 14.0);
    crate::native::edit_mode::label(
        world,
        parent,
        "Requested, giver-reported, receiver-confirmed and settled amounts are separate. Reports do not change your private Record effects.",
        13.0,
    );
    let person = person(world, owner);
    let can_report = capability(occurrence, "report_outcome");
    let role = if occurrence["giver"].as_str() == Some(&person) {
        Some(Role::Giver)
    } else if occurrence["receiver"].as_str() == Some(&person) {
        Some(Role::Receiver)
    } else {
        None
    };
    if can_report && role == Some(Role::Receiver) {
        for option in array(occurrence, "need_outcome_options") {
            for previous in previous_versions(
                entries,
                &person,
                "need_binding",
                &option["terms"]["measure"],
            ) {
                let request = json!({"kind":"bind_need","selection":option["terms"]["selection"],"expected_measure":option["terms"]["measure"],
                    "supersedes":previous.map(|entry| entry["evidence"]["record"].clone())});
                let mut form = action_form(
                    transfer,
                    occurrence,
                    &person,
                    "Review requested outcome",
                    request,
                );
                form.review["requested"] = option["terms"].clone();
                form.review["previous_version"] = previous.cloned().unwrap_or(Value::Null);
                let suffix = previous
                    .map(|entry| format!(" · correct {}", text(&entry["evidence"], "record")))
                    .unwrap_or_default();
                button(
                    world,
                    parent,
                    owner,
                    &format!("Bind Need · {}{suffix}", text(option, "label")),
                    Command::Open(form),
                );
            }
        }
    }
    if can_report && let Some(role) = role {
        for option in array(occurrence, "outcome_measure_options") {
            let Ok(measure) = serde_json::from_value::<Measure>(option["measure"].clone()) else {
                continue;
            };
            for previous in previous_versions(entries, &person, "statement", &option["measure"]) {
                let request = Request::State {
                    role,
                    measure: measure.clone(),
                    amount: "0".into(),
                    withdrawn: false,
                    need_binding: option["need_binding"].as_str().map(str::to_owned),
                    supersedes: previous
                        .and_then(|entry| entry["evidence"]["record"].as_str())
                        .map(str::to_owned),
                };
                let title = if role == Role::Receiver {
                    "Report received amount"
                } else {
                    "Report delivered amount"
                };
                let mut form = statement_form(transfer, occurrence, &person, title, request);
                form.review["meaning"] = option.clone();
                form.review["previous_version"] = previous.cloned().unwrap_or(Value::Null);
                let suffix = previous
                    .map(|entry| format!(" · correct {}", text(&entry["evidence"], "record")))
                    .unwrap_or_default();
                button(
                    world,
                    parent,
                    owner,
                    &format!("{title} · {}{suffix}", text(option, "label")),
                    Command::Open(form),
                );
            }
        }
    }
    for entry in entries {
        let evidence = &entry["evidence"];
        let content = &evidence["content"];
        let uid = text(evidence, "record");
        let state = if entry["superseded"] == true {
            "previous"
        } else if entry["conflicting"] == true {
            "conflicting versions"
        } else if content["withdrawn"] == true {
            "withdrawn"
        } else {
            "current"
        };
        let label = if content["kind"] == "need_binding" {
            format!(
                "Need requested {} · {state}",
                text(&content["terms"]["requested"], "value")
            )
        } else {
            format!(
                "{} reported {} · {state}",
                text(content, "role"),
                text(&content["quantity"], "value")
            )
        };
        detail::evidence(
            world,
            owner,
            parent,
            &format!("outcome-{uid}"),
            &label,
            entry,
        );
        if evidence["author"].as_str() == Some(&person) {
            crate::native::visibility_castle::record_controls(world, parent, &uid);
        }
        if can_report
            && evidence["author"].as_str() == Some(&person)
            && content["kind"] == "statement"
            && entry["superseded"] != true
            && content["withdrawn"] != true
        {
            let mut request = content.clone();
            request["kind"] = json!("state");
            request.as_object_mut().unwrap().remove("quantity");
            request["amount"] = json!("0");
            request["withdrawn"] = json!(true);
            request["supersedes"] = json!(uid);
            let form = action_form(
                transfer,
                occurrence,
                &person,
                "Withdraw this report",
                request,
            );
            let controls = row(world, parent);
            button(
                world,
                controls,
                owner,
                "Withdraw this report",
                Command::Open(form),
            );
        }
    }
    if history["more"] == true {
        crate::native::edit_mode::label(
            world,
            parent,
            "Partial history: the first 256 permitted outcome Records are shown.",
            13.0,
        );
    }
    if occurrence["need_outcome_options_more"] == true {
        crate::native::edit_mode::label(
            world,
            parent,
            "The first 64 Need choices are shown.",
            13.0,
        );
    }
    if can_report
        && role == Some(Role::Receiver)
        && array(occurrence, "need_outcome_options").is_empty()
    {
        crate::native::edit_mode::label(
            world,
            parent,
            "Bind a Need you authored with assertions and units in a named Vocabulary. Existing bindings remain available after its quantity is fulfilled.",
            13.0,
        );
    }
    if can_report && array(occurrence, "outcome_measure_options").is_empty() {
        crate::native::edit_mode::label(
            world,
            parent,
            "Choose meanings through a receiver Need binding or assertions on your source Record in a named Vocabulary.",
            13.0,
        );
    }
}
