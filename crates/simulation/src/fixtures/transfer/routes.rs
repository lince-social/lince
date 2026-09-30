use nucleus::simulation::{Predicate, Quantity};
use serde_json::{Value, json};

use crate::scenario::{Cell, Check, Event, Input, Invocation, Scenario};

pub(super) fn invocation(id: &str, value: Value) -> Invocation {
    Invocation {
        id: id.into(),
        actor: None,
        action: serde_json::from_value(value).unwrap(),
    }
}

pub(super) fn add(case: &mut Scenario, cell: &str, id: &str, event: Event) {
    case.inputs.push(Input {
        id: id.into(),
        cell: cell.into(),
        at_ms: case.start_ms + case.inputs.len() as i64 * 20,
        event,
    });
}

pub(super) fn action(case: &mut Scenario, cell: &str, id: &str, value: Value) {
    add(
        case,
        cell,
        id,
        Event::Action {
            invocation: invocation(id, value),
        },
    );
}

pub(super) fn decision(case: &mut Scenario, cell: &str, id: &str, value: Value) {
    if cell == "a" {
        action(case, cell, id, value);
    } else {
        add(
            case,
            cell,
            id,
            Event::TransferCommand {
                peer: "a".into(),
                transfer: "$routes".into(),
                invocation: invocation(id, value),
                delay_ms: 1,
                copies: 1,
                duplicate_spacing_ms: 0,
                drop: false,
            },
        );
    }
}

pub(super) fn send(case: &mut Scenario, from: &str, to: &str, id: &str) {
    add(
        case,
        from,
        id,
        Event::TransferDelivery {
            peer: to.into(),
            delay_ms: 1,
            copies: 1,
            duplicate_spacing_ms: 0,
            drop: false,
        },
    );
}

pub(super) fn publish(case: &mut Scenario, peer: &str, id: &str) {
    action(
        case,
        "a",
        &format!("enqueue-{id}"),
        json!({
            "action":"enqueue-transfer-delivery", "transfer":"$routes", "delivery":format!("$policy-{peer}"),
            "person":"$ana", "request_id":format!("enqueue-{id}")
        }),
    );
    send(case, "a", peer, &format!("send-{id}"));
}

pub fn three_parties() -> Scenario {
    let mut case = super::super::daily();
    case.name = "transfer-three-parties-observer".into();
    case.end_ms = case.start_ms + 10_000;
    case.cells.clear();
    case.checks.retain(|check| {
        matches!(
            check.predicate,
            Predicate::NoUnexpectedRefusals {} | Predicate::FactChain {}
        )
    });
    for (cell, person, stock) in [
        ("a", "ana", 30),
        ("b", "beto", 0),
        ("c", "carla", 5),
        ("d", "dora", 0),
    ] {
        case.cells.push(Cell { name: cell.into(), database: None, lingua: Vec::new(), seed: vec![
            invocation(person, json!({"action":"create-record","slug":person,"kind":"person","head":person,"body":"","quantity":1})),
            invocation(&format!("stock-{cell}"), json!({"action":"create-record","slug":format!("stock-{cell}"),"kind":"plain","head":"Private crates","body":"","quantity":stock})),
            invocation(&format!("public-{cell}"), json!({"action":"grant-visibility","subject_kind":"public","target":format!("${person}")})),
        ] });
        add(
            &mut case,
            cell,
            &format!("key-{cell}"),
            Event::PersonKey {
                person: format!("${person}"),
            },
        );
    }
    for (from, to) in [("a", "b"), ("a", "c"), ("b", "c"), ("a", "d")] {
        add(
            &mut case,
            from,
            &format!("pair-{from}-{to}"),
            Event::Pair { peer: to.into() },
        );
        for (from, to) in [(from, to), (to, from)] {
            action(
                &mut case,
                from,
                &format!("private-stock-{from}-{to}"),
                json!({
                    "action":"hide-record-from-contact","target":format!("$cell:{to}:organ"),"record":format!("$stock-{from}"),"hidden":true
                }),
            );
            action(
                &mut case,
                from,
                &format!("policy-sync-{from}-{to}"),
                json!({
                    "action":"set-sync-policy","target":format!("$cell:{to}:organ"),"sync_in":true,"sync_out":true
                }),
            );
            add(
                &mut case,
                from,
                &format!("identities-{from}-{to}"),
                Event::Sync {
                    peer: to.into(),
                    delay_ms: 1,
                    copies: 1,
                    duplicate_spacing_ms: 0,
                    drop: false,
                },
            );
        }
    }
    let promise = |uid: &str, giver: &str, receiver: &str, record: &str, amount: i64| {
        json!({
            "uid":uid,"record":record,"party":format!("${giver}"),"delta":-amount,
            "item":{"title":"Crate","description":"A crate of supplies","exchange":{"uid":uid,"giver":format!("${giver}"),"receiver":format!("${receiver}")}}
        })
    };
    action(
        &mut case,
        "a",
        "routes",
        json!({
            "action":"create-transfer-draft","request_id":"routes","creator":"$ana","slug":"routes","head":"Three routed contributions",
            "agreement":"full","visibility":"hidden","reserve_default":"none","invitees":["$beto","$carla"],
            "promises":[promise("ana-beto","ana","beto","$stock-a",2),promise("carla-ana","carla","ana","",1),promise("carla-beto","carla","beto","",1)]
        }),
    );
    for (cell, person) in [("b", "beto"), ("c", "carla")] {
        action(
            &mut case,
            "a",
            &format!("policy-{cell}"),
            json!({
                "action":"configure-transfer-delivery","transfer":"$routes","recipient_person":format!("${person}"),
                "recipient_organ":format!("$cell:{cell}:organ"),"person":"$ana","request_id":format!("policy-{cell}"),"mode":"replicated"
            }),
        );
        publish(&mut case, cell, &format!("invite-{cell}"));
        add(
            &mut case,
            cell,
            &format!("accept-{cell}"),
            Event::AcceptInvitation {
                transfer: "$routes".into(),
                person: format!("${person}"),
                peer: Some("a".into()),
            },
        );
    }
    for peer in ["b", "c"] {
        publish(&mut case, peer, &format!("accepted-{peer}"));
    }
    for (cell, person) in [("a", "ana"), ("b", "beto"), ("c", "carla")] {
        for level in [1, 2] {
            let id = format!("agree-{cell}-{level}");
            decision(
                &mut case,
                cell,
                &id,
                json!({"action":"set-transfer-agreement-level","transfer":"$routes","person":format!("${person}"),"expected_revision":3,"request_id":id,"level":level}),
            );
        }
    }
    for (route, giver_cell, giver, receiver_cell, receiver, quantity) in [
        ("carla-ana", "c", "carla", "a", "ana", 1),
        ("ana-beto", "a", "ana", "b", "beto", 2),
        ("carla-beto", "c", "carla", "b", "beto", 1),
    ] {
        let activate = format!("activate-{route}");
        decision(
            &mut case,
            giver_cell,
            &activate,
            json!({"action":"activate-transfer-occurrence","transfer":"$routes","promise":route,"person":format!("${giver}"),"expected_revision":3,"request_id":activate}),
        );
        for peer in ["b", "c"] {
            publish(&mut case, peer, &format!("active-{route}-{peer}"));
        }
        for (cell, person, role) in [
            (giver_cell, giver, "delivery"),
            (receiver_cell, receiver, "receipt"),
        ] {
            let id = format!("{role}-{route}");
            decision(
                &mut case,
                cell,
                &id,
                json!({"action":"set-transfer-occurrence-claim","occurrence":format!("${activate}"),"person":format!("${person}"),"request_id":id,"role":role,"claimed":true}),
            );
        }
        if giver_cell == "a" {
            add(
                &mut case,
                "a",
                &format!("settle-{route}"),
                Event::SettleReviewed {
                    occurrence: format!("${activate}"),
                    person: format!("${giver}"),
                    quantity: nucleus::DecimalValue::parse_inferred(&quantity.to_string()).unwrap(),
                },
            );
        } else {
            publish(&mut case, giver_cell, &format!("confirmed-{route}"));
            let handoff = format!("handoff-{route}");
            decision(
                &mut case,
                giver_cell,
                &handoff,
                json!({
                    "action":"begin-transfer-settlement","transfer":"$routes","occurrence":format!("${activate}"),"person":format!("${giver}"),
                    "expected_revision":3,"expected_remaining_quantity":quantity,"canonical_quantity":quantity,"request_id":handoff
                }),
            );
            send(&mut case, "a", giver_cell, &format!("handoff-send-{route}"));
            action(
                &mut case,
                giver_cell,
                &format!("apply-{route}"),
                json!({
                    "action":"apply-transfer-application","transfer":"$routes","handoff":format!("${handoff}"),"local_record":format!("$stock-{giver_cell}"),
                    "person":format!("${giver}"),"request_id":format!("apply-{route}"),"expected_formula_hash":nucleus::transfer::occurrence_application_formula_hash("-incoming()"),"expected_formula_version":0,
                    "expected_local_delta":-quantity,"expected_local_cumulative_before":0
                }),
            );
            send(&mut case, giver_cell, "a", &format!("attestation-{route}"));
        }
        if receiver_cell != "a" {
            publish(&mut case, receiver_cell, &format!("receivable-{route}"));
        }
        add(
            &mut case,
            receiver_cell,
            &format!("receive-{route}"),
            Event::ApplyReceivedTransfer {
                transfer: "$routes".into(),
                occurrence: format!("${activate}"),
                person: format!("${receiver}"),
                local_record: format!("$stock-{receiver_cell}"),
            },
        );
        if receiver_cell != "a" {
            send(&mut case, receiver_cell, "a", &format!("received-{route}"));
        }
    }
    for (cell, expected) in [("a", "29"), ("b", "3"), ("c", "3")] {
        case.checks.push(Check {
            options: Default::default(),
            id: format!("balance-{cell}"),
            predicate: Predicate::QuantityEquals {
                cell: cell.into(),
                record: format!("$stock-{cell}"),
                expected: Quantity {
                    value: nucleus::DecimalValue::parse_inferred(expected).unwrap(),
                    unit: None,
                },
                at_ms: case.end_ms,
            },
        });
    }
    for peer in ["b", "c"] {
        publish(&mut case, peer, &format!("completed-{peer}"));
    }
    action(
        &mut case,
        "a",
        "blocked-observer-delivery",
        json!({
            "action":"configure-transfer-delivery","transfer":"$routes","recipient_person":"$dora",
            "recipient_organ":"$cell:d:organ","person":"$ana","request_id":"blocked-observer-delivery","mode":"replicated"
        }),
    );
    case.checks.push(Check {
        options: Default::default(),
        id: "observer-needs-access".into(),
        predicate: Predicate::ExpectedRefusal {
            input: "blocked-observer-delivery".into(),
            refusal: nucleus::simulation::Refusal::Conflict {
                code: "transfer_delivery_recipient_not_eligible".into(),
            },
        },
    });
    action(
        &mut case,
        "a",
        "observer-access",
        json!({
            "action":"grant-visibility","subject_kind":"actor","subject":"$dora","target":"$routes"
        }),
    );
    action(
        &mut case,
        "a",
        "policy-d",
        json!({
            "action":"configure-transfer-delivery","transfer":"$routes","recipient_person":"$dora",
            "recipient_organ":"$cell:d:organ","person":"$ana","request_id":"policy-d","mode":"replicated"
        }),
    );
    publish(&mut case, "d", "observe");
    action(
        &mut case,
        "d",
        "observer",
        json!({
            "action":"create-transfer-draft","request_id":"observer","creator":"$dora","slug":"outcome-observer","head":"Supplies available",
            "agreement":"dependency","visibility":"hidden","reserve_default":"none",
            "promises":[{"uid":"observer-ready","party":"$dora","delta":1,"item":{"title":"Supplies available"}}],
            "dependencies":[{"scope":"transfer","upstream_kind":"transfer","upstream":"$routes","required_state":"kept"}]
        }),
    );
    add(&mut case, "c", "restart-c", Event::Restart {});
    case
}
