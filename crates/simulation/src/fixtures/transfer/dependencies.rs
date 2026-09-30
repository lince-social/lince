use engine::actions::Action;
use nucleus::transfer_delivery::TransferDeliveryMode;
use serde_json::json;

use crate::scenario::{Event, Scenario};

use super::routes::{action, add, decision, publish, send};

fn refresh(case: &mut Scenario, stage: &str) {
    add(
        case,
        "d",
        &format!("refresh-{stage}"),
        Event::RefreshTransfer {
            transfer: "$routes".into(),
            person: "$dora".into(),
        },
    );
    send(case, "d", "a", &format!("observed-{stage}"));
}

pub fn observer_outcomes(replicated: bool) -> Scenario {
    let mut case = super::three_parties();
    case.name = format!(
        "transfer-observer-{}",
        if replicated { "replicated" } else { "hosted" }
    );
    let mut revised = case
        .inputs
        .iter()
        .find_map(|input| {
            if input.id == "routes"
                && let Event::Action { invocation } = &input.event
            {
                Some(serde_json::to_value(&invocation.action).unwrap())
            } else {
                None
            }
        })
        .unwrap();
    revised.as_object_mut().unwrap().remove("action");
    revised.as_object_mut().unwrap().remove("request_id");
    revised["head"] = json!("Revised supply plan");
    revised["invitees"] = json!([]);
    let observer = case
        .inputs
        .iter()
        .position(|input| input.id == "blocked-observer-delivery")
        .unwrap();
    let mut observation = case.inputs.split_off(observer);
    observation.retain(|input| input.id != "restart-c");
    for input in &mut observation {
        if let Event::Action { invocation } = &mut input.event
            && let Action::ConfigureTransferDelivery { mode, .. } = &mut invocation.action
        {
            *mode = if replicated {
                TransferDeliveryMode::Replicated
            } else {
                TransferDeliveryMode::Hosted
            };
        }
    }
    let agreed = case
        .inputs
        .iter()
        .position(|input| input.id == "agree-a-1")
        .unwrap();
    let work = case.inputs.split_off(agreed);
    case.inputs.extend(observation);
    action(
        &mut case,
        "d",
        "agreement-observer",
        json!({
            "action":"create-transfer-draft","request_id":"agreement-observer","creator":"$dora",
            "head":"Supplies agreed","agreement":"dependency","visibility":"hidden","reserve_default":"none",
            "promises":[{"uid":"observer-agreed","party":"$dora","delta":1,"item":{"title":"Supplies agreed"}}],
            "dependencies":[{"scope":"transfer","upstream_kind":"transfer","upstream":"$routes","required_state":"agreed"}]
        }),
    );
    let mut changed = false;
    for mut input in work {
        let agreed = input.id == "agree-c-2";
        if changed {
            let invocation = match &mut input.event {
                Event::Action { invocation } | Event::TransferCommand { invocation, .. } => {
                    Some(invocation)
                }
                _ => None,
            };
            if let Some(invocation) = invocation {
                match &mut invocation.action {
                    Action::ActivateTransferOccurrence {
                        expected_revision, ..
                    }
                    | Action::BeginTransferSettlement {
                        expected_revision, ..
                    } => *expected_revision = 4,
                    _ => {}
                }
            }
        }
        case.inputs.push(input);
        if agreed {
            refresh(&mut case, "agreed");
            action(
                &mut case,
                "a",
                "revise-outcome",
                json!({
                    "action":"revise-transfer-draft", "transfer":"$routes", "expected_revision":3,
                    "request_id":"revise-outcome", "draft":revised
                }),
            );
            refresh(&mut case, "revised");
            for peer in ["b", "c"] {
                publish(&mut case, peer, &format!("revision-{peer}"));
            }
            for (cell, person) in [("a", "ana"), ("b", "beto"), ("c", "carla")] {
                for level in [1, 2] {
                    let id = format!("reagree-{cell}-{level}");
                    decision(
                        &mut case,
                        cell,
                        &id,
                        json!({
                            "action":"set-transfer-agreement-level", "transfer":"$routes", "person":format!("${person}"),
                            "expected_revision":4, "request_id":id, "level":level
                        }),
                    );
                }
            }
            changed = true;
        }
    }
    refresh(&mut case, "settled");
    add(&mut case, "d", "observer-restart", Event::Restart {});
    action(
        &mut case,
        "a",
        "correct-source-accounting",
        json!({
            "action":"compensate-transfer-occurrence-settlement", "settlement":"$settle-ana-beto",
            "person":"$ana", "request_id":"correct-source-accounting"
        }),
    );
    refresh(&mut case, "corrected");
    action(
        &mut case,
        "a",
        "dispute-outcome",
        json!({
            "action":"set-transfer-occurrence-dispute","occurrence":"$activate-ana-beto","person":"$ana",
            "request_id":"dispute-outcome","disputed":true
        }),
    );
    refresh(&mut case, "disputed");
    action(
        &mut case,
        "a",
        "resolve-outcome",
        json!({
            "action":"set-transfer-occurrence-dispute","occurrence":"$activate-ana-beto","person":"$ana",
            "request_id":"resolve-outcome","disputed":false
        }),
    );
    refresh(&mut case, "restored");
    let fresh = case.inputs.len();
    refresh(&mut case, "fresh");
    action(
        &mut case,
        "a",
        "revoke-observer",
        json!({
            "action":"revoke-transfer-delivery","transfer":"$routes","delivery":"$policy-d","person":"$ana",
            "expected_revision":1,"request_id":"revoke-observer"
        }),
    );
    refresh(&mut case, "revoked");
    add(&mut case, "d", "revoked-restart", Event::Restart {});
    for (index, input) in case.inputs.iter_mut().enumerate() {
        input.at_ms = case.start_ms + index as i64 * 20 + if index >= fresh { 130_000 } else { 0 };
    }
    case.end_ms = case.inputs.last().unwrap().at_ms + 100;
    for check in &mut case.checks {
        if let nucleus::simulation::Predicate::QuantityEquals {
            cell,
            expected,
            at_ms,
            ..
        } = &mut check.predicate
        {
            *at_ms = case.end_ms;
            if cell == "a" {
                expected.value = nucleus::DecimalValue::parse_inferred("31").unwrap();
            }
        }
    }
    case
}
